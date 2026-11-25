//! Partie en cours, sans iBox : dans iBox, c'est la partie lancée par iBox (`inetbox::activity`).
//! Ici, une partie est la fenêtre au premier plan qui couvre tout son écran sans barre de titre
//! (plein écran exclusif ou fenêtre sans bordure), d'un programme qui n'est ni l'Explorateur, ni
//! un navigateur, ni un lanceur, ni un lecteur vidéo, ni Wallpaper Engine (comme iBox depuis
//! `f878ad5`). Son nom vient du titre de la fenêtre, sinon de l'exécutable.
//!
//! La partie reste « en cours » tant que son processus vit, même si elle perd le premier plan
//! (panneau d'iClips ouvert, Alt+Tab) : la mémoire des clips ne se vide pas pour si peu.

use std::sync::Mutex;

/// Programmes jamais pris pour une partie (nom de l'exécutable, en minuscules).
const NOT_GAMES: &[&str] = &[
    // Windows
    "explorer.exe", "applicationframehost.exe", "shellexperiencehost.exe", "startmenuexperiencehost.exe", "searchhost.exe",
    "searchapp.exe", "lockapp.exe", "textinputhost.exe", "taskmgr.exe", "systemsettings.exe", "dwm.exe", "msedgewebview2.exe",
    "screenclippingHost.exe", "snippingtool.exe", "magnify.exe",
    // navigateurs
    "chrome.exe", "msedge.exe", "firefox.exe", "opera.exe", "opera_gx.exe", "brave.exe", "vivaldi.exe", "iexplore.exe", "arc.exe",
    "librewolf.exe", "waterfox.exe", "zen.exe", "thorium.exe", "chromium.exe", "yandex.exe",
    // lecteurs vidéo et diffusion
    "vlc.exe", "mpv.exe", "mpc-hc64.exe", "mpc-hc.exe", "mpc-be64.exe", "wmplayer.exe", "microsoft.media.player.exe", "potplayermini64.exe",
    "obs64.exe", "obs32.exe", "discord.exe", "discordptb.exe", "discordcanary.exe",
    // lanceurs et boutiques (liste d'iBox, `pc/running.rs`)
    "riotclientservices.exe", "riot client.exe", "riotclientux.exe", "riotclientuxrender.exe", "riotclientcrashhandler.exe",
    "leagueclient.exe", "leagueclientux.exe", "leagueclientuxrender.exe", "leaguecrashhandler.exe",
    "steam.exe", "steamwebhelper.exe", "steamservice.exe", "epicgameslauncher.exe", "epicwebhelper.exe",
    "galaxyclient.exe", "galaxyclient helper.exe", "eadesktop.exe", "eabackgroundservice.exe", "ealauncher.exe",
    "ubisoftconnect.exe", "upc.exe", "uplaywebcore.exe", "rsi launcher.exe", "battle.net.exe", "agent.exe",
    // Wallpaper Engine tourne en permanence : ce n'est pas une partie
    "wallpaper32.exe", "wallpaper64.exe", "webwallpaper32.exe", "wallpaperservice32.exe", "wallpaperservice32_c.exe",
    // iClips et iBox eux-mêmes
    "iclips.exe", "ibox.exe",
];

/// Partie retenue : processus et nom affiché.
static CURRENT: Mutex<Option<(u32, String)>> = Mutex::new(None);

/// Nom de la partie si cette fenêtre en est une. `covers` : la fenêtre couvre tout son écran ;
/// `caption` : elle a une barre de titre (fenêtre ordinaire agrandie).
pub fn classify(exe: &str, title: &str, covers: bool, caption: bool) -> Option<String> {
    if !covers || caption {
        return None;
    }
    let exe = exe.replace('/', "\\");
    let file = exe.rsplit('\\').next().unwrap_or(&exe).to_lowercase();
    if file.is_empty() || NOT_GAMES.iter().any(|n| n.eq_ignore_ascii_case(&file)) {
        return None;
    }
    // jeux du Microsoft Store : dossier protégé, nom d'exécutable souvent générique
    Some(name(title, &file))
}

