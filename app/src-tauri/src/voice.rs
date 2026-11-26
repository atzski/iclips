//! « Clip ça ! » (iClips, clips automatiques) : reconnaissance d'une phrase au micro par le
//! moteur hors ligne de Windows (`Windows.Media.SpeechRecognition`, voix installées avec la
//! langue). La grammaire est une courte liste de phrases : rien n'est envoyé en ligne, rien
//! n'est enregistré. Le micro n'est ouvert que pendant l'écoute (`clips_auto.rs`).
//!
//! Le moteur classique (SAPI, `voice_sapi.rs`) écoute le micro choisi dans iClips, ou à défaut
//! celui par défaut de Windows, sans toucher aux réglages ; s'il échoue, ce moteur-ci prend le
//! micro par défaut.

/// Phrases reconnues pour une langue (avec une variante de prononciation).
pub fn phrases(tag: &str) -> &'static [&'static str] {
    if tag.to_ascii_lowercase().starts_with("fr") {
        &["clip ça", "clippe ça"]
    } else {
        &["clip that", "clip it"]
    }
}

/// Langue d'écoute : celle d'iBox si Windows la reconnaît hors ligne, sinon l'autre (français
/// ou anglais). Les autres langues n'ont pas de phrase.
pub fn pick(ui: &str, available: &[String]) -> Option<String> {
    let has = |p: &str| available.iter().find(|t| t.to_ascii_lowercase().starts_with(p)).cloned();
    let (first, second) = if ui.starts_with("en") { ("en", "fr") } else { ("fr", "en") };
    has(first).or_else(|| has(second))
}

#[cfg(windows)]
pub use imp::languages;

/// Journal de l'écoute (`logs/voix.log`) : moteur, micro, chaque son reconnu (score, durée) et la
/// décision. Sans lui, un clip parti tout seul ou une phrase ignorée chez un joueur ne s'explique
/// pas. Rien de ce qui est dit n'y est écrit (seulement la grammaire touchée), 64 Ko au plus.
pub mod journal {
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::Mutex;

    static FILE: Mutex<Option<PathBuf>> = Mutex::new(None);
    const MAX: u64 = 64 * 1024;

    pub fn set_dir(dir: PathBuf) {
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(mut f) = FILE.lock() {
            *f = Some(dir.join("voix.log"));
        }
    }

    pub fn write(line: &str) {
        let Some(path) = FILE.lock().ok().and_then(|f| f.clone()) else { return };
        // trop long : on garde la seconde moitié
        if std::fs::metadata(&path).map(|m| m.len() > MAX).unwrap_or(false) {
            if let Ok(old) = std::fs::read(&path) {
                let _ = std::fs::write(&path, &old[old.len() / 2..]);
            }
        }
        let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(f, "{at} {line}");
        }
    }
}

/// Contrôle du micro contre les enceintes (`voice_echo`) pour le micro nommé `mic` : il capte ce
/// micro et toutes les sorties de Windows.
#[cfg(windows)]
pub fn gate(mic: &str) -> Option<crate::voice_sapi::Gate> {
    let id = crate::remote_play::audio::endpoints(true).into_iter().find(|e| crate::voice_sapi::same_device(&e.name, mic)).map(|e| e.id);
    let m = crate::voice_echo::Monitor::start(id);
    journal::write(&format!("contrôle des enceintes : micro « {mic} » comparé à {}", m.outputs().iter().map(|n| format!("« {n} »")).collect::<Vec<_>>().join(", ")));
    Some(Box::new(move |start, ms| m.judge(start, ms)))
}

#[cfg(not(windows))]
pub fn gate(_mic: &str) -> Option<crate::voice_sapi::Gate> {
    None
}

/// Écoute en cours : moteur récent (micro par défaut) ou classique (micro choisi).
pub enum Listener {
    #[cfg(windows)]
    Default(imp::Listener),
    Chosen(crate::voice_sapi::Listener),
}

