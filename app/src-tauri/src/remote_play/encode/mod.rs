//! Encodage vidéo de l'hôte Remote Play (Phase 1b) : conversion BGRA→NV12 sur la carte graphique
//! (processeur vidéo D3D11) puis H.264 par un MFT Media Foundation matériel (NVENC, AMF,
//! Quick Sync), avec repli sur l'encodeur logiciel de Microsoft. Les codecs viennent du système
//! ou du pilote : iBox n'en distribue aucun.

#![allow(dead_code)] // branché sur le transport en Phase 1c

pub mod h264;
pub mod stats;

#[cfg(windows)]
pub mod convert;
#[cfg(windows)]
pub mod mf;
#[cfg(windows)]
pub mod pipeline;

/// Profil H.264 annoncé dans le SDP (`profile-level-id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    /// Compris par tous les décodeurs WebRTC (pas de CABAC ni d'images B).
    Baseline,
    Main,
    High,
}

impl Profile {
    /// Valeur `eAVEncH264VProfile` de Media Foundation.
    pub fn mf_value(self) -> u32 {
        match self {
            Profile::Baseline => 66,
            Profile::Main => 77,
            Profile::High => 100,
        }
    }

    /// Profils à essayer dans l'ordre : le profil demandé, puis les plus simples (un décodeur
    /// High lit du Main et du Baseline ; l'inverse est faux).
    pub fn fallbacks(self) -> &'static [Profile] {
        match self {
            Profile::High => &[Profile::High, Profile::Main, Profile::Baseline],
            Profile::Main => &[Profile::Main, Profile::Baseline],
            Profile::Baseline => &[Profile::Baseline],
        }
    }
}

#[cfg(test)]
mod profile_tests {
    use super::Profile;

    #[test]
    fn profile_fallbacks_only_go_down() {
        assert_eq!(Profile::High.fallbacks(), &[Profile::High, Profile::Main, Profile::Baseline]);
        assert_eq!(Profile::Baseline.fallbacks(), &[Profile::Baseline]);
    }
}

/// Qui encode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preference {
    /// Matériel d'abord, logiciel Microsoft en dernier recours.
    Auto,
    HardwareOnly,
    SoftwareOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncoderConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// Débit constant (CBR), en kbit/s.
    pub bitrate_kbps: u32,
    /// Images entre deux IDR ; 0 = jamais d'office (seulement sur demande, cas du streaming).
    pub gop: u32,
    pub profile: Profile,
}

pub const MAX_WIDTH: u32 = 4096;
pub const MAX_HEIGHT: u32 = 2304; // H.264 niveau 5.1 : 36 864 macroblocs par image
pub const MIN_SIDE: u32 = 64;

impl Default for EncoderConfig {
    fn default() -> Self {
        EncoderConfig { width: 1920, height: 1080, fps: 60, bitrate_kbps: 20_000, gop: 0, profile: Profile::High }
    }
}

impl EncoderConfig {
    /// Bornes dures : dimensions paires (NV12) dans les limites H.264 5.1, fps et débit raisonnables.
    pub fn sanitized(self) -> Self {
        let even = |v: u32, max: u32| v.clamp(MIN_SIDE, max) & !1;
        let fps = self.fps.clamp(1, 240);
        EncoderConfig {
            width: even(self.width, MAX_WIDTH),
            height: even(self.height, MAX_HEIGHT),
            fps,
            bitrate_kbps: self.bitrate_kbps.clamp(250, 200_000),
            gop: self.gop.min(fps * 60),
            profile: self.profile,
        }
    }

    /// Durée d'une image en unités de 100 ns (horodatage Media Foundation).
    pub fn frame_duration_100ns(&self) -> i64 {
        10_000_000 / self.fps.max(1) as i64
    }
}

/// Taille de sortie pour une source donnée : même taille si elle tient, sinon réduite en gardant
/// les proportions ; toujours paire.
pub fn output_size_for(source: (u32, u32), max_height: Option<u32>) -> (u32, u32) {
    let (w, h) = (source.0.max(MIN_SIDE) as u64, source.1.max(MIN_SIDE) as u64);
    let cap_h = max_height.unwrap_or(MAX_HEIGHT).clamp(MIN_SIDE, MAX_HEIGHT) as u64;
    let mut scale_num = 1u64;
    let mut scale_den = 1u64;
    let mut apply = |num: u64, den: u64| {
        if num * scale_den < scale_num * den {
            scale_num = num;
            scale_den = den;
        }
    };
    apply(cap_h, h);
    apply(MAX_WIDTH as u64, w);
    let ow = (w * scale_num / scale_den) as u32;
    let oh = (h * scale_num / scale_den) as u32;
    (ow.clamp(MIN_SIDE, MAX_WIDTH) & !1, oh.clamp(MIN_SIDE, MAX_HEIGHT) & !1)
}

