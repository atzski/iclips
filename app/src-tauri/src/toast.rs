//! Notifications d'iClips en bas à droite de l'écran, reprises d'iBox (`toast.rs`) : clip gardé
//! (« Clip ça ! », Vue + A, fin de partie) avec le son « clip », et témoin du micro quand
//! « Clip ça ! » se met à écouter. Une petite fenêtre transparente au premier plan, qui ne prend
//! pas le focus et n'apparaît ni dans les clips ni dans les captures. Les notifications de
//! Windows sont tues pendant les jeux en plein écran (Assistant de concentration) : d'où une
//! fenêtre à nous, comme dans iBox.
//!
//! La page (`ui/toast.html`) lit la file par `toast.pull` et appelle `toast.done` quand tout est
//! montré ; un clic sur un clip ouvre le Studio (`toast.open`).

use serde_json::Value;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

use crate::protocol::{self, TOAST};

static QUEUE: Mutex<VecDeque<Value>> = Mutex::new(VecDeque::new());
/// Une veille garde la fenêtre au-dessus des jeux tant qu'elle est affichée.
static KEEPER: AtomicBool = AtomicBool::new(false);
/// Taille de la fenêtre (en points) : une carte et toute son ombre (rien de rogné).
const WIDTH: f64 = 430.0;
const HEIGHT: f64 = 160.0;

fn window(app: &AppHandle) -> Option<tauri::WebviewWindow> {
    if let Some(w) = app.get_webview_window(TOAST) {
        return Some(w);
    }
    let url = protocol::page(app, "ui/toast.html", "").parse().ok()?;
    let home = protocol::url("");
    WebviewWindowBuilder::new(app, TOAST, WebviewUrl::CustomProtocol(url))
        .title("iClips")
        .inner_size(WIDTH, HEIGHT)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focusable(false)
        .focused(false)
        .visible(false)
        .on_navigation(move |u| u.as_str().starts_with(&home))
        .build()
        // ni dans les clips, ni dans les captures d'écran, ni dans l'écran partagé
        .inspect(crate::win::exclude_from_capture)
        .map_err(|e| eprintln!("iClips : fenêtre de notification indisponible : {e}"))
        .ok()
}

/// Coin bas-droit de la zone de travail de l'écran principal (au-dessus de la barre des tâches).
fn place(w: &tauri::WebviewWindow) {
    let Ok(Some(monitor)) = w.primary_monitor() else { return };
    let area = monitor.work_area();
    let scale = monitor.scale_factor();
    let (ww, wh) = ((WIDTH * scale) as i32, (HEIGHT * scale) as i32);
    let margin = (8.0 * scale) as i32;
    let x = area.position.x + area.size.width as i32 - ww - margin;
    let y = area.position.y + area.size.height as i32 - wh - margin;
    let _ = w.set_position(PhysicalPosition::new(x, y));
}

/// Repasse la fenêtre au sommet des fenêtres « toujours au premier plan » : un jeu sans bordure
/// l'est aussi, et passerait devant. La fenêtre ne prend jamais le focus.
fn raise(w: &tauri::WebviewWindow) {
    let _ = w.set_always_on_top(false);
    let _ = w.set_always_on_top(true);
}

fn keep_on_top(app: &AppHandle) {
    if KEEPER.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(800));
            let Some(w) = app.get_webview_window(TOAST) else { break };
            if !w.is_visible().unwrap_or(false) {
                break;
            }
            let target = w.clone();
            let _ = app.run_on_main_thread(move || raise(&target));
        }
        KEEPER.store(false, Ordering::SeqCst);
    });
}

/// Ajoute une notification (`{kind: "clip" | "notice", name, body, …}`, format d'iBox).
pub fn push(app: &AppHandle, payload: Value) {
    if let Ok(mut q) = QUEUE.lock() {
        if q.len() >= 6 {
            q.pop_front();
        }
        q.push_back(payload);
    }
    let app = app.clone();
    // créée par la boucle d'événements depuis un autre fil, jamais dans une commande
    std::thread::spawn(move || {
        let main = app.clone();
        let _ = main.run_on_main_thread(move || {
            let Some(w) = window(&app) else { return };
            place(&w);
            let _ = w.show();
            raise(&w);
            crate::host::emit_to(&app, TOAST, "toast://push", Value::Null);
            keep_on_top(&app);
        });
    });
}

/// `toast.pull` : notifications en attente.
pub fn pull() -> Value {
    Value::Array(QUEUE.lock().map(|mut q| q.drain(..).collect()).unwrap_or_default())
}

/// `toast.done` : tout est montré, la fenêtre se cache.
pub fn done(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let main = app.clone();
        let _ = main.run_on_main_thread(move || {
            if QUEUE.lock().map(|q| q.is_empty()).unwrap_or(true) {
                if let Some(w) = app.get_webview_window(TOAST) {
                    let _ = w.hide();
                }
            }
        });
    });
}
