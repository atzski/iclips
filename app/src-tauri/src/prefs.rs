//! Réglages de l'app autonome, gardés dans `settings.json` (dossier de configuration de l'app) et
//! lus au démarrage, avant toute fenêtre. Dans iBox, la fenêtre principale garde ces réglages et
//! les renvoie au socle ; ici, le natif les garde lui-même. Les clips automatiques ont leur
//! propre fichier (`clips-auto.json`, `clips_auto.rs`).

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Prefs {
    /// Langue de l'interface (« fr », « en ») ; absente : celle de Windows.
    pub lang: Option<String>,
    /// Thème (« dark », « light ») ; absent : sombre.
    pub theme: Option<String>,
    /// Raccourci du panneau en jeu (« Alt+X ») ; absent : celui par défaut.
    pub panel_keys: Option<String>,
    /// Raccourci du panneau coupé.
    pub panel_keys_off: bool,
    /// Mémoire des clips tenue pendant les parties.
    #[serde(default = "yes")]
    pub clips: bool,
    /// Qualité, durée, son… (`clips::ClipSettings`) ; absent : réglages de départ.
    pub clip: Option<crate::clips::ClipSettings>,
    /// Sortie écoutée et micro (identifiants Windows) ; absents : ceux par défaut.
    pub output_device: Option<String>,
    pub mic_device: Option<String>,
    /// Dossier des clips ; absent : `Vidéos\iClips`.
    pub folder: Option<String>,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs { lang: None, theme: None, panel_keys: None, panel_keys_off: false, clips: true, clip: None, output_device: None, mic_device: None, folder: None }
    }
}

/// Réglages de départ des clips dans l'app : 30 s en 1080p60, son de Windows (iBox part de
/// 15 s en 720p, réglage pensé pour toutes les machines ; qui installe iClips veut des clips).
pub fn default_clip() -> crate::clips::ClipSettings {
    crate::clips::ClipSettings { quality: crate::clips::Quality::High, seconds: 30, ..Default::default() }
}

static LOCK: Mutex<()> = Mutex::new(());
static CACHE: Mutex<Option<Prefs>> = Mutex::new(None);

fn path(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_config_dir().ok()?.join("settings.json"))
}

pub fn load(app: &AppHandle) -> Prefs {
    if let Some(p) = CACHE.lock().ok().and_then(|c| c.clone()) {
        return p;
    }
    let p: Prefs = path(app).and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    if let Ok(mut c) = CACHE.lock() {
        *c = Some(p.clone());
    }
    p
}

/// Modifie puis réécrit les réglages.
pub fn update(app: &AppHandle, change: impl FnOnce(&mut Prefs)) {
    let _guard = LOCK.lock();
    let mut prefs = load(app);
    change(&mut prefs);
    if let Ok(mut c) = CACHE.lock() {
        *c = Some(prefs.clone());
    }
    let Some(p) = path(app) else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_vec_pretty(&prefs) {
        let _ = std::fs::write(p, json);
    }
}

/// Langue de Windows : français si l'interface est en français, anglais sinon.
#[cfg(windows)]
fn windows_lang() -> &'static str {
    use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
    // identifiant de langue primaire : 0x0C = français
    if unsafe { GetUserDefaultUILanguage() } & 0x3FF == 0x0C { "fr" } else { "en" }
}
#[cfg(not(windows))]
fn windows_lang() -> &'static str {
    "fr"
}

/// Langue et thème de l'interface (comme `plugins::look` dans iBox).
pub fn look(app: &AppHandle) -> (&'static str, bool) {
    let p = load(app);
    let lang = match p.lang.as_deref() {
        Some("en") => "en",
        Some("fr") => "fr",
        _ => windows_lang(),
    };
    (lang, p.theme.as_deref() != Some("light"))
}

#[cfg(test)]
mod tests {
    use super::Prefs;

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let p: Prefs = serde_json::from_str(r#"{ "panelKeys": "Ctrl+F9" }"#).unwrap();
        assert_eq!(p.panel_keys.as_deref(), Some("Ctrl+F9"));
        assert!(p.clips && p.folder.is_none() && p.clip.is_none());
        let off: Prefs = serde_json::from_str(r#"{ "clips": false }"#).unwrap();
        assert!(!off.clips);
    }
}