impl Listener {
    /// `mic` : nom Windows du micro choisi dans iClips ; `None` : celui par défaut.
    pub fn start(tag: &str, mic: Option<&str>, on_heard: impl Fn() + Send + Sync + 'static) -> Result<Self, String> {
        let on_heard = std::sync::Arc::new(on_heard);
        // sans micro choisi, le micro par défaut passe aussi par le moteur classique : son seuil
        // (`voice_sapi::accepted`) est vérifié sur de vrais sons, celui du moteur récent ne peut
        // pas l'être (il n'écoute que le micro). Le moteur récent reste le repli.
        let default_mic = if mic.is_none() {
            crate::remote_play::audio::endpoints(true).into_iter().find(|e| e.default).map(|e| e.name)
        } else {
            None
        };
        if let Some(name) = mic.or(default_mic.as_deref()) {
            let h = on_heard.clone();
            match crate::voice_sapi::Listener::start(tag, phrases(tag), crate::voice_sapi::Input::Device(name.to_string()), gate(name), move || h()) {
                Ok(l) => {
                    journal::write(&format!("écoute : moteur classique, micro « {name} » ({}), langue {tag}", if mic.is_some() { "choisi dans iClips" } else { "par défaut" }));
                    return Ok(Listener::Chosen(l));
                }
                Err(e) => {
                    journal::write(&format!("écoute : moteur classique impossible sur « {name} » : {e}"));
                    eprintln!("iClips : « Clip ça ! » sur « {name} » impossible ({e}) : micro par défaut");
                }
            }
        }
        #[cfg(windows)]
        {
            // moteur récent : micro par défaut de Windows, même contrôle des enceintes
            let default_name = crate::remote_play::audio::endpoints(true).into_iter().find(|e| e.default).map(|e| e.name);
            let r = imp::Listener::start(tag, default_name.as_deref().and_then(gate), move || on_heard()).map(Listener::Default);
            match &r {
                Ok(_) => journal::write(&format!("écoute : moteur récent de Windows, micro par défaut, langue {tag}")),
                Err(e) => journal::write(&format!("écoute impossible : {e}")),
            }
            r
        }
        #[cfg(not(windows))]
        {
            let _ = on_heard;
            Err("La commande vocale n'existe que sous Windows.".into())
        }
    }

    pub fn tag(&self) -> &str {
        match self {
            #[cfg(windows)]
            Listener::Default(l) => &l.tag,
            Listener::Chosen(l) => &l.tag,
        }
    }

    /// Faux si l'écoute s'est arrêtée (micro débranché, erreur) : à relancer.
    pub fn alive(&self) -> bool {
        match self {
            #[cfg(windows)]
            Listener::Default(l) => l.alive(),
            Listener::Chosen(l) => l.alive(),
        }
    }

    /// Micro choisi écouté (moteur classique) plutôt que celui par défaut.
    pub fn on_chosen(&self) -> bool {
        matches!(self, Listener::Chosen(_))
    }
}
#[cfg(all(windows, test))]
use imp::check;

#[cfg(windows)]
mod imp {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use windows::core::{Ref, HSTRING};
    use windows_collections::IIterable;
    use windows::Foundation::{TimeSpan, TypedEventHandler};
    use windows::Globalization::Language;
    use windows::Media::SpeechRecognition::{
        SpeechContinuousRecognitionCompletedEventArgs, SpeechContinuousRecognitionResultGeneratedEventArgs,
        SpeechContinuousRecognitionSession, SpeechRecognitionConfidence, SpeechRecognitionListConstraint,
        SpeechRecognitionResultStatus, SpeechRecognizer,
    };

    /// Langues que Windows sait reconnaître hors ligne sur ce PC (`fr-FR`, `en-US`…).
    pub fn languages() -> Vec<String> {
        SpeechRecognizer::SupportedGrammarLanguages()
            .map(|v| v.into_iter().filter_map(|l| l.LanguageTag().ok().map(|t| t.to_string())).collect())
            .unwrap_or_default()
    }

