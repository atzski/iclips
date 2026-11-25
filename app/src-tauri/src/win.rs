//! Petits outils Windows repris d'iBox (`music/overlay.rs`, `music/backdrop.rs`) : premier plan
//! forcé, manettes XInput, fenêtre exclue des captures.

use tauri::WebviewWindow;

/// Exclut la fenêtre des captures d'écran et des clips (Windows 10 2004 et plus ; sans effet avant).
#[cfg(windows)]
pub fn exclude_from_capture(w: &WebviewWindow) {
    use windows_sys::Win32::UI::WindowsAndMessaging::SetWindowDisplayAffinity;
    const WDA_EXCLUDEFROMCAPTURE: u32 = 0x11;
    if let Ok(h) = w.hwnd() {
        unsafe { SetWindowDisplayAffinity(h.0 as _, WDA_EXCLUDEFROMCAPTURE) };
    }
}
#[cfg(not(windows))]
pub fn exclude_from_capture(_: &WebviewWindow) {}

/// Met une fenêtre au premier plan, même quand le jeu tient le focus.
#[cfg(windows)]
pub fn force_foreground(hwnd: isize) {
    use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows_sys::Win32::UI::WindowsAndMessaging::{BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow};
    unsafe {
        let target = hwnd as _;
        let fg = GetForegroundWindow();
        if fg == target {
            return;
        }
        let me = GetCurrentThreadId();
        let other = if fg.is_null() { 0 } else { GetWindowThreadProcessId(fg, std::ptr::null_mut()) };
        let attached = other != 0 && other != me && AttachThreadInput(me, other, 1) != 0;
        BringWindowToTop(target);
        SetForegroundWindow(target);
        if attached {
            AttachThreadInput(me, other, 0);
        }
    }
}

pub mod pad {
    //! Boutons XInput et traduction en actions du panneau (sans dépendance Windows : testable).
    pub const UP: u16 = 0x0001;
    pub const DOWN: u16 = 0x0002;
    pub const LEFT: u16 = 0x0004;
    pub const RIGHT: u16 = 0x0008;
    pub const START: u16 = 0x0010;
    pub const BACK: u16 = 0x0020;
    pub const LB: u16 = 0x0100;
    pub const RB: u16 = 0x0200;
    pub const A: u16 = 0x1000;
    pub const B: u16 = 0x2000;
    pub const X: u16 = 0x4000;
    pub const Y: u16 = 0x8000;
    /// Combo du panneau : Vue (Back) + X maintenus (comme dans iBox).
    pub const COMBO: u16 = BACK | X;

    const ACTIONS: [(u16, &str); 11] = [
        (A, "ok"), (B, "back"), (X, "x"), (Y, "y"), (LB, "lb"), (RB, "rb"), (START, "start"),
        (UP, "up"), (DOWN, "down"), (LEFT, "left"), (RIGHT, "right"),
    ];
    /// Boutons répétés tant qu'ils restent enfoncés (croix directionnelle).
    pub const REPEAT: u16 = UP | DOWN | LEFT | RIGHT;

    /// Actions des boutons qui viennent d'être enfoncés. Rien tant que Vue est tenu
    /// (le combo ne doit pas aussi agir dans le panneau).
    pub fn pressed(now: u16, before: u16) -> Vec<&'static str> {
        if now & BACK != 0 {
            return Vec::new();
        }
        let fresh = now & !before;
        ACTIONS.iter().filter(|(b, _)| fresh & b != 0).map(|(_, a)| *a).collect()
    }

    pub fn repeated(now: u16) -> Vec<&'static str> {
        if now & BACK != 0 {
            return Vec::new();
        }
        ACTIONS.iter().filter(|(b, _)| now & REPEAT & b != 0).map(|(_, a)| *a).collect()
    }
}

/// Boutons XInput de toutes les manettes réunies ; `None` : aucune manette (ou XInput absent).
/// Lu par le fil de la manette (`panel.rs`).
#[cfg(windows)]
pub fn xinput_buttons() -> Option<u16> {
    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct Gamepad {
        buttons: u16,
        lt: u8,
        rt: u8,
        lx: i16,
        ly: i16,
        rx: i16,
        ry: i16,
    }
    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct XState {
        packet: u32,
        pad: Gamepad,
    }
    type GetState = unsafe extern "system" fn(u32, *mut XState) -> u32;
    static GET: std::sync::OnceLock<Option<GetState>> = std::sync::OnceLock::new();
    let get = (*GET.get_or_init(|| unsafe {
        use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
        let mut lib = LoadLibraryA(c"xinput1_4.dll".as_ptr() as _);
        if lib.is_null() {
            lib = LoadLibraryA(c"xinput9_1_0.dll".as_ptr() as _);
        }
        if lib.is_null() {
            return None;
        }
        GetProcAddress(lib, c"XInputGetState".as_ptr() as _).map(|f| std::mem::transmute::<unsafe extern "system" fn() -> isize, GetState>(f))
    }))?;
    let mut now = 0u16;
    let mut connected = false;
    for slot in 0..4 {
        let mut s = XState::default();
        if unsafe { get(slot, &mut s) } == 0 {
            connected = true;
            now |= s.pad.buttons;
        }
    }
    connected.then_some(now)
}

#[cfg(not(windows))]
pub fn xinput_buttons() -> Option<u16> {
    None
}
