//! Encodeur H.264 Media Foundation.
//!
//! - **Matériel** : MFT asynchrone (NVENC, AMF, Quick Sync) piloté par ses événements
//!   `METransformNeedInput` / `METransformHaveOutput`, lus sur un fil dédié ; les textures NV12 lui
//!   sont passées directement (gestionnaire de device DXGI), sans copie par le processeur.
//! - **Logiciel** : MFT synchrone « Microsoft H264 Encoder », en dernier recours.
//!
//! Chaque candidat est réellement ouvert puis essayé sur une image : un MFT simplement enregistré
//! (par exemple celui d'AMD sur un PC sans carte AMD) n'est jamais retenu sur la foi de la liste.

use std::collections::VecDeque;
use std::mem::ManuallyDrop;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows::core::{Error, Interface, Result, GUID, HRESULT, PWSTR};
use windows::Win32::Foundation::{E_FAIL, E_UNEXPECTED};
use windows::Win32::Graphics::Direct3D11::{ID3D11Texture2D, D3D11_BIND_RENDER_TARGET};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_NV12;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_INPROC_SERVER};
use windows::Win32::System::Variant::VARIANT;

use crate::remote_play::capture::d3d::Device;

use super::{h264, parse_vendor, EncoderConfig, Preference};

/// Résultat de l'essai d'un encodeur candidat.
#[derive(Debug, Clone)]
pub struct Attempt {
    pub name: String,
    pub hardware: bool,
    pub vendor_id: Option<u32>,
    pub result: std::result::Result<(), String>,
}

#[derive(Debug, Clone, Default)]
pub struct EncoderInfo {
    pub name: String,
    pub hardware: bool,
    pub asynchronous: bool,
    /// Réglages acceptés par l'encodeur (`ICodecAPI`).
    pub low_latency: bool,
    pub cbr: bool,
    pub gop: bool,
    pub attempts: Vec<Attempt>,
}

#[derive(Debug, Clone)]
pub struct Packet {
    /// Image H.264 en Annex B ; une IDR commence toujours par ses SPS/PPS.
    pub data: Vec<u8>,
    pub pts: i64,
    pub keyframe: bool,
    /// Envoi à l'encodeur → paquet disponible.
    pub encode_time: Option<Duration>,
    /// Image capturée → envoi à l'encodeur (attente de cadence), renseignée par `pipeline`.
    pub waited: Duration,
}

/// Démarre Media Foundation pour la durée de vie de l'objet (compteur interne à MF).
pub struct MfRuntime;

/// Media Foundation est-il présent ? Absent des éditions « N » de Windows (Europe) tant que
/// le Media Feature Pack n'est pas installé. Ses DLL sont chargées à la demande (`/DELAYLOAD`,
/// build.rs) : sans ce contrôle, le premier appel ferait tomber iBox.
pub fn media_foundation_available() -> bool {
    static PRESENT: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *PRESENT.get_or_init(|| {
        use windows_sys::Win32::System::LibraryLoader::{LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};
        ["mfplat.dll", "mfreadwrite.dll"].iter().all(|name| {
            let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
            // la bibliothèque reste chargée : le chargement différé la retrouvera
            !unsafe { LoadLibraryExW(wide.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32) }.is_null()
        })
    })
}

/// Message montré quand Media Foundation manque (clips, Remote Play, diagnostic).
pub const MEDIA_FOUNDATION_MISSING: &str =
    "Media Foundation est absent de ce Windows (édition N) : installez le « Media Feature Pack » depuis les fonctionnalités facultatives de Windows.";

impl MfRuntime {
    pub fn start() -> Result<MfRuntime> {
        if !media_foundation_available() {
            return Err(windows::core::Error::new(windows::Win32::Foundation::E_NOTIMPL, MEDIA_FOUNDATION_MISSING));
        }
        unsafe { MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET)? };
        Ok(MfRuntime)
    }
}

impl Drop for MfRuntime {
    fn drop(&mut self) {
        let _ = unsafe { MFShutdown() };
    }
}

struct Sendable<T>(T);
// SAFETY : le générateur d'événements d'un MFT asynchrone est libre de fil (contrat MF).
unsafe impl<T> Send for Sendable<T> {}

#[derive(Debug, Clone, Copy)]
enum AsyncEvent {
    NeedInput,
    HaveOutput,
    DrainComplete,
    Failed(HRESULT),
}

