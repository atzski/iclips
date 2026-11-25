//! Captures et clips de l'app autonome, repris d'iBox (`capture.rs`) : pendant une partie
//! (`game.rs`), la mémoire des dernières secondes est tenue (`clips.rs`) ; le panneau en jeu fige
//! l'image du jeu à son ouverture (capture d'écran à garder) et garde le clip. Les fichiers vont
//! dans le dossier des clips (`Vidéos\iClips` par défaut, réglable) : `<horodatage>-<jeu>.mp4`
//! avec un json à côté (jeu, titre, favori, durée, lien…), une vignette `.jpg` et, en pistes
//! séparées, `.game.m4a` et `.mic.m4a`. Rien ne part en ligne sans un partage explicite.

use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

/// Image du jeu prise à l'ouverture du panneau (PNG), en attente d'être gardée.
static PENDING: Mutex<Option<Shot>> = Mutex::new(None);
/// Jeu en cours (titre, plateforme) tel que vu par la dernière vérification.
static GAME: Mutex<Option<(String, String)>> = Mutex::new(None);
/// Réglage « Mémoire des clips » : les dernières secondes sont tenues pendant les parties.
static CLIPS: AtomicBool = AtomicBool::new(true);

struct Shot { png: Vec<u8>, game: Option<(String, String)>, at: u64 }

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Capture { pub name: String, pub kind: String, pub game: Option<String>, pub platform_id: Option<String>, pub at: u64, pub size: u64 }

fn now() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64 }

/// Dossier des clips par défaut : `Vidéos\iClips`.
pub fn default_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().video_dir().ok().map(|d| d.join("iClips"))
}

/// Dossier des clips : celui choisi dans les Réglages, sinon `Vidéos\iClips`.
pub fn folder(app: &AppHandle) -> Result<PathBuf, String> {
    crate::prefs::load(app).folder.map(PathBuf::from).filter(|p| p.is_absolute()).or_else(|| default_dir(app))
        .or_else(|| app.path().app_data_dir().ok().map(|d| d.join("clips")))
        .ok_or_else(|| "Dossier des clips introuvable.".into())
}

fn dir(app: &AppHandle) -> Result<PathBuf, String> {
    let d = folder(app)?;
    fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    Ok(d)
}

/// Nom de fichier sûr : horodatage + titre du jeu réduit à des caractères simples.
pub fn file_name(at: u64, game: Option<&str>, ext: &str) -> String {
    let slug: String = game.unwrap_or("iclips").chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>().split('-').filter(|s| !s.is_empty()).take(6).collect::<Vec<_>>().join("-");
    let slug = if slug.is_empty() { "iclips".into() } else { slug.chars().take(40).collect::<String>() };
    format!("{at}-{slug}.{ext}")
}
/// Nom de capture valide (rien d'autre n'est lu ni supprimé).
pub fn valid_name(name: &str) -> bool {
    name.len() <= 80 && !name.starts_with('.') && (name.ends_with(".png") || name.ends_with(".gif") || name.ends_with(".mp4"))
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
        && name.matches('.').count() == 1
}

fn sidecar(path: &Path) -> PathBuf { path.with_extension("json") }

