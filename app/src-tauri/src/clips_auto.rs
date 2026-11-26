//! Clips automatiques, repris d'iBox (`clips_auto.rs`) : un clip est gardé sans ouvrir le panneau
//! quand on dit « Clip ça ! » (`voice.rs`), quand on maintient Vue + A sur la manette, ou à la fin
//! d'une partie. Chaque déclencheur s'active dans Automatique ; rien ne tourne hors d'une partie
//! avec la mémoire des clips (ou partout, si demandé).
//!
//! Le micro n'est ouvert que pendant l'écoute ; une notification le dit à chaque début d'écoute
//! en partie et Windows montre son propre témoin.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

use crate::voice;

/// Déclencheurs choisis dans iClips. Au départ, « Clip ça ! » est activé (demande de Gébril,
/// 25/11/2026) : le micro n'écoute que pendant une partie, une notification le dit à chaque
/// début d'écoute, et l'option se coupe dans Automatique. Vue + A et fin de partie sont coupés.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Auto {
    #[serde(default = "yes")]
    pub voice: bool,
    #[serde(default)]
    pub pad: bool,
    #[serde(default)]
    pub end: bool,
}

fn yes() -> bool {
    true
}

impl Default for Auto {
    fn default() -> Self {
        Auto { voice: true, pad: false, end: false }
    }
}

/// Rien ne tourne : aucun plugin activé n'a reçu `clips-auto`.
const OFF: Auto = Auto { voice: false, pad: false, end: false };

impl Auto {
    /// Change seulement les champs donnés (`{voice?, pad?, end?}`).
    pub fn patched(mut self, v: &Value) -> Self {
        if let Some(b) = v["voice"].as_bool() { self.voice = b; }
        if let Some(b) = v["pad"].as_bool() { self.pad = b; }
        if let Some(b) = v["end"].as_bool() { self.end = b; }
        self
    }
}

/// Vue + A maintenus (masque XInput).
pub const PAD_COMBO: u16 = 0x0020 | 0x1000;
/// Deux clips automatiques à moins de 5 s d'écart : le second est ignoré (phrase répétée).
const COOLDOWN_MS: u64 = 5_000;
/// Essai de la voix depuis le Studio.
const TEST_MS: u64 = 8_000;
/// Après un échec de l'écoute, nouvel essai dans 30 s.
const RETRY_MS: u64 = 30_000;
/// `auto.pause` : une pause dure au plus 10 s ; le plugin la renouvelle tant qu'un clip joue
/// (une fenêtre fermée en pleine lecture ne laisse donc pas la voix coupée).
const PAUSE_MAX_MS: u64 = 10_000;
/// Après la pause, la voix reste sourde 1,5 s : le moteur rend parfois une phrase entendue
/// juste avant la fin de la lecture (le son du clip qui sort des haut-parleurs).
const PAUSE_GRACE_MS: u64 = 1_500;
/// Après chaque clip gardé, la voix reste sourde 3 s (+ la grâce) : le son de déclencheur de la
/// notification, entendu par le micro, était pris pour « Clip ça ! » et relançait un clip en boucle.
const AFTER_CLIP_MS: u64 = 3_000;
/// Garde-fou : 6 clips à la voix en moins de 2 min mettent « Clip ça ! » en pause 2 min, puis
/// l'écoute reprend seule (comme iBox 1.0.18).
const RUNAWAY_COUNT: usize = 6;
const RUNAWAY_WINDOW_MS: u64 = 120_000;
const RUNAWAY_PAUSE_MS: u64 = 120_000;

