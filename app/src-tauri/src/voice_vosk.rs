//! « Clip ça ! » par Vosk (alphacep, Apache 2.0) : reconnaissance hors ligne moderne (Kaldi),
//! livrée avec iClips (`resources/vosk` : `libvosk.dll` et ses trois bibliothèques, modèle
//! français `vosk-model-small-fr-0.22`). Le moteur classique de Windows (SAPI) prenait n'importe
//! quel son court pour la phrase (« Tac », une conversation, la musique) ; ici la grammaire est
//! fermée : « clip ça » ou « [unk] » (tout le reste), et chaque mot reconnu porte sa confiance.
//!
//! La bibliothèque est chargée à l'exécution (pas de liaison à la compilation) : sans elle, iClips
//! garde le moteur de Windows.

use serde_json::Value;

/// Grammaire : la phrase, ses variantes de prononciation, et tout le reste en « [unk] ».
pub fn grammar(tag: &str) -> &'static str {
    if tag.to_ascii_lowercase().starts_with("fr") {
        // leurres : les sons proches (« Clic », « Tac », « C'est ça », « Les clips ») y vont au
        // lieu de la phrase ; « sa » n'y est pas (même son que « ça » : la confiance se partageait)
        r#"["clip ça", "clic", "clique", "clips", "tac", "glisse", "kiss", "c est", "ça", "quoi", "non", "[unk]"]"#
    } else {
        r#"["clip that", "clip it", "[unk]"]"#
    }
}

/// Un résultat final de Vosk : les mots retenus (texte, confiance, début et fin en s).
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub word: String,
    pub conf: f32,
    pub start: f32,
    pub end: f32,
}