fn describe(path: &Path) -> Option<Capture> {
    let name = path.file_name()?.to_str()?.to_string();
    if !valid_name(&name) { return None; }
    let meta: Value = fs::read(sidecar(path)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
    let at = name.split('-').next().and_then(|n| n.parse().ok()).unwrap_or(0);
    Some(Capture {
        kind: if name.ends_with(".gif") || name.ends_with(".mp4") { "clip".into() } else { "screenshot".into() },
        game: meta["game"].as_str().map(String::from),
        platform_id: meta["platformId"].as_str().map(String::from),
        size: fs::metadata(path).map(|m| m.len()).unwrap_or(0),
        at, name,
    })
}

fn store(app: &AppHandle, bytes: &[u8], ext: &str, game: Option<(String, String)>, at: u64) -> Result<Capture, String> {
    let d = dir(app)?;
    let path = d.join(file_name(at, game.as_ref().map(|g| g.0.as_str()), ext));
    fs::write(&path, bytes).map_err(|e| e.to_string())?;
    if let Some((title, platform)) = &game {
        let _ = fs::write(sidecar(&path), serde_json::to_vec(&json!({ "game": title, "platformId": platform })).unwrap_or_default());
    }
    describe(&path).ok_or_else(|| "Capture illisible.".into())
}

// ——— Capture de l'écran (Windows GDI) ———

/// Image de l'écran principal en RGB, réduite à `max_w` de large au besoin.
#[cfg(windows)]
fn grab(max_w: Option<u32>) -> Option<(u32, u32, Vec<u8>)> {
    use std::ptr::null_mut;
    use windows_sys::Win32::Graphics::Gdi::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    unsafe {
        let (w, h) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        if w <= 0 || h <= 0 { return None; }
        let (tw, th) = match max_w {
            Some(m) if (w as u32) > m => (m as i32, ((h as i64 * m as i64) / w as i64).max(1) as i32),
            _ => (w, h),
        };
        let screen = GetDC(null_mut());
        if screen.is_null() { return None; }
        let mem = CreateCompatibleDC(screen);
        let bmp = CreateCompatibleBitmap(screen, tw, th);
        let old = SelectObject(mem, bmp);
        let ok = if (tw, th) == (w, h) {
            BitBlt(mem, 0, 0, w, h, screen, 0, 0, SRCCOPY | CAPTUREBLT)
        } else {
            SetStretchBltMode(mem, HALFTONE);
            StretchBlt(mem, 0, 0, tw, th, screen, 0, 0, w, h, SRCCOPY)
        };
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = tw;
        info.bmiHeader.biHeight = -th;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        let mut bgra = vec![0u8; (tw * th * 4) as usize];
        let lines = GetDIBits(mem, bmp, 0, th as u32, bgra.as_mut_ptr().cast(), &mut info, DIB_RGB_COLORS);
        SelectObject(mem, old);
        DeleteObject(bmp);
        DeleteDC(mem);
        ReleaseDC(null_mut(), screen);
        if ok == 0 || lines == 0 { return None; }
        let mut rgb = Vec::with_capacity((tw * th * 3) as usize);
        for px in bgra.chunks_exact(4) { rgb.extend_from_slice(&[px[2], px[1], px[0]]); }
        Some((tw as u32, th as u32, rgb))
    }
}
#[cfg(not(windows))]
fn grab(_max_w: Option<u32>) -> Option<(u32, u32, Vec<u8>)> { None }

/// Image de l'écran principal par Windows Graphics Capture (celle des clips) : elle voit aussi
/// les jeux en plein écran exclusif, que GDI rend noirs. `None` si WGC est indisponible.
#[cfg(windows)]
fn grab_wgc() -> Option<(u32, u32, Vec<u8>)> {
    use crate::remote_play::capture::{d3d::Device, wgc::{Capture as Wgc, Next}, windows_build, Options, Target};
    use windows::Win32::Graphics::Direct3D11::{D3D11_CPU_ACCESS_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING};
    let device = Device::create().ok()?;
    let options = Options { cursor: false, borderless: true, require_borderless: false };
    let mut capture = Wgc::start(&device, Target::PrimaryMonitor, options, windows_build()).ok()?;
    let deadline = std::time::Instant::now() + Duration::from_millis(700);
    let frame = loop {
        match capture.next(Duration::from_millis(150)).ok()? {
            Next::Frame(frame) => break frame,
            Next::Timeout if std::time::Instant::now() < deadline => continue,
            _ => return None,
        }
    };
    let (w, h) = frame.size;
    unsafe {
        // copie vers une texture lisible par le processeur, puis BGRA → RGB
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        frame.texture.GetDesc(&mut desc);
        desc.MipLevels = 1;
        desc.ArraySize = 1;
        desc.SampleDesc.Count = 1;
        desc.SampleDesc.Quality = 0;
        desc.Usage = D3D11_USAGE_STAGING;
        desc.BindFlags = 0;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        desc.MiscFlags = 0;
        let mut staging = None;
        device.device.CreateTexture2D(&desc, None, Some(&mut staging)).ok()?;
        let staging = staging?;
        device.context.CopyResource(&staging, &frame.texture);
        let mut map = D3D11_MAPPED_SUBRESOURCE::default();
        device.context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut map)).ok()?;
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h as usize {
            let row = std::slice::from_raw_parts((map.pData as *const u8).add(y * map.RowPitch as usize), w as usize * 4);
            for px in row.chunks_exact(4) { rgb.extend_from_slice(&[px[2], px[1], px[0]]); }
        }
        device.context.Unmap(&staging, 0);
        Some((w, h, rgb))
    }
}
#[cfg(not(windows))]
fn grab_wgc() -> Option<(u32, u32, Vec<u8>)> { None }

