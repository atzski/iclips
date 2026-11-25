//! Capture d'image de l'hôte Remote Play (Phase 1b) : Windows Graphics Capture sur un device
//! Direct3D 11 partagé avec l'encodeur (`encode/`). Aucune copie par le processeur.
//!
//! Règles : une fenêtre capturée qui se ferme arrête la capture (jamais de repli sur le bureau
//! entier) ; les contenus protégés restent noirs (aucun contournement).

#![allow(dead_code)] // branché sur le transport en Phase 1c

#[cfg(windows)]
pub mod d3d;
#[cfg(windows)]
pub mod wgc;

/// Ce que l'hôte partage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// Écran principal (celui qui contient le point 0,0).
    PrimaryMonitor,
    /// Écran donné par son `HMONITOR`.
    Monitor(isize),
    /// Fenêtre donnée par son `HWND`.
    Window(isize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Dessiner le curseur dans l'image.
    pub cursor: bool,
    /// Retirer la bordure jaune de capture (Windows build 20348+ seulement).
    pub borderless: bool,
    /// Refuser la capture si la bordure jaune ne peut pas être retirée (capture discrète, ex. clips).
    pub require_borderless: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { cursor: true, borderless: true, require_borderless: false }
    }
}

/// La bordure de capture est retirable à partir de Windows Server 2022 / Windows 11 (build 20348).
pub fn borderless_supported(build: u32) -> bool {
    build >= 20348
}

/// Numéro de build de Windows (0 si illisible).
#[cfg(windows)]
pub fn windows_build() -> u32 {
    winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
        .and_then(|k| k.get_value::<String, _>("CurrentBuildNumber"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

/// Pourquoi une capture s'est arrêtée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// La fenêtre ou l'écran capturé a disparu.
    SourceClosed,
}

/// Suit la taille du contenu capturé : la réserve d'images WGC doit être recréée quand la fenêtre
/// (ou la définition de l'écran) change, sinon les images restent à l'ancienne taille.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizeTracker {
    pub current: (u32, u32),
    pub changes: u32,
}

impl SizeTracker {
    pub fn new(initial: (u32, u32)) -> Self {
        SizeTracker { current: initial, changes: 0 }
    }

    /// Renvoie la nouvelle taille si la réserve doit être recréée. Une taille nulle (fenêtre
    /// réduite) est ignorée : on garde la dernière image valable.
    pub fn observe(&mut self, content: (u32, u32)) -> Option<(u32, u32)> {
        if content.0 == 0 || content.1 == 0 || content == self.current {
            return None;
        }
        self.current = content;
        self.changes += 1;
        Some(content)
    }
}

/// Zone utile d'une image : le contenu peut être plus petit que la texture juste après un
/// redimensionnement (la réserve n'est pas encore recréée), jamais plus grand.
pub fn visible_area(content: (u32, u32), texture: (u32, u32)) -> (u32, u32) {
    (content.0.min(texture.0), content.1.min(texture.1))
}

/// Convertit un horodatage QPC en unités de 100 ns (base de `SystemRelativeTime` de WGC).
pub fn qpc_to_100ns(counter: i64, frequency: i64) -> i64 {
    if frequency <= 0 {
        return 0;
    }
    let (q, r) = (counter / frequency, counter % frequency);
    q * 10_000_000 + r * 10_000_000 / frequency
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borderless_from_build_20348() {
        assert!(!borderless_supported(19045));
        assert!(borderless_supported(20348));
        assert!(borderless_supported(26100));
    }

    #[test]
    fn size_tracker_reports_real_changes_only() {
        let mut t = SizeTracker::new((1920, 1080));
        assert_eq!(t.observe((1920, 1080)), None);
        assert_eq!(t.observe((0, 0)), None, "fenêtre réduite");
        assert_eq!(t.observe((1280, 0)), None);
        assert_eq!(t.observe((1280, 720)), Some((1280, 720)));
        assert_eq!(t.observe((1280, 720)), None);
        assert_eq!(t.observe((1920, 1080)), Some((1920, 1080)));
        assert_eq!(t.changes, 2);
    }

    #[test]
    fn visible_area_never_exceeds_texture() {
        assert_eq!(visible_area((1280, 720), (1920, 1080)), (1280, 720));
        assert_eq!(visible_area((2560, 1440), (1920, 1080)), (1920, 1080));
    }

    #[test]
    fn qpc_conversion() {
        assert_eq!(qpc_to_100ns(10_000_000, 10_000_000), 10_000_000);
        assert_eq!(qpc_to_100ns(3_000_000, 3_000_000), 10_000_000);
        assert_eq!(qpc_to_100ns(1, 0), 0);
        // pas de débordement sur une machine allumée depuis longtemps (QPC 10 MHz, 100 jours)
        let big = 10_000_000i64 * 86_400 * 100;
        assert_eq!(qpc_to_100ns(big, 10_000_000), big);
        assert_eq!(qpc_to_100ns(big / 10 * 24, 24_000_000), big);
    }
}
