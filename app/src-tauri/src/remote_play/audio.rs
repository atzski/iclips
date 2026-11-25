//! Son de l'hôte : capture WASAPI en boucle (« ce que le PC joue », périphérique de sortie par
//! défaut) → 48 kHz stéréo → Opus 10 ms (`opus-rs`, Rust pur). Les paquets partent sur la piste
//! audio de la session (`transport::peer`).
//!
//! « Masquer l'écho Discord » : la capture passe par le loopback **par processus** de Windows
//! (`PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE`, build 20348+) en excluant l'arbre de
//! processus de Discord : l'invité n'entend plus sa propre voix renvoyée par l'appel Discord.
//!
//! WASAPI en boucle ne livre rien tant que rien ne joue : les trous sont comblés par du silence
//! pour que l'horodatage RTP suive l'horloge réelle (sinon le son prendrait du retard sur l'image).

use opus_rs::{Application, OpusEncoder};

pub const SAMPLE_RATE: u32 = 48_000;
/// 10 ms à 48 kHz (échantillons par voie) : deux fois moins d'attente qu'en 20 ms.
pub const FRAME: usize = 480;
pub const BITRATE_BPS: i32 = 128_000;

/// Paquet Opus prêt à envoyer ; `at` = position (échantillons à 48 kHz) depuis le début du son.
#[derive(Debug, Clone)]
pub struct AudioPacket {
    pub at: u64,
    pub data: Vec<u8>,
}

/// Une image stéréo (gauche, droite).
pub type Stereo = [f32; 2];

/// Réduit une image multivoie (ordre WAVE : FL FR FC LFE BL BR SL SR) en stéréo.
pub fn downmix(frame: &[f32]) -> Stereo {
    match frame.len() {
        0 => [0.0, 0.0],
        1 => [frame[0], frame[0]],
        2 => [frame[0], frame[1]],
        n => {
            let c = frame[2] * 0.707;
            let (mut l, mut r) = (frame[0] + c, frame[1] + c);
            for pair in [(4, 5), (6, 7)] {
                if n > pair.1 {
                    l += frame[pair.0] * 0.5;
                    r += frame[pair.1] * 0.5;
                }
            }
            [l.clamp(-1.0, 1.0), r.clamp(-1.0, 1.0)]
        }
    }
}

/// Rééchantillonnage linéaire vers 48 kHz (44,1 kHz, 96 kHz… selon le format du mélangeur).
pub struct Resampler {
    step: f64,
    pos: f64,
    prev: Stereo,
}

impl Resampler {
    pub fn new(from_hz: u32) -> Self {
        Resampler { step: from_hz.max(1) as f64 / SAMPLE_RATE as f64, pos: 1.0, prev: [0.0, 0.0] }
    }

    /// Ajoute `input` (au débit d'origine) à `out` (stéréo entrelacée, 48 kHz).
    pub fn process(&mut self, input: &[Stereo], out: &mut Vec<f32>) {
        if (self.step - 1.0).abs() < f64::EPSILON {
            out.extend(input.iter().flatten());
            return;
        }
        // indices virtuels : 0 = dernière image du bloc précédent, k = input[k - 1]
        let n = input.len();
        while self.pos < n as f64 {
            let i = self.pos.floor() as usize;
            let f = (self.pos - i as f64) as f32;
            let a = if i == 0 { self.prev } else { input[i - 1] };
            let b = input[i];
            out.push(a[0] + (b[0] - a[0]) * f);
            out.push(a[1] + (b[1] - a[1]) * f);
            self.pos += self.step;
        }
        self.pos -= n as f64;
        if let Some(last) = input.last() {
            self.prev = *last;
        }
    }
}

/// Découpe le flux 48 kHz stéréo en trames de 10 ms et les encode en Opus.
pub struct Encoder {
    opus: OpusEncoder,
    pending: Vec<f32>,
    /// Échantillons (par voie) déjà encodés.
    encoded: u64,
    out: Vec<u8>,
}

impl Encoder {
    pub fn new() -> Result<Self, String> {
        let mut opus = OpusEncoder::new(SAMPLE_RATE as i32, 2, Application::Audio).map_err(|e| format!("Opus : {e}"))?;
        opus.bitrate_bps = BITRATE_BPS;
        Ok(Encoder { opus, pending: Vec::with_capacity(FRAME * 4), encoded: 0, out: vec![0u8; 1500] })
    }