enum Mode {
    Async { events: Receiver<AsyncEvent>, thread: Option<JoinHandle<()>>, need_input: u32 },
    Sync,
}

pub(crate) enum Output {
    Sample(IMFSample),
    NeedMoreInput,
    StreamChanged,
}

/// Un appel à `ProcessOutput` (flux 0), en fournissant l'échantillon si le MFT ne le fait pas.
pub(crate) unsafe fn process_output(transform: &IMFTransform, fallback_size: u32) -> Result<Output> {
    let info = transform.GetOutputStreamInfo(0)?;
    let provides = info.dwFlags & (MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 | MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES.0) as u32 != 0;
    let sample = if provides {
        None
    } else {
        let s = MFCreateSample()?;
        s.AddBuffer(&MFCreateMemoryBuffer(info.cbSize.max(fallback_size))?)?;
        Some(s)
    };
    let mut buffers = [MFT_OUTPUT_DATA_BUFFER { dwStreamID: 0, pSample: ManuallyDrop::new(sample), dwStatus: 0, pEvents: ManuallyDrop::new(None) }];
    let mut status = 0u32;
    let result = transform.ProcessOutput(0, &mut buffers, &mut status);
    let sample = ManuallyDrop::take(&mut buffers[0].pSample);
    drop(ManuallyDrop::take(&mut buffers[0].pEvents));
    match result {
        Ok(()) => Ok(sample.map(Output::Sample).unwrap_or(Output::NeedMoreInput)),
        Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => Ok(Output::NeedMoreInput),
        Err(e) if e.code() == MF_E_TRANSFORM_STREAM_CHANGE => Ok(Output::StreamChanged),
        Err(e) => Err(e),
    }
}

pub(crate) unsafe fn sample_bytes(sample: &IMFSample) -> Result<Vec<u8>> {
    let buffer = sample.ConvertToContiguousBuffer()?;
    let mut ptr = std::ptr::null_mut();
    let mut len = 0u32;
    buffer.Lock(&mut ptr, None, Some(&mut len))?;
    let bytes = if ptr.is_null() { Vec::new() } else { std::slice::from_raw_parts(ptr, len as usize).to_vec() };
    let _ = buffer.Unlock();
    Ok(bytes)
}

pub(crate) unsafe fn video_type(subtype: GUID, cfg: &EncoderConfig) -> Result<IMFMediaType> {
    let t = MFCreateMediaType()?;
    t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
    t.SetGUID(&MF_MT_SUBTYPE, &subtype)?;
    t.SetUINT64(&MF_MT_FRAME_SIZE, ((cfg.width as u64) << 32) | cfg.height as u64)?;
    t.SetUINT64(&MF_MT_FRAME_RATE, ((cfg.fps as u64) << 32) | 1)?;
    t.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1)?;
    t.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
    // la conversion produit du BT.709 plage limitée (convert.rs) : on l'annonce
    t.SetUINT32(&MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709.0 as u32)?;
    t.SetUINT32(&MF_MT_VIDEO_PRIMARIES, MFVideoPrimaries_BT709.0 as u32)?;
    t.SetUINT32(&MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_709.0 as u32)?;
    t.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32)?;
    Ok(t)
}

unsafe fn activate_string(activate: &IMFActivate, key: &GUID) -> Option<String> {
    let mut text = PWSTR::null();
    let mut len = 0u32;
    activate.GetAllocatedString(key, &mut text, &mut len).ok()?;
    let s = text.to_string().ok();
    CoTaskMemFree(Some(text.0 as *const _));
    s
}

/// Lit un tableau d'`IMFActivate` rendu par `MFTEnumEx` / `MFTEnum2`, puis le libère.
unsafe fn take_activates(array: *mut Option<IMFActivate>, count: u32) -> Vec<(IMFActivate, String, Option<u32>)> {
    let mut out = Vec::new();
    if array.is_null() {
        return out;
    }
    for i in 0..count as usize {
        if let Some(activate) = std::ptr::read(array.add(i)) {
            let name = activate_string(&activate, &MFT_FRIENDLY_NAME_Attribute).unwrap_or_else(|| "MFT matériel".into());
            let vendor = activate_string(&activate, &MFT_ENUM_HARDWARE_VENDOR_ID_Attribute).and_then(|v| parse_vendor(&v));
            out.push((activate, name, vendor));
        }
    }
    CoTaskMemFree(Some(array as *const _));
    out
}

