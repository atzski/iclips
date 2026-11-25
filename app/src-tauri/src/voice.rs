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
            match crate::voice_sapi::Listener::start(tag, phrases(tag), crate::voice_sapi::Input::Device(name.to_string()), move || h()) {
                Ok(l) => return Ok(Listener::Chosen(l)),
                Err(e) => eprintln!("iClips : « Clip ça ! » sur « {name} » impossible ({e}) : micro par défaut"),
            }
        }
        #[cfg(windows)]
        {
            imp::Listener::start(tag, move || on_heard()).map(Listener::Default)
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
            0x8007_0005 => "Windows refuse le micro à iBox (Paramètres Windows › Confidentialité › Microphone : autoriser les applications de bureau).".into(),
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
        pub fn start(tag: &str, on_heard: impl Fn() + Send + 'static) -> Result<Self, String> {
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
                            let sure = matches!(r.Confidence()?, SpeechRecognitionConfidence::High);
                            if r.Status()? == SpeechRecognitionResultStatus::Success && sure {
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
            let l = Listener::start(&tag, None, || println!("entendu")).expect("écoute");
            std::thread::sleep(std::time::Duration::from_secs(2));
            assert!(l.alive());
        }
    }
}
