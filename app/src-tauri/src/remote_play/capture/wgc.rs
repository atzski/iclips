//! Windows Graphics Capture : écran (`CreateForMonitor`) ou fenêtre (`CreateForWindow`).
//!
//! La réserve d'images est « free-threaded » : on la lit depuis notre propre fil, réveillé par
//! `FrameArrived`. Seule l'image la plus récente compte (les plus anciennes sont rendues aussitôt
//! à WGC). La fermeture de la source (`GraphicsCaptureItem::Closed`) arrête la capture.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;

use windows::core::{factory, IInspectable, Interface, Result};
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession};
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Direct3D11::{ID3D11Texture2D, D3D11_TEXTURE2D_DESC};
use windows::Win32::Graphics::Gdi::{MonitorFromPoint, HMONITOR, MONITOR_DEFAULTTOPRIMARY};
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::System::WinRT::Direct3D11::IDirect3DDxgiInterfaceAccess;
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::UI::WindowsAndMessaging::IsWindow;

use super::d3d::Device;
use super::{qpc_to_100ns, visible_area, Options, SizeTracker, Stop, Target};

const FORMAT: DirectXPixelFormat = DirectXPixelFormat::B8G8R8A8UIntNormalized;
const BUFFERS: i32 = 2;

/// Une image capturée : texture BGRA de WGC, rendue à la réserve quand l'objet est libéré.
pub struct Frame {
    pub texture: ID3D11Texture2D,
    /// Zone utile (en haut à gauche de la texture).
    pub size: (u32, u32),
    /// Horodatage WGC (`SystemRelativeTime`, base QPC) moins l'instant de réception, en ms.
    /// Mesuré positif sur ce PC : WGC date l'image d'un instant postérieur à sa réception
    /// (présentation prévue), ce n'est donc pas une latence de composition.
    pub timestamp_offset_ms: f64,
    frame: Direct3D11CaptureFrame,
}

impl Drop for Frame {
    fn drop(&mut self) {
        let _ = self.frame.Close();
    }
}

pub enum Next {
    Frame(Frame),
    Timeout,
    Stopped(Stop),
}

pub struct Capture {
    item: GraphicsCaptureItem,
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    device: Device,
    window: Option<HWND>,
    size: SizeTracker,
    wake: Receiver<()>,
    closed: Arc<AtomicBool>,
    frame_token: i64,
    closed_token: i64,
    /// La bordure jaune a bien été retirée.
    pub borderless: bool,
    /// Images rendues à WGC sans être traitées (une plus récente était déjà là).
    pub skipped: u64,
    qpc_frequency: i64,
}

fn size_of(s: SizeInt32) -> (u32, u32) {
    (s.Width.max(0) as u32, s.Height.max(0) as u32)
}

/// Demande une seule fois à Windows l'autorisation de capturer sans bordure jaune.
/// Une app non empaquetée l'obtient d'office ; l'appel reste sans effet sur les Windows anciens.
fn request_borderless() -> bool {
    use windows::Graphics::Capture::{GraphicsCaptureAccess, GraphicsCaptureAccessKind};
    use windows::Security::Authorization::AppCapabilityAccess::AppCapabilityAccessStatus;
    static ALLOWED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ALLOWED.get_or_init(|| {
        match GraphicsCaptureAccess::RequestAccessAsync(GraphicsCaptureAccessKind::Borderless).and_then(|op| op.join()) {
            Ok(status) => status == AppCapabilityAccessStatus::Allowed,
            // API absente : on laisse SetIsBorderRequired trancher.
            Err(_) => true,
        }
    })
}

fn create_item(target: Target) -> Result<GraphicsCaptureItem> {
    let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
    unsafe {
        match target {
            Target::PrimaryMonitor => interop.CreateForMonitor(MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY)),
            Target::Monitor(h) => interop.CreateForMonitor(HMONITOR(h as *mut _)),
            Target::Window(h) => interop.CreateForWindow(HWND(h as *mut _)),
        }
    }
}

impl Capture {
    pub fn start(device: &Device, target: Target, options: Options, windows_build: u32) -> Result<Capture> {
        let window = match target {
            Target::Window(h) => {
                let hwnd = HWND(h as *mut _);
                if !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
                    return Err(windows::Win32::Foundation::E_INVALIDARG.into());
                }
                Some(hwnd)
            }
            _ => None,
        };
        let item = create_item(target)?;
        let initial = item.Size()?;
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(&device.winrt, FORMAT, BUFFERS, initial)?;
        let session = pool.CreateCaptureSession(&item)?;

