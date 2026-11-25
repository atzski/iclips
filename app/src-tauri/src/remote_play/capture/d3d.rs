//! Device Direct3D 11 partagé par la capture (WGC), la conversion BGRA→NV12 et l'encodeur
//! Media Foundation : les images restent sur la carte graphique d'un bout à l'autre.

use windows::core::{Interface, Result};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Win32::Foundation::{HMODULE, LUID};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Multithread, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
    D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_SDK_VERSION,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::WinRT::Direct3D11::CreateDirect3D11DeviceFromDXGIDevice;

/// Classe de priorité du processus auprès de l'ordonnanceur de la carte graphique, le temps d'un
/// flux. `SetGPUThreadPriority` ne classe que les fils d'un même processus : quand un jeu occupe
/// la carte, c'est cette classe (« haute », accordée sans droits d'administrateur) qui fait
/// passer la conversion et l'encodage d'iBox devant son rendu (Sunshine fait de même). Le travail
/// d'iBox est minime (une conversion par image) : le jeu n'y perd rien de mesurable. La classe
/// normale est rétablie à la fin du dernier flux. Sans effet si Windows refuse.
pub struct GpuPriority(());

static GPU_PRIORITY_USERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
const D3DKMT_SCHEDULINGPRIORITYCLASS_NORMAL: i32 = 2;
const D3DKMT_SCHEDULINGPRIORITYCLASS_HIGH: i32 = 4;

/// `D3DKMTSetProcessSchedulingPriorityClass` (gdi32), cherchée à l'exécution.
fn set_gpu_priority_class(class: i32) -> bool {
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    type Setter = unsafe extern "system" fn(*mut core::ffi::c_void, i32) -> i32;
    let name: Vec<u16> = "gdi32.dll\0".encode_utf16().collect();
    unsafe {
        let mut module = GetModuleHandleW(name.as_ptr());
        if module.is_null() {
            module = LoadLibraryExW(name.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
        }
        if module.is_null() {
            return false;
        }
        let Some(f) = GetProcAddress(module, c"D3DKMTSetProcessSchedulingPriorityClass".as_ptr().cast()) else { return false };
        let set: Setter = std::mem::transmute(f);
        set(GetCurrentProcess(), class) >= 0
    }
}

impl GpuPriority {
    pub fn raise() -> GpuPriority {
        if GPU_PRIORITY_USERS.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            set_gpu_priority_class(D3DKMT_SCHEDULINGPRIORITYCLASS_HIGH);
        }
        GpuPriority(())
    }
}

impl Drop for GpuPriority {
    fn drop(&mut self) {
        if GPU_PRIORITY_USERS.fetch_sub(1, std::sync::atomic::Ordering::SeqCst) == 1 {
            set_gpu_priority_class(D3DKMT_SCHEDULINGPRIORITYCLASS_NORMAL);
        }
    }
}

#[derive(Clone)]
pub struct Device {
    pub device: ID3D11Device,
    pub context: ID3D11DeviceContext,
    /// Le même device vu par WinRT (exigé par `Direct3D11CaptureFramePool`).
    pub winrt: IDirect3DDevice,
    pub adapter_name: String,
    pub vendor_id: u32,
    pub adapter_luid: LUID,
}

impl Device {
    /// Device matériel de l'adaptateur par défaut, avec prise en charge vidéo (processeur vidéo
    /// et encodeurs) et protection multi-fil (exigée par Media Foundation).
    pub fn create() -> Result<Device> {
        unsafe {
            let mut device = None;
            let mut context = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0]),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )?;
            let device: ID3D11Device = device.ok_or_else(windows::core::Error::empty)?;
            let context: ID3D11DeviceContext = context.ok_or_else(windows::core::Error::empty)?;
            let _ = context.cast::<ID3D11Multithread>()?.SetMultithreadProtected(true);

            let dxgi: IDXGIDevice = device.cast()?;
            // conversion et encodage passent devant le rendu du jeu sur la carte graphique
            // (Sunshine fait de même) ; sans effet si Windows refuse
            let _ = dxgi.SetGPUThreadPriority(7);
            let desc = dxgi.GetAdapter()?.GetDesc()?;
            let end = desc.Description.iter().position(|&c| c == 0).unwrap_or(desc.Description.len());
            let winrt: IDirect3DDevice = CreateDirect3D11DeviceFromDXGIDevice(&dxgi)?.cast()?;
            Ok(Device {
                device,
                context,
                winrt,
                adapter_name: String::from_utf16_lossy(&desc.Description[..end]).trim().to_owned(),
                vendor_id: desc.VendorId,
                adapter_luid: desc.AdapterLuid,
            })
        }
    }
}