    fn message(e: &windows::core::Error) -> String {
        match e.code().0 as u32 {
            0x8007_0005 => "Windows refuse le micro à iClips (Paramètres Windows › Confidentialité › Microphone : autoriser les applications de bureau).".into(),
            0x8004_503A | 0x8007_0490 => "Aucun micro n'est branché.".into(),
            c => format!("Reconnaissance vocale indisponible (0x{c:08X})."),
        }
    }

    fn status(s: SpeechRecognitionResultStatus) -> String {
        if s == SpeechRecognitionResultStatus::MicrophoneUnavailable {
            "Aucun micro n'est disponible.".into()
        } else {
            format!("Reconnaissance vocale indisponible (état {}).", s.0)
        }
    }

    /// Écoute continue ; `on_heard` est appelé à chaque phrase reconnue avec assurance.
    /// L'écoute s'arrête quand la valeur est lâchée.
    pub struct Listener {
        rec: SpeechRecognizer,
        pub tag: String,
        alive: Arc<AtomicBool>,
    }

    /// Moteur prêt (grammaire compilée), micro encore fermé.
    fn prepare(tag: &str) -> Result<SpeechRecognizer, String> {
        let err = |e: windows::core::Error| message(&e);
        let lang = Language::CreateLanguage(&HSTRING::from(tag)).map_err(err)?;
        let rec = SpeechRecognizer::Create(&lang).map_err(err)?;
        let list: Vec<HSTRING> = super::phrases(tag).iter().map(|p| HSTRING::from(*p)).collect();
        let list: IIterable<HSTRING> = list.into();
        let constraint = SpeechRecognitionListConstraint::CreateWithTag(&list, &HSTRING::from("clip")).map_err(err)?;
        rec.Constraints().map_err(err)?.Append(&constraint).map_err(err)?;
        let compiled = rec.CompileConstraintsAsync().map_err(err)?.join().map_err(err)?;
        let s = compiled.Status().map_err(err)?;
        if s != SpeechRecognitionResultStatus::Success {
            let _ = rec.Close();
            return Err(status(s));
        }
        Ok(rec)
    }

    /// La grammaire se compile-t-elle pour cette langue ? (sans ouvrir le micro ; essai `voix_reelle`)
    #[cfg(test)]
    pub fn check(tag: &str) -> Result<(), String> {
        prepare(tag).map(|rec| { let _ = rec.Close(); })
    }

