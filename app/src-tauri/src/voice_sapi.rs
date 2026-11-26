//! « Clip ça ! » sur un micro précis (celui choisi dans iClips) : le moteur vocal classique de
//! Windows (SAPI 5, reconnaissance en local, voix installées avec la langue) accepte une entrée
//! choisie, ce que `Windows.Media.SpeechRecognition` ne permet pas (il écoute toujours le micro
//! par défaut de Windows). Même grammaire : une courte liste de phrases, rien n'est enregistré.
//!
//! Les objets SAPI vivent dans leur propre fil ; l'écoute s'arrête quand la valeur est lâchée.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

/// Code de langue SAPI (attribut `Language` des moteurs) d'une langue `fr-FR`, `en-US`…
pub fn sapi_language(tag: &str) -> &'static [&'static str] {
    if tag.to_ascii_lowercase().starts_with("fr") {
        &["40C", "C0C", "100C"]
    } else {
        &["409", "809", "C09", "1009"]
    }
}

/// Entrée SAPI qui correspond à un micro de Windows : les noms SAPI (waveIn) sont coupés à
/// 31 caractères.
pub fn same_device(sapi: &str, endpoint: &str) -> bool {
    let (a, b) = (sapi.trim().to_lowercase(), endpoint.trim().to_lowercase());
    !a.is_empty() && (a == b || b.starts_with(&a) || a.starts_with(&b))
}

/// Règle de repli (celle de la 1.0.19), quand le micro n'a pas pu être comparé aux sorties du PC
/// (voir `decide`). Seule, une grammaire de deux phrases range presque tout
/// ce que le moteur entend (musique, sonneries, dialogues d'un jeu) sous « Clip ça ! » en
/// confiance « normale », score 0,5 à 0,77 : la boucle de clips de la 1.0.10. Avec la dictée
/// concurrente (`rival`), une partie de ces sons part en dictée, mais pas tout : avec de la
/// musique ou un jeu dans les enceintes, le journal d'un vrai joueur montre des « phrases »
/// toutes les quelques secondes sans qu'il parle, en confiance « normale », scores 0,54 à 0,68
/// (et « faible » 0,36 à 0,49). Ses vraies phrases sortent en « haute » (0,82 à 0,90) ou en
/// « normale » dès 0,70. Accepté : « haute » dès 0,75, « normale » dès 0,70, rien en « faible ».
/// Les assouplissements des 1.0.13 à 1.0.18 (faible dès 0,40, répétition) laissaient passer ces
/// sons : plus de faux clips que de phrases manquées. Sans dictée : « haute » et score 0,8.
///
/// Durée (`ms`) : sa voix va de 0,37 à 2,9 s (dite fort ou traînée) ; au-delà de 3 s, jamais.
#[cfg(test)]
pub fn accepted(confidence: i8, engine: f32, rival: bool, ms: u32) -> bool {
    refusal(confidence, engine, rival, ms).is_none()
}

/// Pourquoi le moteur vocal seul refuse la phrase (`None` : il l'accepte), voir `accepted`.
fn refusal(confidence: i8, engine: f32, rival: bool, ms: u32) -> Option<String> {
    if ms > 3_000 {
        return Some("plus de 3 s, ce n'est pas la phrase".into());
    }
    let (high, normal) = if rival { (0.75, Some(0.70)) } else { (0.8, None) };
    match (confidence, normal) {
        (c, _) if c >= 1 && engine >= high => None,
        (c, _) if c >= 1 => Some(format!("score trop bas (confiance haute, il faut {high:.2})")),
        (0, Some(n)) if engine >= n => None,
        (0, Some(n)) => Some(format!("score trop bas (confiance normale, il faut {n:.2})")),
        (0, None) => Some("confiance normale sans dictée concurrente".into()),
        _ => Some("confiance faible".into()),
    }
}