static SETTINGS: Mutex<Option<Auto>> = Mutex::new(None);
/// Mémoire des clips tenue en ce moment : partie en cours, ou partout si demandé (`capture.rs`).
static PLAYING: AtomicBool = AtomicBool::new(false);
/// Combo manette armé : lu par le fil XInput du panneau (`panel.rs`).
static PAD: AtomicBool = AtomicBool::new(false);
static LISTENING: AtomicBool = AtomicBool::new(false);
static TEST_UNTIL: AtomicU64 = AtomicU64::new(0);
static RETRY_AT: AtomicU64 = AtomicU64::new(0);
static LAST: AtomicU64 = AtomicU64::new(0);
/// Voix sourde jusqu'à cette heure (ms) : un clip est rejoué dans iClips (`auto.pause`).
static QUIET_UNTIL: AtomicU64 = AtomicU64::new(0);
static VOICE_THREAD: AtomicBool = AtomicBool::new(false);
/// Heures des derniers clips à la voix, et fin de la pause du garde-fou (levée aussi par `auto.set`).
static VOICE_CLIPS: Mutex<Vec<u64>> = Mutex::new(Vec::new());
static RUNAWAY_UNTIL: AtomicU64 = AtomicU64::new(0);
/// Dernière erreur de l'écoute et langue utilisée, pour iClips.
static VOICE: Mutex<(Option<String>, Option<String>)> = Mutex::new((None, None));
/// Micro écouté par « Clip ça ! » : celui choisi dans iClips, `None` : celui par défaut de Windows.
static VOICE_MIC: Mutex<Option<String>> = Mutex::new(None);

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

fn file(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("clips-auto.json"))
}

