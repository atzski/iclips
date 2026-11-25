//! Appels de l'interface (`POST /iclips/__host/<appel>`), repris d'iBox (`plugins/host.rs`) :
//! mêmes noms, mêmes arguments, mêmes réponses. Dans iBox, chaque appel est vérifié contre les
//! permissions accordées au plugin ; ici l'interface est celle de l'app, compilée avec elle, et
//! seules les fenêtres de l'app sont servies (`protocol.rs`). La notification n'a droit qu'à
//! ses propres appels.
//!
//! Appels propres à l'app autonome (`app.*`) : réglages de l'application (démarrage avec Windows,
//! raccourci du panneau, dossier des clips, langue, thème) et Quitter.
//!
//! « Envoyer à un ami » (`share.friends`, `share.send`) et `community.open` demandent iBox et un
//! compte iNetbox : refusés ici, et cachés par l'interface (`standalone`, `ui/src/host.ts`).

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;

use crate::protocol::{PANEL, STUDIO, TOAST};

pub fn call(app: &AppHandle, label: &str, name: &str, arg: Value) -> Result<Value, String> {
    if label == TOAST && !name.starts_with("toast.") {
        return Err(format!("Appel refusé : {name}"));
    }
    match name {
        "overlay.close" => {
            crate::panel::hide(app);
            Ok(Value::Null)
        }
        "clips.list" => crate::capture::clip_list(app).map(Value::from),
        "clips.update" => {
            let name = arg.get("name").and_then(Value::as_str).ok_or("name manquant")?;
            crate::capture::clip_update(app, name, arg.get("title").and_then(Value::as_str), arg.get("favorite").and_then(Value::as_bool))?;
            Ok(Value::Null)
        }
        "clips.delete" => {
            let name = arg.get("name").and_then(Value::as_str).ok_or("name manquant")?;
            crate::capture::clip_delete(app, name)?;
            Ok(Value::Null)
        }
        "clips.thumb" => {
            let name = arg.get("name").and_then(Value::as_str).ok_or("name manquant")?;
            let jpeg = arg.get("jpeg").and_then(Value::as_str).ok_or("jpeg manquant")?;
            let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, jpeg).map_err(|_| "Image illisible.")?;
            crate::capture::clip_thumb(app, name, &bytes, arg.get("duration").and_then(Value::as_f64))?;
            Ok(Value::Null)
        }
        "clips.status" => Ok(crate::capture::clip_status()),
        "clips.save" => {
            let c = tauri::async_runtime::block_on(crate::capture::save_clip(app.clone()))?;
            Ok(json!({ "name": c.name, "game": c.game, "src": format!("__clips/{}", c.name) }))
        }
        "edit.render" => {
            let job = crate::clip_edit::start(app, crate::clip_edit::Request::parse(&arg)?)?;
            Ok(json!({ "job": job }))
        }
        "auto.get" => Ok(crate::clips_auto::status(app)),
        "auto.set" => crate::clips_auto::set(app, &arg),
        "auto.test" => Ok(crate::clips_auto::test(app)),
        "auto.pause" => Ok(crate::clips_auto::pause(app, &arg)),
        "shot.save" => crate::capture::save_frozen(app),
        "capture.get" => Ok(crate::capture::capture_state(app)),
        "capture.set" => crate::capture::capture_apply(app, &arg),
        "capture.folder" => crate::capture::open_folder(app),
        "share.friends" | "share.send" | "community.open" => Err("Réservé à iBox (compte iNetbox).".into()),
        n if n.starts_with("share.") => {
            let name = arg.get("name").and_then(Value::as_str).ok_or("name manquant")?;
            match n {
                "share.copy" => crate::share::copy(app, name),
                "share.drag" => crate::share::drag(app, label, name),
                "share.reveal" => crate::share::reveal(app, name),
                "share.save" => crate::share::save(app, name),
                "share.link" => crate::share::link(app, name),
                "share.copyLink" => crate::share::copy_link(app, name),
                _ => Err(format!("Appel inconnu : {n}")),
            }
        }
        "screen.open" => {
            crate::panel::hide(app);
            crate::open_studio(app);
            Ok(Value::Null)
        }
        "screen.close" => {
            close_studio(app);
            Ok(Value::Null)
        }
        "app.get" => Ok(app_state(app)),
        "app.set" => app_set(app, &arg),
        "app.folder" => pick_folder(app),
        "app.keys.suspend" => {
            crate::shortcut::suspend(arg["suspended"].as_bool().unwrap_or(false));
            Ok(Value::Null)
        }
        "app.quit" => {
            app.exit(0);
            Ok(Value::Null)
        }
        "toast.pull" => Ok(crate::toast::pull()),
        "toast.done" => {
            crate::toast::done(app);
            Ok(Value::Null)
        }
        "toast.open" => {
            crate::open_studio(app);
            Ok(Value::Null)
        }
        // essais réels seulement (`ICLIPS_ESSAI=1` posé au lancement) : la phrase « Clip ça ! »
        // est réputée entendue, pour vérifier la suite (clip gardé, notification, son) sans parler
        // dans le micro. Absent sans la variable.
        "essai.heard" if std::env::var("ICLIPS_ESSAI").as_deref() == Ok("1") => {
            crate::clips_auto::heard(app);
            Ok(Value::Null)
        }
        _ => Err(format!("Appel inconnu : {name}")),
    }
}