/// Décision complète sur un son reconnu comme la phrase (refonte 1.0.20, d'après de vrais
/// enregistrements du joueur, voir `voice_echo`). Le score du moteur ne sépare pas sa voix du
/// reste : sa vraie phrase sort même surtout en confiance « faible » (0,22 à 0,53), comme les
/// sons du PC et sa conversation. Ce qui les sépare, c'est le micro :
/// 1. la grammaire de la phrase (ou la dictée qui l'écrit), score 0,20 au moins, 3 s au plus ;
/// 2. une voix au micro : 20 dB au-dessus de ce que les sorties du PC (musique, jeu,
///    notifications d'iBox) et le bruit de fond y mettent ;
/// 3. une phrase isolée : le micro plus calme de 5 dB juste avant et juste après.
/// `verdict` : `None` si le micro n'a pas pu être comparé aux sorties (capture impossible) : la
/// règle stricte de la 1.0.19 (`refusal`) décide alors seule. Rend la raison du refus.
pub fn decide(confidence: i8, engine: f32, rival: bool, ms: u32, verdict: Option<&crate::voice_echo::Verdict>) -> Result<(), String> {
    let Some(v) = verdict else {
        return refusal(confidence, engine, rival, ms).map_or(Ok(()), Err);
    };
    if ms > 3_000 {
        return Err("plus de 3 s, ce n'est pas la phrase".into());
    }
    if engine < MIN_SCORE {
        return Err(format!("score trop bas (il faut {MIN_SCORE:.2})"));
    }
    if !v.voice() {
        return Err(if v.speakers { "le son vient du PC (musique, jeu ou notification), pas d'une voix au micro".into() } else { "pas de voix au micro (bruit de la pièce)".into() });
    }
    if !v.isolated() {
        return Err("phrase prise dans une conversation ou une chanson (pas de calme autour)".into());
    }
    Ok(())
}

/// Score minimal du moteur quand le micro est comparé aux sorties du PC.
const MIN_SCORE: f32 = 0.20;

/// La dictée a-t-elle écrit la phrase ? Dite fort, au milieu du son d'un jeu, « Clip ça ! » part
/// parfois en dictée (« clip ça », « clic ça », « clips ça », « clippe sa »…) au lieu de la grammaire.
pub fn dictated(text: &str) -> bool {
    let t: String = text.to_lowercase().chars().map(|c| if c.is_alphanumeric() || c == 'ç' { c } else { ' ' }).collect();
    let words: Vec<&str> = t.split_whitespace().collect();
    // au plus quatre mots : une phrase plus longue qui contient « clip ça » n'est pas la commande
    words.len() <= 4
        && words.windows(2).any(|w| {
            matches!(w[0], "clip" | "clips" | "clippe" | "clipe" | "clic" | "clique" | "klip" | "click")
                && matches!(w[1], "ça" | "sa" | "ca" | "cela" | "that" | "it")
        })
}

/// Identifiant de la grammaire de dictée concurrente.
const DICTATION: u64 = 1;

/// Contrôle du micro contre les enceintes (`voice_echo::Monitor::judge`) : début de la phrase
/// (horloge de `voice_echo`) et durée en ms.
pub type Gate = Box<dyn Fn(f64, u32) -> Option<crate::voice_echo::Verdict> + Send>;

pub struct Listener {
    /// Langue écoutée (`fr-FR`…).
    pub tag: String,
    stop: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

/// D'où vient le son : un micro (nom de Windows) ou un fichier WAV (essais).
pub enum Input {
    /// Nom Windows du micro (`Microphone (Yeti Classic)`).
    Device(String),
    #[allow(dead_code)]
    File(std::path::PathBuf),
}

impl Listener {
    pub fn start(tag: &str, phrases: &'static [&'static str], input: Input, gate: Option<Gate>, on_heard: impl Fn() + Send + 'static) -> Result<Self, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let alive = Arc::new(AtomicBool::new(true));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let (s, a, langs) = (stop.clone(), alive.clone(), sapi_language(tag));
        let thread = std::thread::spawn(move || {
            #[cfg(windows)]
            unsafe {
                imp::run(langs, phrases, input, gate, &s, &ready_tx, &on_heard)
            };
            #[cfg(not(windows))]
            {
                let _ = (langs, phrases, input, gate, &s, &on_heard);
                let _ = ready_tx.send(Err("La commande vocale n'existe que sous Windows.".into()));
            }
            a.store(false, Ordering::Relaxed);
        });
        match ready_rx.recv_timeout(std::time::Duration::from_secs(10)) {
            Ok(Ok(())) => Ok(Self { tag: tag.to_string(), stop, alive, thread: Some(thread) }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => {
                stop.store(true, Ordering::Relaxed);
                Err("Le moteur vocal ne répond pas.".into())
            }
        }
    }

    pub fn alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::Input;
    use std::sync::atomic::{AtomicBool, Ordering};
    use windows::core::{w, Interface, HSTRING, PCWSTR};
    use windows::Win32::Media::Speech::*;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED};