/// Rectangle (gauche, haut, droite, bas) où dessiner la source dans l'image encodée : la plus
/// grande possible sans déformation, centrée, bandes noires ailleurs. Coordonnées paires (NV12).
pub fn fit_rect(source: (u32, u32), output: (u32, u32)) -> (i32, i32, i32, i32) {
    let (sw, sh) = (source.0.max(1) as u64, source.1.max(1) as u64);
    let (ow, oh) = (output.0 as u64, output.1 as u64);
    let (w, h) = if sw * oh >= sh * ow { (ow, (sh * ow / sw).min(oh)) } else { ((sw * oh / sh).min(ow), oh) };
    let (w, h) = (w & !1, h & !1);
    let x = ((ow - w) / 2) & !1;
    let y = ((oh - h) / 2) & !1;
    (x as i32, y as i32, (x + w) as i32, (y + h) as i32)
}

/// `MFT_ENUM_HARDWARE_VENDOR_ID_Attribute` (« VEN_10DE ») → identifiant PCI du fabricant.
pub fn parse_vendor(attribute: &str) -> Option<u32> {
    let hex = attribute.trim().strip_prefix("VEN_").or_else(|| attribute.trim().strip_prefix("ven_"))?;
    u32::from_str_radix(hex.get(..4)?, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendor_attribute() {
        assert_eq!(parse_vendor("VEN_10DE"), Some(0x10DE));
        assert_eq!(parse_vendor("VEN_1002"), Some(0x1002));
        assert_eq!(parse_vendor("VEN_8086&DEV_1234"), Some(0x8086));
        assert_eq!(parse_vendor(""), None);
        assert_eq!(parse_vendor("VEN_XYZ1"), None);
        assert_eq!(parse_vendor("10DE"), None);
    }

    #[test]
    fn config_is_bounded_and_even() {
        let c = EncoderConfig { width: 1365, height: 9999, fps: 0, bitrate_kbps: 1, gop: 1_000_000, profile: Profile::Main }.sanitized();
        assert_eq!((c.width, c.height, c.fps, c.bitrate_kbps), (1364, MAX_HEIGHT, 1, 250));
        assert_eq!(c.gop, 60);
        let c = EncoderConfig { width: 10, fps: 1000, bitrate_kbps: u32::MAX, ..Default::default() }.sanitized();
        assert_eq!((c.width, c.fps, c.bitrate_kbps), (MIN_SIDE, 240, 200_000));
        assert_eq!(EncoderConfig::default().frame_duration_100ns(), 166_666);
    }

    #[test]
    fn output_size_keeps_aspect_and_parity() {
        assert_eq!(output_size_for((1920, 1080), None), (1920, 1080));
        assert_eq!(output_size_for((2560, 1440), Some(1080)), (1920, 1080));
        assert_eq!(output_size_for((3840, 2160), None), (3840, 2160));
        assert_eq!(output_size_for((5120, 1440), None), (4096, 1152), "ultra-large limité en largeur");
        assert_eq!(output_size_for((801, 601), None), (800, 600));
        assert_eq!(output_size_for((7680, 4320), None), (4096, 2304));
    }

    #[test]
    fn fit_rect_letterboxes() {
        assert_eq!(fit_rect((1920, 1080), (1920, 1080)), (0, 0, 1920, 1080));
        assert_eq!(fit_rect((3840, 2160), (1920, 1080)), (0, 0, 1920, 1080));
        // 4:3 dans du 16:9 : bandes à gauche et à droite
        assert_eq!(fit_rect((1024, 768), (1920, 1080)), (240, 0, 1680, 1080));
        // fenêtre très large : bandes en haut et en bas
        assert_eq!(fit_rect((2000, 500), (1920, 1080)), (0, 300, 1920, 780));
        for (s, o) in [((1001, 777), (1280, 720)), ((333, 999), (1920, 1080)), ((1, 1), (64, 64))] {
            let (l, t, r, b) = fit_rect(s, o);
            assert!(l >= 0 && t >= 0 && r as u32 <= o.0 && b as u32 <= o.1);
            assert!(l % 2 == 0 && t % 2 == 0 && r % 2 == 0 && b % 2 == 0);
        }
    }

    #[test]
    fn profiles_match_media_foundation() {
        assert_eq!(Profile::Baseline.mf_value(), 66);
        assert_eq!(Profile::High.mf_value(), 100);
    }
}