/// MFT matériels qui encodent du NV12 en H.264 : d'abord ceux rattachés à la carte graphique de
/// notre device (`MFTEnum2` + LUID : indispensable sur les PC à deux GPU, Intel + NVIDIA ou AMD,
/// où le MFT d'une autre carte ne peut pas lire nos textures), puis les autres, ceux du fabricant
/// de notre GPU en tête. NVENC, AMF (AMD) et Quick Sync (Intel) passent tous par ce chemin.
unsafe fn hardware_candidates(device: &Device) -> Vec<(IMFActivate, String, Option<u32>)> {
    let input = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Video, guidSubtype: MFVideoFormat_NV12 };
    let output = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Video, guidSubtype: MFVideoFormat_H264 };
    let flags = MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER;

    let mut out = Vec::new();
    // 1. MFT de notre adaptateur (Windows 10 1703 et plus)
    let mut attrs = None;
    if MFCreateAttributes(&mut attrs, 1).is_ok() {
        if let Some(attrs) = attrs {
            let luid = device.adapter_luid;
            let bytes = std::slice::from_raw_parts(&luid as *const _ as *const u8, std::mem::size_of_val(&luid));
            if attrs.SetBlob(&MFT_ENUM_ADAPTER_LUID, bytes).is_ok() {
                let mut array: *mut Option<IMFActivate> = std::ptr::null_mut();
                let mut count = 0u32;
                if MFTEnum2(MFT_CATEGORY_VIDEO_ENCODER, flags, Some(&input), Some(&output), &attrs, &mut array, &mut count).is_ok() {
                    out = take_activates(array, count);
                }
            }
        }
    }
    // 2. Tous les MFT matériels enregistrés (repli : ancien Windows, pilote sans LUID)
    let mut array: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut count = 0u32;
    if MFTEnumEx(MFT_CATEGORY_VIDEO_ENCODER, flags, Some(&input), Some(&output), &mut array, &mut count).is_ok() {
        let mut rest = take_activates(array, count);
        rest.retain(|(_, name, _)| !out.iter().any(|(_, n, _)| n == name));
        rest.sort_by_key(|(_, _, vendor)| *vendor != Some(device.vendor_id));
        out.extend(rest);
    }
    out
}

/// Préréglage des encodeurs matériels (0 = le plus rapide, 100 = la meilleure qualité).
const QUALITY_VS_SPEED: u32 = 20;

/// Tampon VBV (bits) d'une seule image : chaque image tient dans ce que le débit écoule en un
/// intervalle, donc aucune ne retarde les suivantes sur le réseau (méthode Sunshine/Moonlight).
pub fn vbv_bits(kbps: u32, fps: u32) -> u32 {
    (kbps.saturating_mul(1000) / fps.max(1)).max(8_000)
}

pub struct Encoder {
    transform: IMFTransform,
    codec: Option<ICodecAPI>,
    activate: Option<IMFActivate>,
    mode: Mode,
    cfg: EncoderConfig,
    info: EncoderInfo,
    sequence_header: Vec<u8>,
    pending: VecDeque<Packet>,
    in_flight: VecDeque<(i64, Instant)>,
    keyframe_requested: bool,
    output_hint: u32,
    _manager: Option<IMFDXGIDeviceManager>,
    _mf: MfRuntime,
}

fn describe(e: &Error) -> String {
    let msg = e.message();
    if msg.is_empty() {
        format!("{:#010X}", e.code().0)
    } else {
        format!("{:#010X} {}", e.code().0, msg.trim())
    }
}