    unsafe fn text(p: windows::core::PWSTR) -> String {
        let s = p.to_string().unwrap_or_default();
        CoTaskMemFree(Some(p.0 as *const _));
        s
    }

    unsafe fn tokens(category: PCWSTR, required: Option<&str>) -> windows::core::Result<Vec<ISpObjectToken>> {
        let cat: ISpObjectTokenCategory = CoCreateInstance(&SpObjectTokenCategory, None, CLSCTX_ALL)?;
        cat.SetId(category, false)?;
        let req = required.map(HSTRING::from);
        let list = cat.EnumTokens(req.as_ref().map(|h| PCWSTR(h.as_ptr())).unwrap_or(PCWSTR::null()), PCWSTR::null())?;
        let mut out = Vec::new();
        loop {
            let mut tok: Option<ISpObjectToken> = None;
            let mut n = 0u32;
            if list.Next(1, &mut tok, Some(&mut n)).is_err() || n == 0 {
                break;
            }
            if let Some(t) = tok {
                out.push(t);
            }
        }
        Ok(out)
    }

    /// Entrées audio SAPI (une par micro waveIn), avec leur nom.
    pub unsafe fn audio_inputs() -> Vec<(String, ISpObjectToken)> {
        tokens(SPCAT_AUDIOIN, None)
            .unwrap_or_default()
            .into_iter()
            .map(|t| (t.GetStringValue(PCWSTR::null()).map(|p| text(p)).unwrap_or_default(), t))
            .collect()
    }

    pub(super) unsafe fn prepare(langs: &[&str], phrases: &[&str], input: Input) -> Result<(ISpRecognizer, ISpRecoContext, ISpRecoGrammar, Option<ISpRecoGrammar>), String> {
        let e = |e: windows::core::Error| format!("moteur vocal : {}", e.message());
        let rec: ISpRecognizer = CoCreateInstance(&SpInprocRecognizer, None, CLSCTX_ALL).map_err(e)?;
        let engine = langs
            .iter()
            .find_map(|l| tokens(SPCAT_RECOGNIZERS, Some(&format!("Language={l}"))).ok().and_then(|v| v.into_iter().next()))
            .ok_or("Aucun moteur vocal Windows pour cette langue (Paramètres Windows › Heure et langue › Voix).")?;
        rec.SetRecognizer(&engine).map_err(e)?;
        match input {
            Input::File(path) => {
                let s: ISpStream = CoCreateInstance(&SpStream, None, CLSCTX_ALL).map_err(e)?;
                s.BindToFile(&HSTRING::from(path.as_os_str()), SPFM_OPEN_READONLY, None, None, 0).map_err(e)?;
                rec.SetInput(&s, true).map_err(e)?;
            }
            Input::Device(name) => {
                let (_, token) = audio_inputs()
                    .into_iter()
                    .find(|(n, _)| super::same_device(n, &name))
                    .ok_or_else(|| format!("Le moteur vocal ne trouve pas le micro « {name} »."))?;
                let mut raw = std::ptr::null_mut();
                token.CreateInstance(None, CLSCTX_ALL.0, &windows::core::IUnknown::IID, &mut raw).map_err(e)?;
                let audio = windows::core::IUnknown::from_raw(raw);
                rec.SetInput(&audio, true).map_err(e)?;
            }
        }
        let ctx = rec.CreateRecoContext().map_err(e)?;
        ctx.SetNotifyWin32Event().map_err(e)?;
        // SPFEI : l'événement voulu, plus les deux bits de contrôle de SAPI
        let mask = (1u64 << SPEI_RECOGNITION.0) | (1u64 << SPEI_END_SR_STREAM.0) | (1u64 << 30) | (1u64 << 33);
        ctx.SetInterest(mask, mask).map_err(e)?;
        let g = ctx.CreateGrammar(0).map_err(e)?;
        let mut state = SPSTATEHANDLE::default();
        g.GetRule(w!("clip"), 0, (SPRAF_TopLevel.0 | SPRAF_Active.0) as u32, true, &mut state).map_err(e)?;
        for p in phrases {
            g.AddWordTransition(state, SPSTATEHANDLE::default(), &HSTRING::from(*p), w!(" "), SPWT_LEXICAL, 1.0, std::ptr::null()).map_err(e)?;
        }
        g.Commit(0).map_err(e)?;
        g.SetRuleState(PCWSTR::null(), std::ptr::null_mut(), SPRS_ACTIVE).map_err(e)?;
        // concurrente : la dictée libre (grammaire 1) absorbe ce qui n'est pas la phrase. Seule,
        // la grammaire de deux phrases range presque tout son entendu sous « Clip ça ! ».
        let free = ctx.CreateGrammar(super::DICTATION).ok().filter(|d| d.LoadDictation(PCWSTR::null(), SPLO_STATIC).is_ok() && d.SetDictationState(SPRS_ACTIVE).is_ok());
        crate::voice::journal::write(if free.is_some() { "dictée concurrente chargée" } else { "dictée indisponible : seuil seul" });
        rec.SetRecoState(SPRST_ACTIVE).map_err(e)?;
        Ok((rec, ctx, g, free))
    }