pub fn settings(app: &AppHandle) -> Auto {
    if let Some(s) = SETTINGS.lock().ok().and_then(|g| *g) {
        return s;
    }
    let s = file(app).and_then(|f| std::fs::read(f).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    if let Ok(mut g) = SETTINGS.lock() { *g = Some(s); }
    s
}

/// `auto.set` : enregistre les déclencheurs et rend l'état.
pub fn set(app: &AppHandle, patch: &Value) -> Result<Value, String> {
    let s = settings(app).patched(patch);
    let f = file(app).ok_or("Dossier d'iBox introuvable.")?;
    if let Some(d) = f.parent() { let _ = std::fs::create_dir_all(d); }
    std::fs::write(&f, serde_json::to_vec(&s).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    if let Ok(mut g) = SETTINGS.lock() { *g = Some(s); }
    // toucher aux réglages lève la pause du garde-fou
    RUNAWAY_UNTIL.store(0, Ordering::Relaxed);
    if let Ok(mut v) = VOICE_CLIPS.lock() { v.clear(); }
    if !s.voice {
        // nouvel essai tout de suite à la prochaine activation
        RETRY_AT.store(0, Ordering::Relaxed);
    }
    tick(app, PLAYING.load(Ordering::Relaxed));
    Ok(status(app))
}

/// Dans iBox : un plugin activé a-t-il reçu `clips-auto` ? Ici, toujours.
fn allowed(app: &AppHandle) -> bool {
    let _ = app;
    true
}

fn armed(app: &AppHandle) -> Auto {
    if allowed(app) { settings(app) } else { OFF }
}

pub fn listening() -> bool {
    LISTENING.load(Ordering::Relaxed)
}

pub fn pad_armed() -> bool {
    PAD.load(Ordering::Relaxed)
}

/// `auto.get` : réglages, écoute en cours, langue et dernière erreur de la voix.
pub fn status(app: &AppHandle) -> Value {
    let s = settings(app);
    let (error, language) = VOICE.lock().map(|v| v.clone()).unwrap_or_default();
    let language = language.or_else(|| voice::pick(crate::prefs::look(app).0, &voice::languages()));
    json!({
        "voice": s.voice, "pad": s.pad, "end": s.end,
        "listening": listening(), "paused": QUIET_UNTIL.load(Ordering::Relaxed) > now(), "language": language, "error": error,
        "phrase": language.as_deref().map(|l| voice::phrases(l)[0]),
        "mic": VOICE_MIC.lock().ok().and_then(|m| m.clone()),
    })
}

/// Veille de `capture.rs`, toutes les 2 s : partie en cours (avec la mémoire des clips) ou non.
pub fn tick(app: &AppHandle, playing: bool) {
    PLAYING.store(playing, Ordering::Relaxed);
    let a = armed(app);
    PAD.store(playing && a.pad, Ordering::Relaxed);
    if a.voice || TEST_UNTIL.load(Ordering::Relaxed) > now() {
        voice_thread(app);
    }
}

/// `auto.test` : écoute 8 s, même hors partie ; `voice://heard` si la phrase est reconnue.
pub fn test(app: &AppHandle) -> Value {
    TEST_UNTIL.store(now() + TEST_MS, Ordering::Relaxed);
    RETRY_AT.store(0, Ordering::Relaxed);
    voice_thread(app);
    json!({ "ms": TEST_MS })
}

/// Fin de la surdité après un `auto.pause` reçu à `at` : `ms` > 0 la pose (ou la prolonge, sans
/// jamais raccourcir celle d'une autre fenêtre), 0 la lève après le court délai de grâce.
fn quiet_until(current: u64, at: u64, ms: u64) -> u64 {
    if ms > 0 {
        current.max(at + ms.min(PAUSE_MAX_MS) + PAUSE_GRACE_MS)
    } else {
        current.min(at + PAUSE_GRACE_MS)
    }
}

/// `auto.pause` (`{ms}`, 1.0.9) : un clip est rejoué dans iClips, la voix n'écoute plus ce qui
/// sort des haut-parleurs. Les réglages enregistrés ne changent pas ; le micro reste ouvert
/// (pas de nouvelle notification d'écoute), seules les phrases reconnues sont écartées.
pub fn pause(app: &AppHandle, arg: &Value) -> Value {
    hush(arg["ms"].as_u64().unwrap_or(0));
    status(app)
}

/// Pose, prolonge ou lève la surdité de la voix (voir `quiet_until`).
fn hush(ms: u64) {
    let at = now();
    let mut current = QUIET_UNTIL.load(Ordering::Relaxed);
    loop {
        match QUIET_UNTIL.compare_exchange(current, quiet_until(current, at, ms), Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(seen) => current = seen,
        }
    }
}

/// Un clip automatique peut-il partir maintenant ? (pas deux à moins de 5 s)
fn take_turn(last: &AtomicU64, at: u64) -> bool {
    let prev = last.load(Ordering::Relaxed);
    if prev != 0 && at.saturating_sub(prev) < COOLDOWN_MS {
        return false;
    }
    last.compare_exchange(prev, at, Ordering::Relaxed, Ordering::Relaxed).is_ok()
}

fn body(how: &str) -> &'static str {
    match how {
        "voice" => "Clip gardé : « Clip ça ! »",
        "pad" => "Clip gardé : Vue + A",
        _ => "Clip gardé : fin de partie",
    }
}

fn toast(app: &AppHandle, title: &str, body: &str, icon: &str, tone: &str) {
    crate::toast::push(app, json!({ "kind": "notice", "always": true, "friendId": "", "name": title, "body": body, "icon": icon, "tone": tone }));
}

fn save(app: &AppHandle, how: &'static str) {
    let saved = tauri::async_runtime::block_on(crate::capture::save_clip_as(app.clone(), Some(how)));
    // la notification qui suit sonne : le micro ne doit pas la prendre pour la phrase
    hush(AFTER_CLIP_MS);
    match saved {
        // notification d'iClips : jaquette du jeu, son de déclencheur d'appareil photo
        Ok(c) => crate::toast::push(app, json!({
            "kind": "clip", "friendId": "", "name": c.game.clone().unwrap_or_else(|| "Bureau".into()),
            "gameTitle": c.game, "platformId": c.platform_id, "body": body(how), "clip": c.name,
        })),
        Err(e) => toast(app, "Clip impossible", &e, "play", "warn"),
    }
}

/// Voix ou manette : garde le clip sur un autre fil (la phrase ou le combo n'attendent pas).
pub fn trigger(app: &AppHandle, how: &'static str) {
    if !PLAYING.load(Ordering::Relaxed) || !take_turn(&LAST, now()) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || save(&app, how));
}

/// Fin de partie (veille de `capture.rs`, avant que la mémoire ne se vide).
pub fn on_game_end(app: &AppHandle) {
    if armed(app).end {
        save(app, "end");
    }
}

fn want_voice(app: &AppHandle) -> bool {
    TEST_UNTIL.load(Ordering::Relaxed) > now()
        || (PLAYING.load(Ordering::Relaxed) && armed(app).voice)
}