impl Encoder {
    /// Ouvre le premier encodeur qui fonctionne vraiment, selon la préférence.
    pub fn new(device: &Device, cfg: EncoderConfig, preference: Preference) -> Result<Encoder> {
        let cfg = cfg.sanitized();
        let _mf = MfRuntime::start()?;
        let manager = unsafe {
            let mut token = 0u32;
            let mut manager = None;
            MFCreateDXGIDeviceManager(&mut token, &mut manager)?;
            let manager = manager.ok_or_else(Error::empty)?;
            manager.ResetDevice(&device.device, token)?;
            manager
        };
        let mut attempts = Vec::new();
        if preference != Preference::SoftwareOnly {
            for (activate, name, vendor) in unsafe { hardware_candidates(device) } {
                let opened = unsafe { activate.ActivateObject::<IMFTransform>() }
                    .and_then(|t| Encoder::open(device, &manager, t, Some(activate.clone()), cfg, true, &name));
                match opened {
                    Ok(mut enc) => {
                        attempts.push(Attempt { name, hardware: true, vendor_id: vendor, result: Ok(()) });
                        enc.info.attempts = attempts;
                        return Ok(enc);
                    }
                    Err(e) => {
                        let _ = unsafe { activate.ShutdownObject() };
                        attempts.push(Attempt { name, hardware: true, vendor_id: vendor, result: Err(describe(&e)) });
                    }
                }
            }
        }
        if preference != Preference::HardwareOnly {
            let name = "Microsoft H264 Encoder MFT (logiciel)".to_string();
            let opened = unsafe { CoCreateInstance::<_, IMFTransform>(&CLSID_MSH264EncoderMFT, None, CLSCTX_INPROC_SERVER) }
                .and_then(|t| Encoder::open(device, &manager, t, None, cfg, false, &name));
            match opened {
                Ok(mut enc) => {
                    attempts.push(Attempt { name, hardware: false, vendor_id: None, result: Ok(()) });
                    enc.info.attempts = attempts;
                    return Ok(enc);
                }
                Err(e) => attempts.push(Attempt { name, hardware: false, vendor_id: None, result: Err(describe(&e)) }),
            }
        }
        let summary: Vec<String> = attempts.iter().map(|a| format!("{} : {}", a.name, a.result.as_ref().err().map(String::as_str).unwrap_or("?"))).collect();
        Err(Error::new(E_FAIL, format!("aucun encodeur H.264 utilisable ({})", summary.join(" ; "))))
    }

    /// Essaie chaque encodeur candidat (matériels enregistrés puis logiciel) sur une vraie image.
    pub fn probe_all(device: &Device, cfg: EncoderConfig) -> Vec<Attempt> {
        let mut out = Vec::new();
        let Ok(_mf) = MfRuntime::start() else { return out };
        let Ok(manager) = (unsafe {
            let mut token = 0u32;
            let mut manager = None;
            MFCreateDXGIDeviceManager(&mut token, &mut manager).and_then(|_| {
                let m: IMFDXGIDeviceManager = manager.ok_or_else(Error::empty)?;
                m.ResetDevice(&device.device, token)?;
                Ok(m)
            })
        }) else {
            return out;
        };
        let cfg = cfg.sanitized();
        for (activate, name, vendor) in unsafe { hardware_candidates(device) } {
            let result = unsafe { activate.ActivateObject::<IMFTransform>() }
                .and_then(|t| Encoder::open(device, &manager, t, Some(activate.clone()), cfg, true, &name))
                .map(drop)
                .map_err(|e| describe(&e));
            if result.is_err() {
                let _ = unsafe { activate.ShutdownObject() };
            }
            out.push(Attempt { name, hardware: true, vendor_id: vendor, result });
        }
        let name = "Microsoft H264 Encoder MFT (logiciel)".to_string();
        let result = unsafe { CoCreateInstance::<_, IMFTransform>(&CLSID_MSH264EncoderMFT, None, CLSCTX_INPROC_SERVER) }
            .and_then(|t| Encoder::open(device, &manager, t, None, cfg, false, &name))
            .map(drop)
            .map_err(|e| describe(&e));
        out.push(Attempt { name, hardware: false, vendor_id: None, result });
        out
    }

    fn open(
        device: &Device,
        manager: &IMFDXGIDeviceManager,
        transform: IMFTransform,
        activate: Option<IMFActivate>,
        cfg: EncoderConfig,
        hardware: bool,
        name: &str,
    ) -> Result<Encoder> {
        let mf = MfRuntime::start()?;
        unsafe {
            let attributes = transform.GetAttributes().ok();
            let asynchronous = attributes.as_ref().and_then(|a| a.GetUINT32(&MF_TRANSFORM_ASYNC).ok()) == Some(1);
            if asynchronous {
                attributes.as_ref().ok_or_else(Error::empty)?.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1)?;
            }
            // les encodeurs matériels lisent les textures NV12 sur la carte graphique
            let d3d = transform.ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, manager.as_raw() as usize);
            // mode faible latence au niveau du MFT (AMF et Quick Sync le lisent ici, pas via ICodecAPI)
            if let Some(a) = attributes.as_ref() {
                let _ = a.SetUINT32(&MF_LOW_LATENCY, 1);
            }
            if hardware {
                d3d?;
            }