/// Capture en pleine taille : WGC d'abord, GDI en repli (Windows anciens, pilote sans WGC).
fn grab_screen() -> Option<(u32, u32, Vec<u8>)> {
    grab_wgc().or_else(|| grab(None))
}

pub fn encode_png(w: u32, h: u32, rgb: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut writer = enc.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(rgb).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// Palette fixe 6×7×6 (252 couleurs) : rapide, sans tramage, assez fidèle pour un clip.
pub fn palette() -> Vec<u8> {
    let mut p = Vec::with_capacity(256 * 3);
    for r in 0..6u32 { for g in 0..7u32 { for b in 0..6u32 { p.extend_from_slice(&[(r * 255 / 5) as u8, (g * 255 / 6) as u8, (b * 255 / 5) as u8]); } } }
    p.resize(256 * 3, 0);
    p
}
pub fn quantize(rgb: &[u8]) -> Vec<u8> {
    rgb.chunks_exact(3).map(|c| {
        let (r, g, b) = ((c[0] as u32 * 5 + 127) / 255, (c[1] as u32 * 6 + 127) / 255, (c[2] as u32 * 5 + 127) / 255);
        (r * 42 + g * 6 + b) as u8
    }).collect()
}
pub fn encode_gif(w: u32, h: u32, frames: &[Vec<u8>], delay_cs: u16) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut enc = gif::Encoder::new(&mut out, w as u16, h as u16, &palette()).map_err(|e| e.to_string())?;
        enc.set_repeat(gif::Repeat::Infinite).map_err(|e| e.to_string())?;
        for f in frames {
            let mut frame = gif::Frame::from_indexed_pixels(w as u16, h as u16, f.clone(), None);
            frame.delay = delay_cs;
            enc.write_frame(&frame).map_err(|e| e.to_string())?;
        }
    }
    Ok(out)
}

// ——— Veille : jeu en cours, mémoire du clip ———

fn game_now(_app: &AppHandle) -> Option<(String, String)> {
    crate::game::now()
}

/// Démarre la veille (réglages gardés d'abord : mémoire, qualité, appareils de son).
pub fn start(app: AppHandle) {
    let p = crate::prefs::load(&app);
    CLIPS.store(p.clips, Ordering::Relaxed);
    crate::clips::configure(p.clip.unwrap_or_else(crate::prefs::default_clip).sanitized());
    crate::clips::set_devices(p.output_device, p.mic_device);
    // vérification du jeu en cours toutes les 2 s
    let watch = app.clone();
    std::thread::spawn(move || loop {
        let game = game_now(&watch);
        let clips = CLIPS.load(Ordering::Relaxed);
        let before = GAME.lock().ok().and_then(|g| g.clone());
        // fin de partie : le clip est gardé avant que la mémoire ne se vide
        if before.is_some() && game.is_none() && clips {
            crate::clips_auto::on_game_end(&watch);
        }
        // clip vidéo tenu pendant une partie (ou partout, si demandé), si les clips sont activés
        let active = clips && (game.is_some() || crate::clips::settings().everywhere);
        crate::clips::set_active(active);
        crate::clips_auto::tick(&watch, active);
        if let Ok(mut g) = GAME.lock() { *g = game; }
        std::thread::sleep(Duration::from_secs(2));
    });
    crate::clips::start();
}

/// Garde les dernières secondes de jeu (panneau, clips automatiques).
pub async fn save_clip(app: AppHandle) -> Result<Capture, String> {
    save_clip_as(app, None).await
}