    /// Échantillons (par voie) reçus jusqu'ici, encodés ou en attente.
    pub fn position(&self) -> u64 {
        self.encoded + (self.pending.len() / 2) as u64
    }

    /// Ajoute de l'audio 48 kHz stéréo entrelacé ; renvoie les paquets complets.
    pub fn push(&mut self, samples: &[f32]) -> Vec<AudioPacket> {
        self.pending.extend_from_slice(samples);
        let mut packets = Vec::new();
        while self.pending.len() >= FRAME * 2 {
            let at = self.encoded;
            if let Ok(n) = self.opus.encode(&self.pending[..FRAME * 2], FRAME, &mut self.out) {
                if n > 0 {
                    packets.push(AudioPacket { at, data: self.out[..n].to_vec() });
                }
            }
            self.pending.drain(..FRAME * 2);
            self.encoded += FRAME as u64;
        }
        packets
    }

    /// Comble par du silence jusqu'à la position `target` (échantillons par voie).
    pub fn pad_to(&mut self, target: u64) -> Vec<AudioPacket> {
        let missing = target.saturating_sub(self.position()) as usize;
        if missing == 0 {
            return Vec::new();
        }
        self.push(&vec![0.0; missing * 2])
    }
}

#[cfg(windows)]
pub use win::{endpoints, pcm, stream};

/// Appareil de son de Windows (sortie ou micro), pour le choix dans iClips.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Endpoint {
    pub id: String,
    pub name: String,
    /// Appareil par défaut de Windows.
    pub default: bool,
}

#[cfg(not(windows))]
pub fn endpoints(_capture: bool) -> Vec<Endpoint> {
    Vec::new()
}

/// Noms des exécutables Discord (application, PTB, Canary), en minuscules.
const DISCORD: [&str; 3] = ["discord.exe", "discordptb.exe", "discordcanary.exe"];

/// Processus racine de Discord parmi `(pid, parent, nom)` : un processus Discord dont le parent
/// n'est pas Discord (ses voix sortent d'un processus enfant, exclu avec tout l'arbre).
pub fn discord_root(processes: &[(u32, Option<u32>, String)]) -> Option<u32> {
    let is_discord = |name: &str| DISCORD.contains(&name.to_ascii_lowercase().as_str());
    let pids: Vec<u32> = processes.iter().filter(|p| is_discord(&p.2)).map(|p| p.0).collect();
    processes
        .iter()
        .filter(|p| is_discord(&p.2) && !p.1.is_some_and(|parent| pids.contains(&parent)))
        .map(|p| p.0)
        .min()
}