            let codec = transform.cast::<ICodecAPI>().ok();
            let set = |api: &GUID, value: VARIANT| codec.as_ref().is_some_and(|c| c.SetValue(api, &value).is_ok());
            let cbr = set(&CODECAPI_AVEncCommonRateControlMode, VARIANT::from(eAVEncCommonRateControlMode_CBR.0 as u32))
                & set(&CODECAPI_AVEncCommonMeanBitRate, VARIANT::from(cfg.bitrate_kbps * 1000));
            let low_latency = set(&CODECAPI_AVLowLatencyMode, VARIANT::from(true));
            set(&CODECAPI_AVEncMPVDefaultBPictureCount, VARIANT::from(0u32));
            // gop = 0 : pas d'IDR périodique (une heure d'images), seulement à la demande
            let gop = if cfg.gop > 0 { cfg.gop } else { cfg.fps * 3600 };
            let gop_applied = set(&CODECAPI_AVEncMPVGOPSize, VARIANT::from(gop));
            // faible latence à la Sunshine/Moonlight : tampon VBV d'une seule image (aucune image
            // ne dépasse ce que le lien écoule en un intervalle), préréglage rapide, 1 référence
            if hardware {
                set(&CODECAPI_AVEncCommonBufferSize, VARIANT::from(vbv_bits(cfg.bitrate_kbps, cfg.fps)));
                set(&CODECAPI_AVEncCommonQualityVsSpeed, VARIANT::from(QUALITY_VS_SPEED));
                set(&CODECAPI_AVEncVideoMaxNumRefFrame, VARIANT::from(1u32));
            }

            // le type de sortie se fixe avant celui d'entrée (règle des encodeurs MF). Certains
            // encodeurs (AMF, Quick Sync anciens) refusent un profil : on descend vers un profil
            // plus simple, que tout décodeur du profil négocié sait lire (High ⊃ Main ⊃ Baseline).
            let mut cfg = cfg;
            let mut out_err = None;
            for &profile in super::Profile::fallbacks(cfg.profile) {
                let output = video_type(MFVideoFormat_H264, &cfg)?;
                output.SetUINT32(&MF_MT_AVG_BITRATE, cfg.bitrate_kbps * 1000)?;
                output.SetUINT32(&MF_MT_MPEG2_PROFILE, profile.mf_value())?;
                match transform.SetOutputType(0, &output, 0) {
                    Ok(()) => {
                        cfg.profile = profile;
                        out_err = None;
                        break;
                    }
                    Err(e) => out_err = Some(e),
                }
            }
            if let Some(e) = out_err {
                return Err(e);
            }
            let input = video_type(MFVideoFormat_NV12, &cfg)?;
            if transform.SetInputType(0, &input, 0).is_err() {
                // certains encodeurs n'acceptent que leurs propres types : on complète le leur
                let mut accepted = false;
                for i in 0.. {
                    let Ok(t) = transform.GetInputAvailableType(0, i) else { break };
                    if t.GetGUID(&MF_MT_SUBTYPE).ok() != Some(MFVideoFormat_NV12) {
                        continue;
                    }
                    t.SetUINT64(&MF_MT_FRAME_SIZE, ((cfg.width as u64) << 32) | cfg.height as u64)?;
                    t.SetUINT64(&MF_MT_FRAME_RATE, ((cfg.fps as u64) << 32) | 1)?;
                    if transform.SetInputType(0, &t, 0).is_ok() {
                        accepted = true;
                        break;
                    }
                }
                if !accepted {
                    return Err(Error::new(MF_E_INVALIDMEDIATYPE, "NV12 refusé en entrée"));
                }
            }

            // Quick Sync et certains pilotes AMD remettent leurs réglages par défaut en fixant les
            // types : on les réapplique (sans échec si refusés à ce stade)
            if hardware {
                set(&CODECAPI_AVEncCommonRateControlMode, VARIANT::from(eAVEncCommonRateControlMode_CBR.0 as u32));
                set(&CODECAPI_AVEncCommonMeanBitRate, VARIANT::from(cfg.bitrate_kbps * 1000));
                set(&CODECAPI_AVLowLatencyMode, VARIANT::from(true));
                set(&CODECAPI_AVEncMPVDefaultBPictureCount, VARIANT::from(0u32));
                set(&CODECAPI_AVEncMPVGOPSize, VARIANT::from(gop));
                set(&CODECAPI_AVEncCommonBufferSize, VARIANT::from(vbv_bits(cfg.bitrate_kbps, cfg.fps)));
                set(&CODECAPI_AVEncCommonQualityVsSpeed, VARIANT::from(QUALITY_VS_SPEED));
            }

