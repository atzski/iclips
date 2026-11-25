//! Clips vidéo (0.1.72) : pendant une partie, iBox garde en mémoire les dernières secondes de
//! l'écran principal, déjà encodées en H.264 (même chaîne que Remote Play : capture Windows
//! Graphics Capture → NV12 → encodeur Media Foundation matériel), avec le son de Windows et/ou
//! le micro en PCM 48 kHz. « Garder le clip » assemble un MP4 (H.264 + AAC) par le Sink Writer
//! de Media Foundation : aucun codec livré par iBox, rien n'est réencodé côté image.
//!
//! Réglages (Paramètres › Captures, ou iClips Studio) : qualité, durée (5 s à 2 min), son de
//! Windows et micro avec leur volume. Pistes séparées (iClips) : le MP4 garde le mélange seul
//! (lu partout : navigateurs, Discord) ; le son de Windows et le micro sont écrits à côté, chacun
//! dans `<clip>.game.m4a` et `<clip>.mic.m4a`, pour régler leur volume au montage (`clip_edit`,
//! rendu `mix`). Un MP4 à plusieurs pistes de son n'est pas relu dans l'ordre écrit par Media
//! Foundation : un lecteur pourrait jouer la mauvaise.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const MIN_SECONDS: u32 = 5;
pub const MAX_SECONDS: u32 = 120;
const RATE: u64 = 48_000;
/// Marge gardée en plus de la durée demandée (une image clé par seconde).
const SLACK: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Quality {
    Low,
    Medium,
    High,
    /// 1440p, 60 i/s (iClips).
    Max,
}

impl Quality {
    /// Hauteur maximale, images par seconde, débit (kbit/s).
    pub fn params(self) -> (u32, u32, u32) {
        match self {
            Quality::Low => (480, 30, 2_500),
            Quality::Medium => (720, 30, 6_000),
            Quality::High => (1080, 60, 12_000),
            Quality::Max => (1440, 60, 20_000),
        }
    }
}

