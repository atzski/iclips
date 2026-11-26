//! Protocole `iboxplugin` repris d'iBox (`plugins/protocol.rs`) : l'interface d'iClips (`ui/`)
//! parle au natif comme dans iBox, par `POST /iclips/__host/<appel>` sur
//! `http://iboxplugin.localhost/iclips/…`, et lit `__clips/`, `__thumbs/`, `__shot`. Le même
//! paquet d'interface sert donc le plugin et l'app autonome, sans une ligne de Tauri.
//!
//! Ici, l'interface n'est pas un plugin installé : elle est compilée dans l'exécutable (Vite,
//! `dist/ui`, `frontendDist` de Tauri) et servie sous `/iclips/ui/`. Elle n'est servie qu'aux
//! fenêtres de l'app ; aucune commande Tauri n'est déclarée.

use tauri::http::{header, Request, Response, StatusCode};
use tauri::AppHandle;

pub const SCHEME: &str = "iboxplugin";
/// Identifiant du plugin dans les adresses (`/iclips/…`), comme dans iBox.
pub const ID: &str = "iclips";

/// Fenêtres de l'app : le Studio, le panneau en jeu, la notification.
pub const STUDIO: &str = "studio";
pub const PANEL: &str = "panel";
pub const TOAST: &str = "toast";

pub fn owns(label: &str) -> bool {
    matches!(label, STUDIO | PANEL | TOAST)
}

/// Adresse d'une page de l'interface (`ui/studio.html`…) pour une fenêtre.
pub fn url(path: &str) -> String {
    if cfg!(windows) || cfg!(target_os = "android") {
        format!("http://{SCHEME}.localhost/{ID}/{path}")
    } else {
        format!("{SCHEME}://localhost/{ID}/{path}")
    }
}

/// Adresse d'une page avec la langue, le thème et la marque de l'app autonome (`app=1`).
pub fn page(app: &AppHandle, entry: &str, extra: &str) -> String {
    let (lang, dark) = crate::prefs::look(app);
    format!("{}?lang={lang}&scheme={}&app=1{extra}", url(entry), if dark { "dark" } else { "light" })
}

/// CSP des pages : leurs propres fichiers seulement (comme une fenêtre de plugin dans iBox).
pub const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; media-src 'self' data: blob:; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'; object-src 'none'";

pub fn safe_path(p: &str) -> bool {
    !p.is_empty() && p.len() <= 200 && !p.starts_with('/') && !p.contains('\\') && !p.contains(':') && p.split('/').all(|s| !s.is_empty() && s != "." && s != "..")
}

/// Découpe `/iclips/<chemin>` ; rien en dehors d'un chemin relatif sûr.
pub fn split(path: &str) -> Option<&str> {
    let file = path.strip_prefix('/')?.strip_prefix(ID)?.strip_prefix('/')?;
    (file.starts_with("__host/") || safe_path(file)).then_some(file)
}

fn respond(status: StatusCode, mime: &str, body: Vec<u8>, csp: Option<&str>) -> Response<Vec<u8>> {
    let mut b = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, mime)
        .header("X-Content-Type-Options", "nosniff")
        .header(header::CACHE_CONTROL, "no-store");
    if let Some(c) = csp {
        b = b.header("Content-Security-Policy", c);
    }
    b.body(body).unwrap_or_else(|_| Response::new(Vec::new()))
}

fn text(status: StatusCode, msg: &str) -> Response<Vec<u8>> {
    respond(status, "text/plain; charset=utf-8", msg.as_bytes().to_vec(), None)
}

