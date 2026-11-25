//! Montage des clips (iClips C3, permission `clips-edit`) : nouveaux fichiers faits à partir
//! des clips du profil, jamais de modification des originaux.
//!
//! - **MP4** : un ou plusieurs passages (coupe, assemblage), recopiés **sans réencodage**
//!   (Media Foundation lit les échantillons compressés et les réécrit). Rapide et sans perte,
//!   mais chaque passage commence sur l'image clé qui précède le début demandé (une par seconde
//!   dans les clips d'iBox). Les passages doivent avoir le même format vidéo (même taille, mêmes
//!   paramètres H.264) : c'est le cas des clips gardés avec la même qualité.
//! - **GIF** : un passage de 10 s au plus, décodé par Windows, réduit à 480 px de large,
//!   12 images par seconde (`capture::encode_gif`).
//! - **Vertical** (iClips 0.5) : un passage recadré en 9:16 (TikTok, Shorts, Reels) là où
//!   l'utilisateur a placé le cadre, **réencodé** en H.264 (coupe exacte, à l'image près) ; le
//!   son est recopié tel quel.
//! - **Mix** (iClips 0.6) : le son refait avec un volume pour le jeu et un pour le micro (pistes
//!   séparées du clip ; sinon un volume pour tout le son). L'image est recopiée sans réencodage,
//!   le son est décodé, mélangé puis réencodé en AAC ; les pistes séparées suivent.
//!
//! Un seul rendu à la fois, dans son propre fil ; la progression part vers les fenêtres du
//! plugin (`edit://progress`, `edit://done`, `plugins::host::notify`).

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use serde_json::{json, Value};
use tauri::AppHandle;

pub const MAX_SEGMENTS: usize = 20;
/// Durée totale d'un montage MP4 (s).
pub const MAX_TOTAL: f64 = 600.0;
pub const GIF_MAX: f64 = 10.0;
pub const GIF_WIDTH: u32 = 480;
pub const GIF_FPS: u32 = 12;

static BUSY: Mutex<()> = Mutex::new(());
static NEXT: AtomicU32 = AtomicU32::new(1);

/// Passage d'un clip, en secondes.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub path: PathBuf,
    pub start: f64,
    pub end: f64,
}

/// Passages acceptables : 1 à 20, début avant fin, 10 minutes au plus en tout.
pub fn validate(spans: &[(f64, f64)]) -> Result<(), String> {
    if spans.is_empty() || spans.len() > MAX_SEGMENTS {
        return Err(format!("Un montage prend de 1 à {MAX_SEGMENTS} passages."));
    }
    let mut total = 0.0;
    for &(s, e) in spans {
        if !(s.is_finite() && e.is_finite()) || s < 0.0 || e - s < 0.5 {
            return Err("Passage invalide : il doit durer une demi-seconde au moins.".into());
        }
        total += e - s;
    }
    if total > MAX_TOTAL {
        return Err("Montage trop long : 10 minutes au plus.".into());
    }
    Ok(())
}

/// Taille du GIF (paire, 480 px de large au plus, proportions gardées) et nombre d'images.
pub fn gif_plan(start: f64, end: f64, w: u32, h: u32) -> Result<(u32, u32, u32), String> {
    if !(start.is_finite() && end.is_finite()) || start < 0.0 || end - start < 0.5 {
        return Err("Passage invalide : il doit durer une demi-seconde au moins.".into());
    }
    if end - start > GIF_MAX + 0.01 {
        return Err("Un GIF dure 10 secondes au plus.".into());
    }
    if w == 0 || h == 0 {
        return Err("Vidéo illisible.".into());
    }
    let tw = w.min(GIF_WIDTH) & !1;
    let th = (((h as u64 * tw as u64) / w as u64) as u32).max(2) & !1;
    Ok((tw, th, ((end - start) * GIF_FPS as f64).ceil() as u32))
}

/// Cadre vertical 9:16 dans une image `w` × `h` : largeur et hauteur (paires), bord gauche,
/// centré sur `focus` (0 : à gauche, 1 : à droite).
pub fn vertical_plan(w: u32, h: u32, focus: f64) -> Result<(u32, u32, u32), String> {
    if w < 16 || h < 16 {
        return Err("Vidéo illisible.".into());
    }
    let ch = h & !1;
    let cw = ((ch as f64 * 9.0 / 16.0).round() as u32) & !1;
    if cw >= w {
        return Err("Ce clip est déjà vertical.".into());
    }
    let focus = if focus.is_finite() { focus.clamp(0.0, 1.0) } else { 0.5 };
    let x0 = ((focus * w as f64 - cw as f64 / 2.0).clamp(0.0, (w - cw) as f64) as u32) & !1;
    Ok((cw, ch, x0))
}

/// Ce que l'interface demande.
pub enum Request {
    Mp4 { segments: Vec<(String, f64, f64)>, title: Option<String> },
    Gif { name: String, start: f64, end: f64 },
    Vertical { name: String, start: f64, end: f64, focus: f64 },
    Mix { name: String, game: f32, mic: f32 },
}

/// Volume accepté (0 à 2, 1 : tel quel).
fn gain(v: Option<f64>) -> f32 {
    v.filter(|v| v.is_finite()).map(|v| v.clamp(0.0, 2.0) as f32).unwrap_or(1.0)
}

/// Mélange des pistes décodées (PCM 16 bits), chacune à son volume.
pub fn mix_tracks(tracks: &[(&[i16], f32)]) -> Vec<i16> {
    let len = tracks.iter().map(|(t, _)| t.len()).max().unwrap_or(0);
    (0..len)
        .map(|i| {
            let s: f32 = tracks.iter().map(|(t, g)| t.get(i).copied().unwrap_or(0) as f32 * g).sum();
            s.clamp(-32768.0, 32767.0) as i16
        })
        .collect()
}