            let _ = transform.ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0); // refusé par certains MFT avant le début du flux
            transform.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
            transform.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)?;

            let mode = if asynchronous {
                let generator = Sendable(transform.cast::<IMFMediaEventGenerator>()?);
                let (tx, events) = channel();
                let thread = std::thread::Builder::new()
                    .name("rp-encoder-events".into())
                    .spawn(move || {
                        // ce fil annonce chaque image encodée : en priorité normale, il attendait
                        // son tour derrière le jeu sur un processeur chargé
                        if !crate::remote_play::lever_off("events") {
                            crate::remote_play::latency_critical_thread();
                        }
                        let generator = generator;
                        loop {
                            let event = match generator.0.GetEvent(MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS(0)) {
                                Ok(ev) => match MF_EVENT_TYPE(ev.GetType().unwrap_or(0) as i32) {
                                    t if t == METransformNeedInput => AsyncEvent::NeedInput,
                                    t if t == METransformHaveOutput => AsyncEvent::HaveOutput,
                                    t if t == METransformDrainComplete => AsyncEvent::DrainComplete,
                                    t if t == MEError => AsyncEvent::Failed(ev.GetStatus().unwrap_or(E_FAIL)),
                                    _ => continue,
                                },
                                Err(e) => {
                                    if e.code() != MF_E_SHUTDOWN {
                                        let _ = tx.send(AsyncEvent::Failed(e.code()));
                                    }
                                    break;
                                }
                            };
                            if tx.send(event).is_err() {
                                break;
                            }
                        }
                    })
                    .map_err(|_| Error::from(E_UNEXPECTED))?;
                Mode::Async { events, thread: Some(thread), need_input: 0 }
            } else {
                Mode::Sync
            };

            let mut encoder = Encoder {
                sequence_header: Vec::new(),
                transform,
                codec,
                activate,
                mode,
                cfg,
                info: EncoderInfo { name: name.to_owned(), hardware, asynchronous, low_latency, cbr, gop: gop_applied, attempts: Vec::new() },
                pending: VecDeque::new(),
                in_flight: VecDeque::new(),
                keyframe_requested: true,
                output_hint: cfg.width * cfg.height * 3 / 2,
                _manager: Some(manager.clone()),
                _mf: mf,
            };
            encoder.refresh_sequence_header();
            encoder.self_test(device)?;
            Ok(encoder)
        }
    }

    /// Encode une image noire et attend le paquet : prouve que l'encodeur fonctionne vraiment
    /// sur ce device (un MFT d'une autre marque échoue ici).
    fn self_test(&mut self, device: &Device) -> Result<()> {
        let texture = super::convert::texture(&device.device, (self.cfg.width, self.cfg.height), DXGI_FORMAT_NV12, D3D11_BIND_RENDER_TARGET)?;
        // un encodeur peut demander quelques images avant de produire la première
        for i in 0..4 {
            self.submit(&texture, i * self.cfg.frame_duration_100ns(), i == 0)?;
            if self.receive(Duration::from_millis(1500))?.is_some() {
                self.pending.clear();
                self.in_flight.clear();
                self.keyframe_requested = true; // la vraie première image sera une IDR
                return Ok(());
            }
        }
        Err(Error::new(E_FAIL, "aucune image encodée"))
    }

    pub fn info(&self) -> &EncoderInfo {
        &self.info
    }

    pub fn config(&self) -> &EncoderConfig {
        &self.cfg
    }

    /// La prochaine image sera une IDR (nouvel invité, perte signalée par PLI/FIR…).
    pub fn request_keyframe(&mut self) {
        self.keyframe_requested = true;
    }

    /// Change le débit moyen en cours d'encodage (adaptation au réseau, Phase 2). Renvoie
    /// `false` si l'encodeur refuse : le débit précédent reste alors en vigueur.
    pub fn set_bitrate(&mut self, kbps: u32) -> bool {
        let kbps = kbps.clamp(crate::remote_play::config::BITRATE_RANGE_KBPS.0, crate::remote_play::config::BITRATE_RANGE_KBPS.1);
        let ok = self
            .codec
            .as_ref()
            .is_some_and(|c| unsafe { c.SetValue(&CODECAPI_AVEncCommonMeanBitRate, &VARIANT::from(kbps * 1000)) }.is_ok());
        if ok {
            if self.info.hardware {
                if let Some(c) = self.codec.as_ref() {
                    let _ = unsafe { c.SetValue(&CODECAPI_AVEncCommonBufferSize, &VARIANT::from(vbv_bits(kbps, self.cfg.fps))) };
                }
            }
            self.cfg.bitrate_kbps = kbps;
        }
        ok
    }

    fn refresh_sequence_header(&mut self) {
        unsafe {
            let Ok(t) = self.transform.GetOutputCurrentType(0) else { return };
            let Ok(size) = t.GetBlobSize(&MF_MT_MPEG_SEQUENCE_HEADER) else { return };
            let mut blob = vec![0u8; size as usize];
            if t.GetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, &mut blob, None).is_ok() {
                let sets = h264::sequence_header_to_annexb(&blob);
                if !sets.is_empty() {
                    self.sequence_header = sets;
                }
            }
        }
    }

    /// Envoie une texture NV12 (taille de la configuration) à l'encodeur.
    pub fn submit(&mut self, texture: &ID3D11Texture2D, pts: i64, keyframe: bool) -> Result<()> {
        unsafe {
            if keyframe || self.keyframe_requested {
                if let Some(codec) = &self.codec {
                    let _ = codec.SetValue(&CODECAPI_AVEncVideoForceKeyFrame, &VARIANT::from(1u32));
                }
                self.keyframe_requested = false;
            }
            let buffer = MFCreateDXGISurfaceBuffer(&ID3D11Texture2D::IID, texture, 0, false)?;
            if let Ok(len) = buffer.cast::<IMF2DBuffer>().and_then(|b| b.GetContiguousLength()) {
                buffer.SetCurrentLength(len)?;
            }
            let sample = MFCreateSample()?;
            sample.AddBuffer(&buffer)?;
            sample.SetSampleTime(pts)?;
            sample.SetSampleDuration(self.cfg.frame_duration_100ns())?;

            if matches!(self.mode, Mode::Async { .. }) {
                self.pump(Duration::ZERO)?;
                let deadline = Instant::now() + Duration::from_secs(2);
                while matches!(self.mode, Mode::Async { need_input: 0, .. }) {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Err(Error::new(E_FAIL, "l'encodeur ne demande plus d'image"));
                    }
                    self.pump(left)?;
                }
                if let Mode::Async { need_input, .. } = &mut self.mode {
                    *need_input -= 1;
                }
                self.in_flight.push_back((pts, Instant::now()));
                self.transform.ProcessInput(0, &sample, 0)?;
            } else {
                self.in_flight.push_back((pts, Instant::now()));
                loop {
                    match self.transform.ProcessInput(0, &sample, 0) {
                        Ok(()) => break,
                        Err(e) if e.code() == MF_E_NOTACCEPTING => {
                            if self.drain_sync()? == 0 {
                                return Err(e);
                            }
                        }
                        Err(e) => return Err(e),
                    }
                }
                self.drain_sync()?;
            }
        }
        Ok(())
    }

    /// Prochain paquet, en attendant au plus `timeout`.
    pub fn receive(&mut self, timeout: Duration) -> Result<Option<Packet>> {
        if let Some(p) = self.pending.pop_front() {
            return Ok(Some(p));
        }
        if matches!(self.mode, Mode::Async { .. }) {
            let deadline = Instant::now() + timeout;
            while self.pending.is_empty() {
                let left = deadline.saturating_duration_since(Instant::now());
                if !self.pump(left)? && left.is_zero() {
                    break;
                }
            }
        }
        Ok(self.pending.pop_front())
    }

    /// Vide l'encodeur en fin de flux et renvoie les derniers paquets.
    pub fn finish(&mut self) -> Result<Vec<Packet>> {
        unsafe {
            self.transform.ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0)?;
            self.transform.ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0)?;
        }
        match self.mode {
            Mode::Sync => {
                self.drain_sync()?;
            }
            Mode::Async { .. } => {
                let deadline = Instant::now() + Duration::from_secs(2);
                loop {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        break;
                    }
                    if let Ok(AsyncEvent::DrainComplete) = self.next_event(left) {
                        break;
                    }
                }
            }
        }
        Ok(self.pending.drain(..).collect())
    }

    fn next_event(&mut self, timeout: Duration) -> Result<AsyncEvent> {
        let Mode::Async { events, .. } = &self.mode else { return Err(Error::from(E_UNEXPECTED)) };
        let event = if timeout.is_zero() {
            events.try_recv().map_err(|_| Error::from(MF_E_NO_EVENTS_AVAILABLE))?
        } else {
            match events.recv_timeout(timeout) {
                Ok(ev) => ev,
                Err(RecvTimeoutError::Timeout) => return Err(Error::from(MF_E_NO_EVENTS_AVAILABLE)),
                Err(RecvTimeoutError::Disconnected) => return Err(Error::new(MF_E_SHUTDOWN, "encodeur arrêté")),
            }
        };
        match event {
            AsyncEvent::NeedInput => {
                if let Mode::Async { need_input, .. } = &mut self.mode {
                    *need_input += 1;
                }
            }
            AsyncEvent::HaveOutput => {
                self.pull_once()?;
            }
            AsyncEvent::DrainComplete => {}
            AsyncEvent::Failed(hr) => return Err(Error::new(hr, "erreur de l'encodeur")),
        }
        Ok(event)
    }

    /// Traite les événements en attente (et en attend un au plus `timeout`). Vrai si un événement
    /// a été traité.
    fn pump(&mut self, timeout: Duration) -> Result<bool> {
        let mut handled = false;
        let mut wait = timeout;
        loop {
            match self.next_event(wait) {
                Ok(_) => {
                    handled = true;
                    wait = Duration::ZERO;
                }
                Err(e) if e.code() == MF_E_NO_EVENTS_AVAILABLE => return Ok(handled),
                Err(e) => return Err(e),
            }
        }
    }

    fn drain_sync(&mut self) -> Result<usize> {
        let mut n = 0;
        loop {
            match unsafe { process_output(&self.transform, self.output_hint)? } {
                Output::Sample(s) => {
                    self.accept(s)?;
                    n += 1;
                }
                Output::StreamChanged => self.renegotiate()?,
                Output::NeedMoreInput => return Ok(n),
            }
        }
    }

    fn pull_once(&mut self) -> Result<()> {
        match unsafe { process_output(&self.transform, self.output_hint)? } {
            Output::Sample(s) => self.accept(s),
            Output::StreamChanged => self.renegotiate(),
            Output::NeedMoreInput => Ok(()),
        }
    }

    fn renegotiate(&mut self) -> Result<()> {
        unsafe {
            let t = self.transform.GetOutputAvailableType(0, 0)?;
            self.transform.SetOutputType(0, &t, 0)?;
        }
        self.refresh_sequence_header();
        Ok(())
    }

    fn accept(&mut self, sample: IMFSample) -> Result<()> {
        let (data, pts, clean) = unsafe {
            (
                sample_bytes(&sample)?,
                sample.GetSampleTime().unwrap_or(0),
                sample.GetUINT32(&MFSampleExtension_CleanPoint).unwrap_or(0) == 1,
            )
        };
        if data.is_empty() {
            return Ok(());
        }
        // l'IDR se lit dans le flux lui-même ; « clean point » seul ne suffit pas (récupération progressive)
        let keyframe = h264::is_keyframe(&data);
        let _ = clean;
        let sets = h264::parameter_sets(&data);
        if !sets.is_empty() {
            self.sequence_header = sets;
        }
        let data = h264::with_parameter_sets(data, &self.sequence_header);
        let encode_time = match self.in_flight.iter().position(|(p, _)| *p == pts) {
            Some(i) => {
                let (_, at) = self.in_flight[i];
                self.in_flight.drain(..=i);
                Some(at.elapsed())
            }
            None => self.in_flight.pop_front().map(|(_, at)| at.elapsed()),
        };
        self.pending.push_back(Packet { data, pts, keyframe, encode_time, waited: Duration::ZERO });
        Ok(())
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        unsafe {
            let _ = self.transform.ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0);
            let stopped = self.transform.cast::<IMFShutdown>().and_then(|s| s.Shutdown()).is_ok();
            if let Mode::Async { thread, .. } = &mut self.mode {
                // sans arrêt confirmé, GetEvent pourrait bloquer : on ne l'attend pas
                if let Some(t) = thread.take().filter(|_| stopped) {
                    let _ = t.join();
                }
            }
            if let Some(a) = &self.activate {
                let _ = a.ShutdownObject();
            }
        }
    }
}
