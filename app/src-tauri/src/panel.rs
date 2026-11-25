//! Panneau en jeu, repris d'iBox (`plugins/panel.rs`) : la page `ui/panel.html` dans une fenêtre
//! transparente qui couvre l'écran principal, par-dessus le jeu.
//!
//! - ouverture : raccourci (Alt+X par défaut, `shortcut.rs`) ou Vue + X maintenus à la manette ;
//! - l'image du jeu est figée avant que le panneau ne la couvre (capture d'écran à garder) ;
//! - le jeu au premier plan est retenu à l'ouverture et retrouve le focus à la fermeture ;
//! - la fenêtre est exclue des captures : elle n'apparaît ni dans les clips ni dans les images ;
//! - panneau ouvert, la manette est transmise à la page (`ibox:host`, événement `pad`) ;
//! - événements `panel://open` et `panel://close-request` (l'interface anime puis appelle
//!   `overlay.close`).

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};

use crate::protocol::{self, PANEL};

static VISIBLE: AtomicBool = AtomicBool::new(false);
/// Fenêtre au premier plan avant l'ouverture (le jeu), pour lui rendre le focus.
static PREVIOUS: AtomicIsize = AtomicIsize::new(0);
const COMBO_HOLD: Duration = Duration::from_millis(400);
/// Largeur de la colonne du panneau (px CSS), côté droit, comme le manifeste du plugin.
const WIDTH: u32 = 480;

pub fn visible() -> bool {
    VISIBLE.load(Ordering::Relaxed)
}

/// Envoie un événement à la page du panneau.
fn emit(app: &AppHandle, event: &str, payload: serde_json::Value) {
    crate::host::emit_to(app, PANEL, event, payload);
}

/// Raccourci, combo ou icône : ouvre, ou demande à la page de fermer (elle anime la sortie puis
/// appelle `overlay.close`). Sans réponse, le panneau est caché quand même.
pub fn toggle(app: &AppHandle) {
    if !visible() {
        show(app);
        return;
    }
    emit(app, "panel://close-request", serde_json::Value::Null);
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(700));
        if visible() {
            hide(&app);
        }
    });
}

/// Ouvre le panneau sur tout l'écran principal. La fenêtre se crée par la boucle d'événements
/// depuis un autre fil (jamais dans une commande : blocage de WebView2).
fn show(app: &AppHandle) {
    remember_foreground();
    VISIBLE.store(true, Ordering::Relaxed);
    let app = app.clone();
    std::thread::spawn(move || {
        // l'image du jeu est figée avant que le panneau ne la couvre (capture d'écran à garder)
        crate::capture::freeze(&app);
        if !visible() {
            return;
        }
        let main = app.clone();
        let _ = main.run_on_main_thread(move || {
            let w = match app.get_webview_window(PANEL) {
                Some(w) => w,
                None => {
                    let url = protocol::page(&app, "ui/panel.html", &format!("&side=right&width={WIDTH}"));
                    let Ok(url) = url.parse() else { return };
                    let home = protocol::url("");
                    match tauri::WebviewWindowBuilder::new(&app, PANEL, tauri::WebviewUrl::CustomProtocol(url))
                        .title("iClips")
                        .decorations(false)
                        .transparent(true)
                        .shadow(false)
                        .resizable(false)
                        .always_on_top(true)
                        .skip_taskbar(true)
                        .visible(false)
                        .on_navigation(move |u| u.as_str().starts_with(&home))
                        .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
                        .build()
                    {
                        Ok(w) => {
                            // ni dans les clips, ni dans les captures
                            crate::win::exclude_from_capture(&w);
                            w
                        }
                        Err(e) => {
                            eprintln!("iClips : panneau indisponible : {e}");
                            VISIBLE.store(false, Ordering::Relaxed);
                            return;
                        }
                    }
                }
            };
            if let Ok(Some(m)) = w.primary_monitor() {
                let _ = w.set_position(*m.position());
                let _ = w.set_size(*m.size());
            }
            let _ = w.show();
            let _ = w.set_always_on_top(true);
            let _ = w.set_focus();
            #[cfg(windows)]
            if let Ok(h) = w.hwnd() {
                crate::win::force_foreground(h.0 as isize);
            }
            emit(&app, "panel://open", serde_json::Value::Null);
        });
    });
}

