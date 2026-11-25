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

/// Reconnaissance de la phrase acceptée. Seule, une grammaire de deux phrases range presque tout
/// ce que le moteur entend (musique, sonneries, dialogues d'un jeu) sous « Clip ça ! » en
/// confiance « normale », score 0,5 à 0,77 : la boucle de clips de la 1.0.10. Avec la dictée
/// concurrente (`rival`), ces sons partent en dictée et la phrase dite au micro sort en « normale »
/// (0,57 à 0,78 mesuré sur une vraie voix), acceptée aussitôt, ou en « faible » (0,45 et 0,46, la
/// même voix) : une phrase ordinaire sur sept tombe aussi là (0,47, dialogue de jeu), donc une
/// « faible » n'est acceptée que répétée (`faint`, deux en moins de 5 s : le joueur qui insiste).
/// Sans dictée (moteur qui ne la propose pas) : « haute » et score 0,8, seuil seul.
pub fn accepted(confidence: i8, engine: f32, rival: bool) -> bool {
    if rival {
        confidence >= 0
    } else {
        confidence >= 1 && engine >= 0.8
    }
}

/// « Faible » mais plausible : comptée pour la répétition (voir `accepted`).
pub fn faint(confidence: i8, engine: f32, rival: bool) -> bool {
    rival && confidence < 0 && engine >= 0.40
}

/// Deux « faibles » à moins de 5 s : la seconde est acceptée.
pub const FAINT_PAIR_MS: u128 = 5_000;

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
    pub fn start(tag: &str, phrases: &'static [&'static str], input: Input, on_heard: impl Fn() + Send + 'static) -> Result<Self, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let alive = Arc::new(AtomicBool::new(true));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let (s, a, langs) = (stop.clone(), alive.clone(), sapi_language(tag));
        let thread = std::thread::spawn(move || {
            #[cfg(windows)]
            unsafe {
                imp::run(langs, phrases, input, &s, &ready_tx, &on_heard)
            };
            #[cfg(not(windows))]
            {
                let _ = (langs, phrases, input, &s, &on_heard);
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

    unsafe fn prepare(langs: &[&str], phrases: &[&str], input: Input) -> Result<(ISpRecognizer, ISpRecoContext, ISpRecoGrammar, Option<ISpRecoGrammar>), String> {
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

    pub unsafe fn run(langs: &[&str], phrases: &[&str], input: Input, stop: &AtomicBool, ready: &std::sync::mpsc::Sender<Result<(), String>>, on_heard: &dyn Fn()) {
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
        let mut last_faint: Option<std::time::Instant> = None;
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
                    if let Ok(phrase) = result.GetPhrase() {
                        let confidence = (*phrase).Base.Rule.Confidence;
                        let engine = (*phrase).Base.Rule.SREngineConfidence;
                        let ours = (*phrase).Base.ullGrammarID != super::DICTATION;
                        let ms = (*phrase).Base.ulAudioSizeTime / 10_000;
                        CoTaskMemFree(Some(phrase as *const _));
                        // dictée : son texte peut être la phrase (dite au milieu du son d'un jeu)
                        let said = if ours { false } else {
                            let mut p = windows::core::PWSTR::null();
                            result.GetText(u32::MAX, u32::MAX, true, &mut p, None).is_ok() && !p.is_null() && super::dictated(&text(p))
                        };
                        let mut ok = (ours && super::accepted(confidence, engine, free.is_some())) || said;
                        if ours && !ok && super::faint(confidence, engine, free.is_some()) {
                            ok = last_faint.is_some_and(|t: std::time::Instant| t.elapsed().as_millis() < super::FAINT_PAIR_MS);
                            last_faint = if ok { None } else { Some(std::time::Instant::now()) };
                        }
                        // rien de ce qui est dit n'est écrit : seulement la grammaire touchée
                        crate::voice::journal::write(&format!(
                            "entendu : {}, confiance {confidence}, score {engine:.2}, {ms} ms, accepté {ok}",
                            if ours { "la phrase" } else if said { "la phrase (en dictée)" } else { "autre chose (dictée)" }
                        ));
                        if std::env::var_os("IBOX_VOICE_DEBUG").is_some() {
                            println!("  reconnu : {}, confiance {confidence}, moteur {engine:.3}, {ms} ms, accepté {ok}", if ours { "phrase" } else if said { "dictée = phrase" } else { "dictée" });
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
        assert!(accepted(1, 0.90, false) && !accepted(0, 0.77, false) && !accepted(1, 0.6, false) && !accepted(-1, 0.9, false));
        // avec la dictée : normale aussitôt ; faible dès 0,40 (vraie voix : 0,45) seulement répétée
        assert!(accepted(0, 0.3, true) && !accepted(-1, 0.445, true));
        assert!(faint(-1, 0.445, true) && !faint(-1, 0.376, true) && !faint(0, 0.6, true) && !faint(-1, 0.9, false));
        for yes in ["Clip ça !", "clip sa", "Clic ça", "clips ça", "Clippe ça.", "ok clip ça"] {
            assert!(dictated(yes), "{yes}");
        }
        for no in ["clip", "ça va", "regarde ce clip ça marche pas du tout", "un clip de ça et de ça", "attention ennemi"] {
            assert!(!dictated(no), "{no}");
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
        let l = Listener::start("fr-FR", crate::voice::phrases("fr-FR"), Input::Device(name), move || println!("  ACCEPTÉ à {:.1} s", t0.elapsed().as_secs_f32())).expect("moteur");
        println!("écoute {secs} s");
        std::thread::sleep(std::time::Duration::from_secs(secs));
        drop(l);
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
            let l = Listener::start("fr-FR", crate::voice::phrases("fr-FR"), Input::File(f.clone()), move || { c.fetch_add(1, Ordering::Relaxed); }).expect("moteur");
            let t0 = std::time::Instant::now();
            while l.alive() && t0.elapsed() < std::time::Duration::from_secs(30) {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            drop(l);
            let n = count.load(Ordering::Relaxed);
            let yes = f.file_name().unwrap().to_string_lossy().starts_with("oui");
            println!("  => {n} déclenchement(s)");
            if (n > 0) != yes { wrong.push(f.file_name().unwrap().to_string_lossy().into_owned()); }
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
            let l = Listener::start("fr-FR", crate::voice::phrases("fr-FR"), Input::File(dir.join(file)), move || h.store(true, Ordering::Relaxed)).expect("moteur");
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
