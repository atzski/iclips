//! Partage des clips (iClips 0.5, permission `clips-share`), sans stockage chez iNetbox :
//!
//! - glisser le clip hors du Studio (vers Discord, un dossier, un navigateur) : glisser-déposer
//!   de l'Explorateur (`SHDoDragDrop` sur l'objet de données du fichier) ;
//! - copier le clip (à coller dans Discord, WhatsApp, un dossier) : même objet de données ;
//! - enregistrer une copie ailleurs, ouvrir le dossier ;
//! - lien temporaire : le clip est déposé chez Litterbox (litterbox.catbox.moe, gratuit, sans
//!   compte, 72 h, adresse impossible à deviner). L'adresse est fixée ici, jamais par le plugin ;
//!   le lien est gardé à côté du clip et resservi tant qu'il vaut encore plus d'une heure ;
//! - (dans iBox seulement : envoyer le lien à un ami iNetbox.)

use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

const LITTERBOX: &str = "https://litterbox.catbox.moe/resources/internals/api.php";
const LINK_PREFIX: &str = "https://litter.catbox.moe/";
/// Durée de vie d'un lien chez Litterbox.
const LINK_HOURS: u64 = 72;
/// Litterbox refuse au-delà de 1 Go.
const MAX_UPLOAD: u64 = 1_000_000_000;

static NEXT: AtomicU32 = AtomicU32::new(1);
/// Un envoi à la fois (la connexion de l'utilisateur ne se partage pas bien).
static UPLOADING: Mutex<()> = Mutex::new(());

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

fn path(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    crate::capture::clip_path(app, name).ok_or_else(|| "Clip introuvable.".into())
}

/// Lien encore bon (plus d'une heure devant lui) gardé à côté du clip.
pub fn kept_link(meta: &Value, at: u64) -> Option<(String, u64)> {
    let link = meta["link"].as_str().filter(|l| l.starts_with(LINK_PREFIX))?;
    let until = meta["linkUntil"].as_u64()?;
    (until > at + 3_600_000).then(|| (link.to_string(), until))
}

/// Corps multipart de Litterbox : champs, puis le fichier, lus à la suite.
pub fn multipart(boundary: &str, file_name: &str) -> (Vec<u8>, Vec<u8>) {
    let mut head = String::new();
    for (k, v) in [("reqtype", "fileupload"), ("time", "72h")] {
        head.push_str(&format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n"));
    }
    let safe: String = file_name.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')).collect();
    let mime = if safe.ends_with(".gif") { "image/gif" } else { "video/mp4" };
    head.push_str(&format!("--{boundary}\r\nContent-Disposition: form-data; name=\"fileToUpload\"; filename=\"{safe}\"\r\nContent-Type: {mime}\r\n\r\n"));
    (head.into_bytes(), format!("\r\n--{boundary}--\r\n").into_bytes())
}

/// Lecteur qui compte ce qui part (progression de l'envoi).
struct Counted<R> {
    inner: R,
    sent: u64,
    total: u64,
    tick: Box<dyn FnMut(f32) + Send>,
}

impl<R: Read> Read for Counted<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.sent += n as u64;
        (self.tick)((self.sent as f64 / self.total.max(1) as f64) as f32);
        Ok(n)
    }
}

fn upload(file: &Path, name: &str, tick: Box<dyn FnMut(f32) + Send>) -> Result<String, String> {
    let len = std::fs::metadata(file).map_err(|e| e.to_string())?.len();
    if len > MAX_UPLOAD {
        return Err("Ce clip dépasse 1 Go : Litterbox ne le prend pas.".into());
    }
    let boundary = format!("ibox{:x}", now_ms());
    let (head, tail) = multipart(&boundary, name);
    let total = head.len() as u64 + len + tail.len() as u64;
    let body = std::io::Cursor::new(head).chain(std::fs::File::open(file).map_err(|e| e.to_string())?).chain(std::io::Cursor::new(tail));
    let reader = Counted { inner: body, sent: 0, total, tick };
    let client = reqwest::blocking::Client::builder()
        .user_agent("iClips")
        .timeout(Duration::from_secs(15 * 60))
        .build()
        .map_err(|e| e.to_string())?;
    let res = client
        .post(LITTERBOX)
        .header("Content-Type", format!("multipart/form-data; boundary={boundary}"))
        .body(reqwest::blocking::Body::sized(reader, total))
        .send()
        .map_err(|_| "Litterbox ne répond pas. Vérifiez la connexion, puis réessayez.".to_string())?;
    let status = res.status();
    let text = res.text().unwrap_or_default();
    let link = text.trim();
    if !status.is_success() || !link.starts_with(LINK_PREFIX) || link.len() > 120 || link.contains(char::is_whitespace) {
        return Err(format!("Litterbox a refusé l'envoi ({}).", status.as_u16()));
    }
    Ok(link.to_string())
}

