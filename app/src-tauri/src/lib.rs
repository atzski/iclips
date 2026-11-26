//! iClips, application autonome : le studio de clips d'iBox sans iBox.
//!
//! L'app vit dans la zone de notification. Pendant une partie (`game.rs`), elle tient en mémoire
//! les dernières secondes (`clips.rs`) ; Alt+X (réglable) ou Vue + X sur une manette ouvre le
//! panneau en jeu (`panel.rs`), « Clip ça ! », Vue + A ou la fin de la partie gardent un clip
//! sans rien ouvrir (`clips_auto.rs`). Le Studio s'ouvre depuis l'icône.
//!
//! L'interface est celle du plugin iClips (`ui/`, compilée dans `dist/ui`), servie par le même
//! protocole qu'iBox (`protocol.rs`, `host.rs`) : un seul code d'interface pour les deux.
//!
//! Règle reprise d'iBox : aucune fenêtre n'est créée dans une commande synchrone ni dans un
//! gestionnaire de la boucle d'événements (blocage de WebView2) ; on passe par un fil qui
//! demande la création à la boucle (`run_on_main_thread`).

pub mod capture;
pub mod clip_edit;
pub mod clips;
pub mod clips_auto;
pub mod game;
pub mod host;
pub mod panel;
pub mod prefs;
pub mod protocol;
pub mod remote_play;
pub mod share;
pub mod shortcut;
pub mod toast;
pub mod voice;
pub mod voice_echo;
pub mod voice_sapi;
pub mod win;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};

/// Argument du démarrage avec Windows : l'app reste discrète (pas de Studio à l'écran).
const HIDDEN: &str = "--hidden";

/// Ouvre (ou ramène devant) le Studio.
pub fn open_studio(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let main = app.clone();
        let _ = main.run_on_main_thread(move || {
            if let Some(w) = app.get_webview_window(protocol::STUDIO) {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
                return;
            }
            let Ok(url) = protocol::page(&app, "ui/studio.html", "").parse() else { return };
            let home = protocol::url("");
            let built = WebviewWindowBuilder::new(&app, protocol::STUDIO, WebviewUrl::CustomProtocol(url))
                .title("iClips Studio")
                .inner_size(1280.0, 800.0)
                .min_inner_size(960.0, 600.0)
                .center()
                .background_color(tauri::window::Color(12, 11, 18, 255))
                // l'interface reste chez elle : ni autre site, ni fenêtre surgissante
                .on_navigation(move |u| u.as_str().starts_with(&home))
                .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
                .build();
            match built {
                Ok(w) => {
                    let _ = w.set_focus();
                }
                Err(e) => eprintln!("iClips : Studio indisponible : {e}"),
            }
        });
    });
}

fn tray(app: &AppHandle) -> tauri::Result<()> {
    let en = prefs::look(app).0 == "en";
    let tr = |fr: &'static str, english: &'static str| if en { english } else { fr };
    let studio = MenuItem::with_id(app, "studio", tr("Ouvrir le Studio", "Open the Studio"), true, None::<&str>)?;
    let panel = MenuItem::with_id(app, "panel", tr("Panneau en jeu", "In-game panel"), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", tr("Quitter iClips", "Quit iClips"), true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&studio, &panel, &PredefinedMenuItem::separator(app)?, &quit])?;
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("iClips")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id.as_ref() {
            "studio" => open_studio(app),
            "panel" => panel::toggle(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                open_studio(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        // une seule instance : relancer l'app ouvre le Studio
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| open_studio(app)))
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec![HIDDEN])))
        .plugin(tauri_plugin_dialog::init())
        // interface et appels au natif, comme dans iBox (protocol.rs)
        .register_asynchronous_uri_scheme_protocol(protocol::SCHEME, |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let label = ctx.webview_label().to_string();
            std::thread::spawn(move || responder.respond(protocol::handle(&app, &label, &request)));
        })
        .setup(|app| {
            let handle = app.handle().clone();
            let p = prefs::load(&handle);
            let keys = if p.panel_keys_off { None } else { p.panel_keys.as_deref().and_then(|k| k.parse().ok()).or_else(|| shortcut::DEFAULT_PANEL.parse().ok()) };
            shortcut::start(handle.clone(), keys, |app, slot| match slot {
                shortcut::Slot::Panel => panel::toggle(app),
            });
            capture::start(handle.clone());
            panel::start(handle.clone());
            tray(&handle)?;
            if !std::env::args().any(|a| a == HIDDEN) {
                open_studio(&handle);
            }
            Ok(())
        })
        // panneau et notification : fermés (Alt+F4…), ils seraient perdus jusqu'au redémarrage
        // (Tauri garde le libellé) ; ils se cachent seulement
        .on_window_event(|w, e| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = e {
                match w.label() {
                    protocol::PANEL => {
                        api.prevent_close();
                        panel::hide(w.app_handle());
                    }
                    protocol::TOAST => {
                        api.prevent_close();
                        let _ = w.hide();
                    }
                    _ => {}
                }
            }
        })
        .invoke_handler(tauri::generate_handler![])
        .build(tauri::generate_context!())
        .expect("iClips n'a pas pu démarrer");
    app.run(|_app, event| {
        // fermer le Studio ne quitte pas l'app : elle reste dans la zone de notification
        if let RunEvent::ExitRequested { api, code: None, .. } = event {
            api.prevent_exit();
        }
    });
}