fn full() -> f32 {
    1.0
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipSettings {
    pub quality: Quality,
    pub seconds: u32,
    pub system_audio: bool,
    pub microphone: bool,
    /// Volumes dans le mélange (0 à 2 ; 1 : tel quel).
    #[serde(default = "full")]
    pub system_volume: f32,
    #[serde(default = "full")]
    pub mic_volume: f32,
    /// Pistes séparées en plus du mélange.
    #[serde(default)]
    pub tracks: bool,
    /// Mémoire tenue aussi hors des parties (bureau, vidéo, appel…), pas seulement en jeu.
    #[serde(default)]
    pub everywhere: bool,
}
impl Default for ClipSettings {
    fn default() -> Self {
        ClipSettings { quality: Quality::Medium, seconds: 15, system_audio: true, microphone: false, system_volume: 1.0, mic_volume: 1.0, tracks: false, everywhere: false }
    }
}
impl ClipSettings {
    pub fn sanitized(self) -> Self {
        let vol = |v: f32| if v.is_finite() { v.clamp(0.0, 2.0) } else { 1.0 };
        ClipSettings { seconds: self.seconds.clamp(MIN_SECONDS, MAX_SECONDS), system_volume: vol(self.system_volume), mic_volume: vol(self.mic_volume), ..self }
    }
    fn keep(&self) -> Duration {
        Duration::from_secs(self.seconds as u64) + SLACK
    }
}

#[derive(Clone)]
struct VideoPacket {
    at: Instant,
    data: Vec<u8>,
    key: bool,
}

struct Video {
    packets: VecDeque<VideoPacket>,
    size: (u32, u32),
    fps: u32,
    kbps: u32,
}

/// Son reçu : l'échantillon stéréo n° `dropped + i` date de `origin + n / 48 000`.
#[derive(Default)]
struct Audio {
    origin: Option<Instant>,
    dropped: u64,
    samples: VecDeque<f32>,
}

impl Audio {
    fn push(&mut self, pcm: &[f32], keep: Duration) {
        if self.origin.is_none() {
            self.origin = Some(Instant::now() - Duration::from_secs_f64(pcm.len() as f64 / 2.0 / RATE as f64));
        }
        self.samples.extend(pcm.iter().copied());
        let max = (keep.as_secs_f64() * RATE as f64) as usize * 2;
        if self.samples.len() > max {
            let extra = (self.samples.len() - max) & !1;
            self.samples.drain(..extra);
            self.dropped += extra as u64 / 2;
        }
    }

    /// `frames` échantillons stéréo à partir de l'instant `from` (silence là où rien n'est connu).
    fn slice(&self, from: Instant, frames: usize) -> Vec<f32> {
        let mut out = vec![0f32; frames * 2];
        let Some(origin) = self.origin else { return out };
        let start = if from >= origin { ((from - origin).as_secs_f64() * RATE as f64) as i64 } else { -(((origin - from).as_secs_f64() * RATE as f64) as i64) };
        let first = start - self.dropped as i64;
        let have = (self.samples.len() / 2) as i64;
        for i in 0..frames as i64 {
            let k = first + i;
            if k >= 0 && k < have {
                out[i as usize * 2] = self.samples[k as usize * 2];
                out[i as usize * 2 + 1] = self.samples[k as usize * 2 + 1];
            }
        }
        out
    }
}

/// Retire les paquets trop anciens en gardant toujours une image clé en tête.
fn prune(packets: &mut VecDeque<VideoPacket>, limit: Instant) {
    loop {
        let Some(k) = packets.iter().skip(1).position(|p| p.key).map(|i| i + 1) else { break };
        if packets[k].at <= limit {
            packets.drain(..k);
        } else {
            break;
        }
    }
}

/// Paquets d'un clip de `seconds` s se terminant à `end` : depuis la dernière image clé
/// antérieure au début voulu (sinon la première disponible).
fn select(packets: &VecDeque<VideoPacket>, end: Instant, seconds: u32) -> Vec<VideoPacket> {
    let from = end.checked_sub(Duration::from_secs(seconds as u64)).unwrap_or(end);
    let start = packets.iter().rposition(|p| p.key && p.at <= from).or_else(|| packets.iter().position(|p| p.key));
    match start {
        Some(i) => packets.iter().skip(i).filter(|p| p.at <= end).cloned().collect(),
        None => Vec::new(),
    }
}

/// Mélange son de Windows et micro en PCM 16 bits, chacun à son volume.
fn mix(sources: &[(Vec<f32>, f32)], len: usize) -> Vec<i16> {
    (0..len)
        .map(|i| {
            let s: f32 = sources.iter().map(|(v, g)| v.get(i).copied().unwrap_or(0.0) * g).sum();
            (s.clamp(-1.0, 1.0) * 32767.0) as i16
        })
        .collect()
}

static SETTINGS: Mutex<Option<ClipSettings>> = Mutex::new(None);
/// Appareils de son choisis (sortie écoutée, micro) ; `None` : ceux par défaut de Windows.
static DEVICES: Mutex<(Option<String>, Option<String>)> = Mutex::new((None, None));
/// Change à chaque choix d'appareil : l'enregistrement repart sur le nouveau.
static DEVICES_GEN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Appareils choisis (sortie, micro).
pub fn devices() -> (Option<String>, Option<String>) {
    DEVICES.lock().map(|d| d.clone()).unwrap_or_default()
}

/// Choix des appareils de son (iClips) ; vide : celui par défaut.
pub fn set_devices(output: Option<String>, mic: Option<String>) {
    let clean = |d: Option<String>| d.filter(|s| !s.is_empty() && s.len() < 300);
    let next = (clean(output), clean(mic));
    if let Ok(mut d) = DEVICES.lock() {
        if *d != next {
            *d = next;
            DEVICES_GEN.fetch_add(1, Ordering::Relaxed);
        }
    }
}
static ACTIVE: AtomicBool = AtomicBool::new(false);
static VIDEO: Mutex<Option<Video>> = Mutex::new(None);
static SYSTEM: Mutex<Option<Audio>> = Mutex::new(None);
static MIC: Mutex<Option<Audio>> = Mutex::new(None);

pub fn settings() -> ClipSettings {
    SETTINGS.lock().ok().and_then(|s| *s).unwrap_or_default()
}

pub fn configure(s: ClipSettings) {
    if let Ok(mut g) = SETTINGS.lock() {
        *g = Some(s.sanitized());
    }
}

/// Enregistrement voulu (partie en cours et clips activés).
pub fn set_active(active: bool) {
    ACTIVE.store(active, Ordering::Relaxed);
}

/// Secondes déjà disponibles pour un clip.
pub fn available_seconds() -> u32 {
    let v = VIDEO.lock().ok();
    let Some(Some(video)) = v.as_deref() else { return 0 };
    match (video.packets.front(), video.packets.back()) {
        (Some(a), Some(b)) => ((b.at - a.at).as_secs() as u32).min(settings().seconds),
        _ => 0,
    }
}

fn clear() {
    if let Ok(mut v) = VIDEO.lock() { *v = None; }
    if let Ok(mut a) = SYSTEM.lock() { *a = None; }
    if let Ok(mut a) = MIC.lock() { *a = None; }
}

/// Veille : démarre ou arrête l'enregistrement selon la partie et les réglages.
pub fn start() {
    std::thread::spawn(|| {
        let mut running: Option<(ClipSettings, Arc<AtomicBool>, Vec<std::thread::JoinHandle<()>>)> = None;
        let mut generation = DEVICES_GEN.load(Ordering::Relaxed);
        loop {
            let want = ACTIVE.load(Ordering::Relaxed);
            let current = settings();
            let changed = DEVICES_GEN.load(Ordering::Relaxed) != generation;
            generation = DEVICES_GEN.load(Ordering::Relaxed);
            let stale = running.as_ref().is_some_and(|(s, _, _)| !want || *s != current || changed);
            if stale {
                if let Some((_, stop, threads)) = running.take() {
                    stop.store(true, Ordering::Relaxed);
                    for t in threads { let _ = t.join(); }
                }
                clear();
            }
            if want && running.is_none() {
                let stop = Arc::new(AtomicBool::new(false));
                running = Some((current, stop.clone(), session(current, stop)));
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });
}

#[cfg(windows)]
fn session(s: ClipSettings, stop: Arc<AtomicBool>) -> Vec<std::thread::JoinHandle<()>> {
    let mut threads = Vec::new();
    let v = stop.clone();
    threads.push(std::thread::spawn(move || video_loop(s, &v)));
    for (enabled, mic) in [(s.system_audio, false), (s.microphone, true)] {
        if !enabled { continue; }
        let a = stop.clone();
        threads.push(std::thread::spawn(move || audio_loop(s, mic, &a)));
    }
    threads
}

#[cfg(not(windows))]
fn session(_s: ClipSettings, _stop: Arc<AtomicBool>) -> Vec<std::thread::JoinHandle<()>> {
    Vec::new()
}

/// Attente interruptible.
fn pause(stop: &AtomicBool, d: Duration) {
    let end = Instant::now() + d;
    while !stop.load(Ordering::Relaxed) && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(windows)]
fn video_loop(s: ClipSettings, stop: &AtomicBool) {
    use crate::remote_play::capture::{Options, Target};
    use crate::remote_play::encode::pipeline::{stream, Control, Settings};
    use crate::remote_play::encode::{Preference, Profile};
    let (max_height, fps, kbps) = s.quality.params();
    let keep = s.keep();
    while !stop.load(Ordering::Relaxed) {
        let settings = Settings {
            target: Target::PrimaryMonitor,
            capture: Options { cursor: false, borderless: true, require_borderless: true },
            duration: Duration::from_secs(60 * 60 * 24 * 365),
            fps,
            bitrate_kbps: kbps,
            max_height: Some(max_height),
            preference: Preference::Auto,
            profile: Profile::High,
            keyframe_every: Some(fps),
        };
        let control = Control::new(kbps);
        let result = stream(
            &settings,
            &control,
            &mut |opened| {
                if let Ok(mut v) = VIDEO.lock() {
                    *v = Some(Video { packets: VecDeque::new(), size: opened.output, fps, kbps });
                }
            },
            &mut |p, origin| {
                let at = origin + Duration::from_nanos(p.pts.max(0) as u64 * 100);
                if let Ok(mut v) = VIDEO.lock() {
                    if let Some(video) = v.as_mut() {
                        video.packets.push_back(VideoPacket { at, data: p.data.clone(), key: p.keyframe });
                        if let Some(limit) = at.checked_sub(keep) { prune(&mut video.packets, limit); }
                    }
                }
                !stop.load(Ordering::Relaxed)
            },
        );
        if let Err(e) = result {
            eprintln!("[clips] image : {e}");
        }
        pause(stop, Duration::from_secs(3));
    }
}

#[cfg(windows)]
fn audio_loop(s: ClipSettings, mic: bool, stop: &AtomicBool) {
    let ring = if mic { &MIC } else { &SYSTEM };
    let keep = s.keep();
    let mut warned = false;
    while !stop.load(Ordering::Relaxed) {
        if let Ok(mut a) = ring.lock() { *a = Some(Audio::default()); }
        let (output, microphone) = devices();
        let device = if mic { microphone } else { output };
        let r = crate::remote_play::audio::pcm(stop, mic, device.as_deref(), &mut |pcm| {
            if let Ok(mut a) = ring.lock() {
                if let Some(a) = a.as_mut() { a.push(pcm, keep); }
            }
            !stop.load(Ordering::Relaxed)
        });
        if let Err(e) = r {
            if !warned { eprintln!("[clips] {e}"); warned = true; }
        }
        pause(stop, Duration::from_secs(5));
    }
}

/// Écrit le clip des dernières secondes dans `path` (MP4). Rend le nom des pistes de son :
/// `mix` d'abord, puis `game` (son de Windows) et `mic` si les pistes séparées sont demandées.
pub fn save(path: &std::path::Path) -> Result<Vec<&'static str>, String> {
    let s = settings();
    let end = Instant::now();
    let (packets, size, fps, kbps) = {
        let v = VIDEO.lock().map_err(|_| "Indisponible.")?;
        let video = v.as_ref().ok_or("Les clips démarrent avec la partie : rejouez quelques secondes.")?;
        (select(&video.packets, end, s.seconds), video.size, video.fps, video.kbps)
    };
    let (Some(first), Some(last)) = (packets.first(), packets.last()) else {
        return Err("Pas encore assez d'images : rejouez quelques secondes.".into());
    };
    let span = last.at - first.at + Duration::from_secs_f64(1.0 / fps as f64);
    if span < Duration::from_secs(1) {
        return Err("Pas encore assez d'images : rejouez quelques secondes.".into());
    }
    let frames = (span.as_secs_f64() * RATE as f64) as usize;
    let mut sources = Vec::new();
    for (on, ring, gain, name) in [(s.system_audio, &SYSTEM, s.system_volume, "game"), (s.microphone, &MIC, s.mic_volume, "mic")] {
        if !on { continue; }
        if let Some(a) = ring.lock().ok().as_ref().and_then(|a| a.as_ref()) {
            sources.push((a.slice(first.at, frames), gain, name));
        }
    }
    let (tracks, names) = audio_tracks(&sources, frames * 2, s.tracks);
    write_mp4(path, &packets, size, fps, kbps, &tracks[..tracks.len().min(1)])?;
    for (pcm, name) in tracks.iter().zip(&names).skip(1) {
        write_m4a(&track_file(path, name), pcm)?;
    }
    Ok(names)
}

/// Fichier d'une piste séparée d'un clip (`game`, `mic`) : invisible des listes de captures.
pub fn track_file(clip: &std::path::Path, name: &str) -> std::path::PathBuf {
    clip.with_extension(format!("{name}.m4a"))
}

#[cfg(windows)]
pub fn write_m4a(path: &std::path::Path, pcm: &[i16]) -> Result<(), String> {
    unsafe { mux::write_audio(path, pcm) }.map_err(|e| format!("Piste de son impossible à écrire : {e}"))
}

#[cfg(not(windows))]
pub fn write_m4a(_: &std::path::Path, _: &[i16]) -> Result<(), String> {
    Err("Clips disponibles sur Windows seulement.".into())
}

/// Pistes à écrire : le mélange, puis chaque source à part (son brut, volume 1) si demandé.
fn audio_tracks(sources: &[(Vec<f32>, f32, &'static str)], len: usize, separate: bool) -> (Vec<Vec<i16>>, Vec<&'static str>) {
    if sources.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let weighted: Vec<(Vec<f32>, f32)> = sources.iter().map(|(v, g, _)| (v.clone(), *g)).collect();
    let mut tracks = vec![mix(&weighted, len)];
    let mut names = vec!["mix"];
    if separate {
        for (v, _, name) in sources {
            tracks.push(mix(&[(v.clone(), 1.0)], len));
            names.push(name);
        }
    }
    (tracks, names)
}

#[cfg(windows)]
fn write_mp4(path: &std::path::Path, packets: &[VideoPacket], size: (u32, u32), fps: u32, kbps: u32, audio: &[Vec<i16>]) -> Result<(), String> {
    unsafe { mux::write(path, packets, size, fps, kbps, audio) }.map_err(|e| format!("Clip impossible à écrire : {e}"))
}

#[cfg(not(windows))]
fn write_mp4(_: &std::path::Path, _: &[VideoPacket], _: (u32, u32), _: u32, _: u32, _: &[Vec<i16>]) -> Result<(), String> {
    Err("Clips disponibles sur Windows seulement.".into())
}

#[cfg(windows)]
mod mux {
    use super::{VideoPacket, RATE};
    use crate::remote_play::encode::h264;
    use crate::remote_play::encode::mf::MfRuntime;
    use windows::core::{Result, HSTRING};
    use windows::Win32::Media::MediaFoundation::*;
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

    struct Com(bool);
    impl Drop for Com {
        fn drop(&mut self) {
            if self.0 { unsafe { CoUninitialize() }; }
        }
    }

    unsafe fn sample(bytes: &[u8], time: i64, duration: i64) -> Result<IMFSample> {
        let buffer = MFCreateMemoryBuffer(bytes.len() as u32)?;
        let mut ptr: *mut u8 = std::ptr::null_mut();
        buffer.Lock(&mut ptr, None, None)?;
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
        buffer.Unlock()?;
        buffer.SetCurrentLength(bytes.len() as u32)?;
        let s = MFCreateSample()?;
        s.AddBuffer(&buffer)?;
        s.SetSampleTime(time)?;
        s.SetSampleDuration(duration)?;
        Ok(s)
    }

    unsafe fn audio_type(subtype: &windows::core::GUID, aac: bool) -> Result<IMFMediaType> {
        let t = MFCreateMediaType()?;
        t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
        t.SetGUID(&MF_MT_SUBTYPE, subtype)?;
        t.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2)?;
        t.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, RATE as u32)?;
        t.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        if aac {
            t.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 24_000)?; // 192 kbit/s
        } else {
            t.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, 4)?;
            t.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, RATE as u32 * 4)?;
        }
        Ok(t)
    }

    pub unsafe fn write(path: &std::path::Path, packets: &[VideoPacket], size: (u32, u32), fps: u32, kbps: u32, audio: &[Vec<i16>]) -> Result<()> {
        let _com = Com(CoInitializeEx(None, COINIT_MULTITHREADED).is_ok());
        let _mf = MfRuntime::start()?;
        let mut attrs: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut attrs, 2)?;
        let attrs = attrs.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?;
        attrs.SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_MPEG4)?;
        attrs.SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1)?;
        let writer = MFCreateSinkWriterFromURL(&HSTRING::from(path.as_os_str()), None, &attrs)?;

        let video = MFCreateMediaType()?;
        video.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        video.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
        video.SetUINT64(&MF_MT_FRAME_SIZE, ((size.0 as u64) << 32) | size.1 as u64)?;
        video.SetUINT64(&MF_MT_FRAME_RATE, ((fps as u64) << 32) | 1)?;
        video.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1)?;
        video.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        video.SetUINT32(&MF_MT_AVG_BITRATE, kbps * 1000)?;
        let header = h264::parameter_sets(&packets[0].data);
        if !header.is_empty() {
            video.SetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, &header)?;
        }
        let vs = writer.AddStream(&video)?;
        writer.SetInputMediaType(vs, &video, None)?;

        // une piste AAC par entrée (mélange, puis pistes séparées)
        let mut audio_streams = Vec::new();
        for _ in audio {
            let s = writer.AddStream(&audio_type(&MFAudioFormat_AAC, true)?)?;
            writer.SetInputMediaType(s, &audio_type(&MFAudioFormat_PCM, false)?, None)?;
            audio_streams.push(s);
        }
        writer.BeginWriting()?;

        let t0 = packets[0].at;
        let frame = 10_000_000 / fps as i64;
        for (i, p) in packets.iter().enumerate() {
            let time = (p.at - t0).as_nanos() as i64 / 100;
            let duration = packets.get(i + 1).map(|n| (n.at - p.at).as_nanos() as i64 / 100).filter(|d| *d > 0).unwrap_or(frame);
            let s = sample(&p.data, time, duration)?;
            if p.key {
                s.SetUINT32(&MFSampleExtension_CleanPoint, 1)?;
            }
            writer.WriteSample(vs, &s)?;
        }
        for (&stream, pcm) in audio_streams.iter().zip(audio) {
            const CHUNK: usize = 1024; // échantillons stéréo par paquet
            for (k, chunk) in pcm.chunks(CHUNK * 2).enumerate() {
                let bytes: Vec<u8> = chunk.iter().flat_map(|v| v.to_le_bytes()).collect();
                let time = (k * CHUNK) as i64 * 10_000_000 / RATE as i64;
                let duration = (chunk.len() / 2) as i64 * 10_000_000 / RATE as i64;
                writer.WriteSample(stream, &sample(&bytes, time, duration)?)?;
            }
        }
        writer.Finalize()
    }

    /// Son seul (piste séparée d'un clip) : AAC stéréo 48 kHz dans un conteneur MP4 (.m4a).
    pub unsafe fn write_audio(path: &std::path::Path, pcm: &[i16]) -> Result<()> {
        let _com = Com(CoInitializeEx(None, COINIT_MULTITHREADED).is_ok());
        let _mf = MfRuntime::start()?;
        let mut attrs: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut attrs, 2)?;
        let attrs = attrs.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?;
        attrs.SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_MPEG4)?;
        attrs.SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1)?;
        let writer = MFCreateSinkWriterFromURL(&HSTRING::from(path.as_os_str()), None, &attrs)?;
        let s = writer.AddStream(&audio_type(&MFAudioFormat_AAC, true)?)?;
        writer.SetInputMediaType(s, &audio_type(&MFAudioFormat_PCM, false)?, None)?;
        writer.BeginWriting()?;
        const CHUNK: usize = 1024;
        for (k, chunk) in pcm.chunks(CHUNK * 2).enumerate() {
            let bytes: Vec<u8> = chunk.iter().flat_map(|v| v.to_le_bytes()).collect();
            let time = (k * CHUNK) as i64 * 10_000_000 / RATE as i64;
            let duration = (chunk.len() / 2) as i64 * 10_000_000 / RATE as i64;
            writer.WriteSample(s, &sample(&bytes, time, duration)?)?;
        }
        writer.Finalize()
    }

    /// Relit un MP4 (essais) : nombre d'images vidéo et de paquets audio décodables.
    #[cfg(test)]
    pub unsafe fn read_back(path: &std::path::Path) -> Result<(usize, usize)> {
        let _com = Com(CoInitializeEx(None, COINIT_MULTITHREADED).is_ok());
        let _mf = MfRuntime::start()?;
        let reader = MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), None)?;
        let (mut video, mut audio) = (0, 0);
        let mut streams = 0u32;
        for i in 0..4u32 {
            if reader.GetStreamSelection(i).is_err() { break; }
            let _ = reader.SetStreamSelection(i, true);
            streams += 1;
        }
        let mut ended = std::collections::HashSet::new();
        while ended.len() < streams as usize {
            let mut index = 0u32;
            let mut flags = 0u32;
            let mut s: Option<IMFSample> = None;
            reader.ReadSample(MF_SOURCE_READER_ANY_STREAM.0 as u32, 0, Some(&mut index), Some(&mut flags), None, Some(&mut s))?;
            if s.is_some() {
                let t = reader.GetCurrentMediaType(index)?;
                if t.GetGUID(&MF_MT_MAJOR_TYPE)? == MFMediaType_Video { video += 1 } else { audio += 1 }
            }
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                ended.insert(index);
            }
        }
        Ok((video, audio))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pk(at: Instant, key: bool) -> VideoPacket {
        VideoPacket { at, data: vec![key as u8], key }
    }

    /// Appareils de son de ce PC (choix dans iClips). `cargo test --lib appareils_reels -- --ignored --nocapture`
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn appareils_reels() {
        let outputs = crate::remote_play::audio::endpoints(false);
        let inputs = crate::remote_play::audio::endpoints(true);
        for e in outputs.iter().chain(&inputs) {
            println!("{} {}", if e.default { "*" } else { " " }, e.name);
        }
        assert!(!outputs.is_empty(), "aucune sortie de son");
        assert!(outputs.iter().filter(|e| e.default).count() <= 1);
    }

    #[test]
    fn appareils_choisis() {
        set_devices(Some("{0.0.0.00000000}.{abc}".into()), Some(String::new()));
        assert_eq!(devices(), (Some("{0.0.0.00000000}.{abc}".into()), None));
        let gen = DEVICES_GEN.load(Ordering::Relaxed);
        set_devices(Some("{0.0.0.00000000}.{abc}".into()), None);
        assert_eq!(DEVICES_GEN.load(Ordering::Relaxed), gen, "même choix : pas de redémarrage");
        set_devices(None, None);
        assert_eq!(devices(), (None, None));
    }

    #[test]
    fn reglages_bornes() {
        let s = ClipSettings { seconds: 400, system_volume: 9.0, mic_volume: f32::NAN, ..Default::default() }.sanitized();
        assert_eq!(s.seconds, 120);
        assert_eq!((s.system_volume, s.mic_volume), (2.0, 1.0));
        // anciens réglages (sans volumes ni pistes) : valeurs par défaut
        let old: ClipSettings = serde_json::from_str(r#"{"quality":"high","seconds":30,"systemAudio":true,"microphone":false}"#).unwrap();
        assert_eq!((old.system_volume, old.mic_volume, old.tracks), (1.0, 1.0, false));
        assert_eq!(Quality::Max.params(), (1440, 60, 20_000));
        assert_eq!(ClipSettings { seconds: 1, ..Default::default() }.sanitized().seconds, 5);
        let json = serde_json::to_string(&ClipSettings::default()).unwrap();
        assert!(json.contains("\"quality\":\"medium\"") && json.contains("\"systemAudio\":true"), "{json}");
    }

    #[test]
    fn tampon_video_garde_une_image_cle_en_tete() {
        let t0 = Instant::now();
        let mut q: VecDeque<VideoPacket> = (0..100).map(|i| pk(t0 + Duration::from_millis(100 * i), i % 10 == 0)).collect();
        prune(&mut q, t0 + Duration::from_millis(5_050));
        assert!(q[0].key);
        assert_eq!(q[0].at, t0 + Duration::from_millis(5_000));
        let clip = select(&q, t0 + Duration::from_millis(9_900), 3);
        assert!(clip[0].key && clip[0].at <= t0 + Duration::from_millis(6_900));
        assert_eq!(clip[0].at, t0 + Duration::from_millis(6_000));
        assert_eq!(clip.len(), 40);
    }

    #[test]
    fn son_aligne_sur_l_image() {
        let mut a = Audio::default();
        let keep = Duration::from_secs(2);
        a.push(&vec![0.5; 48_000 * 2], keep); // 1 s
        a.push(&vec![0.25; 48_000 * 2 * 2], keep); // 2 s de plus : la première seconde sort
        assert_eq!(a.dropped, 48_000);
        let origin = a.origin.unwrap();
        let s = a.slice(origin + Duration::from_millis(1_500), 10);
        assert!(s.iter().all(|v| (*v - 0.25).abs() < 1e-6));
        let before = a.slice(origin, 4);
        assert!(before.iter().all(|v| *v == 0.0), "seconde déjà sortie : silence");
        let m = mix(&[(vec![0.8, 0.8], 1.0), (vec![0.8, -0.1], 1.0)], 2);
        assert_eq!(m, vec![32767, 22936]);
        // volumes : le micro à moitié, le jeu coupé
        assert_eq!(mix(&[(vec![0.5], 0.0), (vec![0.5], 0.5)], 1), vec![8191]);
        // pistes : le mélange, puis chaque source à part, telle quelle
        let sources = vec![(vec![0.5f32, 0.5], 0.5, "game"), (vec![0.25f32, 0.25], 2.0, "mic")];
        let (tracks, names) = audio_tracks(&sources, 2, true);
        assert_eq!(names, vec!["mix", "game", "mic"]);
        assert_eq!(tracks[0][0], 24575);
        assert_eq!(tracks[1][0], 16383);
        assert_eq!(audio_tracks(&sources, 2, false).1, vec!["mix"]);
        assert!(audio_tracks(&[], 2, true).0.is_empty());
    }

    /// Clip synthétique comme ceux d'iBox (encodeur matériel, une image clé par seconde, son
    /// sinusoïdal en AAC), sans filmer l'écran : 10 s en 1280 × 720 à 30 i/s, pour les essais
    /// de montage (`clip_edit`). `IBOX_SYNTH_OUT=<fichier.mp4> cargo test --lib clips::tests::clip_synthetique -- --ignored`
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn clip_synthetique() {
        use crate::remote_play::encode::{mf::Encoder, EncoderConfig, Preference};
        use windows::Win32::Graphics::Direct3D11::D3D11_BIND_RENDER_TARGET;
        use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_NV12;
        let out = std::env::var("IBOX_SYNTH_OUT").expect("IBOX_SYNTH_OUT");
        let _ = unsafe { windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED) };
        let device = crate::remote_play::capture::d3d::Device::create().unwrap();
        let cfg = EncoderConfig { width: 1280, height: 720, fps: 30, gop: 0, ..Default::default() };
        let mut enc = Encoder::new(&device, cfg, Preference::Auto).unwrap();
        let texture = crate::remote_play::encode::convert::texture(&device.device, (1280, 720), DXGI_FORMAT_NV12, D3D11_BIND_RENDER_TARGET).unwrap();
        let t0 = Instant::now();
        let mut packets = Vec::new();
        for i in 0..300u32 {
            enc.submit(&texture, i as i64 * cfg.frame_duration_100ns(), i % 30 == 0).unwrap();
            while let Some(p) = enc.receive(Duration::from_millis(50)).unwrap() {
                packets.push(VideoPacket { at: t0 + Duration::from_nanos(p.pts as u64 * 100), data: p.data, key: p.keyframe });
            }
        }
        for p in enc.finish().unwrap() {
            packets.push(VideoPacket { at: t0 + Duration::from_nanos(p.pts as u64 * 100), data: p.data, key: p.keyframe });
        }
        // son : 440 Hz (« jeu ») ; avec IBOX_SYNTH_TRACKS, aussi 880 Hz (« micro ») et trois pistes
        let tone = |f: f64| -> Vec<i16> { (0..RATE as usize * 10).flat_map(|n| {
            let v = ((n as f64 * f * std::f64::consts::TAU / RATE as f64).sin() * 6000.0) as i16;
            [v, v]
        }).collect() };
        let game = tone(440.0);
        let separate = std::env::var("IBOX_SYNTH_TRACKS").is_ok();
        let mic = tone(880.0);
        let main = if separate { game.iter().zip(&mic).map(|(a, b)| a.saturating_add(*b)).collect() } else { game.clone() };
        unsafe { mux::write(std::path::Path::new(&out), &packets, (1280, 720), 30, 4000, &[main]) }.unwrap();
        if separate {
            write_m4a(&track_file(std::path::Path::new(&out), "game"), &game).unwrap();
            write_m4a(&track_file(std::path::Path::new(&out), "mic"), &mic).unwrap();
        }
        let (video, audio) = unsafe { mux::read_back(std::path::Path::new(&out)) }.unwrap();
        println!("{} paquets, {} images clés ; relu : {video} images, {audio} paquets audio", packets.len(), packets.iter().filter(|p| p.key).count());
        assert_eq!(video, 300);
    }

    /// Enregistrement réel (écran principal, son de Windows) puis relecture du MP4 :
    /// `cargo test --lib clips::tests::clip_reel -- --ignored --nocapture`
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn clip_reel() {
        configure(ClipSettings { seconds: 5, microphone: std::env::var("IBOX_MICRO").is_ok(), ..ClipSettings::default() });
        let stop = Arc::new(AtomicBool::new(false));
        let threads = session(settings(), stop.clone());
        std::thread::sleep(Duration::from_secs(7));
        println!("disponible : {} s", available_seconds());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("essai clip é.mp4");
        save(&path).unwrap();
        stop.store(true, Ordering::Relaxed);
        for t in threads { t.join().unwrap(); }
        let size = std::fs::metadata(&path).unwrap().len();
        let (video, audio) = unsafe { mux::read_back(&path) }.unwrap();
        println!("{size} octets, {video} images, {audio} paquets audio");
        if let Ok(out) = std::env::var("IBOX_CLIP_OUT") { std::fs::copy(&path, out).unwrap(); }
        assert!(video >= 5 * 25, "au moins 5 s d'images");
        assert!(audio > 100, "son présent");
    }
}