/// Titre de la fenêtre s'il est lisible, sinon l'exécutable sans « .exe » ni suffixe technique.
pub fn name(title: &str, file: &str) -> String {
    let t: String = title.trim().chars().filter(|c| !c.is_control()).take(80).collect();
    let stem = file.strip_suffix(".exe").unwrap_or(file);
    if !t.is_empty() && !t.eq_ignore_ascii_case(stem) && t.chars().any(char::is_alphanumeric) {
        return t;
    }
    let mut s = stem.to_string();
    for suffix in ["-win64-shipping", "-win32-shipping", "_x64", "-x64", "64", "_dx12", "_dx11", "dx12", "dx11"] {
        if let Some(rest) = s.strip_suffix(suffix) {
            if !rest.is_empty() {
                s = rest.to_string();
            }
        }
    }
    let words: Vec<String> = s.split(['_', '-', '.', ' ']).filter(|w| !w.is_empty()).map(|w| {
        let mut c = w.chars();
        c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
    }).collect();
    if words.is_empty() { "Jeu".into() } else { words.join(" ") }
}

/// Partie en cours (titre, plateforme « pc »), vue toutes les 2 s par `capture.rs`.
pub fn now() -> Option<(String, String)> {
    let seen = foreground();
    let mut current = CURRENT.lock().ok()?;
    match seen {
        Some(g) => *current = Some(g),
        None => {
            if current.as_ref().is_some_and(|(pid, _)| !alive(*pid)) {
                *current = None;
            }
        }
    }
    current.as_ref().map(|(_, name)| (name.clone(), "pc".to_string()))
}

#[cfg(windows)]
fn foreground() -> Option<(u32, String)> {
    use windows_sys::Win32::Foundation::{CloseHandle, RECT};
    use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    use windows_sys::Win32::System::Threading::{GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetForegroundWindow, GetWindowLongW, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, IsIconic, GWL_STYLE, WS_CAPTION};
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() || IsIconic(hwnd) != 0 {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 || pid == GetCurrentProcessId() {
            return None;
        }
        // bureau (fond d'écran) : jamais une partie
        let mut class = [0u16; 64];
        let n = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32);
        let class = String::from_utf16_lossy(&class[..n.max(0) as usize]);
        if class == "Progman" || class == "WorkerW" || class == "Shell_TrayWnd" {
            return None;
        }
        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return None;
        }
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut info) == 0 {
            return None;
        }
        let m = info.rcMonitor;
        let covers = rect.left <= m.left && rect.top <= m.top && rect.right >= m.right && rect.bottom >= m.bottom;
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let caption = style & WS_CAPTION == WS_CAPTION;
        if !covers || caption {
            return None;
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut len);
        CloseHandle(process);
        if ok == 0 {
            return None;
        }
        let exe = String::from_utf16_lossy(&buf[..len as usize]);
        let mut title = [0u16; 256];
        let n = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32);
        let title = String::from_utf16_lossy(&title[..n.max(0) as usize]);
        classify(&exe, &title, covers, caption).map(|name| (pid, name))
    }
}
#[cfg(not(windows))]
fn foreground() -> Option<(u32, String)> {
    None
}

#[cfg(windows)]
fn alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
    unsafe {
        let h = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if h.is_null() {
            return false;
        }
        let running = WaitForSingleObject(h, 0) == WAIT_TIMEOUT;
        CloseHandle(h);
        running
    }
}
#[cfg(not(windows))]
fn alive(_pid: u32) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_fenetre_plein_ecran_sans_bordure_est_une_partie() {
        assert_eq!(classify(r"C:\Games\Hollow Knight\hollow_knight.exe", "Hollow Knight", true, false).as_deref(), Some("Hollow Knight"));
        // fenêtre ordinaire agrandie, ou qui ne couvre pas l'écran : non
        assert_eq!(classify(r"C:\Games\x.exe", "X", true, true), None);
        assert_eq!(classify(r"C:\Games\x.exe", "X", false, false), None);
    }

    #[test]
    fn ni_navigateur_ni_explorateur_ni_wallpaper_engine() {
        for exe in [r"C:\Windows\explorer.exe", r"C:\Program Files\Google\Chrome\Application\chrome.exe", r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
            r"C:\Steam\steamapps\common\wallpaper_engine\wallpaper64.exe", r"C:\Steam\steamapps\common\wallpaper_engine\bin\webwallpaper32.exe", r"C:\Program Files\VideoLAN\VLC\vlc.exe"] {
            assert_eq!(classify(exe, "Plein écran", true, false), None, "{exe}");
        }
    }

    #[test]
    fn le_nom_vient_du_titre_sinon_de_l_executable() {
        assert_eq!(name("Celeste", "celeste.exe"), "Celeste");
        assert_eq!(name("", "eldenring.exe"), "Eldenring");
        assert_eq!(name("  ", "Hades2-Win64-Shipping.exe".to_lowercase().as_str()), "Hades2");
        assert_eq!(name("", "ready_or_not.exe"), "Ready Or Not");
        assert_eq!(name("UnityWndClass", "unitywndclass.exe"), "Unitywndclass");
    }
}