/// Comme `save_clip` ; `auto` dit ce qui l'a déclenché sans l'utilisateur devant iBox
/// (`voice`, `pad`, `end` : `clips_auto.rs`), gardé à côté du clip.
pub async fn save_clip_as(app: AppHandle, auto: Option<&'static str>) -> Result<Capture, String> {
    let game = PENDING.lock().ok().and_then(|p| p.as_ref().and_then(|s| s.game.clone())).or_else(|| GAME.lock().ok().and_then(|g| g.clone()));
    tauri::async_runtime::spawn_blocking(move || {
        let d = dir(&app)?;
        let at = now();
        let path = d.join(file_name(at, game.as_ref().map(|g| g.0.as_str()), "mp4"));
        // nom de travail hors liste (`valid_name` refuse « _ ») puis renommage
        let work = d.join(format!("_clip-{at}.mp4"));
        let written = crate::clips::save(&work).and_then(|tracks| {
            fs::rename(&work, &path).map_err(|e| e.to_string())?;
            for name in tracks.iter().skip(1) {
                let _ = fs::rename(crate::clips::track_file(&work, name), crate::clips::track_file(&path, name));
            }
            Ok(tracks)
        });
        let tracks = match written {
            Ok(t) => t,
            Err(e) => {
                let _ = fs::remove_file(&work);
                return Err(e);
            }
        };
        let mut meta = json!({});
        if let Some((title, platform)) = &game {
            meta = json!({ "game": title, "platformId": platform });
        }
        if let Some(a) = auto { meta["auto"] = a.into(); }
        // pistes séparées : leur nom, pour le réglage des volumes au montage
        if tracks.len() > 1 { meta["tracks"] = tracks.into(); }
        if meta.as_object().is_some_and(|m| !m.is_empty()) {
            let _ = fs::write(sidecar(&path), serde_json::to_vec(&meta).unwrap_or_default());
        }
        crate::host::notify(&app, "clips://changed", Value::Null);
        describe(&path).ok_or_else(|| "Clip illisible.".to_string())
    }).await.map_err(|e| e.to_string())?
}

// ——— Clips pour les plugins (permission `clips`, plugins/host.rs) ———

/// Chemin d'un clip existant du profil (MP4, ou GIF fait par le montage) ; rien d'autre
/// (ni capture d'écran, ni fichier de travail).
pub fn clip_path(app: &AppHandle, name: &str) -> Option<PathBuf> {
    if !valid_name(name) || !(name.ends_with(".mp4") || name.ends_with(".gif") || name.ends_with(".png")) { return None; }
    let p = dir(app).ok()?.join(name);
    p.is_file().then_some(p)
}

/// Clips vidéo du profil, du plus récent au plus ancien, avec leur titre et leur favori.
pub fn clip_list(app: &AppHandle) -> Result<Vec<Value>, String> {
    let d = dir(app)?;
    let mut list: Vec<(u64, Value)> = fs::read_dir(&d).map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            let c = describe(&path).filter(|c| c.name.ends_with(".mp4") || c.name.ends_with(".gif") || c.name.ends_with(".png"))?;
            let meta: Value = fs::read(sidecar(&path)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
            Some((c.at, json!({
                "name": c.name, "kind": if c.name.ends_with(".gif") { "gif" } else if c.name.ends_with(".png") { "image" } else { "video" },
                "game": c.game, "platformId": c.platform_id, "at": c.at, "size": c.size,
                "title": meta["title"].as_str(), "favorite": meta["favorite"].as_bool().unwrap_or(false),
                "duration": meta["duration"].as_f64(), "auto": meta["auto"].as_str(),
                "link": meta["link"].as_str(), "linkUntil": meta["linkUntil"].as_u64(),
                "tracks": meta["tracks"].as_array().cloned().unwrap_or_default(),
                "src": format!("__clips/{}", c.name),
                "thumb": thumb(&path).is_file().then(|| format!("__thumbs/{}", c.name)),
            })))
        })
        .collect();
    list.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(list.into_iter().map(|(_, v)| v).collect())
}