/// Cache le panneau et rend le focus au jeu.
pub fn hide(app: &AppHandle) {
    VISIBLE.store(false, Ordering::Relaxed);
    let app = app.clone();
    std::thread::spawn(move || {
        let main = app.clone();
        let _ = main.run_on_main_thread(move || {
            if let Some(w) = app.get_webview_window(PANEL) {
                let _ = w.hide();
            }
            restore_foreground();
        });
    });
}

#[cfg(windows)]
fn remember_foreground() {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    let fg = unsafe { GetForegroundWindow() } as isize;
    if fg != 0 && !visible() {
        PREVIOUS.store(fg, Ordering::Relaxed);
    }
}
#[cfg(not(windows))]
fn remember_foreground() {}

#[cfg(windows)]
fn restore_foreground() {
    use windows_sys::Win32::UI::WindowsAndMessaging::IsWindow;
    let prev = PREVIOUS.swap(0, Ordering::Relaxed);
    if prev != 0 && unsafe { IsWindow(prev as _) } != 0 {
        crate::win::force_foreground(prev);
    }
}
#[cfg(not(windows))]
fn restore_foreground() {}

/// Manette : Vue + X maintenus ouvrent ou ferment ; panneau ouvert, les boutons vont à la page.
/// Le même fil garde un clip quand Vue + A sont maintenus (clips automatiques, `clips_auto.rs`).
#[cfg(windows)]
pub fn start(app: AppHandle) {
    use crate::win::pad;
    std::thread::spawn(move || {
        let mut before = 0u16;
        let mut combo_since: Option<Instant> = None;
        let mut fired = false;
        let mut repeat_at: Option<Instant> = None;
        let mut clip_since: Option<Instant> = None;
        let mut clip_fired = false;
        loop {
            let Some(now) = crate::win::xinput_buttons() else {
                before = 0;
                std::thread::sleep(Duration::from_millis(1000));
                continue;
            };
            let mask = pad::COMBO;
            if now & mask == mask {
                let since = *combo_since.get_or_insert_with(Instant::now);
                if !fired && since.elapsed() >= COMBO_HOLD {
                    fired = true;
                    toggle(&app);
                }
            } else {
                combo_since = None;
                fired = false;
            }
            // clip automatique : Vue + A maintenus en partie, panneau fermé
            let auto = crate::clips_auto::PAD_COMBO;
            if crate::clips_auto::pad_armed() && !visible() && now & auto == auto {
                let since = *clip_since.get_or_insert_with(Instant::now);
                if !clip_fired && since.elapsed() >= COMBO_HOLD {
                    clip_fired = true;
                    crate::clips_auto::trigger(&app, "pad");
                }
            } else {
                clip_since = None;
                clip_fired = false;
            }
            if visible() {
                for action in pad::pressed(now, before) {
                    emit(&app, "pad", action.into());
                }
                if now & pad::REPEAT != 0 && now & pad::REPEAT == before & pad::REPEAT {
                    let at = *repeat_at.get_or_insert_with(|| Instant::now() + Duration::from_millis(400));
                    if Instant::now() >= at {
                        for action in pad::repeated(now) {
                            emit(&app, "pad", action.into());
                        }
                        repeat_at = Some(Instant::now() + Duration::from_millis(120));
                    }
                } else {
                    repeat_at = None;
                }
            }
            before = now;
            std::thread::sleep(Duration::from_millis(16));
        }
    });
}
#[cfg(not(windows))]
pub fn start(_app: AppHandle) {}