fn close_studio(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(STUDIO) {
        let _ = w.close();
    }
}

/// `app.get` : réglages de l'application.
pub fn app_state(app: &AppHandle) -> Value {
    let p = crate::prefs::load(app);
    let (keys, taken) = crate::shortcut::state();
    json!({
        "version": app.package_info().version.to_string(),
        "autostart": app.autolaunch().is_enabled().unwrap_or(false),
        "keys": keys, "keysTaken": taken, "keysDefault": crate::shortcut::DEFAULT_PANEL,
        "folder": crate::capture::folder(app).ok().map(|f| f.to_string_lossy().into_owned()),
        "folderDefault": p.folder.is_none(),
        "lang": p.lang, "theme": p.theme,
    })
}

/// `app.set` `{autostart?, keys? (texte, null : coupé), lang?, theme?, folder? (null : par défaut)}`.
fn app_set(app: &AppHandle, patch: &Value) -> Result<Value, String> {
    if let Some(on) = patch["autostart"].as_bool() {
        let launcher = app.autolaunch();
        if on { launcher.enable() } else { launcher.disable() }.map_err(|e| e.to_string())?;
    }
    if let Some(v) = patch.get("keys") {
        let keys = v.as_str().map(|s| s.parse::<crate::shortcut::Shortcut>()).transpose().map_err(|e| format!("Raccourci invalide : {e}"))?;
        crate::shortcut::apply(keys);
        crate::prefs::update(app, |p| {
            p.panel_keys = keys.map(|k| k.to_string());
            p.panel_keys_off = keys.is_none();
        });
    }
    if patch.get("folder").is_some_and(Value::is_null) {
        crate::prefs::update(app, |p| p.folder = None);
        notify(app, "clips://changed", Value::Null);
    }
    let before = crate::prefs::look(app);
    if let Some(v) = patch.get("lang") {
        let lang = v.as_str().filter(|l| matches!(*l, "fr" | "en")).map(String::from);
        crate::prefs::update(app, |p| p.lang = lang);
    }
    if let Some(v) = patch.get("theme") {
        let theme = v.as_str().filter(|t| matches!(*t, "dark" | "light")).map(String::from);
        crate::prefs::update(app, |p| p.theme = theme);
    }
    let after = crate::prefs::look(app);
    if before != after {
        // les fenêtres ouvertes se rechargent avec la nouvelle langue ou le nouveau thème
        let payload = json!({ "lang": after.0, "scheme": if after.1 { "dark" } else { "light" } });
        for label in [STUDIO, PANEL, TOAST] {
            emit_to(app, label, "look://changed", payload.clone());
        }
    }
    Ok(app_state(app))
}

/// `app.folder` : choisit un autre dossier pour les clips (les anciens restent où ils sont).
fn pick_folder(app: &AppHandle) -> Result<Value, String> {
    use tauri_plugin_dialog::DialogExt;
    let mut dialog = app.dialog().file().set_title("Dossier des clips");
    if let Ok(dir) = crate::capture::folder(app) {
        dialog = dialog.set_directory(dir);
    }
    if let Some(dir) = dialog.blocking_pick_folder().and_then(|p| p.into_path().ok()) {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        crate::prefs::update(app, |p| p.folder = Some(dir.to_string_lossy().into_owned()));
        notify(app, "clips://changed", Value::Null);
    }
    Ok(app_state(app))
}

/// Envoie un événement à une fenêtre de l'interface (`ibox:host`, comme dans iBox).
pub fn emit_to(app: &AppHandle, label: &str, event: &str, payload: Value) {
    if let Some(w) = app.get_webview_window(label) {
        let detail = json!({ "event": event, "payload": payload }).to_string();
        let _ = w.eval(format!("window.dispatchEvent(new CustomEvent(\"ibox:host\", {{ detail: {detail} }}))"));
    }
}

/// Envoie un événement du socle au Studio et au panneau (`clips://changed`, `edit://progress`…).
pub fn notify(app: &AppHandle, event: &str, payload: Value) {
    for label in [STUDIO, PANEL] {
        emit_to(app, label, event, payload.clone());
    }
}