    pub unsafe fn run(langs: &[&str], phrases: &[&str], input: Input, gate: Option<super::Gate>, stop: &AtomicBool, ready: &std::sync::mpsc::Sender<Result<(), String>>, on_heard: &dyn Fn()) {
        let com = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
        let (rec, ctx, _grammar, free) = match prepare(langs, phrases, input) {
            Ok(v) => v,
            Err(err) => {
                let _ = ready.send(Err(err));
                if com {
                    CoUninitialize();
                }
                return;
            }
        };
        let _ = ready.send(Ok(()));
        'outer: while !stop.load(Ordering::Relaxed) {
            let _ = ctx.WaitForNotifyEvent(250);
            loop {
                let mut ev = SPEVENT::default();
                let mut n = 0u32;
                if ctx.GetEvents(1, &mut ev, &mut n).is_err() || n == 0 {
                    break;
                }
                let id = ev._bitfield & 0xFFFF;
                if id == SPEI_RECOGNITION.0 && ev.lParam.0 != 0 {
                    // le résultat appartient à l'événement : il est relâché ici
                    let result = ISpRecoResult::from_raw(ev.lParam.0 as *mut _);
                    // début de la phrase : compte de l'horloge de Windows, ramené à celle de `voice_echo`
                    let mut times = SPRECORESULTTIMES::default();
                    let age = if result.GetResultTimes(&mut times).is_ok() && times.dwTickCount != 0 {
                        windows_sys::Win32::System::SystemInformation::GetTickCount().wrapping_sub(times.dwTickCount).min(60_000)
                    } else {
                        0
                    };
                    let begun = crate::voice_echo::clock() - age as f64;
                    if let Ok(phrase) = result.GetPhrase() {
                        let confidence = (*phrase).Base.Rule.Confidence;
                        let engine = (*phrase).Base.Rule.SREngineConfidence;
                        let ours = (*phrase).Base.ullGrammarID != super::DICTATION;
                        let ms = (*phrase).Base.ulAudioSizeTime / 10_000;
                        CoTaskMemFree(Some(phrase as *const _));
                        // dictée : son texte peut être la phrase (dite au milieu du son d'un jeu)
                        let words = if ours { String::new() } else {
                            let mut p = windows::core::PWSTR::null();
                            if result.GetText(u32::MAX, u32::MAX, true, &mut p, None).is_ok() && !p.is_null() { text(p) } else { String::new() }
                        };
                        let said = !ours && super::dictated(&words);
                        // rien de ce qui est dit n'est écrit : seulement la grammaire touchée, et
                        // pour la phrase, ce que le micro a entendu comparé aux enceintes
                        let line = if ours || said {
                            let verdict = if age == 0 { None } else { gate.as_ref().and_then(|g| g(begun, ms)) };
                            let decision = if ours {
                                super::decide(confidence, engine, free.is_some(), ms, verdict.as_ref())
                            } else {
                                super::decide(1, 1.0, true, ms, verdict.as_ref())
                            };
                            let room = match (&verdict, &gate) {
                                (Some(v), _) => v.describe(),
                                (None, Some(_)) => "contrôle des enceintes impossible".into(),
                                (None, None) => "sans contrôle des enceintes".into(),
                            };
                            let what = if ours { "la phrase" } else { "la phrase (en dictée)" };
                            let verdict_txt = match &decision { Ok(()) => "accepté".to_string(), Err(why) => format!("refusé : {why}") };
                            (decision.is_ok(), format!("entendu : {what}, confiance {confidence}, score {engine:.2}, {ms} ms ; {room} ; {verdict_txt}"))
                        } else {
                            (false, format!("entendu : autre chose (dictée), confiance {confidence}, score {engine:.2}, {ms} ms ; refusé : pas la phrase"))
                        };
                        let (ok, line) = line;
                        crate::voice::journal::write(&line);
                        if std::env::var_os("IBOX_VOICE_DEBUG").is_some() {
                            // essais seulement : le texte dicté n'est jamais écrit au journal
                            println!("  {:.1} s (début {:.2} s, âge {age} ms)  {line}{}", crate::voice_echo::clock() / 1000.0, begun / 1000.0, if ours { String::new() } else { format!(" [« {words} »]") });
                        }
                        if ok {
                            on_heard();
                        }
                    }
                } else if id == SPEI_END_SR_STREAM.0 {
                    // fin d'un fichier (essais) ou micro débranché
                    break 'outer;
                }
            }
        }
        let _ = rec.SetRecoState(SPRST_INACTIVE);
        drop(ctx);
        drop(rec);
        if com {
            CoUninitialize();
        }
    }
}