/// `share.link` : rend le lien gardé s'il vaut encore, sinon lance l'envoi et rend `{job}` ;
/// `share://progress` `{job, ratio}` puis `share://done` `{job, name, link, until}` ou `{job, name, error}`.
pub fn link(app: &AppHandle, name: &str) -> Result<Value, String> {
    let file = path(app, name)?;
    if let Some((link, until)) = kept_link(&crate::capture::clip_meta(app, name), now_ms()) {
        return Ok(json!({ "link": link, "until": until }));
    }
    let job = NEXT.fetch_add(1, Ordering::Relaxed);
    let (app, name) = (app.clone(), name.to_string());
    std::thread::spawn(move || {
        let Ok(_one) = UPLOADING.lock() else { return };
        let progress_app = app.clone();
        let mut last = 0.0f32;
        let tick = Box::new(move |r: f32| {
            if r - last >= 0.02 || r >= 1.0 {
                last = r;
                crate::host::notify(&progress_app, "share://progress", json!({ "job": job, "ratio": r.clamp(0.0, 1.0) }));
            }
        });
        let payload = match upload(&file, &name, tick) {
            Ok(link) => {
                let until = now_ms() + LINK_HOURS * 3_600_000;
                let _ = crate::capture::clip_set_meta(&app, &name, &[("link", json!(link)), ("linkUntil", json!(until))]);
                json!({ "job": job, "name": name, "link": link, "until": until })
            }
            Err(error) => json!({ "job": job, "name": name, "error": error }),
        };
        crate::host::notify(&app, "share://done", payload);
    });
    Ok(json!({ "job": job }))
}

/// Lien gardé d'un clip, tel quel (copie, envoi à un ami).
fn stored_link(app: &AppHandle, name: &str) -> Result<String, String> {
    kept_link(&crate::capture::clip_meta(app, name), now_ms()).map(|(l, _)| l).ok_or_else(|| "Créez d'abord le lien de ce clip.".into())
}

/// `share.copyLink` : copie le lien du clip.
pub fn copy_link(app: &AppHandle, name: &str) -> Result<Value, String> {
    let link = stored_link(app, name)?;
    on_main(app, move |_| os::copy_text(&link))?;
    Ok(Value::Null)
}

// « Envoyer à un ami » (`share.friends`, `share.send`) demande un compte iNetbox : seulement
// dans iBox. L'interface le cache dans l'app autonome (`standalone`, ui/src/host.ts).

/// Exécute `f` sur le fil de la fenêtre (OLE : glisser, presse-papiers) et attend le résultat.
fn on_main<T: Send + 'static>(app: &AppHandle, f: impl FnOnce(&AppHandle) -> Result<T, String> + Send + 'static) -> Result<T, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let _ = tx.send(f(&handle));
    })
    .map_err(|e| e.to_string())?;
    rx.recv_timeout(Duration::from_secs(120)).map_err(|_| "Partage interrompu.".to_string())?
}

/// `share.copy` : copie le fichier du clip (coller dans Discord, un dossier…).
pub fn copy(app: &AppHandle, name: &str) -> Result<Value, String> {
    let file = path(app, name)?;
    on_main(app, move |_| os::copy_file(&file))?;
    Ok(Value::Null)
}

/// `share.drag` : glisser le clip hors du Studio. Appelé au début du glisser (bouton tenu) ;
/// la boucle de glisser-déposer tourne sur le fil de la fenêtre jusqu'au lâcher.
pub fn drag(app: &AppHandle, label: &str, name: &str) -> Result<Value, String> {
    let file = path(app, name)?;
    let label = label.to_string();
    let app2 = app.clone();
    // pas d'attente : le glisser se termine au lâcher, l'appel répond tout de suite
    app.run_on_main_thread(move || {
        if let Some(w) = app2.get_webview_window(&label) {
            if let Err(e) = os::drag_file(&w, &file) {
                eprintln!("iClips : glisser le clip impossible : {e}");
            }
        }
    })
    .map_err(|e| e.to_string())?;
    Ok(Value::Null)
}

/// `share.reveal` : ouvre le dossier des clips, le clip sélectionné.
pub fn reveal(app: &AppHandle, name: &str) -> Result<Value, String> {
    let file = path(app, name)?;
    os::reveal(&file)?;
    Ok(Value::Null)
}

/// `share.save` : enregistre une copie où l'on veut. `{path}` ou `null` si l'on annule.
pub fn save(app: &AppHandle, name: &str) -> Result<Value, String> {
    use tauri_plugin_dialog::DialogExt;
    let file = path(app, name)?;
    let meta = crate::capture::clip_meta(app, name);
    let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("mp4").to_string();
    let base: String = meta["title"].as_str().or(meta["game"].as_str()).unwrap_or("Clip").chars().filter(|c| !"\\/:*?\"<>|".contains(*c)).take(80).collect();
    let mut dialog = app.dialog().file().set_file_name(format!("{}.{ext}", base.trim())).add_filter(if ext == "gif" { "GIF" } else { "Vidéo MP4" }, &[ext.as_str()]);
    if let Ok(dir) = app.path().video_dir() {
        dialog = dialog.set_directory(dir);
    }
    let Some(target) = dialog.blocking_save_file().and_then(|p| p.into_path().ok()) else { return Ok(Value::Null) };
    std::fs::copy(&file, &target).map_err(|e| e.to_string())?;
    Ok(json!({ "path": target.to_string_lossy() }))
}