impl Request {
    pub fn parse(arg: &Value) -> Result<Request, String> {
        let num = |v: &Value, k: &str| v.get(k).and_then(Value::as_f64).ok_or_else(|| format!("{k} manquant"));
        match arg.get("kind").and_then(Value::as_str) {
            Some("mp4") => {
                let list = arg.get("segments").and_then(Value::as_array).ok_or("segments manquants")?;
                let segments = list
                    .iter()
                    .map(|s| Ok((s.get("name").and_then(Value::as_str).ok_or("name manquant")?.to_string(), num(s, "start")?, num(s, "end")?)))
                    .collect::<Result<Vec<_>, String>>()?;
                let title = arg.get("title").and_then(Value::as_str).map(str::to_string);
                Ok(Request::Mp4 { segments, title })
            }
            Some("gif") => Ok(Request::Gif { name: arg.get("name").and_then(Value::as_str).ok_or("name manquant")?.to_string(), start: num(arg, "start")?, end: num(arg, "end")? }),
            Some("vertical") => Ok(Request::Vertical {
                name: arg.get("name").and_then(Value::as_str).ok_or("name manquant")?.to_string(),
                start: num(arg, "start")?,
                end: num(arg, "end")?,
                focus: arg.get("focus").and_then(Value::as_f64).unwrap_or(0.5),
            }),
            Some("mix") => Ok(Request::Mix {
                name: arg.get("name").and_then(Value::as_str).ok_or("name manquant")?.to_string(),
                game: gain(arg.get("game").and_then(Value::as_f64)),
                mic: gain(arg.get("mic").and_then(Value::as_f64)),
            }),
            _ => Err("kind : mp4, gif, vertical ou mix".into()),
        }
    }
}

/// Lance un rendu ; rend son numéro. Les événements suivent : `edit://progress`
/// `{job, ratio}`, puis `edit://done` `{job, name}` ou `{job, error}`.
pub fn start(app: &AppHandle, request: Request) -> Result<u32, String> {
    // vérifications avant de rendre la main : l'interface apprend tout de suite ce qui ne va pas
    let names: Vec<&String> = match &request {
        Request::Mp4 { segments, .. } => segments.iter().map(|s| &s.0).collect(),
        Request::Gif { name, .. } | Request::Vertical { name, .. } | Request::Mix { name, .. } => vec![name],
    };
    if names.iter().any(|n| !n.ends_with(".mp4")) {
        return Err("Seuls les clips vidéo se montent.".into());
    }
    let (segments, meta, ext) = match &request {
        Request::Mp4 { segments, title } => {
            validate(&segments.iter().map(|s| (s.1, s.2)).collect::<Vec<_>>())?;
            let segs = segments
                .iter()
                .map(|(n, s, e)| Ok(Segment { path: crate::capture::clip_path(app, n).ok_or("Clip introuvable.")?, start: *s, end: *e }))
                .collect::<Result<Vec<_>, String>>()?;
            let mut meta = crate::capture::clip_meta(app, &segments[0].0);
            meta["from"] = segments.iter().map(|s| s.0.clone()).collect::<Vec<_>>().into();
            if let Some(t) = title.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
                meta["title"] = t.chars().take(80).collect::<String>().into();
            }
            (segs, meta, "mp4")
        }
        Request::Gif { name, start, end } => {
            if end - start > GIF_MAX + 0.01 || end - start < 0.5 {
                return Err("Un GIF dure de 0,5 à 10 secondes.".into());
            }
            let mut meta = crate::capture::clip_meta(app, name);
            meta["from"] = vec![name.clone()].into();
            (vec![Segment { path: crate::capture::clip_path(app, name).ok_or("Clip introuvable.")?, start: *start, end: *end }], meta, "gif")
        }
        Request::Vertical { name, start, end, .. } => {
            validate(&[(*start, *end)])?;
            let mut meta = crate::capture::clip_meta(app, name);
            meta["from"] = vec![name.clone()].into();
            meta["vertical"] = true.into();
            (vec![Segment { path: crate::capture::clip_path(app, name).ok_or("Clip introuvable.")?, start: *start, end: *end }], meta, "vertical")
        }
        Request::Mix { name, .. } => {
            let mut meta = crate::capture::clip_meta(app, name);
            meta["from"] = vec![name.clone()].into();
            (vec![Segment { path: crate::capture::clip_path(app, name).ok_or("Clip introuvable.")?, start: 0.0, end: 0.0 }], meta, "mix")
        }
    };
    // volumes demandés (jeu, micro)
    let mix = match &request {
        Request::Mix { game, mic, .. } => (*game, *mic),
        _ => (1.0, 1.0),
    };
    let focus = match &request { Request::Vertical { focus, .. } => *focus, _ => 0.5 };
    let guard = BUSY.try_lock().map_err(|_| "Un montage est déjà en cours.".to_string())?;
    drop(guard);
    let job = NEXT.fetch_add(1, Ordering::Relaxed);
    let app = app.clone();
    std::thread::spawn(move || {
        let Ok(_busy) = BUSY.lock() else { return };
        let mut last = 0.0f32;
        let mut progress = |r: f32| {
            if r - last >= 0.04 || r >= 1.0 {
                last = r;
                crate::host::notify(&app, "edit://progress", json!({ "job": job, "ratio": r.clamp(0.0, 1.0) }));
            }
        };
        let game = meta["game"].as_str().map(String::from);
        let file_ext = if ext == "gif" { "gif" } else { "mp4" };
        let result = crate::capture::new_capture(&app, game.as_deref(), file_ext).and_then(|(work, path)| {
            let done = match ext {
                "gif" => render_gif(&segments[0], &work, &mut progress),
                "vertical" => render_vertical(&segments[0], focus, &work, &mut progress),
                "mix" => render_mix(&segments[0].path, mix.0, mix.1, &work, &mut progress),
                _ => render_mp4(&segments, &work, &mut progress),
            };
            match done {
                Ok(()) => crate::capture::finish_capture(&work, &path, &meta),
                Err(e) => {
                    let _ = std::fs::remove_file(&work);
                    Err(e)
                }
            }
        });
        let payload = match result {
            Ok(name) => json!({ "job": job, "name": name }),
            Err(error) => json!({ "job": job, "error": error }),
        };
        crate::host::notify(&app, "edit://done", payload);
        crate::host::notify(&app, "clips://changed", Value::Null);
    });
    Ok(job)
}

#[cfg(windows)]
pub fn render_mp4(segments: &[Segment], out: &std::path::Path, progress: &mut dyn FnMut(f32)) -> Result<(), String> {
    if !crate::remote_play::encode::mf::media_foundation_available() {
        return Err("Media Foundation manque sur ce Windows (édition N : Media Feature Pack).".into());
    }
    unsafe { mf::concat(segments, out, progress) }
}