/// Un son reconnu dans un fichier (essais) : début et durée (ms depuis le début du fichier),
/// grammaire de la phrase ou dictée, confiance, score, texte dicté.
#[cfg(all(windows, test))]
#[derive(Debug)]
pub struct Hit {
    pub start: u64,
    pub ms: u32,
    pub ours: bool,
    pub confidence: i8,
    pub engine: f32,
    pub words: String,
}

/// Le moteur vocal, réglé comme en vrai (phrase + dictée concurrente), sur un fichier WAV.
#[cfg(all(windows, test))]
pub fn recognize_file(path: &std::path::Path) -> Vec<Hit> {
    use windows::core::Interface;
    use windows::Win32::Media::Speech::*;
    use windows::Win32::System::Com::*;
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let (rec, ctx, _g, _d) = imp::prepare(sapi_language("fr-FR"), crate::voice::phrases("fr-FR"), Input::File(path.to_path_buf())).unwrap();
        let mut out = vec![];
        'outer: loop {
            let _ = ctx.WaitForNotifyEvent(5_000);
            loop {
                let mut ev = SPEVENT::default();
                let mut n = 0u32;
                if ctx.GetEvents(1, &mut ev, &mut n).is_err() || n == 0 {
                    break;
                }
                let id = ev._bitfield & 0xFFFF;
                if id == SPEI_RECOGNITION.0 && ev.lParam.0 != 0 {
                    let r = ISpRecoResult::from_raw(ev.lParam.0 as *mut _);
                    let mut times = SPRECORESULTTIMES::default();
                    let _ = r.GetResultTimes(&mut times);
                    let Ok(phrase) = r.GetPhrase() else { continue };
                    let ours = (*phrase).Base.ullGrammarID != DICTATION;
                    let (confidence, engine) = ((*phrase).Base.Rule.Confidence, (*phrase).Base.Rule.SREngineConfidence);
                    CoTaskMemFree(Some(phrase as *const _));
                    let mut p = windows::core::PWSTR::null();
                    let words = if !ours && r.GetText(u32::MAX, u32::MAX, true, &mut p, None).is_ok() && !p.is_null() {
                        let s = p.to_string().unwrap_or_default();
                        CoTaskMemFree(Some(p.0 as *const _));
                        s
                    } else {
                        String::new()
                    };
                    out.push(Hit { start: times.ullStart / 10_000, ms: (times.ullLength / 10_000) as u32, ours, confidence, engine, words });
                } else if id == SPEI_END_SR_STREAM.0 {
                    break 'outer;
                }
            }
        }
        drop(rec);
        out
    }
}