/// Traite une requête (bloquant : appelée hors de la boucle d'événements).
pub fn handle(app: &AppHandle, webview_label: &str, req: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let path = percent_decode(req.uri().path());
    let Some(file) = split(&path) else { return text(StatusCode::BAD_REQUEST, "Adresse invalide.") };
    if !owns(webview_label) {
        return text(StatusCode::FORBIDDEN, "Réservé aux fenêtres d'iClips.");
    }
    if let Some(call) = file.strip_prefix("__host/") {
        if req.method() != tauri::http::Method::POST {
            return text(StatusCode::METHOD_NOT_ALLOWED, "POST attendu.");
        }
        let arg: serde_json::Value = serde_json::from_slice(req.body()).unwrap_or(serde_json::Value::Null);
        let out = match crate::host::call(app, webview_label, call, arg) {
            Ok(v) => serde_json::json!({ "ok": v }),
            Err(e) => serde_json::json!({ "error": e }),
        };
        return respond(StatusCode::OK, "application/json", out.to_string().into_bytes(), None);
    }
    if let Some(name) = file.strip_prefix("__clips/") {
        // piste séparée d'un clip (`a.game.m4a`, `a.mic.m4a`) : le lecteur fait entendre les volumes
        let path = match track_of(name) {
            Some((clip, track)) => crate::capture::clip_path(app, &clip).map(|p| crate::clips::track_file(&p, track)).filter(|p| p.is_file()),
            None => crate::capture::clip_path(app, name),
        };
        let Some(path) = path else { return text(StatusCode::NOT_FOUND, "Clip introuvable.") };
        let range = req.headers().get(header::RANGE).and_then(|v| v.to_str().ok());
        return clip(&path, range);
    }
    // image figée à l'ouverture du panneau
    if file == "__shot" {
        return match crate::capture::frozen_png() {
            Some(png) => respond(StatusCode::OK, "image/png", png, None),
            None => text(StatusCode::NOT_FOUND, "Aucune image figée."),
        };
    }
    if let Some(name) = file.strip_prefix("__thumbs/") {
        return match crate::capture::clip_thumb_path(app, name).and_then(|p| std::fs::read(p).ok()) {
            Some(bytes) => respond(StatusCode::OK, "image/jpeg", bytes, None),
            None => text(StatusCode::NOT_FOUND, "Vignette introuvable."),
        };
    }
    // interface compilée dans l'exécutable (dist/ui)
    let Some(asset) = file.strip_prefix("ui/").and_then(|f| app.asset_resolver().get(f.to_string())) else {
        return text(StatusCode::NOT_FOUND, "Fichier introuvable.");
    };
    let html = asset.mime_type.starts_with("text/html");
    respond(StatusCode::OK, &asset.mime_type, asset.bytes, html.then_some(CSP))
}

/// Plus grand morceau servi par réponse : la balise `<video>` redemande la suite.
const CHUNK: u64 = 4 * 1024 * 1024;

/// `Range: bytes=a-b`, `bytes=a-`, `bytes=-n` → (début, fin incluse), bornés au fichier et à
/// `CHUNK`. `None` : demande hors du fichier.
pub fn byte_range(header: Option<&str>, len: u64) -> Option<(u64, u64)> {
    if len == 0 {
        return None;
    }
    let (start, end) = match header.and_then(|h| h.trim().strip_prefix("bytes=")) {
        None => (0, len - 1),
        Some(spec) => {
            let (a, b) = spec.split(',').next()?.trim().split_once('-')?;
            match (a.trim(), b.trim()) {
                ("", n) => {
                    let n: u64 = n.parse().ok()?;
                    (len.saturating_sub(n), len - 1)
                }
                (a, "") => (a.parse().ok()?, len - 1),
                (a, b) => (a.parse().ok()?, b.parse::<u64>().ok()?.min(len - 1)),
            }
        }
    };
    (start <= end && start < len).then(|| (start, end.min(start + CHUNK - 1)))
}

/// `a.game.m4a` → (`a.mp4`, `game`) ; seulement les pistes du jeu et du micro.
pub fn track_of(name: &str) -> Option<(String, &'static str)> {
    ["game", "mic"].into_iter().find_map(|t| name.strip_suffix(&format!(".{t}.m4a")).map(|stem| (format!("{stem}.mp4"), t)))
}