#[cfg(windows)]
mod os {
    use std::path::Path;
    use windows::core::HSTRING;
    use windows::Win32::Foundation::{HANDLE, HWND};
    use windows::Win32::System::Com::IDataObject;
    use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::System::Ole::{IDropSource, OleFlushClipboard, OleSetClipboard, CF_UNICODETEXT, DROPEFFECT_COPY};
    use windows::Win32::UI::Shell::{IShellItem, SHCreateItemFromParsingName, SHDoDragDrop, BHID_DataObject};

    /// Objet de données du fichier, comme dans l'Explorateur (CF_HDROP, nom, contenu…).
    unsafe fn data_object(file: &Path) -> windows::core::Result<IDataObject> {
        let item: IShellItem = SHCreateItemFromParsingName(&HSTRING::from(file.as_os_str()), None)?;
        item.BindToHandler(None, &BHID_DataObject)
    }

    pub fn copy_file(file: &Path) -> Result<(), String> {
        unsafe {
            let data = data_object(file).map_err(|e| e.message())?;
            OleSetClipboard(&data).map_err(|e| e.message())?;
            // le presse-papiers garde le fichier même si iBox se ferme
            OleFlushClipboard().map_err(|e| e.message())
        }
    }

    pub fn drag_file(w: &tauri::WebviewWindow, file: &Path) -> Result<(), String> {
        let hwnd = w.hwnd().map_err(|e| e.to_string())?;
        unsafe {
            let data = data_object(file).map_err(|e| e.message())?;
            SHDoDragDrop(Some(HWND(hwnd.0)), &data, None::<&IDropSource>, DROPEFFECT_COPY).map(drop).map_err(|e| e.message())
        }
    }

    pub fn copy_text(text: &str) -> Result<(), String> {
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            OpenClipboard(None).map_err(|e| e.message())?;
            let done = (|| -> windows::core::Result<()> {
                EmptyClipboard()?;
                let mem = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2)?;
                let ptr = GlobalLock(mem) as *mut u16;
                if ptr.is_null() {
                    return Err(windows::core::Error::from_thread());
                }
                std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
                let _ = GlobalUnlock(mem);
                SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(mem.0)))?;
                Ok(())
            })();
            let _ = CloseClipboard();
            done.map_err(|e| e.message())
        }
    }

    pub fn reveal(file: &Path) -> Result<(), String> {
        std::process::Command::new("explorer.exe")
            .arg(format!("/select,{}", file.display()))
            .spawn()
            .map(drop)
            .map_err(|e| e.to_string())
    }
}

#[cfg(not(windows))]
mod os {
    use std::path::Path;
    const NO: &str = "Partage disponible sur Windows seulement.";
    pub fn copy_file(_: &Path) -> Result<(), String> { Err(NO.into()) }
    pub fn drag_file(_: &tauri::WebviewWindow, _: &Path) -> Result<(), String> { Err(NO.into()) }
    pub fn copy_text(_: &str) -> Result<(), String> { Err(NO.into()) }
    pub fn reveal(_: &Path) -> Result<(), String> { Err(NO.into()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_lien_garde_vaut_s_il_lui_reste_une_heure() {
        let meta = json!({ "link": "https://litter.catbox.moe/abc123.mp4", "linkUntil": 10_000_000u64 });
        assert_eq!(kept_link(&meta, 0).map(|l| l.0).as_deref(), Some("https://litter.catbox.moe/abc123.mp4"));
        assert!(kept_link(&meta, 10_000_000 - 3_600_000).is_none());
        // seule l'adresse de Litterbox est resservie
        assert!(kept_link(&json!({ "link": "https://exemple.fr/x", "linkUntil": u64::MAX }), 0).is_none());
        assert!(kept_link(&json!({}), 0).is_none());
    }

    #[test]
    fn le_corps_multipart_suit_litterbox() {
        let (head, tail) = multipart("B", "17-mon jeu/../x.mp4");
        let head = String::from_utf8(head).unwrap();
        assert!(head.contains("name=\"reqtype\"\r\n\r\nfileupload\r\n"));
        assert!(head.contains("name=\"time\"\r\n\r\n72h\r\n"));
        assert!(head.contains("filename=\"17-monjeu..x.mp4\"\r\nContent-Type: video/mp4\r\n\r\n"));
        assert_eq!(tail, b"\r\n--B--\r\n");
    }

    /// Envoi réel chez Litterbox (`IBOX_SHARE_SAMPLE`) : le lien répond.
    #[test]
    #[ignore]
    fn lien_reel() {
        let file = std::env::var("IBOX_SHARE_SAMPLE").expect("IBOX_SHARE_SAMPLE");
        let link = upload(Path::new(&file), "essai.mp4", Box::new(|_| {})).expect("envoi");
        println!("lien : {link}");
        assert!(link.starts_with(LINK_PREFIX));
    }
}