/// Ajoute ou remplace des champs du json d'un clip (lien de partage…).
pub fn clip_set_meta(app: &AppHandle, name: &str, fields: &[(&str, Value)]) -> Result<(), String> {
    let path = clip_path(app, name).ok_or("Clip introuvable.")?;
    let mut meta: Value = fs::read(sidecar(&path)).ok().and_then(|b| serde_json::from_slice(&b).ok()).filter(Value::is_object).unwrap_or_else(|| json!({}));
    for (k, v) in fields { meta[*k] = v.clone(); }
    fs::write(sidecar(&path), serde_json::to_vec(&meta).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Titre (80 caractères au plus, vide : retiré) et favori d'un clip, gardés à côté du fichier.
pub fn clip_update(app: &AppHandle, name: &str, title: Option<&str>, favorite: Option<bool>) -> Result<(), String> {
    let path = clip_path(app, name).ok_or("Clip introuvable.")?;
    let mut meta: Value = fs::read(sidecar(&path)).ok().and_then(|b| serde_json::from_slice(&b).ok()).filter(Value::is_object).unwrap_or_else(|| json!({}));
    if let Some(t) = title {
        let t: String = t.trim().chars().filter(|c| !c.is_control()).take(80).collect();
        if t.is_empty() { meta.as_object_mut().map(|m| m.remove("title")); } else { meta["title"] = t.into(); }
    }
    if let Some(f) = favorite { meta["favorite"] = f.into(); }
    fs::write(sidecar(&path), serde_json::to_vec(&meta).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Fichiers qui accompagnent un clip : json, vignette, pistes de son séparées.
fn remove_companions(path: &Path) {
    let _ = fs::remove_file(sidecar(path));
    let _ = fs::remove_file(thumb(path));
    for t in ["game", "mic"] {
        let _ = fs::remove_file(crate::clips::track_file(path, t));
    }
}

pub fn clip_delete(app: &AppHandle, name: &str) -> Result<(), String> {
    let path = clip_path(app, name).ok_or("Clip introuvable.")?;
    remove_companions(&path);
    fs::remove_file(path).map_err(|e| e.to_string())
}

/// Vignette d'un clip (`<clip>.jpg`, invisible de la liste des captures : `valid_name` ne
/// prend pas les .jpg). Faite par le plugin à partir de la vidéo, que WebView2 décode déjà.
fn thumb(path: &Path) -> PathBuf { path.with_extension("jpg") }

pub fn clip_thumb_path(app: &AppHandle, name: &str) -> Option<PathBuf> {
    clip_path(app, name).map(|p| thumb(&p)).filter(|t| t.is_file())
}

/// Données du clip gardées à côté de lui (jeu, titre, favori…).
pub fn clip_meta(app: &AppHandle, name: &str) -> Value {
    clip_path(app, name).and_then(|p| fs::read(sidecar(&p)).ok()).and_then(|b| serde_json::from_slice(&b).ok()).filter(Value::is_object).unwrap_or_else(|| json!({}))
}

/// Nouveau fichier de montage : nom de travail (hors liste) et nom final.
pub(crate) fn new_capture(app: &AppHandle, game: Option<&str>, ext: &str) -> Result<(PathBuf, PathBuf), String> {
    let d = dir(app)?;
    let at = now();
    Ok((d.join(format!("_edit-{at}.{ext}")), d.join(file_name(at, game, ext))))
}

/// Termine un montage : nom final et données du clip (jeu, titre, clips d'origine), sans favori
/// ni vignette ni durée hérités. Rend le nom du nouveau fichier.
pub(crate) fn finish_capture(work: &Path, path: &Path, meta: &Value) -> Result<String, String> {
    fs::rename(work, path).map_err(|e| e.to_string())?;
    // pistes séparées : seulement si le rendu en a écrit (rendu `mix`)
    let mut kept = Vec::new();
    for t in ["game", "mic"] {
        if fs::rename(crate::clips::track_file(work, t), crate::clips::track_file(path, t)).is_ok() {
            kept.push(t);
        }
    }
    let mut meta = meta.clone();
    if let Some(m) = meta.as_object_mut() {
        for k in ["favorite", "duration", "auto", "link", "linkUntil", "tracks"] { m.remove(k); }
    }
    if !kept.is_empty() {
        let mut names = vec!["mix"];
        names.extend(kept);
        meta["tracks"] = names.into();
    }
    fs::write(sidecar(path), serde_json::to_vec(&meta).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    path.file_name().and_then(|n| n.to_str()).map(String::from).ok_or_else(|| "Nom invalide.".into())
}

/// Garde la vignette (JPEG de 300 Ko au plus) et la durée d'un clip.
pub fn clip_thumb(app: &AppHandle, name: &str, jpeg: &[u8], duration: Option<f64>) -> Result<(), String> {
    let path = clip_path(app, name).ok_or("Clip introuvable.")?;
    if jpeg.len() > 300 * 1024 || !jpeg.starts_with(&[0xFF, 0xD8, 0xFF]) { return Err("Vignette invalide.".into()); }
    fs::write(thumb(&path), jpeg).map_err(|e| e.to_string())?;
    if let Some(d) = duration.filter(|d| d.is_finite() && *d > 0.0 && *d < 86_400.0) {
        let mut meta: Value = fs::read(sidecar(&path)).ok().and_then(|b| serde_json::from_slice(&b).ok()).filter(Value::is_object).unwrap_or_else(|| json!({}));
        meta["duration"] = ((d * 10.0).round() / 10.0).into();
        fs::write(sidecar(&path), serde_json::to_vec(&meta).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Fige l'image de l'écran (ouverture du panneau d'iClips, qui remplace l'overlay).
pub(crate) fn freeze(app: &AppHandle) {
    let _ = app;
    let game = GAME.lock().ok().and_then(|g| g.clone());
    let shot = grab_screen().and_then(|(w, h, rgb)| encode_png(w, h, &rgb).ok());
    if let Ok(mut p) = PENDING.lock() { *p = shot.map(|png| Shot { png, game, at: now() }); }
}

/// Image figée (PNG), pour le panneau d'iClips.
pub(crate) fn frozen_png() -> Option<Vec<u8>> {
    PENDING.lock().ok().and_then(|p| p.as_ref().map(|s| s.png.clone()))
}

/// `shot.save` : garde l'image figée (dossier des captures, visible dans la Communauté et iClips).
pub fn save_frozen(app: &AppHandle) -> Result<Value, String> {
    let shot = PENDING.lock().map_err(|_| "Indisponible.")?.take().ok_or("Aucune image à garder.")?;
    let c = store(app, &shot.png, "png", shot.game, shot.at)?;
    crate::host::notify(app, "clips://changed", Value::Null);
    Ok(json!({ "name": c.name }))
}

/// `capture.get` : réglages des clips, place prise par les captures du profil.
pub fn capture_state(app: &AppHandle) -> Value {
    let s = crate::clips::settings();
    let (mut count, mut bytes) = (0u64, 0u64);
    if let Ok(entries) = dir(app).and_then(|d| fs::read_dir(d).map_err(|e| e.to_string())) {
        for e in entries.flatten() {
            if let Some(c) = describe(&e.path()) { count += 1; bytes += c.size; }
        }
    }
    let (height, fps, kbps) = s.quality.params();
    // mémoire tenue en jeu : la vidéo à son débit, plus 48 kHz stéréo en f32 par source de son
    let sources = s.system_audio as u64 + s.microphone as u64;
    let memory = (kbps as u64 * 125 + sources * 48_000 * 8) * (s.seconds as u64 + 3);
    json!({
        "enabled": CLIPS.load(Ordering::Relaxed), "quality": s.quality, "seconds": s.seconds,
        "systemAudio": s.system_audio, "microphone": s.microphone, "systemVolume": s.system_volume, "micVolume": s.mic_volume, "tracks": s.tracks,
        "everywhere": s.everywhere,
        "height": height, "fps": fps, "kbps": kbps, "memory": memory,
        "min": crate::clips::MIN_SECONDS, "max": crate::clips::MAX_SECONDS,
        "count": count, "bytes": bytes,
        // appareils de son : choisis (null : celui par défaut) et branchés
        "outputDevice": crate::clips::devices().0, "micDevice": crate::clips::devices().1,
        "outputs": crate::remote_play::audio::endpoints(false), "inputs": crate::remote_play::audio::endpoints(true),
        // raccourci du panneau en jeu (null : coupé)
        "overlayKeys": crate::shortcut::state().0,
        // app autonome : dossier des clips, réglable
        "folder": folder(app).ok().map(|p| p.to_string_lossy().into_owned()),
    })
}

/// `capture.set` : change les réglages des clips ; iBox les garde (Paramètres › Captures suit).
pub fn capture_apply(app: &AppHandle, patch: &Value) -> Result<Value, String> {
    let mut s = crate::clips::settings();
    if let Some(q) = patch.get("quality") { s.quality = serde_json::from_value(q.clone()).map_err(|_| "Qualité inconnue.")?; }
    if let Some(n) = patch["seconds"].as_u64() { s.seconds = n as u32; }
    if let Some(b) = patch["systemAudio"].as_bool() { s.system_audio = b; }
    if let Some(b) = patch["microphone"].as_bool() { s.microphone = b; }
    if let Some(v) = patch["systemVolume"].as_f64() { s.system_volume = v as f32; }
    if let Some(v) = patch["micVolume"].as_f64() { s.mic_volume = v as f32; }
    if let Some(b) = patch["tracks"].as_bool() { s.tracks = b; }
    if let Some(b) = patch["everywhere"].as_bool() { s.everywhere = b; }
    // appareils : identifiant de Windows, ou null pour revenir à celui par défaut
    let (mut output, mut mic) = crate::clips::devices();
    if let Some(v) = patch.get("outputDevice") { output = v.as_str().map(String::from); }
    if let Some(v) = patch.get("micDevice") { mic = v.as_str().map(String::from); }
    crate::clips::set_devices(output, mic);
    let s = s.sanitized();
    crate::clips::configure(s);
    if let Some(b) = patch["enabled"].as_bool() { CLIPS.store(b, Ordering::Relaxed); }
    // gardés pour le prochain démarrage (dans iBox, la fenêtre principale s'en charge)
    let (output, mic) = crate::clips::devices();
    let enabled = CLIPS.load(Ordering::Relaxed);
    crate::prefs::update(app, |p| {
        p.clip = Some(s);
        p.output_device = output;
        p.mic_device = mic;
        p.clips = enabled;
    });
    Ok(capture_state(app))
}

/// `capture.folder` : ouvre le dossier des clips du profil.
pub fn open_folder(app: &AppHandle) -> Result<Value, String> {
    let d = dir(app)?;
    std::process::Command::new("explorer.exe").arg(&d).spawn().map_err(|e| e.to_string())?;
    Ok(Value::Null)
}

/// Ce que le panneau d'iClips doit savoir pour proposer « Garder le clip ».
pub fn clip_status() -> Value {
    let game = GAME.lock().ok().and_then(|g| g.clone());
    json!({
        "game": game.as_ref().map(|g| &g.0), "platformId": game.as_ref().map(|g| &g.1),
        "enabled": CLIPS.load(Ordering::Relaxed), "seconds": crate::clips::available_seconds(), "max": crate::clips::settings().seconds,
        "listening": crate::clips_auto::listening(),
        "hasShot": PENDING.lock().map(|p| p.is_some()).unwrap_or(false),
    })
}

/// Réduction douce (moyenne des pixels couverts) d'une image RGB.
pub(crate) fn shrink(buf: &[u8], w: u32, h: u32, tw: u32, th: u32) -> Vec<u8> {
    let mut out = vec![0u8; (tw * th * 3) as usize];
    for y in 0..th {
        let (y0, y1) = ((y * h / th) as usize, (((y + 1) * h / th).max(y * h / th + 1)) as usize);
        for x in 0..tw {
            let (x0, x1) = ((x * w / tw) as usize, (((x + 1) * w / tw).max(x * w / tw + 1)) as usize);
            let mut acc = [0u32; 3];
            let mut n = 0u32;
            for sy in y0..y1.min(h as usize) { for sx in x0..x1.min(w as usize) {
                let i = (sy * w as usize + sx) * 3;
                acc[0] += buf[i] as u32; acc[1] += buf[i + 1] as u32; acc[2] += buf[i + 2] as u32; n += 1;
            } }
            let o = ((y * tw + x) * 3) as usize;
            for c in 0..3 { out[o + c] = (acc[c] / n.max(1)) as u8; }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vraie capture WGC de l'écran principal (demande une session graphique) :
    /// `cargo test --lib capture::tests::ecran_par_wgc -- --ignored --nocapture`
    #[test]
    #[ignore]
    #[cfg(windows)]
    fn ecran_par_wgc() {
        let (w, h, rgb) = grab_wgc().expect("WGC indisponible");
        assert!(w >= 640 && h >= 480, "taille {w}×{h}");
        assert_eq!(rgb.len(), (w * h * 3) as usize);
        let lit = rgb.iter().filter(|v| **v > 8).count();
        println!("WGC : {w}×{h}, {} % d'octets non noirs", lit * 100 / rgb.len());
        assert!(lit > 0, "image entièrement noire");
    }


    #[test]
    fn names_are_safe() {
        assert_eq!(file_name(1700000000000, Some("Zelda: Link's Awakening DX"), "png"), "1700000000000-zelda-link-s-awakening-dx.png");
        assert_eq!(file_name(1, None, "gif"), "1-iclips.gif");
        assert!(valid_name("1700000000000-zelda.png"));
        assert!(!valid_name("../x.png"));
        assert!(!valid_name("a.json"));
        assert!(!valid_name("a.b.png"));
    }

    #[test]
    fn png_and_gif_encode() {
        let rgb = vec![200u8; 4 * 3 * 3];
        let png = encode_png(4, 3, &rgb).unwrap();
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
        let frame = quantize(&rgb);
        assert_eq!(frame.len(), 12);
        let gif = encode_gif(4, 3, &[frame.clone(), frame], 12).unwrap();
        assert!(gif.starts_with(b"GIF89a"));
        assert_eq!(palette().len(), 768);
    }
}