/// Note un clip à la voix à `at` ; vrai s'il fait le compte du garde-fou dans la fenêtre.
fn runaway(times: &mut Vec<u64>, at: u64) -> bool {
    times.retain(|t| at.saturating_sub(*t) < RUNAWAY_WINDOW_MS);
    times.push(at);
    times.len() >= RUNAWAY_COUNT
}

fn set_listening(app: &AppHandle, on: bool) {
    if LISTENING.swap(on, Ordering::Relaxed) != on {
        crate::host::notify(app, "voice://state", json!({ "listening": on }));
    }
}

pub(crate) fn heard(app: &AppHandle) {
    if TEST_UNTIL.swap(0, Ordering::Relaxed) > now() {
        crate::host::notify(app, "voice://heard", Value::Null);
        return;
    }
    // un clip rejoué dans iClips, ou la notification d'un clip qui vient d'être gardé :
    // la phrase vient des haut-parleurs, pas du joueur
    if QUIET_UNTIL.load(Ordering::Relaxed) > now() {
        eprintln!("iClips : « Clip ça ! » écarté (clip en lecture ou notification de clip)");
        return;
    }
    if RUNAWAY_UNTIL.load(Ordering::Relaxed) > now() || !armed(app).voice {
        return;
    }
    if !PLAYING.load(Ordering::Relaxed) || !take_turn(&LAST, now()) {
        return;
    }
    let stop = VOICE_CLIPS.lock().map(|mut v| runaway(&mut v, now())).unwrap_or(false);
    let app = app.clone();
    std::thread::spawn(move || {
        save(&app, "voice");
        if stop {
            RUNAWAY_UNTIL.store(now() + RUNAWAY_PAUSE_MS, Ordering::Relaxed);
            if let Ok(mut v) = VOICE_CLIPS.lock() { v.clear(); }
            toast(&app, "« Clip ça ! » en pause",
                "Six clips à la voix en moins de 2 min : l'écoute fait une pause de 2 min, au cas où un son du PC ressemblerait à la phrase.",
                "sound", "warn");
        }
    });
}