/// Dictée seule sur un fichier WAV (essais) : (début ms, durée ms, texte) de chaque phrase.
#[cfg(all(windows, test))]
pub fn dictate_file(path: &std::path::Path) -> Vec<(u64, u64, String)> {
    use windows::core::{Interface, HSTRING, PCWSTR};
    use windows::Win32::Media::Speech::*;
    use windows::Win32::System::Com::*;
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let rec: ISpRecognizer = CoCreateInstance(&SpInprocRecognizer, None, CLSCTX_ALL).unwrap();
        let cat: ISpObjectTokenCategory = CoCreateInstance(&SpObjectTokenCategory, None, CLSCTX_ALL).unwrap();
        cat.SetId(SPCAT_RECOGNIZERS, false).unwrap();
        let list = cat.EnumTokens(&HSTRING::from("Language=40C"), PCWSTR::null()).unwrap();
        let mut tok: Option<ISpObjectToken> = None;
        list.Next(1, &mut tok, None).unwrap();
        rec.SetRecognizer(&tok.unwrap()).unwrap();
        let s: ISpStream = CoCreateInstance(&SpStream, None, CLSCTX_ALL).unwrap();
        s.BindToFile(&HSTRING::from(path.as_os_str()), SPFM_OPEN_READONLY, None, None, 0).unwrap();
        rec.SetInput(&s, true).unwrap();
        let ctx = rec.CreateRecoContext().unwrap();
        ctx.SetNotifyWin32Event().unwrap();
        let mask = (1u64 << SPEI_RECOGNITION.0) | (1u64 << SPEI_END_SR_STREAM.0) | (1u64 << 30) | (1u64 << 33);
        ctx.SetInterest(mask, mask).unwrap();
        let d = ctx.CreateGrammar(1).unwrap();
        d.LoadDictation(PCWSTR::null(), SPLO_STATIC).unwrap();
        d.SetDictationState(SPRS_ACTIVE).unwrap();
        rec.SetRecoState(SPRST_ACTIVE).unwrap();
        let mut out = vec![];
        'outer: loop {
            let _ = ctx.WaitForNotifyEvent(5_000);
            loop {
                let mut ev = SPEVENT::default();
                let mut n = 0u32;
                if ctx.GetEvents(1, &mut ev, &mut n).is_err() || n == 0 {
                    break;
                }
                let id = ev._bitfield & 0xFFFF;
                if id == SPEI_RECOGNITION.0 && ev.lParam.0 != 0 {
                    let r = ISpRecoResult::from_raw(ev.lParam.0 as *mut _);
                    let mut times = SPRECORESULTTIMES::default();
                    let _ = r.GetResultTimes(&mut times);
                    let mut p = windows::core::PWSTR::null();
                    let t = if r.GetText(u32::MAX, u32::MAX, true, &mut p, None).is_ok() && !p.is_null() {
                        let s = p.to_string().unwrap_or_default();
                        CoTaskMemFree(Some(p.0 as *const _));
                        s
                    } else {
                        String::new()
                    };
                    out.push((times.ullStart / 10_000, times.ullLength / 10_000, t));
                } else if id == SPEI_END_SR_STREAM.0 {
                    break 'outer;
                }
            }
        }
        out
    }
}