/// Processus racine de Discord en cours d'exécution, s'il y en a un.
pub fn find_discord() -> Option<u32> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    let list: Vec<(u32, Option<u32>, String)> =
        sys.processes().values().map(|p| (p.pid().as_u32(), p.parent().map(|x| x.as_u32()), p.name().to_string_lossy().into_owned())).collect();
    discord_root(&list)
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};
    use windows::core::{implement, Interface, Ref, HRESULT};
    use windows::Win32::Media::Audio::{
        eConsole, eRender, ActivateAudioInterfaceAsync, IActivateAudioInterfaceAsyncOperation, IActivateAudioInterfaceCompletionHandler,
        IActivateAudioInterfaceCompletionHandler_Impl, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator, AUDCLNT_BUFFERFLAGS_SILENT,
        AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_LOOPBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
        AUDIOCLIENT_ACTIVATION_PARAMS, AUDIOCLIENT_ACTIVATION_PARAMS_0, AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK, AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS,
        PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE, VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, WAVEFORMATEX,
    };
    use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemAlloc, CoTaskMemFree, CoUninitialize, BLOB, CLSCTX_ALL, COINIT_MULTITHREADED};
    use windows::Win32::System::Variant::VT_BLOB;

    /// Retard toléré avant de combler par du silence (rien ne joue sur le PC).
    const GAP: u64 = SAMPLE_RATE as u64 / 10;

    #[derive(Clone, Copy)]
    enum Sample {
        F32,
        I16,
        I32,
    }

    struct Com(bool);
    impl Drop for Com {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    /// Signale la fin de l'activation asynchrone (appelé sur un fil de Windows).
    #[implement(IActivateAudioInterfaceCompletionHandler)]
    struct Activated(std::sync::mpsc::SyncSender<()>);

    impl IActivateAudioInterfaceCompletionHandler_Impl for Activated_Impl {
        fn ActivateCompleted(&self, _operation: Ref<IActivateAudioInterfaceAsyncOperation>) -> windows::core::Result<()> {
            let _ = self.0.try_send(());
            Ok(())
        }
    }

    /// Capture le son du PC jusqu'à `stop` ; `send` reçoit chaque paquet Opus (et l'instant
    /// correspondant au début du son) et renvoie `false` pour arrêter. `exclude` : processus dont
    /// l'arbre est retiré du son (Discord) ; si Windows refuse, capture complète.
    pub fn stream(stop: &AtomicBool, exclude: Option<u32>, send: &mut dyn FnMut(AudioPacket, Instant) -> bool) -> Result<(), String> {
        let _com = Com(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() });
        let opened = unsafe {
            match exclude.map(|pid| open_excluding(pid)) {
                Some(Ok(c)) => Ok(c),
                Some(Err(e)) => {
                    eprintln!("[remote-play] son sans Discord indisponible ({e}) : capture complète");
                    open_default(None)
                }
                None => open_default(None),
            }
        };
        let (client, fmt) = opened.map_err(|e| format!("capture du son : {e}"))?;
        unsafe { capture(&client, fmt, stop, send) }.map_err(|e| format!("capture du son : {e}"))
    }

    /// Appareil choisi s'il est branché, sinon celui par défaut de Windows.
    unsafe fn endpoint(flow: windows::Win32::Media::Audio::EDataFlow, id: Option<&str>) -> windows::core::Result<windows::Win32::Media::Audio::IMMDevice> {
        use windows::Win32::Media::Audio::DEVICE_STATE_ACTIVE;
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        if let Some(id) = id {
            if let Ok(d) = devices.GetDevice(&windows::core::HSTRING::from(id)) {
                if d.GetState().is_ok_and(|s| s == DEVICE_STATE_ACTIVE) {
                    return Ok(d);
                }
            }
        }
        devices.GetDefaultAudioEndpoint(flow, eConsole)
    }

    unsafe fn text(p: windows::core::PWSTR) -> String {
        let s = p.to_string().unwrap_or_default();
        CoTaskMemFree(Some(p.0 as *const _));
        s
    }

    /// Sorties (ou micros) branchés, avec leur nom de Windows ; celui par défaut est marqué.
    pub fn endpoints(capture: bool) -> Vec<super::Endpoint> {
        use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
        use windows::Win32::Media::Audio::{eCapture, DEVICE_STATE_ACTIVE};
        use windows::Win32::System::Com::STGM_READ;
        let _com = Com(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() });
        let flow = if capture { eCapture } else { eRender };
        let list = || -> windows::core::Result<Vec<super::Endpoint>> {
            unsafe {
                let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
                let default = devices.GetDefaultAudioEndpoint(flow, eConsole).and_then(|d| d.GetId()).map(|p| text(p)).unwrap_or_default();
                let all = devices.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE)?;
                let mut out = Vec::new();
                for i in 0..all.GetCount()? {
                    let d = all.Item(i)?;
                    let id = text(d.GetId()?);
                    let name = d.OpenPropertyStore(STGM_READ).and_then(|p| p.GetValue(&PKEY_Device_FriendlyName)).map(|v| v.to_string()).unwrap_or_default();
                    out.push(super::Endpoint { default: id == default, name: if name.is_empty() { id.clone() } else { name }, id });
                }
                Ok(out)
            }
        };
        list().unwrap_or_default()
    }

    /// Sortie (par défaut, ou celle choisie), au format du mélangeur de Windows.
    unsafe fn open_default(id: Option<&str>) -> windows::core::Result<(IAudioClient, WAVEFORMATEX)> {
        let device = endpoint(eRender, id)?;
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
        let format = client.GetMixFormat()?;
        let fmt: WAVEFORMATEX = std::ptr::read_unaligned(format);
        let init = client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, 2_000_000, 0, format, None);
        CoTaskMemFree(Some(format as *const _));
        init?;
        Ok((client, fmt))
    }

    /// Tout le son du PC sauf l'arbre de processus `pid` (loopback par processus).
    unsafe fn open_excluding(pid: u32) -> windows::core::Result<(IAudioClient, WAVEFORMATEX)> {
        let params = AUDIOCLIENT_ACTIVATION_PARAMS {
            ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
            Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
                ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS { TargetProcessId: pid, ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE },
            },
        };
        // PROPVARIANT VT_BLOB : Windows libère lui-même le bloc (CoTaskMemFree) après l'activation.
        // Sur la pile, il corrompait le tas (0xC0000374, vérifié le 17/11/2026) : tas COM obligatoire.
        let data = CoTaskMemAlloc(size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>()) as *mut AUDIOCLIENT_ACTIVATION_PARAMS;
        if data.is_null() {
            return Err(windows::core::Error::from(HRESULT(0x8007000Eu32 as i32)));
        }
        data.write(params);
        let mut variant = PROPVARIANT::default();
        {
            let inner = &mut *variant.Anonymous.Anonymous;
            inner.vt = VT_BLOB;
            inner.Anonymous.blob = BLOB { cbSize: size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32, pBlobData: data.cast() };
        }
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let handler: IActivateAudioInterfaceCompletionHandler = Activated(tx).into();
        let operation = ActivateAudioInterfaceAsync(VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, &IAudioClient::IID, Some(&variant), &handler)?;
        if rx.recv_timeout(Duration::from_secs(5)).is_err() {
            return Err(windows::core::Error::new(HRESULT(0x800705B4u32 as i32), "activation du son trop longue"));
        }
        let mut result = HRESULT(0);
        let mut unknown = None;
        operation.GetActivateResult(&mut result, &mut unknown)?;
        result.ok()?;
        let client: IAudioClient = unknown.ok_or_else(|| windows::core::Error::from(HRESULT(0x80004003u32 as i32)))?.cast()?;
        // pas de format de mélangeur ici : 48 kHz stéréo flottant, converti par Windows
        let fmt = WAVEFORMATEX {
            wFormatTag: 3,
            nChannels: 2,
            nSamplesPerSec: SAMPLE_RATE,
            nAvgBytesPerSec: SAMPLE_RATE * 8,
            nBlockAlign: 8,
            wBitsPerSample: 32,
            cbSize: 0,
        };
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
            2_000_000,
            0,
            &fmt,
            None,
        )?;
        Ok((client, fmt))
    }

    unsafe fn capture(client: &IAudioClient, fmt: WAVEFORMATEX, stop: &AtomicBool, send: &mut dyn FnMut(AudioPacket, Instant) -> bool) -> windows::core::Result<()> {
        let channels = fmt.nChannels.max(1) as usize;
        let kind = match (fmt.wFormatTag, fmt.wBitsPerSample) {
            (3, 32) | (0xFFFE, 32) => Sample::F32, // IEEE float (le mélangeur partagé l'est presque toujours)
            (1, 16) | (0xFFFE, 16) => Sample::I16,
            (1, 32) => Sample::I32,
            _ => return Err(windows::core::Error::new(windows::core::HRESULT(0x88890008u32 as i32), "format audio non pris en charge")),
        };
        let block = fmt.nBlockAlign as usize;
        let capture: IAudioCaptureClient = client.GetService()?;
        let mut resampler = Resampler::new(fmt.nSamplesPerSec);
        let mut encoder = Encoder::new().map_err(|e| windows::core::Error::new(windows::core::HRESULT(-1), e))?;
        let mut frames: Vec<Stereo> = Vec::new();
        let mut converted: Vec<f32> = Vec::new();
        let mut scratch = vec![0f32; channels];

        client.Start()?;
        let origin = Instant::now();
        let mut result = Ok(());
        'outer: while !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(5));
            loop {
                let size = match capture.GetNextPacketSize() {
                    Ok(s) => s,
                    Err(e) => {
                        result = Err(e);
                        break 'outer;
                    }
                };
                if size == 0 {
                    break;
                }
                let mut data: *mut u8 = std::ptr::null_mut();
                let mut count = 0u32;
                let mut flags = 0u32;
                if let Err(e) = capture.GetBuffer(&mut data, &mut count, &mut flags, None, None) {
                    result = Err(e);
                    break 'outer;
                }
                frames.clear();
                let silent = flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0 || data.is_null();
                for i in 0..count as usize {
                    if silent {
                        frames.push([0.0, 0.0]);
                        continue;
                    }
                    let base = data.add(i * block);
                    for (c, s) in scratch.iter_mut().enumerate() {
                        *s = match kind {
                            Sample::F32 => std::ptr::read_unaligned(base.add(c * 4) as *const f32),
                            Sample::I16 => std::ptr::read_unaligned(base.add(c * 2) as *const i16) as f32 / 32768.0,
                            Sample::I32 => std::ptr::read_unaligned(base.add(c * 4) as *const i32) as f32 / 2147483648.0,
                        };
                    }
                    frames.push(downmix(&scratch));
                }
                let _ = capture.ReleaseBuffer(count);
                converted.clear();
                resampler.process(&frames, &mut converted);
                for p in encoder.push(&converted) {
                    if !send(p, origin) {
                        break 'outer;
                    }
                }
            }
            // rien ne joue : WASAPI se tait, l'horloge RTP doit continuer d'avancer
            let expected = (origin.elapsed().as_secs_f64() * SAMPLE_RATE as f64) as u64;
            if expected > encoder.position() + GAP {
                for p in encoder.pad_to(expected - FRAME as u64) {
                    if !send(p, origin) {
                        break 'outer;
                    }
                }
            }
        }
        let _ = client.Stop();
        result
    }

    /// Micro (par défaut, ou celui choisi), au format de son mélangeur.
    unsafe fn open_microphone(id: Option<&str>) -> windows::core::Result<(IAudioClient, WAVEFORMATEX)> {
        use windows::Win32::Media::Audio::eCapture;
        let device = endpoint(eCapture, id)?;
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
        let format = client.GetMixFormat()?;
        let fmt: WAVEFORMATEX = std::ptr::read_unaligned(format);
        let init = client.Initialize(AUDCLNT_SHAREMODE_SHARED, 0, 2_000_000, 0, format, None);
        CoTaskMemFree(Some(format as *const _));
        init?;
        Ok((client, fmt))
    }

    /// Son brut 48 kHz stéréo entrelacé (clips) : sortie de Windows (`microphone = false`) ou
    /// micro par défaut. Les silences de WASAPI sont comblés : l'échantillon n° k correspond à
    /// `k / 48 000` s après le début. `on` renvoie `false` pour arrêter.
    /// Son en continu (micro ou sortie écoutée) ; `device` : appareil choisi, sinon celui par défaut.
    pub fn pcm(stop: &AtomicBool, microphone: bool, device: Option<&str>, on: &mut dyn FnMut(&[f32]) -> bool) -> Result<(), String> {
        let _com = Com(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() });
        let what = if microphone { "micro" } else { "son de Windows" };
        let (client, fmt) = unsafe { if microphone { open_microphone(device) } else { open_default(device) } }.map_err(|e| format!("{what} : {e}"))?;
        unsafe { pump(&client, fmt, stop, on) }.map_err(|e| format!("{what} : {e}"))
    }

    unsafe fn pump(client: &IAudioClient, fmt: WAVEFORMATEX, stop: &AtomicBool, on: &mut dyn FnMut(&[f32]) -> bool) -> windows::core::Result<()> {
        let channels = fmt.nChannels.max(1) as usize;
        let kind = match (fmt.wFormatTag, fmt.wBitsPerSample) {
            (3, 32) | (0xFFFE, 32) => Sample::F32,
            (1, 16) | (0xFFFE, 16) => Sample::I16,
            (1, 32) => Sample::I32,
            _ => return Err(windows::core::Error::new(windows::core::HRESULT(0x88890008u32 as i32), "format audio non pris en charge")),
        };
        let block = fmt.nBlockAlign as usize;
        let capture: IAudioCaptureClient = client.GetService()?;
        let mut resampler = Resampler::new(fmt.nSamplesPerSec);
        let mut frames: Vec<Stereo> = Vec::new();
        let mut converted: Vec<f32> = Vec::new();
        let mut scratch = vec![0f32; channels];
        let mut position = 0u64;
        client.Start()?;
        let origin = Instant::now();
        let mut result = Ok(());
        'outer: while !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(10));
            loop {
                let size = match capture.GetNextPacketSize() {
                    Ok(s) => s,
                    Err(e) => {
                        result = Err(e);
                        break 'outer;
                    }
                };
                if size == 0 {
                    break;
                }
                let mut data: *mut u8 = std::ptr::null_mut();
                let mut count = 0u32;
                let mut flags = 0u32;
                if let Err(e) = capture.GetBuffer(&mut data, &mut count, &mut flags, None, None) {
                    result = Err(e);
                    break 'outer;
                }
                frames.clear();
                let silent = flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0 || data.is_null();
                for i in 0..count as usize {
                    if silent {
                        frames.push([0.0, 0.0]);
                        continue;
                    }
                    let base = data.add(i * block);
                    for (c, s) in scratch.iter_mut().enumerate() {
                        *s = match kind {
                            Sample::F32 => std::ptr::read_unaligned(base.add(c * 4) as *const f32),
                            Sample::I16 => std::ptr::read_unaligned(base.add(c * 2) as *const i16) as f32 / 32768.0,
                            Sample::I32 => std::ptr::read_unaligned(base.add(c * 4) as *const i32) as f32 / 2147483648.0,
                        };
                    }
                    frames.push(downmix(&scratch));
                }
                let _ = capture.ReleaseBuffer(count);
                converted.clear();
                resampler.process(&frames, &mut converted);
                position += (converted.len() / 2) as u64;
                if !converted.is_empty() && !on(&converted) {
                    break 'outer;
                }
            }
            let expected = (origin.elapsed().as_secs_f64() * SAMPLE_RATE as f64) as u64;
            if expected > position + GAP {
                let missing = (expected - FRAME as u64 - position) as usize;
                position += missing as u64;
                if !on(&vec![0.0; missing * 2]) {
                    break;
                }
            }
        }
        let _ = client.Stop();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Capture réelle (sortie par défaut de ce PC) : `cargo test --lib capture_reelle -- --ignored --nocapture`
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn capture_reelle() {
        use std::sync::atomic::AtomicBool;
        let stop = AtomicBool::new(false);
        let start = std::time::Instant::now();
        let mut packets = 0u64;
        let mut last = 0u64;
        let exclude = std::env::var("IBOX_EXCLURE_PID").ok().and_then(|v| v.parse().ok()).or_else(find_discord);
        println!("exclusion : {exclude:?}");
        stream(&stop, exclude, &mut |p, _| {
            assert!(p.at >= last);
            last = p.at;
            packets += 1;
            start.elapsed() < std::time::Duration::from_millis(1500)
        })
        .unwrap();
        println!("{packets} paquets, position {last}");
        assert!(packets >= 50, "au moins 1 s de son (silence compris)");
    }

    #[test]
    fn racine_de_discord() {
        let list = vec![
            (10, Some(1), "explorer.exe".to_string()),
            (40, Some(10), "Discord.exe".to_string()),
            (41, Some(40), "Discord.exe".to_string()),
            (42, Some(40), "Discord.exe".to_string()),
            (50, Some(10), "chrome.exe".to_string()),
        ];
        assert_eq!(discord_root(&list), Some(40));
        assert_eq!(discord_root(&list[..1]), None);
        assert_eq!(discord_root(&[(7, None, "DiscordPTB.exe".into())]), Some(7));
    }

    #[test]
    fn mixage_stereo() {
        assert_eq!(downmix(&[0.5]), [0.5, 0.5]);
        assert_eq!(downmix(&[0.1, 0.2]), [0.1, 0.2]);
        let six = downmix(&[0.0, 0.0, 1.0, 0.0, 0.0, 0.0]);
        assert!((six[0] - 0.707).abs() < 1e-3 && (six[1] - 0.707).abs() < 1e-3, "voix centrale conservée");
    }

    #[test]
    fn reechantillonnage_44100() {
        let mut r = Resampler::new(44_100);
        let mut out = Vec::new();
        for _ in 0..10 {
            r.process(&vec![[0.25, -0.25]; 4410], &mut out);
        }
        let frames = out.len() / 2;
        assert!((frames as i64 - 48_000).abs() <= 2, "{frames} images pour 1 s");
        assert!(out[200..].chunks(2).all(|c| (c[0] - 0.25).abs() < 1e-6 && (c[1] + 0.25).abs() < 1e-6));
    }

    #[test]
    fn trames_opus_de_20_ms() {
        let mut e = Encoder::new().unwrap();
        let tone: Vec<f32> = (0..FRAME * 3).flat_map(|i| {
            let s = (i as f32 * 440.0 * std::f32::consts::TAU / SAMPLE_RATE as f32).sin() * 0.3;
            [s, s]
        }).collect();
        let packets = e.push(&tone[..(FRAME * 2 + 5) * 2]);
        assert_eq!(packets.len(), 2);
        assert_eq!(packets[1].at, FRAME as u64);
        assert!(packets.iter().all(|p| !p.data.is_empty() && p.data.len() < 1500));
        let padded = e.pad_to(FRAME as u64 * 5);
        assert_eq!(padded.len(), 3);
        assert_eq!(e.position(), FRAME as u64 * 5);
    }
}