/// Fil de l'écoute : ouvre le micro quand il le faut, le ferme dès que ce n'est plus le cas.
fn voice_thread(app: &AppHandle) {
    if VOICE_THREAD.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    // journal de l'écoute (logs/voix.log), comme dans iBox
    if let Ok(dir) = app.path().app_log_dir() {
        crate::voice::journal::set_dir(dir);
    }
    std::thread::spawn(move || {
        let mut listener: Option<(voice::Listener, &'static str, Option<String>)> = None;
        loop {
            let want = want_voice(&app);
            let ui = crate::prefs::look(&app).0;
            // micro choisi dans iClips (identifiant de Windows) : l'écoute le suit
            let mic = crate::clips::devices().1;
            if listener.as_ref().is_some_and(|(l, lang, m)| !want || !l.alive() || *lang != ui || *m != mic) {
                listener = None;
                set_listening(&app, false);
            }
            if want && listener.is_none() && now() >= RETRY_AT.load(Ordering::Relaxed) {
                match start(&app, ui, mic.as_deref()) {
                    Ok(l) => {
                        if let Ok(mut v) = VOICE.lock() { *v = (None, Some(l.tag().to_string())); }
                        let heard_on = if l.on_chosen() { mic.as_deref().and_then(|id| crate::remote_play::audio::endpoints(true).into_iter().find(|e| e.id == id).map(|e| e.name)) } else { None };
                        if let Ok(mut m) = VOICE_MIC.lock() { *m = heard_on; }
                        listener = Some((l, ui, mic));
                        set_listening(&app, true);
                        if PLAYING.load(Ordering::Relaxed) && TEST_UNTIL.load(Ordering::Relaxed) <= now() {
                            toast(&app, "« Clip ça ! » vous écoute", "Le micro écoute pour « Clip ça ! », sur ce PC seulement.", "sound", "info");
                        }
                    }
                    Err(e) => {
                        eprintln!("iClips : écoute de « Clip ça ! » impossible : {e}");
                        if let Ok(mut v) = VOICE.lock() { v.0 = Some(e.clone()); }
                        RETRY_AT.store(now() + RETRY_MS, Ordering::Relaxed);
                        crate::host::notify(&app, "voice://state", json!({ "listening": false, "error": e }));
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(400));
        }
    });
}

fn start(app: &AppHandle, ui: &str, mic: Option<&str>) -> Result<voice::Listener, String> {
    let tag = voice::pick(ui, &voice::languages())
        .ok_or("Windows ne sait reconnaître ni le français ni l'anglais sur ce PC (Paramètres Windows › Heure et langue › Langue : reconnaissance vocale).")?;
    // nom Windows du micro choisi (le moteur classique l'ouvre par son nom)
    let name = mic.and_then(|id| crate::remote_play::audio::endpoints(true).into_iter().find(|e| e.id == id).map(|e| e.name));
    let app = app.clone();
    voice::Listener::start(&tag, name.as_deref(), move || heard(&app))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_reglages_changent_champ_par_champ() {
        // au départ : « Clip ça ! » seul
        assert_eq!(Auto::default(), Auto { voice: true, pad: false, end: false });
        let a = OFF.patched(&json!({ "voice": true }));
        assert_eq!(a, Auto { voice: true, pad: false, end: false });
        let b = a.patched(&json!({ "pad": true, "end": "oui" }));
        assert_eq!(b, Auto { voice: true, pad: true, end: false });
        let read: Auto = serde_json::from_str(r#"{"pad":true}"#).unwrap();
        assert_eq!(read, Auto { voice: true, pad: true, end: false });
        // un « non » déjà donné reste un « non »
        let kept: Auto = serde_json::from_str(r#"{"voice":false}"#).unwrap();
        assert!(!kept.voice);
    }

    #[test]
    fn pas_deux_clips_automatiques_a_la_suite() {
        let last = AtomicU64::new(0);
        assert!(take_turn(&last, 10_000));
        assert!(!take_turn(&last, 12_000));
        assert!(!take_turn(&last, 14_999));
        assert!(take_turn(&last, 15_000));
    }

    #[test]
    fn la_pause_de_la_voix_se_renouvelle_et_se_leve() {
        // pause de 4 s à t = 1000 : sourde jusqu'à 1000 + 4000 + grâce
        let a = quiet_until(0, 1_000, 4_000);
        assert_eq!(a, 1_000 + 4_000 + PAUSE_GRACE_MS);
        // une durée démesurée est bornée
        assert_eq!(quiet_until(0, 0, 3_600_000), PAUSE_MAX_MS + PAUSE_GRACE_MS);
        // une pause plus courte d'une autre fenêtre ne raccourcit pas la première
        assert_eq!(quiet_until(a, 1_500, 500), a);
        // fin de lecture : encore la grâce, puis la voix revient
        let b = quiet_until(a, 2_000, 0);
        assert_eq!(b, 2_000 + PAUSE_GRACE_MS);
        // lever une pause déjà finie ne la rallume pas
        assert_eq!(quiet_until(0, 9_000, 0), 0);
        // clip gardé à t = 20 s : sourde pendant que la notification sonne, puis la voix revient
        let c = quiet_until(0, 20_000, AFTER_CLIP_MS);
        assert!(c >= 20_000 + 4_000 && c <= 20_000 + COOLDOWN_MS);
        // une lecture plus longue en cours n'est pas raccourcie par un clip gardé
        assert_eq!(quiet_until(30_000, 20_000, AFTER_CLIP_MS), 30_000);
    }

    #[test]
    fn six_clips_a_la_voix_en_2_min_mettent_la_voix_en_pause() {
        let mut t = vec![];
        for i in 0..5 {
            assert!(!runaway(&mut t, 10_000 + i * 6_000));
        }
        assert!(runaway(&mut t, 50_000));
        // un clip toutes les 50 s : jamais de pause
        let mut s = vec![];
        for i in 0..10 {
            assert!(!runaway(&mut s, i * 50_000));
        }
    }

    #[test]
    fn le_combo_est_vue_et_a() {
        assert_eq!(PAD_COMBO, crate::win::pad::BACK | crate::win::pad::A);
    }
}