        let (tx, wake) = channel();
        let closed = Arc::new(AtomicBool::new(false));
        let frame_tx = tx.clone();
        let frame_token = pool.FrameArrived(&TypedEventHandler::<Direct3D11CaptureFramePool, IInspectable>::new(move |_, _| {
            let _ = frame_tx.send(());
            Ok(())
        }))?;
        let flag = closed.clone();
        let closed_token = item.Closed(&TypedEventHandler::<GraphicsCaptureItem, IInspectable>::new(move |_, _| {
            flag.store(true, Ordering::SeqCst);
            let _ = tx.send(());
            Ok(())
        }))?;

        let _ = session.SetIsCursorCaptureEnabled(options.cursor);
        // Windows 11 24H2+ : sans plancher, WGC ne livre pas plus vite que ~60 Hz ; à 1 ms, chaque
        // image du jeu arrive dès qu'elle est composée (on garde toujours la plus récente : moins
        // d'attente avant l'encodage). Refusé sans conséquence sur les Windows plus anciens.
        let _ = session.SetMinUpdateInterval(windows::Foundation::TimeSpan { Duration: 10_000 });
        let borderless = options.borderless && super::borderless_supported(windows_build) && request_borderless() && session.SetIsBorderRequired(false).is_ok();
        if options.require_borderless && !borderless {
            return Err(windows::core::Error::new(windows::core::HRESULT(0x80070032u32 as i32), "bordure de capture non retirable"));
        }
        session.StartCapture()?;

        let mut frequency = 0i64;
        unsafe { QueryPerformanceFrequency(&mut frequency)? };
        Ok(Capture {
            item,
            pool,
            session,
            device: device.clone(),
            window,
            size: SizeTracker::new(size_of(initial)),
            wake,
            closed,
            frame_token,
            closed_token,
            borderless,
            skipped: 0,
            qpc_frequency: frequency,
        })
    }

    /// Taille actuelle de la source.
    pub fn size(&self) -> (u32, u32) {
        self.size.current
    }

    /// Nombre de changements de taille de la source depuis le début.
    pub fn resizes(&self) -> u32 {
        self.size.changes
    }

    fn source_gone(&self) -> bool {
        self.closed.load(Ordering::SeqCst) || self.window.is_some_and(|w| !unsafe { IsWindow(Some(w)) }.as_bool())
    }

    /// Image la plus récente, en attendant au plus `timeout` (zéro = sans attendre).
    pub fn next(&mut self, timeout: Duration) -> Result<Next> {
        loop {
            if self.source_gone() {
                return Ok(Next::Stopped(Stop::SourceClosed));
            }
            let mut latest: Option<Direct3D11CaptureFrame> = None;
            while let Ok(frame) = self.pool.TryGetNextFrame() {
                if let Some(old) = latest.replace(frame) {
                    let _ = old.Close();
                    self.skipped += 1;
                }
            }
            if let Some(frame) = latest {
                if let Some(frame) = self.wrap(frame)? {
                    return Ok(Next::Frame(frame));
                }
                continue;
            }
            if timeout.is_zero() {
                return Ok(Next::Timeout);
            }
            match self.wake.recv_timeout(timeout) {
                Ok(()) => continue,
                Err(RecvTimeoutError::Timeout) => {
                    return Ok(if self.source_gone() { Next::Stopped(Stop::SourceClosed) } else { Next::Timeout })
                }
                Err(RecvTimeoutError::Disconnected) => return Ok(Next::Stopped(Stop::SourceClosed)),
            }
        }
    }

    fn wrap(&mut self, frame: Direct3D11CaptureFrame) -> Result<Option<Frame>> {
        let content = frame.ContentSize()?;
        if let Some(_new) = self.size.observe(size_of(content)) {
            // la fenêtre ou l'écran a changé de taille : nouvelles textures à la bonne taille
            self.pool.Recreate(&self.device.winrt, FORMAT, BUFFERS, content)?;
        }
        let texture: ID3D11Texture2D = unsafe { frame.Surface()?.cast::<IDirect3DDxgiInterfaceAccess>()?.GetInterface()? };
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { texture.GetDesc(&mut desc) };
        let size = visible_area(size_of(content), (desc.Width, desc.Height));
        if size.0 == 0 || size.1 == 0 {
            let _ = frame.Close();
            return Ok(None);
        }
        let mut now = 0i64;
        unsafe { QueryPerformanceCounter(&mut now)? };
        let composed = frame.SystemRelativeTime()?.Duration;
        let offset = composed - qpc_to_100ns(now, self.qpc_frequency);
        Ok(Some(Frame { texture, size, timestamp_offset_ms: offset as f64 / 10_000.0, frame }))
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.pool.RemoveFrameArrived(self.frame_token);
        let _ = self.item.RemoveClosed(self.closed_token);
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}