    impl Listener {
        pub fn start(tag: &str, gate: Option<crate::voice_sapi::Gate>, on_heard: impl Fn() + Send + 'static) -> Result<Self, String> {
            let err = |e: windows::core::Error| message(&e);
            let rec = prepare(tag)?;
            let session = rec.ContinuousRecognitionSession().map_err(err)?;
            // par défaut, l'écoute s'arrête après 20 s de silence
            session.SetAutoStopSilenceTimeout(TimeSpan { Duration: 24 * 3600 * 10_000_000 }).map_err(err)?;
            session
                .ResultGenerated(&TypedEventHandler::new(
                    move |_: Ref<SpeechContinuousRecognitionSession>, args: Ref<SpeechContinuousRecognitionResultGeneratedEventArgs>| {
                        if let Some(a) = args.as_ref() {
                            let r = a.Result()?;
                            // « haute » seulement : en « moyenne », la musique et les dialogues d'un
                            // jeu passaient pour « Clip ça ! » (boucle de clips de la 1.0.10)
                            let confidence = r.Confidence()?;
                            let sure = matches!(confidence, SpeechRecognitionConfidence::High);
                            let raw = r.RawConfidence().unwrap_or(0.0);
                            let ms = r.PhraseDuration().map(|d| d.Duration / 10_000).unwrap_or(0).clamp(0, 60_000) as u32;
                            // début de la phrase (heure UTC de Windows), ramené à l'horloge de `voice_echo`
                            let begun = r.PhraseStartTime().ok().map(|d| {
                                let then = (d.UniversalTime - 116_444_736_000_000_000) as f64 / 10_000.0;
                                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64() * 1_000.0).unwrap_or(then);
                                crate::voice_echo::clock() - (now - then).clamp(0.0, 60_000.0)
                            });
                            let verdict = match (&gate, begun) { (Some(g), Some(b)) => g(b, ms), _ => None };
                            let decision = if r.Status()? != SpeechRecognitionResultStatus::Success {
                                Err("reconnaissance incomplète".to_string())
                            } else if !sure {
                                Err("confiance pas assez haute".to_string())
                            } else {
                                crate::voice_sapi::decide(1, 1.0, true, ms, verdict.as_ref())
                            };
                            let ok = decision.is_ok();
                            let room = verdict.map(|v| v.describe()).unwrap_or_else(|| "sans contrôle des enceintes".into());
                            let why = match decision { Ok(()) => "accepté".to_string(), Err(w) => format!("refusé : {w}") };
                            super::journal::write(&format!("entendu (moteur récent) : confiance {}, score {raw:.2}, {ms} ms ; {room} ; {why}", confidence.0));
                            if ok {
                                on_heard();
                            }
                        }
                        Ok(())
                    },
                ))
                .map_err(err)?;
            let alive = Arc::new(AtomicBool::new(true));
            let flag = alive.clone();
            session
                .Completed(&TypedEventHandler::new(
                    move |_: Ref<SpeechContinuousRecognitionSession>, _: Ref<SpeechContinuousRecognitionCompletedEventArgs>| {
                        flag.store(false, Ordering::Relaxed);
                        Ok(())
                    },
                ))
                .map_err(err)?;
            session.StartAsync().map_err(err)?.join().map_err(err)?;
            Ok(Self { rec, tag: tag.to_string(), alive })
        }

        /// Faux si Windows a arrêté l'écoute (micro débranché, erreur) : à relancer.
        pub fn alive(&self) -> bool {
            self.alive.load(Ordering::Relaxed)
        }
    }

    impl Drop for Listener {
        fn drop(&mut self) {
            if let Ok(s) = self.rec.ContinuousRecognitionSession() {
                if let Ok(op) = s.CancelAsync() {
                    let _ = op.join();
                }
            }
            let _ = self.rec.Close();
        }
    }
}

#[cfg(not(windows))]
pub fn languages() -> Vec<String> {
    Vec::new()
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_phrase_par_langue() {
        assert_eq!(phrases("fr-FR")[0], "clip ça");
        assert_eq!(phrases("en-GB")[0], "clip that");
    }

    #[test]
    fn la_langue_d_ibox_d_abord() {
        let both = vec!["en-US".to_string(), "fr-FR".to_string()];
        assert_eq!(pick("fr", &both).as_deref(), Some("fr-FR"));
        assert_eq!(pick("en", &both).as_deref(), Some("en-US"));
        assert_eq!(pick("en", &["fr-CA".to_string()]).as_deref(), Some("fr-CA"));
        assert_eq!(pick("fr", &["de-DE".to_string()]), None);
    }

    /// Moteur réel : langues hors ligne et grammaire compilée, puis 2 s d'écoute si
    /// `IBOX_VOICE_LISTEN` est posée (le micro s'ouvre). `cargo test --lib voix_reelle -- --ignored --nocapture`
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn voix_reelle() {
        let langs = languages();
        println!("langues : {langs:?}");
        let tag = pick("fr", &langs).expect("ni français ni anglais");
        check(&tag).expect("grammaire");
        println!("grammaire compilée pour {tag}");
        if std::env::var("IBOX_VOICE_LISTEN").is_ok() {
            let t0 = std::time::Instant::now();
            let l = Listener::start(&tag, None, || println!("entendu")).expect("écoute");
            println!("démarrée en {} ms, moteur classique : {}", t0.elapsed().as_millis(), l.on_chosen());
            std::thread::sleep(std::time::Duration::from_secs(2));
            assert!(l.alive());
        }
    }
}