#[cfg(windows)]
pub fn render_gif(seg: &Segment, out: &std::path::Path, progress: &mut dyn FnMut(f32)) -> Result<(), String> {
    if !crate::remote_play::encode::mf::media_foundation_available() {
        return Err("Media Foundation manque sur ce Windows (édition N : Media Feature Pack).".into());
    }
    let bytes = unsafe { mf::gif(seg, progress) }?;
    std::fs::write(out, bytes).map_err(|e| e.to_string())
}

#[cfg(windows)]
pub fn render_vertical(seg: &Segment, focus: f64, out: &std::path::Path, progress: &mut dyn FnMut(f32)) -> Result<(), String> {
    if !crate::remote_play::encode::mf::media_foundation_available() {
        return Err("Media Foundation manque sur ce Windows (édition N : Media Feature Pack).".into());
    }
    unsafe { mf::vertical(seg, focus, out, progress) }
}

#[cfg(windows)]
pub fn render_mix(src: &std::path::Path, game: f32, mic: f32, out: &std::path::Path, progress: &mut dyn FnMut(f32)) -> Result<(), String> {
    if !crate::remote_play::encode::mf::media_foundation_available() {
        return Err("Media Foundation manque sur ce Windows (édition N : Media Feature Pack).".into());
    }
    unsafe { mf::remix(src, game, mic, out, progress) }
}

#[cfg(not(windows))]
pub fn render_mix(_: &std::path::Path, _: f32, _: f32, _: &std::path::Path, _: &mut dyn FnMut(f32)) -> Result<(), String> {
    Err("Montage disponible sur Windows seulement.".into())
}

#[cfg(not(windows))]
pub fn render_vertical(_: &Segment, _: f64, _: &std::path::Path, _: &mut dyn FnMut(f32)) -> Result<(), String> {
    Err("Montage disponible sur Windows seulement.".into())
}

#[cfg(not(windows))]
pub fn render_mp4(_: &[Segment], _: &std::path::Path, _: &mut dyn FnMut(f32)) -> Result<(), String> {
    Err("Montage disponible sur Windows seulement.".into())
}
#[cfg(not(windows))]
pub fn render_gif(_: &Segment, _: &std::path::Path, _: &mut dyn FnMut(f32)) -> Result<(), String> {
    Err("Montage disponible sur Windows seulement.".into())
}

#[cfg(windows)]
mod mf {
    use super::Segment;
    use crate::remote_play::encode::mf::MfRuntime;
    use windows::core::{GUID, HSTRING};
    use windows::Win32::Media::MediaFoundation::*;
    use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
    use windows::Win32::System::Variant::VT_I8;

    trait W<T> {
        fn w(self) -> Result<T, String>;
    }
    impl<T> W<T> for windows::core::Result<T> {
        fn w(self) -> Result<T, String> {
            self.map_err(|e| format!("Montage impossible : {}", e.message()))
        }
    }

    struct Com(bool);
    impl Drop for Com {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    const VIDEO: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
    const AUDIO: u32 = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
    const ALL: u32 = MF_SOURCE_READER_ALL_STREAMS.0 as u32;
    const EOS: u32 = MF_SOURCE_READERF_ENDOFSTREAM.0 as u32;
    const SEC: f64 = 10_000_000.0;

    unsafe fn seek(r: &IMFSourceReader, t: i64) -> windows::core::Result<()> {
        let mut v = PROPVARIANT::default();
        (*v.Anonymous.Anonymous).vt = VT_I8;
        (*v.Anonymous.Anonymous).Anonymous.hVal = t.max(0);
        r.SetCurrentPosition(&GUID::zeroed(), &v)
    }

    unsafe fn only(r: &IMFSourceReader, stream: u32) -> windows::core::Result<()> {
        r.SetStreamSelection(ALL, false)?;
        r.SetStreamSelection(stream, true)
    }

    unsafe fn read(r: &IMFSourceReader, stream: u32) -> windows::core::Result<Option<(i64, IMFSample)>> {
        loop {
            let mut flags = 0u32;
            let mut ts = 0i64;
            let mut s: Option<IMFSample> = None;
            r.ReadSample(stream, 0, None, Some(&mut flags), Some(&mut ts), Some(&mut s))?;
            if let Some(s) = s {
                return Ok(Some((ts, s)));
            }
            if flags & EOS != 0 {
                return Ok(None);
            }
        }
    }

    unsafe fn blob(t: &IMFMediaType, key: &GUID) -> Vec<u8> {
        let Ok(n) = t.GetBlobSize(key) else { return Vec::new() };
        let mut out = vec![0u8; n as usize];
        if t.GetBlob(key, &mut out, None).is_err() {
            return Vec::new();
        }
        out
    }

    unsafe fn copy(t: &IMFMediaType) -> windows::core::Result<IMFMediaType> {
        let out = MFCreateMediaType()?;
        t.CopyAllItems(&out)?;
        Ok(out)
    }

    /// Signature d'un format vidéo : deux passages s'enchaînent sans réencodage s'ils l'ont en commun.
    unsafe fn video_key(t: &IMFMediaType) -> (GUID, u64, Vec<u8>) {
        (t.GetGUID(&MF_MT_SUBTYPE).unwrap_or_default(), t.GetUINT64(&MF_MT_FRAME_SIZE).unwrap_or(0), blob(t, &MF_MT_MPEG_SEQUENCE_HEADER))
    }