/// Sert un clip par morceaux (lecture et déplacement dans la vidéo).
fn clip(path: &std::path::Path, range: Option<&str>) -> Response<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(path) else { return text(StatusCode::NOT_FOUND, "Clip introuvable.") };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let Some((start, end)) = byte_range(range, len) else {
        return Response::builder().status(StatusCode::RANGE_NOT_SATISFIABLE).header(header::CONTENT_RANGE, format!("bytes */{len}")).body(Vec::new()).unwrap_or_else(|_| Response::new(Vec::new()));
    };
    let mut body = vec![0u8; (end - start + 1) as usize];
    if f.seek(SeekFrom::Start(start)).and_then(|_| f.read_exact(&mut body)).is_err() {
        return text(StatusCode::INTERNAL_SERVER_ERROR, "Lecture impossible.");
    }
    Response::builder()
        .status(StatusCode::PARTIAL_CONTENT)
        .header(header::CONTENT_TYPE, match path.extension().and_then(|e| e.to_str()) { Some("gif") => "image/gif", Some("png") => "image/png", Some("m4a") => "audio/mp4", _ => "video/mp4" })
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"))
        .header("X-Content-Type-Options", "nosniff")
        .header(header::CACHE_CONTROL, "no-store")
        .body(body)
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() && b[i + 1].is_ascii_hexdigit() && b[i + 2].is_ascii_hexdigit() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pistes_du_jeu_et_du_micro() {
        assert_eq!(track_of("a-1.game.m4a"), Some(("a-1.mp4".into(), "game")));
        assert_eq!(track_of("a-1.mic.m4a"), Some(("a-1.mp4".into(), "mic")));
        assert_eq!(track_of("a-1.mix.m4a"), None);
        assert_eq!(track_of("a-1.mp4"), None);
        // le nom du clip passe ensuite par `clip_path` (`valid_name` : pas de « .. » ni de « / »)
        assert_eq!(track_of("../x.game.m4a").map(|(c, _)| crate::capture::valid_name(&c)), Some(false));
    }

    #[test]
    fn paths_stay_inside_the_ui() {
        assert_eq!(split("/iclips/ui/studio.html"), Some("ui/studio.html"));
        assert_eq!(split("/iclips/__host/clips.list"), Some("__host/clips.list"));
        assert_eq!(split("/iclips/../other/x"), None);
        assert_eq!(split("/iclips/ui/../../x"), None);
        assert_eq!(split("/other/ui/x"), None);
        assert_eq!(split("/iclips"), None);
        assert_eq!(split(&percent_decode("/iclips/%2e%2e/x")), None);
        assert_eq!(percent_decode("/a%20b"), "/a b");
    }

    #[test]
    fn only_the_app_windows_are_served() {
        assert!(owns("studio") && owns("panel") && owns("toast"));
        assert!(!owns("main") && !owns("plugin-iclips"));
    }

    #[test]
    fn byte_ranges_stay_inside_the_file() {
        let mb = 1024 * 1024;
        assert_eq!(byte_range(None, 100), Some((0, 99)));
        assert_eq!(byte_range(Some("bytes=0-"), 100), Some((0, 99)));
        assert_eq!(byte_range(Some("bytes=10-19"), 100), Some((10, 19)));
        assert_eq!(byte_range(Some("bytes=90-500"), 100), Some((90, 99)));
        assert_eq!(byte_range(Some("bytes=-10"), 100), Some((90, 99)));
        assert_eq!(byte_range(Some("bytes=100-"), 100), None);
        assert_eq!(byte_range(Some("bytes=20-10"), 100), None);
        assert_eq!(byte_range(Some("bytes=x-"), 100), None);
        assert_eq!(byte_range(None, 0), None);
        // morceau de 4 Mo au plus
        assert_eq!(byte_range(Some("bytes=0-"), 50 * mb), Some((0, 4 * mb - 1)));
    }
}
