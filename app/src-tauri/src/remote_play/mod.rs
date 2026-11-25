//! Chaîne vidéo et son reprise de Remote Play d'iBox (capture Windows Graphics Capture, encodeur
//! H.264 Media Foundation matériel, WASAPI) : seulement ce dont la mémoire des clips a besoin.

pub mod audio;
pub mod capture;
pub mod encode;

pub mod config {
    /// Débits acceptés par l'encodeur (kbit/s).
    pub const BITRATE_RANGE_KBPS: (u32, u32) = (500, 150_000);
}

#[cfg(windows)]
use std::sync::Mutex;

/// Essais A/B seulement (`IBOX_RP_OFF=gpu,defer,hold…`) : désactive un levier de latence pour
/// mesurer son effet à charge égale. Toujours faux dans l'application.
#[cfg(windows)]
pub(crate) fn lever_off(name: &str) -> bool {
    #[cfg(test)]
    {
        std::env::var("IBOX_RP_OFF").is_ok_and(|v| v.split(',').any(|n| n.trim() == name))
    }
    #[cfg(not(test))]
    {
        let _ = name;
        false
    }
}

/// Fil critique pour la latence (capture/encodage, réseau, son) : inscrit auprès du planificateur
/// multimédia de Windows (MMCSS, tâche « Games ») et en priorité haute, pour ne jamais attendre
/// derrière le jeu ou le reste d'iBox. Sans effet si Windows refuse.
pub fn latency_critical_thread() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Threading::{AvSetMmThreadCharacteristicsW, GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_HIGHEST};
        let task: Vec<u16> = "Games\0".encode_utf16().collect();
        let mut index = 0u32;
        let _ = AvSetMmThreadCharacteristicsW(task.as_ptr(), &mut index);
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
    }
}

/// Classe de priorité du processus le temps d'un flux. Les fils d'iBox critiques pour la latence
/// sont déjà prioritaires ([`latency_critical_thread`]), mais pas ceux que Windows et le pilote
/// créent pour nous : fils internes de l'encodeur matériel, rappels de la capture. Quand un jeu
/// occupe tous les cœurs, ils attendent leur tour et chaque image encodée prend des dizaines de
/// millisecondes. Une classe supérieure pour tout le processus les fait passer devant (Sunshine
/// fait de même) ; la classe d'origine est rétablie à la fin du dernier flux.
#[cfg(windows)]
pub struct ProcessPriority(());

#[cfg(windows)]
static PROCESS_PRIORITY: Mutex<(usize, u32)> = Mutex::new((0, 0));

#[cfg(windows)]
impl ProcessPriority {
    pub fn raise() -> ProcessPriority {
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetPriorityClass, SetPriorityClass, ABOVE_NORMAL_PRIORITY_CLASS, HIGH_PRIORITY_CLASS, REALTIME_PRIORITY_CLASS};
        if let Ok(mut state) = PROCESS_PRIORITY.lock() {
            if state.0 == 0 {
                let previous = unsafe { GetPriorityClass(GetCurrentProcess()) };
                state.1 = previous;
                let wanted = if cfg!(test) && std::env::var("IBOX_RP_PROCESS").is_ok_and(|v| v == "above") { ABOVE_NORMAL_PRIORITY_CLASS } else { HIGH_PRIORITY_CLASS };
                // jamais d'abaissement : un processus déjà lancé en priorité haute le reste
                if previous != 0 && previous != HIGH_PRIORITY_CLASS && previous != REALTIME_PRIORITY_CLASS {
                    unsafe { SetPriorityClass(GetCurrentProcess(), wanted) };
                }
            }
            state.0 += 1;
        }
        ProcessPriority(())
    }
}

#[cfg(windows)]
impl Drop for ProcessPriority {
    fn drop(&mut self) {
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, SetPriorityClass};
        if let Ok(mut state) = PROCESS_PRIORITY.lock() {
            state.0 = state.0.saturating_sub(1);
            if state.0 == 0 && state.1 != 0 {
                unsafe { SetPriorityClass(GetCurrentProcess(), state.1) };
            }
        }
    }
}