/// Noms des micros que le moteur vocal classique voit (essais).
#[cfg(all(windows, test))]
pub fn inputs() -> Vec<String> {
    unsafe {
        let com = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED).is_ok();
        let out = imp::audio_inputs().into_iter().map(|(n, _)| n).collect();
        if com {
            windows::Win32::System::Com::CoUninitialize();
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noms_des_micros() {
        assert!(same_device("Microphone (Yeti Classic)", "Microphone (Yeti Classic)"));
        // nom SAPI coupé à 31 caractères
        assert!(same_device("Microphone (PRO X 2 LIGHTSPEED", "Microphone (PRO X 2 LIGHTSPEED)"));
        assert!(!same_device("Microphone (Yeti Classic)", "Microphone (PRO X 2 LIGHTSPEED)"));
        assert!(!same_device("", "Microphone (Yeti Classic)"));
        assert_eq!(sapi_language("fr-FR")[0], "40C");
        assert_eq!(sapi_language("en-US")[0], "409");
        assert!(accepted(1, 0.90, false, 800) && !accepted(0, 0.77, false, 800) && !accepted(1, 0.6, false, 800) && !accepted(-1, 0.9, false, 800));
        // journal d'un vrai joueur, musique ou jeu dans les enceintes, sans qu'il parle (11 h 37 à
        // 11 h 41) : aucun de ces sons ne garde de clip…
        for (c, s, ms) in [(0, 0.55, 730), (0, 0.58, 450), (-1, 0.49, 620), (0, 0.58, 290), (0, 0.62, 1160), (-1, 0.46, 530),
            (0, 0.57, 520), (0, 0.68, 1380), (0, 0.54, 380), (0, 0.61, 670), (0, 0.54, 310), (0, 0.63, 450), (0, 0.65, 980),
            (0, 0.78, 4360), (-1, 0.37, 560)] {
            assert!(!accepted(c, s, true, ms), "{c} {s} {ms}");
        }
        // … ses phrases nettes, si (haute 0,82 à 0,90 ; normale dès 0,70, jusqu'à 2,6 s)
        for (c, s, ms) in [(1, 0.90, 580), (1, 0.88, 680), (1, 0.88, 1300), (1, 0.82, 810), (1, 0.86, 670), (0, 0.80, 520),
            (0, 0.74, 740), (0, 0.76, 980), (0, 0.73, 970), (0, 0.70, 2620), (0, 0.70, 1060)] {
            assert!(accepted(c, s, true, ms), "{c} {s} {ms}");
        }
        assert!(!accepted(1, 0.9, true, 3100) && !accepted(-1, 0.9, true, 800));
        for yes in ["Clip ça !", "clip sa", "Clic ça", "clips ça", "Clippe ça.", "ok clip ça"] {
            assert!(dictated(yes), "{yes}");
        }
        for no in ["clip", "ça va", "regarde ce clip ça marche pas du tout", "un clip de ça et de ça", "attention ennemi"] {
            assert!(!dictated(no), "{no}");
        }
    }

    fn heard(voice_db: f32, isolation_db: f32, speakers: bool) -> crate::voice_echo::Verdict {
        crate::voice_echo::Verdict { output: String::new(), speakers, lag_ms: 0, gain_db: None, voice_db, isolation_db }
    }

    /// Valeurs mesurées le 26/11/2026 sur les vrais enregistrements du joueur (micro Yeti).
    #[test]
    fn regle_mesuree() {
        // ses « Clip ça ! » : surtout en confiance faible, refusés par la règle de repli…
        let his = [(-1, 0.48, 830, 39.0, 13.6), (-1, 0.45, 570, 25.4, 29.8), (0, 0.55, 830, 38.8, 18.3), (-1, 0.34, 860, 33.6, 31.9),
            (-1, 0.22, 900, 27.0, 20.0), (0, 0.50, 560, 24.5, 30.3), (-1, 0.25, 780, 25.1, 30.8), (-1, 0.39, 700, 27.5, 32.0),
            (-1, 0.20, 450, 25.7, 6.2), (0, 0.61, 710, 29.9, 16.3), (-1, 0.41, 720, 27.7, 6.4), (-1, 0.44, 610, 34.0, 6.0),
            (0, 0.54, 610, 32.8, 42.0), (0, 0.71, 690, 42.3, 20.0)];
        for (c, s, ms, v, iso) in his {
            assert!(decide(c, s, true, ms, Some(&heard(v, iso, true))).is_ok(), "{c} {s} {v} {iso}");
        }
        assert_eq!(his.iter().filter(|h| decide(h.0, h.1, true, h.2, None).is_ok()).count(), 1);
        // … une phrase jouée par des haut-parleurs collés au micro : de -26 à +19 dB
        for v in [-26.2, -9.5, -0.1, 3.2, 5.1, 6.1, 6.6, 7.3, 11.3, 11.5, 13.8, 14.9, 15.5, 16.3, 17.3, 18.9] {
            assert!(decide(1, 0.9, true, 700, Some(&heard(v, 30.0, true))).is_err(), "{v}");
        }
        // une conversation : pas de calme autour
        assert!(decide(0, 0.6, true, 700, Some(&heard(30.0, 2.0, false))).is_err());
        // trop long, score trop bas : refusés même avec une voix
        assert!(decide(1, 0.9, true, 3_200, Some(&heard(30.0, 30.0, false))).is_err());
        assert!(decide(-1, 0.15, true, 700, Some(&heard(30.0, 30.0, false))).is_err());
        let why = decide(0, 0.6, true, 700, Some(&heard(4.0, 30.0, true))).unwrap_err();
        assert!(why.contains("PC"), "{why}");
    }

    /// Journal réel du 26/11/2026 (1.0.19, 11 h 54 à 12 h 02) : sans comparaison possible avec les
    /// sorties, la règle de repli décide comme la 1.0.19.
    #[test]
    fn journal_rejoue_en_repli() {
        let log = "0 0.64 1000 false|0 0.70 1060 true|0 0.78 540 true|-1 0.42 520 false|0 0.61 700 false|1 0.81 730 true|0 0.56 560 false|1 0.86 640 true|0 0.62 800 false|0 0.57 900 false|1 0.81 590 true|1 0.84 630 true|0 0.63 470 false|1 0.80 1510 true|1 0.82 910 true|0 0.76 2280 true|0 0.56 12920 false|0 0.71 1840 true|0 0.78 1310 true|0 0.69 600 false|1 0.87 580 true|0 0.72 510 true|0 0.73 780 true|0 0.66 480 false";
        for line in log.split('|') {
            let f: Vec<&str> = line.split(' ').collect();
            let (c, s, ms, was): (i8, f32, u32, bool) = (f[0].parse().unwrap(), f[1].parse().unwrap(), f[2].parse().unwrap(), f[3] == "true");
            assert_eq!(decide(c, s, true, ms, None).is_ok(), was, "{line}");
        }
    }

    /// Moteur réel sur le micro (`IBOX_VOICE_MIC` : nom, sinon celui par défaut), pendant
    /// `IBOX_VOICE_SECS` s (60) : affiche chaque reconnaissance avec sa confiance et son score
    /// (`IBOX_VOICE_DEBUG=1`), pour régler `accepted` sur une vraie voix.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn sapi_micro() {
        let name = std::env::var("IBOX_VOICE_MIC").ok().or_else(|| {
            crate::remote_play::audio::endpoints(true).into_iter().find(|e| e.default).map(|e| e.name)
        }).expect("aucun micro");
        let secs: u64 = std::env::var("IBOX_VOICE_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(60);
        println!("micro : {name} ; micros SAPI : {:?}", inputs());
        let t0 = std::time::Instant::now();
        let gate = crate::voice::gate(&name);
        let l = Listener::start("fr-FR", crate::voice::phrases("fr-FR"), Input::Device(name), gate, move || println!("  ACCEPTÉ à {:.1} s", t0.elapsed().as_secs_f32())).expect("moteur");
        println!("écoute {secs} s");
        std::thread::sleep(std::time::Duration::from_secs(secs));
        drop(l);
    }

    /// Dictée seule sur un WAV (`IBOX_VOICE_WAV`) : `cargo test --lib dictee_fichier -- --ignored --nocapture`
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn dictee_fichier() {
        let f = std::path::PathBuf::from(std::env::var("IBOX_VOICE_WAV").expect("IBOX_VOICE_WAV"));
        for (start, ms, text) in dictate_file(&f) {
            println!("{:7.2} s  {ms:5} ms  dictée « {text} »  phrase : {}", start as f32 / 1000.0, dictated(&text));
        }
    }

    /// Moteur réel sur un dossier de fichiers (`IBOX_VOICE_NOISE`) : combien de « Clip ça ! »
    /// chacun déclenche (ceux dont le nom commence par `oui` doivent déclencher, les autres non).
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn sapi_bruit() {
        let dir = std::path::PathBuf::from(std::env::var("IBOX_VOICE_NOISE").expect("IBOX_VOICE_NOISE"));
        let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "wav")).collect();
        files.sort();
        let mut wrong = vec![];
        for f in files {
            let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let c = count.clone();
            println!("{}", f.file_name().unwrap().to_string_lossy());
            let l = Listener::start("fr-FR", crate::voice::phrases("fr-FR"), Input::File(f.clone()), None, move || { c.fetch_add(1, Ordering::Relaxed); }).expect("moteur");
            let t0 = std::time::Instant::now();
            while l.alive() && t0.elapsed() < std::time::Duration::from_secs(30) {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            drop(l);
            let n = count.load(Ordering::Relaxed);
            let name = f.file_name().unwrap().to_string_lossy().into_owned();
            let yes = name.starts_with("oui");
            println!("  => {n} déclenchement(s)");
            // `tolere…` : faux déclenchement accepté en connaissance de cause (voir `accepted`)
            if (n > 0) != yes && !name.starts_with("tolere") { wrong.push(name); }
        }
        assert!(wrong.is_empty(), "mal classés : {wrong:?}");
    }

    /// Moteur réel sur deux fichiers lus par la voix Hortense (`IBOX_VOICE_WAVS` : dossier avec
    /// `oui.wav` « Clip ça ! » et `non.wav`, une autre phrase) : seul le premier déclenche.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn sapi_reel() {
        println!("micros vus : {:?}", inputs());
        let dir = std::path::PathBuf::from(std::env::var("IBOX_VOICE_WAVS").expect("IBOX_VOICE_WAVS"));
        for (file, expected) in [("oui.wav", true), ("non.wav", false)] {
            let heard = Arc::new(AtomicBool::new(false));
            let h = heard.clone();
            let l = Listener::start("fr-FR", crate::voice::phrases("fr-FR"), Input::File(dir.join(file)), None, move || h.store(true, Ordering::Relaxed)).expect("moteur");
            let t0 = std::time::Instant::now();
            while l.alive() && t0.elapsed() < std::time::Duration::from_secs(15) {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            drop(l);
            println!("{file} : entendu = {}", heard.load(Ordering::Relaxed));
            assert_eq!(heard.load(Ordering::Relaxed), expected, "{file}");
        }
    }
}