    unsafe fn audio_key(t: &IMFMediaType) -> (GUID, u32, u32) {
        (t.GetGUID(&MF_MT_SUBTYPE).unwrap_or_default(), t.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND).unwrap_or(0), t.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS).unwrap_or(0))
    }

    pub unsafe fn concat(segments: &[Segment], out: &std::path::Path, progress: &mut dyn FnMut(f32)) -> Result<(), String> {
        let _com = Com(CoInitializeEx(None, COINIT_MULTITHREADED).is_ok());
        let _mf = MfRuntime::start().w()?;
        let mut sources = Vec::new();
        for s in segments {
            let r = MFCreateSourceReaderFromURL(&HSTRING::from(s.path.as_os_str()), None).w()?;
            let v = r.GetNativeMediaType(VIDEO, 0).w()?;
            let a = r.GetNativeMediaType(AUDIO, 0).ok();
            sources.push((r, v, a));
        }
        let first = video_key(&sources[0].1);
        if sources.iter().any(|(_, v, _)| video_key(v) != first) {
            return Err("Ces clips n'ont pas le même format vidéo (qualité différente) : ils ne s'assemblent pas encore.".into());
        }
        // son : seulement si tous les passages en ont un, au même format
        let with_audio = sources.iter().all(|(_, _, a)| a.is_some())
            && sources.iter().all(|(_, _, a)| audio_key(a.as_ref().unwrap()) == audio_key(sources[0].2.as_ref().unwrap()));

        let mut attrs: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut attrs, 2).w()?;
        let attrs = attrs.ok_or("Montage impossible.")?;
        attrs.SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_MPEG4).w()?;
        attrs.SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1).w()?;
        let writer = MFCreateSinkWriterFromURL(&HSTRING::from(out.as_os_str()), None, &attrs).w()?;
        let vt = copy(&sources[0].1).w()?;
        let vs = writer.AddStream(&vt).w()?;
        writer.SetInputMediaType(vs, &vt, None).w()?;
        let audio_stream = if with_audio {
            let at = copy(sources[0].2.as_ref().unwrap()).w()?;
            let s = writer.AddStream(&at).w()?;
            writer.SetInputMediaType(s, &at, None).w()?;
            Some(s)
        } else {
            None
        };
        writer.BeginWriting().w()?;

        let total: f64 = segments.iter().map(|s| s.end - s.start).sum();
        let mut done = 0.0;
        let mut offset = 0i64;
        for (seg, (r, _, _)) in segments.iter().zip(&sources) {
            let (start, end) = ((seg.start * SEC) as i64, (seg.end * SEC) as i64);
            // image : depuis l'image clé qui précède le début
            only(r, VIDEO).w()?;
            seek(r, start).w()?;
            let mut origin: Option<i64> = None;
            let mut span = 0i64;
            while let Some((t, s)) = read(r, VIDEO).w()? {
                if t >= end {
                    break;
                }
                if origin.is_none() {
                    if s.GetUINT32(&MFSampleExtension_CleanPoint).unwrap_or(0) == 0 {
                        continue;
                    }
                    origin = Some(t);
                }
                let o = origin.unwrap();
                let d = s.GetSampleDuration().unwrap_or(166_667).max(1);
                s.SetSampleTime(t - o + offset).w()?;
                writer.WriteSample(vs, &s).w()?;
                span = span.max(t - o + d);
                progress(((done + (t - start).max(0) as f64 / SEC) / total * 0.9) as f32);
            }
            let Some(o) = origin else { return Err("Passage vide : aucune image clé avant la fin demandée.".into()) };
            // son : même origine, même longueur
            if let Some(as_) = audio_stream {
                only(r, AUDIO).w()?;
                seek(r, o).w()?;
                while let Some((t, s)) = read(r, AUDIO).w()? {
                    if t < o {
                        continue;
                    }
                    if t - o >= span {
                        break;
                    }
                    s.SetSampleTime(t - o + offset).w()?;
                    writer.WriteSample(as_, &s).w()?;
                }
            }
            offset += span;
            done += seg.end - seg.start;
        }
        writer.Finalize().w()?;
        progress(1.0);
        Ok(())
    }

    /// Relecture (essais) : durée vidéo (dernière image), nombre d'images et de paquets audio.
    #[cfg(test)]
    pub unsafe fn probe(path: &std::path::Path) -> Result<(f64, usize, usize), String> {
        let _com = Com(CoInitializeEx(None, COINIT_MULTITHREADED).is_ok());
        let _mf = MfRuntime::start().w()?;
        let r = MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), None).w()?;
        only(&r, VIDEO).w()?;
        let (mut last, mut frames) = (0i64, 0);
        while let Some((t, _)) = read(&r, VIDEO).w()? {
            last = last.max(t);
            frames += 1;
        }
        let mut audio = 0;
        if r.GetNativeMediaType(AUDIO, 0).is_ok() {
            only(&r, AUDIO).w()?;
            seek(&r, 0).w()?;
            while read(&r, AUDIO).w()?.is_some() {
                audio += 1;
            }
        }
        Ok((last as f64 / SEC, frames, audio))
    }

    /// Recadrage 9:16 réencodé en H.264 ; le son est recopié tel quel.
    pub unsafe fn vertical(seg: &Segment, focus: f64, out: &std::path::Path, progress: &mut dyn FnMut(f32)) -> Result<(), String> {
        let _com = Com(CoInitializeEx(None, COINIT_MULTITHREADED).is_ok());
        let _mf = MfRuntime::start().w()?;
        let mut attrs: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut attrs, 1).w()?;
        let attrs = attrs.ok_or("Montage impossible.")?;
        attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1).w()?;
        let r = MFCreateSourceReaderFromURL(&HSTRING::from(seg.path.as_os_str()), &attrs).w()?;
        let native = r.GetNativeMediaType(VIDEO, 0).w()?;
        let audio = r.GetNativeMediaType(AUDIO, 0).ok();
        only(&r, VIDEO).w()?;
        let rgb = MFCreateMediaType().w()?;
        rgb.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).w()?;
        rgb.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32).w()?;
        r.SetCurrentMediaType(VIDEO, None, &rgb).w()?;
        let cur = r.GetCurrentMediaType(VIDEO).w()?;
        let size = cur.GetUINT64(&MF_MT_FRAME_SIZE).w()?;
        let (bw, bh) = ((size >> 32) as u32, size as u32);
        let shown = native.GetUINT64(&MF_MT_FRAME_SIZE).unwrap_or(size);
        let (w, h) = (bw.min((shown >> 32) as u32), bh.min(shown as u32));
        let stride = cur.GetUINT32(&MF_MT_DEFAULT_STRIDE).map(|v| v as i32).unwrap_or(bw as i32 * 4);
        let (cw, ch, x0) = super::vertical_plan(w, h, focus)?;
        let rate = native.GetUINT64(&MF_MT_FRAME_RATE).ok().filter(|r| (r >> 32) > 0 && (*r as u32) > 0).unwrap_or((30u64 << 32) | 1);
        let fps = (rate >> 32) as f64 / (rate as u32) as f64;
        let bitrate = ((cw as f64 * ch as f64 * fps * 0.15) as u32).clamp(2_000_000, 12_000_000);

        let mut wattrs: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut wattrs, 2).w()?;
        let wattrs = wattrs.ok_or("Montage impossible.")?;
        wattrs.SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_MPEG4).w()?;
        wattrs.SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1).w()?;
        let writer = MFCreateSinkWriterFromURL(&HSTRING::from(out.as_os_str()), None, &wattrs).w()?;
        let frame_size = ((cw as u64) << 32) | ch as u64;
        let vt = MFCreateMediaType().w()?;
        vt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).w()?;
        vt.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264).w()?;
        vt.SetUINT64(&MF_MT_FRAME_SIZE, frame_size).w()?;
        vt.SetUINT64(&MF_MT_FRAME_RATE, rate).w()?;
        vt.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1).w()?;
        vt.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32).w()?;
        vt.SetUINT32(&MF_MT_AVG_BITRATE, bitrate).w()?;
        let vs = writer.AddStream(&vt).w()?;
        let it = MFCreateMediaType().w()?;
        it.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).w()?;
        it.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32).w()?;
        it.SetUINT64(&MF_MT_FRAME_SIZE, frame_size).w()?;
        it.SetUINT64(&MF_MT_FRAME_RATE, rate).w()?;
        it.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1).w()?;
        it.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32).w()?;
        // lignes rangées de haut en bas
        it.SetUINT32(&MF_MT_DEFAULT_STRIDE, cw * 4).w()?;
        writer.SetInputMediaType(vs, &it, None).w()?;
        let audio_stream = match &audio {
            Some(a) => {
                let at = copy(a).w()?;
                let s = writer.AddStream(&at).w()?;
                writer.SetInputMediaType(s, &at, None).w()?;
                Some(s)
            }
            None => None,
        };
        writer.BeginWriting().w()?;

        let (start, end) = ((seg.start * SEC) as i64, (seg.end * SEC) as i64);
        let frame_len = (cw * ch * 4) as usize;
        let pitch = stride.unsigned_abs() as usize;
        seek(&r, start).w()?;
        let mut span = 0i64;
        while let Some((t, s)) = read(&r, VIDEO).w()? {
            if t >= end {
                break;
            }
            let d = s.GetSampleDuration().unwrap_or((SEC / fps) as i64).max(1);
            // coupe exacte : les images d'avant le début (depuis l'image clé) sont décodées puis laissées
            if t + d / 2 < start {
                continue;
            }
            let buf = s.ConvertToContiguousBuffer().w()?;
            let mut ptr: *mut u8 = std::ptr::null_mut();
            let mut len = 0u32;
            buf.Lock(&mut ptr, None, Some(&mut len)).w()?;
            let ok = pitch * bh as usize <= len as usize;
            let mb = MFCreateMemoryBuffer(frame_len as u32).w()?;
            if ok {
                let data = std::slice::from_raw_parts(ptr, len as usize);
                let mut dst: *mut u8 = std::ptr::null_mut();
                mb.Lock(&mut dst, None, None).w()?;
                let dst = std::slice::from_raw_parts_mut(dst, frame_len);
                for y in 0..ch as usize {
                    let row = if stride < 0 { (bh as usize - 1 - y) * pitch } else { y * pitch };
                    let from = row + x0 as usize * 4;
                    dst[y * cw as usize * 4..(y + 1) * cw as usize * 4].copy_from_slice(&data[from..from + cw as usize * 4]);
                }
                mb.Unlock().w()?;
                mb.SetCurrentLength(frame_len as u32).w()?;
            }
            buf.Unlock().w()?;
            if !ok {
                continue;
            }
            let at = (t - start).max(0);
            let sample = MFCreateSample().w()?;
            sample.AddBuffer(&mb).w()?;
            sample.SetSampleTime(at).w()?;
            sample.SetSampleDuration(d).w()?;
            writer.WriteSample(vs, &sample).w()?;
            span = span.max(at + d);
            progress(((t - start).max(0) as f64 / (end - start) as f64 * 0.9) as f32);
        }
        if span == 0 {
            return Err("Aucune image dans ce passage.".into());
        }
        if let Some(as_) = audio_stream {
            only(&r, AUDIO).w()?;
            seek(&r, start).w()?;
            while let Some((t, s)) = read(&r, AUDIO).w()? {
                if t < start {
                    continue;
                }
                if t - start >= span {
                    break;
                }
                s.SetSampleTime(t - start).w()?;
                writer.WriteSample(as_, &s).w()?;
            }
        }
        writer.Finalize().w()?;
        progress(1.0);
        Ok(())
    }

    /// Flux d'un fichier : index de la vidéo et des pistes de son, dans l'ordre.
    unsafe fn streams(r: &IMFSourceReader) -> (Option<u32>, Vec<u32>) {
        let (mut video, mut audio) = (None, Vec::new());
        for i in 0..8u32 {
            let Ok(t) = r.GetNativeMediaType(i, 0) else { break };
            match t.GetGUID(&MF_MT_MAJOR_TYPE) {
                Ok(g) if g == MFMediaType_Video && video.is_none() => video = Some(i),
                Ok(g) if g == MFMediaType_Audio => audio.push(i),
                _ => {}
            }
        }
        (video, audio)
    }

    /// Une piste de son décodée : PCM 16 bits entrelacé, fréquence, voies, premier instant.
    unsafe fn decode(path: &std::path::Path, stream: u32) -> Result<(Vec<i16>, u32, u32, i64), String> {
        let r = MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), None).w()?;
        let pcm = MFCreateMediaType().w()?;
        pcm.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).w()?;
        pcm.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM).w()?;
        pcm.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16).w()?;
        r.SetCurrentMediaType(stream, None, &pcm).w()?;
        let cur = r.GetCurrentMediaType(stream).w()?;
        let rate = cur.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND).w()?;
        let channels = cur.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS).w()?;
        only(&r, stream).w()?;
        let mut out = Vec::new();
        let mut first: Option<i64> = None;
        while let Some((t, s)) = read(&r, stream).w()? {
            first.get_or_insert(t);
            let buf = s.ConvertToContiguousBuffer().w()?;
            let mut ptr: *mut u8 = std::ptr::null_mut();
            let mut len = 0u32;
            buf.Lock(&mut ptr, None, Some(&mut len)).w()?;
            let bytes = std::slice::from_raw_parts(ptr, len as usize);
            out.extend(bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])));
            buf.Unlock().w()?;
        }
        Ok((out, rate, channels, first.unwrap_or(0)))
    }

    unsafe fn aac_type(rate: u32, channels: u32, aac: bool) -> windows::core::Result<IMFMediaType> {
        let t = MFCreateMediaType()?;
        t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
        t.SetGUID(&MF_MT_SUBTYPE, if aac { &MFAudioFormat_AAC } else { &MFAudioFormat_PCM })?;
        t.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, channels)?;
        t.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, rate)?;
        t.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        if aac {
            t.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 24_000)?;
        } else {
            t.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, channels * 2)?;
            t.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, rate * channels * 2)?;
        }
        Ok(t)
    }

    /// Son refait : `game` et `mic` sur les pistes séparées (`<clip>.game.m4a`, `.mic.m4a`) ;
    /// sans elles, `game` règle tout le son. Les pistes séparées sont recopiées à côté du rendu.
    pub unsafe fn remix(src: &std::path::Path, game: f32, mic: f32, out: &std::path::Path, progress: &mut dyn FnMut(f32)) -> Result<(), String> {
        let _com = Com(CoInitializeEx(None, COINIT_MULTITHREADED).is_ok());
        let _mf = MfRuntime::start().w()?;
        let r = MFCreateSourceReaderFromURL(&HSTRING::from(src.as_os_str()), None).w()?;
        let (video, audio) = streams(&r);
        let video = video.ok_or("Vidéo illisible.")?;
        let files: Vec<(std::path::PathBuf, f32)> = [("game", game), ("mic", mic)]
            .into_iter()
            .map(|(n, g)| (crate::clips::track_file(src, n), g))
            .filter(|(f, _)| f.is_file())
            .collect();
        let mut decoded = Vec::new();
        if files.is_empty() {
            let first = *audio.first().ok_or("Ce clip n'a pas de son.")?;
            decoded.push((decode(src, first)?, game));
        } else {
            for (f, g) in &files {
                let (_, tracks) = { let rr = MFCreateSourceReaderFromURL(&HSTRING::from(f.as_os_str()), None).w()?; streams(&rr) };
                decoded.push((decode(f, *tracks.first().ok_or("Piste de son illisible.")?)?, *g));
            }
        }
        progress(0.3);
        let (_, rate, channels, a0) = decoded[0].0;
        if decoded.iter().any(|((_, r, c, _), _)| (*r, *c) != (rate, channels)) {
            return Err("Les pistes de son de ce clip ne sont pas au même format.".into());
        }
        let mixed = super::mix_tracks(&decoded.iter().map(|((pcm, _, _, _), g)| (pcm.as_slice(), *g)).collect::<Vec<_>>());

        let mut attrs: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut attrs, 2).w()?;
        let attrs = attrs.ok_or("Montage impossible.")?;
        attrs.SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_MPEG4).w()?;
        attrs.SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1).w()?;
        let writer = MFCreateSinkWriterFromURL(&HSTRING::from(out.as_os_str()), None, &attrs).w()?;
        let vt = copy(&r.GetNativeMediaType(video, 0).w()?).w()?;
        let vs = writer.AddStream(&vt).w()?;
        writer.SetInputMediaType(vs, &vt, None).w()?;
        let as_ = writer.AddStream(&aac_type(rate, channels, true).w()?).w()?;
        writer.SetInputMediaType(as_, &aac_type(rate, channels, false).w()?, None).w()?;
        writer.BeginWriting().w()?;

        // image : recopiée telle quelle, depuis le début
        only(&r, video).w()?;
        seek(&r, 0).w()?;
        let mut v0: Option<i64> = None;
        while let Some((t, s)) = read(&r, video).w()? {
            let o = *v0.get_or_insert(t);
            s.SetSampleTime(t - o).w()?;
            writer.WriteSample(vs, &s).w()?;
        }
        progress(0.7);
        // les pistes séparées partent de l'instant 0 du clip ; le son du MP4, de son propre début
        let shift = if files.is_empty() { a0 - v0.unwrap_or(0) } else { 0 };
        let chunk = 1024 * channels as usize;
        for (k, part) in mixed.chunks(chunk).enumerate() {
            let bytes: Vec<u8> = part.iter().flat_map(|v| v.to_le_bytes()).collect();
            let buffer = MFCreateMemoryBuffer(bytes.len() as u32).w()?;
            let mut ptr: *mut u8 = std::ptr::null_mut();
            buffer.Lock(&mut ptr, None, None).w()?;
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
            buffer.Unlock().w()?;
            buffer.SetCurrentLength(bytes.len() as u32).w()?;
            let sample = MFCreateSample().w()?;
            sample.AddBuffer(&buffer).w()?;
            sample.SetSampleTime((shift + (k * 1024) as i64 * 10_000_000 / rate as i64).max(0)).w()?;
            sample.SetSampleDuration((part.len() / channels as usize) as i64 * 10_000_000 / rate as i64).w()?;
            writer.WriteSample(as_, &sample).w()?;
        }
        writer.Finalize().w()?;
        // pistes séparées recopiées : le réglage peut se refaire sur le nouveau clip
        for n in ["game", "mic"] {
            let f = crate::clips::track_file(src, n);
            if f.is_file() {
                std::fs::copy(&f, crate::clips::track_file(out, n)).map_err(|e| e.to_string())?;
            }
        }
        progress(1.0);
        Ok(())
    }

    /// Nombre de pistes de son et niveau moyen de chacune (essais).
    #[cfg(test)]
    pub unsafe fn levels(path: &std::path::Path) -> Result<Vec<f64>, String> {
        let r = {
            let _com = Com(CoInitializeEx(None, COINIT_MULTITHREADED).is_ok());
            let _mf = MfRuntime::start().w()?;
            let r = MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), None).w()?;
            streams(&r).1
        };
        let _com = Com(CoInitializeEx(None, COINIT_MULTITHREADED).is_ok());
        let _mf = MfRuntime::start().w()?;
        let mut out = Vec::new();
        for i in r {
            let (pcm, _, _, _) = decode(path, i)?;
            let rms = (pcm.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / pcm.len().max(1) as f64).sqrt();
            let crossings = pcm.chunks(2).collect::<Vec<_>>().windows(2).filter(|w| (w[0][0] < 0) != (w[1][0] < 0)).count();
            println!("piste {i} : rms {rms:.0}, {crossings} passages par zéro, {} échantillons", pcm.len());
            out.push(rms);
        }
        Ok(out)
    }

    /// Taille d'image d'une vidéo (essais).
    #[cfg(test)]
    pub unsafe fn frame_size(path: &std::path::Path) -> Result<(u32, u32), String> {
        let _com = Com(CoInitializeEx(None, COINIT_MULTITHREADED).is_ok());
        let _mf = MfRuntime::start().w()?;
        let r = MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), None).w()?;
        let s = r.GetNativeMediaType(VIDEO, 0).w()?.GetUINT64(&MF_MT_FRAME_SIZE).w()?;
        Ok(((s >> 32) as u32, s as u32))
    }

    pub unsafe fn gif(seg: &Segment, progress: &mut dyn FnMut(f32)) -> Result<Vec<u8>, String> {
        let _com = Com(CoInitializeEx(None, COINIT_MULTITHREADED).is_ok());
        let _mf = MfRuntime::start().w()?;
        let mut attrs: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut attrs, 1).w()?;
        let attrs = attrs.ok_or("Montage impossible.")?;
        attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1).w()?;
        let r = MFCreateSourceReaderFromURL(&HSTRING::from(seg.path.as_os_str()), &attrs).w()?;
        only(&r, VIDEO).w()?;
        let native = r.GetNativeMediaType(VIDEO, 0).w()?;
        let rgb = MFCreateMediaType().w()?;
        rgb.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).w()?;
        rgb.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32).w()?;
        r.SetCurrentMediaType(VIDEO, None, &rgb).w()?;
        let cur = r.GetCurrentMediaType(VIDEO).w()?;
        let size = cur.GetUINT64(&MF_MT_FRAME_SIZE).w()?;
        let (bw, bh) = ((size >> 32) as u32, size as u32);
        // la taille affichée (le décodeur peut compléter à 1088 lignes)
        let shown = native.GetUINT64(&MF_MT_FRAME_SIZE).unwrap_or(size);
        let (w, h) = (bw.min((shown >> 32) as u32), bh.min(shown as u32));
        let stride = cur.GetUINT32(&MF_MT_DEFAULT_STRIDE).map(|v| v as i32).unwrap_or(bw as i32 * 4);
        let (tw, th, _) = super::gif_plan(seg.start, seg.end, w, h)?;

        let (start, end) = ((seg.start * SEC) as i64, (seg.end * SEC) as i64);
        let step = (SEC / super::GIF_FPS as f64) as i64;
        seek(&r, start).w()?;
        let mut next = start;
        let mut frames = Vec::new();
        while let Some((t, s)) = read(&r, VIDEO).w()? {
            if t >= end {
                break;
            }
            if t + step / 2 < next {
                continue;
            }
            while next <= t {
                next += step;
            }
            let buf = s.ConvertToContiguousBuffer().w()?;
            let mut ptr: *mut u8 = std::ptr::null_mut();
            let mut len = 0u32;
            buf.Lock(&mut ptr, None, Some(&mut len)).w()?;
            let pitch = stride.unsigned_abs() as usize;
            let mut rgb = Vec::with_capacity((w * h * 3) as usize);
            if pitch * bh as usize <= len as usize {
                let data = std::slice::from_raw_parts(ptr, len as usize);
                for y in 0..h as usize {
                    // pas négatif : l'image est rangée de bas en haut
                    let row = if stride < 0 { (bh as usize - 1 - y) * pitch } else { y * pitch };
                    for x in 0..w as usize {
                        let p = row + x * 4;
                        rgb.extend_from_slice(&[data[p + 2], data[p + 1], data[p]]);
                    }
                }
            }
            buf.Unlock().w()?;
            if rgb.len() == (w * h * 3) as usize {
                let small = crate::capture::shrink(&rgb, w, h, tw, th);
                frames.push(crate::capture::quantize(&small));
            }
            progress(((t - start).max(0) as f64 / (end - start) as f64 * 0.9) as f32);
        }
        if frames.is_empty() {
            return Err("Aucune image dans ce passage.".into());
        }
        let gif = crate::capture::encode_gif(tw, th, &frames, (100 / super::GIF_FPS) as u16)?;
        progress(1.0);
        Ok(gif)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_are_checked() {
        assert!(validate(&[(0.0, 5.0)]).is_ok());
        assert!(validate(&[]).is_err());
        assert!(validate(&[(3.0, 3.2)]).is_err());
        assert!(validate(&[(-1.0, 3.0)]).is_err());
        assert!(validate(&[(0.0, f64::NAN)]).is_err());
        assert!(validate(&vec![(0.0, 1.0); MAX_SEGMENTS + 1]).is_err());
        assert!(validate(&vec![(0.0, 30.0); 20]).is_ok());
        assert!(validate(&vec![(0.0, 31.0); 20]).is_err());
    }

    #[test]
    fn le_cadre_vertical_reste_dans_l_image() {
        assert_eq!(vertical_plan(1920, 1080, 0.5).unwrap(), (608, 1080, 656));
        assert_eq!(vertical_plan(1920, 1080, 0.0).unwrap().2, 0);
        assert_eq!(vertical_plan(1920, 1080, 1.0).unwrap().2, 1312);
        assert_eq!(vertical_plan(1280, 720, 0.5).unwrap(), (404, 720, 438));
        assert!(vertical_plan(720, 1280, 0.5).is_err());
        assert_eq!(vertical_plan(1920, 1080, f64::NAN).unwrap().2, 656);
        assert!(matches!(Request::parse(&json!({ "kind": "vertical", "name": "a.mp4", "start": 0, "end": 3 })).unwrap(), Request::Vertical { focus, .. } if focus == 0.5));
    }

    #[test]
    fn le_mix_suit_les_volumes() {
        let game = [1000i16, -1000, 30000];
        let mic = [500i16, 500];
        assert_eq!(mix_tracks(&[(&game, 1.0), (&mic, 0.0)]), vec![1000, -1000, 30000]);
        assert_eq!(mix_tracks(&[(&game, 0.5), (&mic, 2.0)]), vec![1500, 500, 15000]);
        assert_eq!(mix_tracks(&[(&game, 2.0)])[2], 32767);
        assert!(matches!(Request::parse(&json!({ "kind": "mix", "name": "a.mp4", "game": 0.3, "mic": 9 })).unwrap(), Request::Mix { game, mic, .. } if (game - 0.3).abs() < 1e-6 && mic == 2.0));
    }

    /// Son refait sur un clip à pistes séparées (`IBOX_EDIT_SAMPLE` fait avec IBOX_SYNTH_TRACKS) :
    /// micro coupé, le mélange ne garde que le jeu.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn mix_reel() {
        let src = PathBuf::from(std::env::var("IBOX_EDIT_SAMPLE").expect("IBOX_EDIT_SAMPLE"));
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("mix.mp4");
        render_mix(&src, 1.0, 0.0, &out, &mut |_| {}).unwrap();
        let before = unsafe { mf::levels(&src) }.unwrap();
        let game = unsafe { mf::levels(&crate::clips::track_file(&src, "game")) }.unwrap();
        let after = unsafe { mf::levels(&out) }.unwrap();
        let (len, frames, _) = unsafe { mf::probe(&out) }.unwrap();
        println!("source {before:?} ; jeu {game:?} ; mix {after:?} ; {len:.2} s, {frames} images");
        assert_eq!(after.len(), 1, "une seule piste : le mélange");
        // le nouveau mélange vaut la piste du jeu (le micro est coupé)
        assert!((after[0] - game[0]).abs() / game[0] < 0.05, "{after:?} / {game:?}");
        assert!(after[0] < before[0] * 0.9);
        assert!(crate::clips::track_file(&out, "mic").is_file(), "pistes recopiées");
        assert!(len > 9.5);
        if let Ok(o) = std::env::var("IBOX_EDIT_OUT") {
            std::fs::copy(&out, PathBuf::from(o).join("mix.mp4")).unwrap();
        }
    }

    #[test]
    fn gif_size_keeps_proportions() {
        assert_eq!(gif_plan(0.0, 5.0, 1920, 1080).unwrap(), (480, 270, 60));
        assert_eq!(gif_plan(1.0, 3.5, 320, 240).unwrap(), (320, 240, 30));
        assert_eq!(gif_plan(0.0, 2.0, 1921, 1080).unwrap().0, 480);
        assert!(gif_plan(0.0, 11.0, 1920, 1080).is_err());
        assert!(gif_plan(0.0, 5.0, 0, 0).is_err());
    }

    #[test]
    fn requests_are_read() {
        let r = Request::parse(&json!({ "kind": "mp4", "segments": [{ "name": "a.mp4", "start": 1, "end": 4.5 }], "title": "x" })).unwrap();
        assert!(matches!(r, Request::Mp4 { ref segments, .. } if segments[0] == ("a.mp4".to_string(), 1.0, 4.5)));
        assert!(matches!(Request::parse(&json!({ "kind": "gif", "name": "a.mp4", "start": 0, "end": 3 })).unwrap(), Request::Gif { .. }));
        assert!(Request::parse(&json!({ "kind": "avi" })).is_err());
        assert!(Request::parse(&json!({ "kind": "mp4", "segments": [{ "name": "a.mp4" }] })).is_err());
    }

    /// Montage réel sur une vidéo H.264 + AAC (`IBOX_EDIT_SAMPLE`) : coupe, assemblage, GIF.
    /// `IBOX_EDIT_SAMPLE=… cargo test --lib clip_edit::tests::montage_reel -- --ignored --nocapture`
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn montage_reel() {
        let src = PathBuf::from(std::env::var("IBOX_EDIT_SAMPLE").expect("IBOX_EDIT_SAMPLE"));
        let dir = tempfile::tempdir().unwrap();
        let cut = dir.path().join("coupe.mp4");
        let seg = |s, e| Segment { path: src.clone(), start: s, end: e };
        render_mp4(&[seg(2.0, 6.0)], &cut, &mut |_| {}).unwrap();
        let joined = dir.path().join("assemblage.mp4");
        render_mp4(&[seg(0.0, 3.0), seg(5.0, 8.0)], &joined, &mut |_| {}).unwrap();
        let gif = dir.path().join("passage.gif");
        let mut ratios = Vec::new();
        render_gif(&seg(1.0, 3.0), &gif, &mut |r| ratios.push(r)).unwrap();
        for p in [&cut, &joined, &gif] {
            println!("{} : {} octets", p.display(), std::fs::metadata(p).unwrap().len());
        }
        let source = unsafe { mf::probe(&src) }.unwrap();
        let (cut_len, cut_frames, cut_audio) = unsafe { mf::probe(&cut) }.unwrap();
        let (joined_len, joined_frames, joined_audio) = unsafe { mf::probe(&joined) }.unwrap();
        println!("source {source:?} ; coupe {cut_len:.2} s, {cut_frames} images, {cut_audio} audio ; assemblage {joined_len:.2} s, {joined_frames} images, {joined_audio} audio");
        // coupe 2-6 s : de l'image clé qui précède 2 s jusqu'à 6 s
        assert!(cut_len > 3.5 && cut_len < 6.1, "{cut_len}");
        assert!(joined_len > 5.5 && joined_len < 8.1, "{joined_len}");
        assert_eq!(cut_audio > 0, source.2 > 0, "le son suit la source");
        if let Ok(out) = std::env::var("IBOX_EDIT_OUT") {
            for p in [&cut, &joined, &gif] {
                std::fs::copy(p, PathBuf::from(&out).join(p.file_name().unwrap())).unwrap();
            }
        }
        assert!(std::fs::read(&gif).unwrap().starts_with(b"GIF89a"));
        assert_eq!(ratios.last().copied(), Some(1.0));
    }

    /// Recadrage vertical réel (`IBOX_EDIT_SAMPLE`) : 9:16, coupe exacte, son gardé.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn vertical_reel() {
        let src = PathBuf::from(std::env::var("IBOX_EDIT_SAMPLE").expect("IBOX_EDIT_SAMPLE"));
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("vertical.mp4");
        let t = std::time::Instant::now();
        render_vertical(&Segment { path: src.clone(), start: 2.3, end: 6.3 }, 0.8, &out, &mut |_| {}).unwrap();
        let (len, frames, audio) = unsafe { mf::probe(&out) }.unwrap();
        let size = unsafe { mf::frame_size(&out) }.unwrap();
        let source = unsafe { mf::probe(&src) }.unwrap();
        println!("vertical {size:?}, {len:.2} s, {frames} images, {audio} audio, {} octets, {:?}", std::fs::metadata(&out).unwrap().len(), t.elapsed());
        assert!((size.0 as f64 / size.1 as f64 - 9.0 / 16.0).abs() < 0.01, "9:16 attendu : {size:?}");
        assert!(len > 3.8 && len < 4.1, "{len}");
        assert_eq!(audio > 0, source.2 > 0, "le son suit la source");
        if let Ok(o) = std::env::var("IBOX_EDIT_OUT") {
            std::fs::copy(&out, PathBuf::from(o).join("vertical.mp4")).unwrap();
        }
    }
}