pub fn parse(json: &str) -> Vec<Word> {
    let v: Value = serde_json::from_str(json).unwrap_or(Value::Null);
    v["result"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|w| Word {
                    word: w["word"].as_str().unwrap_or_default().to_string(),
                    conf: w["conf"].as_f64().unwrap_or(0.0) as f32,
                    start: w["start"].as_f64().unwrap_or(0.0) as f32,
                    end: w["end"].as_f64().unwrap_or(0.0) as f32,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Confiance minimale de chaque mot de la phrase.
pub const MIN_CONF: f32 = 0.85;

/// Silence minimal (s) entre la phrase et un autre mot de la même prise : « Clip ça ! » dit puis
/// suivi d'autre chose passe ; « les clips ça marche », dit d'une traite, non.
pub const GAP: f32 = 0.3;

/// La phrase, chaque mot sûr, seule ou séparée des autres mots par un vrai silence : ce que garde
/// « Clip ça ! ». Rend (début, fin) en s.
pub fn phrase(words: &[Word], tag: &str) -> Option<(f32, f32)> {
    let second: &[&str] = if tag.to_ascii_lowercase().starts_with("fr") { &["ça"] } else { &["that", "it"] };
    (0..words.len().saturating_sub(1)).find_map(|i| {
        let (a, b) = (&words[i], &words[i + 1]);
        let ours = a.word == "clip" && second.contains(&b.word.as_str()) && a.conf >= MIN_CONF && b.conf >= MIN_CONF;
        let before = i == 0 || a.start - words[i - 1].end >= GAP;
        let after = i + 2 == words.len() || words[i + 2].start - b.end >= GAP;
        (ours && before && after).then_some((a.start, b.end))
    })
}

/// 48 kHz → 16 kHz (moyenne de trois échantillons : filtre passe-bas suffisant pour la voix).
pub fn to_16k(samples: &[f32], carry: &mut Vec<f32>, out: &mut Vec<i16>) {
    carry.extend_from_slice(samples);
    let n = carry.len() / 3 * 3;
    out.extend(carry[..n].chunks_exact(3).map(|c| (((c[0] + c[1] + c[2]) / 3.0).clamp(-1.0, 1.0) * 32767.0) as i16));
    carry.drain(..n);
}

/// Décision sur une phrase rendue par Vosk : la phrase seule, chaque mot sûr, puis (si le micro a
/// pu être comparé aux sorties du PC) une voix au micro. Rend la raison du refus.
pub fn decide(words: &[Word], tag: &str, verdict: Option<&crate::voice_echo::Verdict>) -> Result<(), String> {
    let has_clip = words.iter().any(|w| w.word == "clip");
    if phrase(words, tag).is_none() {
        let sure = words.iter().all(|w| w.word != "clip" || w.conf >= MIN_CONF);
        return Err(if !has_clip {
            "pas la phrase".to_string()
        } else if !sure || words.len() == 2 {
            format!("mot pas assez sûr (il faut {MIN_CONF:.2})")
        } else {
            "la phrase prise dans d'autres mots, sans silence autour".to_string()
        });
    }
    match verdict {
        Some(v) if !v.voice() => Err(if v.speakers { "le son vient du PC (musique, jeu ou notification), pas d'une voix au micro" } else { "pas de voix au micro" }.to_string()),
        _ => Ok(()),
    }
}

#[cfg(windows)]
pub use engine::{Model, Recognizer};

#[cfg(windows)]
pub use listen::Listener;

#[cfg(windows)]
mod listen {
    use super::{decide, parse, to_16k, Model, Recognizer, Word};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc, Mutex, OnceLock};
    use std::thread::JoinHandle;

    /// Modèle chargé une fois (1 à 2 s), gardé pour les écoutes suivantes.
    fn model(dir: &Path) -> Result<Arc<Model>, String> {
        static MODEL: OnceLock<Mutex<Option<Arc<Model>>>> = OnceLock::new();
        let slot = MODEL.get_or_init(|| Mutex::new(None));
        let mut g = slot.lock().map_err(|e| e.to_string())?;
        if let Some(m) = g.as_ref() {
            return Ok(m.clone());
        }
        let m = Arc::new(Model::load(dir, &dir.join("model"))?);
        *g = Some(m.clone());
        Ok(m)
    }

    /// Écoute continue du micro par Vosk ; s'arrête quand la valeur est lâchée.
    pub struct Listener {
        pub tag: String,
        stop: Arc<AtomicBool>,
        alive: Arc<AtomicBool>,
        threads: Vec<JoinHandle<()>>,
    }

    impl Listener {
        /// `dir` : dossier de Vosk (`libvosk.dll`, `model/`) ; `mic` : identifiant Windows du
        /// micro (`None` : celui par défaut) ; `gate` : contrôle du micro contre les sorties.
        pub fn start(dir: &Path, tag: &str, mic: Option<String>, gate: Option<crate::voice_sapi::Gate>, on_heard: impl Fn() + Send + 'static) -> Result<Self, String> {
            let model = model(dir)?;
            let mut rec = Recognizer::new(&model, super::grammar(tag))?;
            let stop = Arc::new(AtomicBool::new(false));
            let alive = Arc::new(AtomicBool::new(true));
            // les phrases sont jugées sur un autre fil : la capture du micro n'attend jamais
            let (tx, rx) = mpsc::channel::<(Vec<Word>, f64, u32)>();
            let lang = tag.to_string();
            let judge = std::thread::spawn(move || {
                for (words, start, ms) in rx {
                    let verdict = if super::phrase(&words, &lang).is_some() { gate.as_ref().and_then(|g| g(start, ms)) } else { None };
                    let decision = decide(&words, &lang, verdict.as_ref());
                    let room = match (&verdict, &gate) {
                        (Some(v), _) => format!(" ; {}", v.describe()),
                        _ => String::new(),
                    };
                    let why = match &decision { Ok(()) => "accepté".to_string(), Err(w) => format!("refusé : {w}") };
                    // seuls les mots de la grammaire (la phrase, les leurres, « [unk] ») existent : rien
                    // de ce qui est dit d'autre n'est écrit
                    let said: Vec<String> = words.iter().map(|w| format!("{} {:.2}", w.word, w.conf)).collect();
                    let line = format!("entendu (Vosk) : « {} », {ms} ms{room} ; {}", said.join(", "), if decision.is_ok() { "accepté".to_string() } else { why });
                    crate::voice::journal::write(&line);
                    if std::env::var_os("IBOX_VOICE_DEBUG").is_some() {
                        println!("  {:.1} s  {line}", crate::voice_echo::clock() / 1000.0);
                    }
                    if decision.is_ok() {
                        on_heard();
                    }
                }
            });
            let (st, al) = (stop.clone(), alive.clone());
            let capture = std::thread::spawn(move || {
                let (mut carry, mut pcm) = (Vec::new(), Vec::new());
                let mut fed = 0u64;
                let r = crate::remote_play::audio::pcm(&st, true, mic.as_deref(), &mut |chunk| {
                    let now = crate::voice_echo::clock();
                    let mono: Vec<f32> = chunk.chunks_exact(2).map(|s| (s[0] + s[1]) * 0.5).collect();
                    pcm.clear();
                    to_16k(&mono, &mut carry, &mut pcm);
                    fed += pcm.len() as u64;
                    let debug = std::env::var_os("IBOX_VOICE_DEBUG").is_some();
                    if debug && fed / 32_000 != (fed - pcm.len() as u64) / 32_000 {
                        let peak = mono.iter().fold(0f32, |a, &b| a.max(b.abs()));
                        println!("    [{:.0} s de son, crête {:.3}] en cours : {}", fed as f64 / 16_000.0, peak, rec.partial().replace('\n', " "));
                    }
                    if let Some(json) = rec.accept(&pcm) {
                        let words = parse(&json);
                        if let (Some(first), Some(last)) = (words.first(), words.last()) {
                            // instant du début de la phrase sur l'horloge de `voice_echo`
                            let start = now - (fed as f64 / 16.0 - first.start as f64 * 1_000.0);
                            let ms = ((last.end - first.start) * 1_000.0).max(0.0) as u32;
                            let _ = tx.send((words, start, ms));
                        }
                    }
                    !st.load(Ordering::Relaxed)
                });
                if let Err(e) = r {
                    crate::voice::journal::write(&format!("écoute (Vosk) arrêtée : {e}"));
                }
                al.store(false, Ordering::Relaxed);
            });
            Ok(Listener { tag: tag.to_string(), stop, alive, threads: vec![capture, judge] })
        }

        pub fn alive(&self) -> bool {
            self.alive.load(Ordering::Relaxed)
        }
    }

    impl Drop for Listener {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            // la capture s'arrête, ferme le canal, et le fil des jugements finit
            for t in self.threads.drain(..) {
                let _ = t.join();
            }
        }
    }

    /// Dossier de Vosk livré avec iClips (`resources/vosk`), posé au démarrage de l'écoute.
    pub static DIR: Mutex<Option<PathBuf>> = Mutex::new(None);
}

/// Pose le dossier de Vosk livré avec iClips.
#[cfg(windows)]
pub fn set_dir(dir: std::path::PathBuf) {
    if let Ok(mut d) = listen::DIR.lock() {
        *d = Some(dir);
    }
}

/// Dossier de Vosk s'il est complet (bibliothèque et modèle).
#[cfg(windows)]
pub fn dir() -> Option<std::path::PathBuf> {
    listen::DIR.lock().ok().and_then(|d| d.clone()).filter(|d| d.join("libvosk.dll").is_file() && d.join("model").join("am").join("final.mdl").is_file())
}

#[cfg(windows)]
mod engine {
    use std::ffi::{c_char, c_float, c_int, c_short, c_void, CStr, CString};
    use std::path::Path;
    use std::sync::OnceLock;

    type ModelNew = unsafe extern "C" fn(*const c_char) -> *mut c_void;
    type ModelFree = unsafe extern "C" fn(*mut c_void);
    type RecNew = unsafe extern "C" fn(*mut c_void, c_float) -> *mut c_void;
    type RecNewGrm = unsafe extern "C" fn(*mut c_void, c_float, *const c_char) -> *mut c_void;
    type SetWords = unsafe extern "C" fn(*mut c_void, c_int);
    type AcceptS = unsafe extern "C" fn(*mut c_void, *const c_short, c_int) -> c_int;
    type GetText = unsafe extern "C" fn(*mut c_void) -> *const c_char;
    type Partial = unsafe extern "C" fn(*mut c_void) -> *const c_char;
    type RecFree = unsafe extern "C" fn(*mut c_void);
    type LogLevel = unsafe extern "C" fn(c_int);

    struct Api {
        model_new: ModelNew,
        model_free: ModelFree,
        rec_new: RecNew,
        rec_new_grm: RecNewGrm,
        set_words: SetWords,
        accept: AcceptS,
        result: GetText,
        final_result: GetText,
        partial: Partial,
        rec_free: RecFree,
    }

    static API: OnceLock<Result<Api, String>> = OnceLock::new();

    fn api(dir: &Path) -> Result<&'static Api, String> {
        API.get_or_init(|| unsafe {
            use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_WITH_ALTERED_SEARCH_PATH};
            let dll = dir.join("libvosk.dll");
            let wide: Vec<u16> = dll.as_os_str().encode_wide_nul();
            // les trois bibliothèques dont dépend libvosk.dll sont cherchées dans son dossier
            let h = LoadLibraryExW(wide.as_ptr(), std::ptr::null_mut(), LOAD_WITH_ALTERED_SEARCH_PATH);
            if h.is_null() {
                return Err(format!("Vosk introuvable ({})", dll.display()));
            }
            macro_rules! sym {
                ($name:literal) => {
                    std::mem::transmute(GetProcAddress(h, concat!($name, "\0").as_ptr()).ok_or(concat!("Vosk : ", $name, " absent"))?)
                };
            }
            let log: LogLevel = sym!("vosk_set_log_level");
            log(-1);
            Ok(Api {
                model_new: sym!("vosk_model_new"),
                model_free: sym!("vosk_model_free"),
                rec_new: sym!("vosk_recognizer_new"),
                rec_new_grm: sym!("vosk_recognizer_new_grm"),
                set_words: sym!("vosk_recognizer_set_words"),
                accept: sym!("vosk_recognizer_accept_waveform_s"),
                result: sym!("vosk_recognizer_result"),
                final_result: sym!("vosk_recognizer_final_result"),
                partial: sym!("vosk_recognizer_partial_result"),
                rec_free: sym!("vosk_recognizer_free"),
            })
        })
        .as_ref()
        .map_err(|e| e.clone())
    }

    trait Wide {
        fn encode_wide_nul(&self) -> Vec<u16>;
    }
    impl Wide for std::ffi::OsStr {
        fn encode_wide_nul(&self) -> Vec<u16> {
            use std::os::windows::ffi::OsStrExt;
            self.encode_wide().chain(std::iter::once(0)).collect()
        }
    }

    /// Modèle chargé (lecture seule, partageable entre fils).
    pub struct Model {
        ptr: *mut c_void,
        api: &'static Api,
    }
    unsafe impl Send for Model {}
    unsafe impl Sync for Model {}

    impl Model {
        /// `dir` : dossier de `libvosk.dll` ; `model` : dossier du modèle.
        pub fn load(dir: &Path, model: &Path) -> Result<Self, String> {
            let api = api(dir)?;
            let path = CString::new(model.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
            let ptr = unsafe { (api.model_new)(path.as_ptr()) };
            if ptr.is_null() {
                return Err(format!("modèle Vosk illisible ({})", model.display()));
            }
            Ok(Model { ptr, api })
        }
    }

    impl Drop for Model {
        fn drop(&mut self) {
            unsafe { (self.api.model_free)(self.ptr) }
        }
    }

    /// Reconnaissance en continu (16 kHz) sur une grammaire fermée.
    pub struct Recognizer {
        ptr: *mut c_void,
        api: &'static Api,
    }
    unsafe impl Send for Recognizer {}

    impl Recognizer {
        /// `grammar` : liste JSON de phrases permises ; vide : transcription libre.
        pub fn new(model: &Model, grammar: &str) -> Result<Self, String> {
            let g = CString::new(grammar).map_err(|e| e.to_string())?;
            let ptr = unsafe { if grammar.is_empty() { (model.api.rec_new)(model.ptr, 16_000.0) } else { (model.api.rec_new_grm)(model.ptr, 16_000.0, g.as_ptr()) } };
            if ptr.is_null() {
                return Err("grammaire Vosk refusée".into());
            }
            unsafe { (model.api.set_words)(ptr, 1) };
            Ok(Recognizer { ptr, api: model.api })
        }

        fn text(p: *const c_char) -> String {
            if p.is_null() { String::new() } else { unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned() }
        }

        /// Ajoute du son ; rend le résultat (JSON) quand une phrase se termine (silence).
        pub fn accept(&mut self, pcm: &[i16]) -> Option<String> {
            let done = unsafe { (self.api.accept)(self.ptr, pcm.as_ptr(), pcm.len() as c_int) };
            (done > 0).then(|| Self::text(unsafe { (self.api.result)(self.ptr) }))
        }

        /// Phrase en cours (JSON `partial`), sans confiance.
        pub fn partial(&mut self) -> String {
            Self::text(unsafe { (self.api.partial)(self.ptr) })
        }

        /// Fin du son : la dernière phrase en cours.
        pub fn finish(&mut self) -> String {
            Self::text(unsafe { (self.api.final_result)(self.ptr) })
        }
    }

    impl Drop for Recognizer {
        fn drop(&mut self) {
            unsafe { (self.api.rec_free)(self.ptr) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(word: &str, conf: f32) -> Word {
        Word { word: word.into(), conf, start: 1.0, end: 1.5 }
    }

    fn at(word: &str, start: f32, end: f32) -> Word {
        Word { word: word.into(), conf: 1.0, start, end }
    }

    #[test]
    fn la_phrase_seule_et_sure() {
        let json = r#"{"result":[{"conf":1.0,"end":1.2,"start":0.9,"word":"clip"},{"conf":0.97,"end":1.5,"start":1.2,"word":"ça"}],"text":"clip ça"}"#;
        let words = parse(json);
        assert_eq!(phrase(&words, "fr-FR"), Some((0.9, 1.5)));
        assert_eq!(phrase(&[w("clip", 1.0), w("ça", 0.5)], "fr-FR"), None);
        assert_eq!(phrase(&[w("[unk]", 1.0)], "fr-FR"), None);
        // « les clips ça marche » d'une traite : non ; « Clip ça ! », silence, puis autre chose : oui
        assert_eq!(phrase(&[at("[unk]", 0.0, 1.0), at("clip", 1.0, 1.3), at("ça", 1.3, 1.6), at("[unk]", 1.6, 2.0)], "fr-FR"), None);
        assert_eq!(phrase(&[at("clip", 1.0, 1.3), at("ça", 1.3, 1.6), at("[unk]", 2.1, 2.5)], "fr-FR"), Some((1.0, 1.6)));
        assert_eq!(phrase(&[at("[unk]", 0.0, 0.5), at("clip", 1.0, 1.3), at("ça", 1.3, 1.6)], "fr-FR"), Some((1.0, 1.6)));
        assert_eq!(phrase(&[at("[unk]", 0.0, 0.9), at("clip", 1.0, 1.3), at("ça", 1.3, 1.6)], "fr-FR"), None);
        assert_eq!(phrase(&[w("clip", 1.0)], "fr-FR"), None);
        assert!(phrase(&[w("clip", 0.9), w("that", 0.9)], "en-US").is_some());
        assert!(decide(&[w("tac", 1.0)], "fr-FR", None).unwrap_err().contains("pas la phrase"));
        assert!(decide(&[w("clip", 0.6), w("ça", 1.0)], "fr-FR", None).unwrap_err().contains("pas assez sûr"));
        assert!(decide(&[at("non", 0.0, 1.0), at("clip", 1.0, 1.3), at("ça", 1.3, 1.6)], "fr-FR", None).unwrap_err().contains("autres mots"));
        assert!(parse("{\"text\":\"\"}").is_empty());
    }

    #[test]
    fn reechantillonnage() {
        let (mut carry, mut out) = (vec![], vec![]);
        to_16k(&[0.3; 7], &mut carry, &mut out);
        assert_eq!((out.len(), carry.len()), (2, 1));
        to_16k(&[0.3; 2], &mut carry, &mut out);
        assert_eq!((out.len(), carry.len()), (3, 0));
    }

    /// Chaîne réelle complète (micro `IBOX_VOICE_MIC`, Vosk de `IBOX_VOSK_DIR`, contrôle des
    /// sorties, décision) pendant `IBOX_VOICE_SECS` s ; `IBOX_VOICE_DEBUG=1` affiche chaque son.
    /// `cargo test --lib vosk_micro -- --ignored --nocapture`
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn vosk_micro() {
        set_dir(std::path::PathBuf::from(std::env::var("IBOX_VOSK_DIR").expect("IBOX_VOSK_DIR")));
        let secs: u64 = std::env::var("IBOX_VOICE_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(60);
        let mic = std::env::var("IBOX_VOICE_MIC").ok();
        let t0 = std::time::Instant::now();
        let l = crate::voice::Listener::start("fr-FR", mic.as_deref(), move || println!("  >>> CLIP GARDÉ à {:.1} s", t0.elapsed().as_secs_f32())).expect("écoute");
        assert!(matches!(l, crate::voice::Listener::Vosk(..)), "Vosk non chargé");
        println!("ÉCOUTE {secs} s");
        std::thread::sleep(std::time::Duration::from_secs(secs));
        drop(l);
    }

    /// Vosk réel sur un WAV 48 kHz 16 bits mono (`IBOX_VOICE_WAV`), bibliothèque et modèle dans
    /// `IBOX_VOSK_DIR` (`libvosk.dll`, `model/`) : chaque phrase reconnue, ses mots et confiances.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn vosk_fichier() {
        let dir = std::path::PathBuf::from(std::env::var("IBOX_VOSK_DIR").expect("IBOX_VOSK_DIR"));
        let model = Model::load(&dir, &dir.join("model")).unwrap();
        let g = std::env::var("IBOX_VOSK_GRAMMAIRE").unwrap_or_else(|_| grammar("fr-FR").into());
        let mut rec = Recognizer::new(&model, if g == "libre" { "" } else { &g }).unwrap();
        let wav = std::fs::read(std::env::var("IBOX_VOICE_WAV").expect("IBOX_VOICE_WAV")).unwrap();
        let data = wav.windows(4).position(|w| w == b"data").unwrap() + 8;
        let s: Vec<f32> = wav[data..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect();
        let (mut carry, mut pcm) = (vec![], vec![]);
        to_16k(&s, &mut carry, &mut pcm);
        let show = |json: String| {
            let words = parse(&json);
            if !words.is_empty() {
                let list: Vec<String> = words.iter().map(|w| format!("{}({:.2})", w.word, w.conf)).collect();
                println!("{:7.2} s  {:<40} -> {}", words[0].start, list.join(" "), if phrase(&words, "fr-FR").is_some() { "CLIP" } else { "-" });
            }
        };
        for c in pcm.chunks(1_600) {
            if let Some(j) = rec.accept(c) {
                show(j);
            }
        }
        show(rec.finish());
    }
}
