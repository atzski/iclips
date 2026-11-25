//! Lecture minimale d'un flux H.264 « Annex B » (codes de départ 00 00 01 / 00 00 00 01) :
//! découpage en unités NAL, repérage des IDR et des jeux de paramètres (SPS/PPS), regroupement en
//! images. Sert à garantir que chaque IDR transporte ses SPS/PPS (indispensable pour qu'un invité
//! qui arrive en cours de route puisse décoder) et à vérifier les fichiers produits.

pub const NAL_SLICE: u8 = 1;
pub const NAL_IDR: u8 = 5;
pub const NAL_SEI: u8 = 6;
pub const NAL_SPS: u8 = 7;
pub const NAL_PPS: u8 = 8;
pub const NAL_AUD: u8 = 9;

/// Unités NAL d'un tampon Annex B, sans leur code de départ. Un tampon sans code de départ est
/// considéré comme une seule unité.
pub fn nal_units(data: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new(); // (début du code, début de la charge)
    let mut i = 0;
    while i + 2 < data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            let code = if i > 0 && data[i - 1] == 0 { i - 1 } else { i };
            starts.push((code, i + 3));
            i += 3;
        } else {
            i += 1;
        }
    }
    if starts.is_empty() {
        return if data.is_empty() { Vec::new() } else { vec![data] };
    }
    let mut out = Vec::with_capacity(starts.len());
    for (n, &(_, payload)) in starts.iter().enumerate() {
        let end = starts.get(n + 1).map(|&(code, _)| code).unwrap_or(data.len());
        // zéros de remplissage en fin d'unité (trailing_zero_8bits)
        let mut end = end.max(payload);
        while end > payload && data[end - 1] == 0 {
            end -= 1;
        }
        if end > payload {
            out.push(&data[payload..end]);
        }
    }
    out
}

pub fn nal_type(nal: &[u8]) -> u8 {
    nal.first().map(|b| b & 0x1F).unwrap_or(0)
}

/// Le tampon contient une image IDR (point d'accès aléatoire).
pub fn is_keyframe(data: &[u8]) -> bool {
    nal_units(data).iter().any(|n| nal_type(n) == NAL_IDR)
}

pub fn has_parameter_sets(data: &[u8]) -> bool {
    let units = nal_units(data);
    units.iter().any(|n| nal_type(n) == NAL_SPS) && units.iter().any(|n| nal_type(n) == NAL_PPS)
}

/// SPS et PPS d'un tampon, réécrits avec des codes de départ de 4 octets ; vide s'il en manque un.
pub fn parameter_sets(data: &[u8]) -> Vec<u8> {
    let units = nal_units(data);
    let mut out = Vec::new();
    for wanted in [NAL_SPS, NAL_PPS] {
        let mut found = false;
        for n in units.iter().filter(|n| nal_type(n) == wanted) {
            out.extend_from_slice(&[0, 0, 0, 1]);
            out.extend_from_slice(n);
            found = true;
        }
        if !found {
            return Vec::new();
        }
    }
    out
}

/// En-tête de séquence Media Foundation (`MF_MT_MPEG_SEQUENCE_HEADER`) → SPS/PPS Annex B.
/// Accepte l'Annex B ou un enregistrement `avcC` (ISO 14496-15).
pub fn sequence_header_to_annexb(header: &[u8]) -> Vec<u8> {
    if header.starts_with(&[0, 0, 1]) || header.starts_with(&[0, 0, 0, 1]) {
        return parameter_sets(header);
    }
    if header.len() < 7 || header[0] != 1 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut pos = 5;
    let read_sets = |count: usize, pos: &mut usize, out: &mut Vec<u8>| -> bool {
        for _ in 0..count {
            let Some(len) = header.get(*pos..*pos + 2).map(|b| u16::from_be_bytes([b[0], b[1]]) as usize) else { return false };
            let Some(set) = header.get(*pos + 2..*pos + 2 + len) else { return false };
            out.extend_from_slice(&[0, 0, 0, 1]);
            out.extend_from_slice(set);
            *pos += 2 + len;
        }
        true
    };
    let sps = (header[pos] & 0x1F) as usize;
    pos += 1;
    if !read_sets(sps, &mut pos, &mut out) {
        return Vec::new();
    }
    let Some(&pps) = header.get(pos) else { return Vec::new() };
    pos += 1;
    if !read_sets(pps as usize, &mut pos, &mut out) {
        return Vec::new();
    }
    if has_parameter_sets(&out) {
        out
    } else {
        Vec::new()
    }
}

/// Garantit qu'une image IDR commence par ses SPS/PPS (préfixe `cached` si le codeur ne les a
/// pas répétés). Les autres images sont renvoyées telles quelles.
pub fn with_parameter_sets(packet: Vec<u8>, cached: &[u8]) -> Vec<u8> {
    if cached.is_empty() || !is_keyframe(&packet) || has_parameter_sets(&packet) {
        return packet;
    }
    let mut out = Vec::with_capacity(cached.len() + packet.len());
    out.extend_from_slice(cached);
    out.extend_from_slice(&packet);
    out
}

/// Première tranche d'une image : `first_mb_in_slice` (ue(v)) vaut 0, donc son premier bit est 1.
fn starts_picture(nal: &[u8]) -> bool {
    matches!(nal_type(nal), NAL_SLICE | NAL_IDR) && nal.get(1).is_some_and(|b| b & 0x80 != 0)
}

/// Regroupe un flux Annex B en images (« access units ») : une nouvelle image commence à un AUD,
/// SPS, PPS ou SEI qui suit une tranche, ou à une tranche dont `first_mb_in_slice` vaut 0.
pub fn access_units(stream: &[u8]) -> Vec<Vec<u8>> {
    let mut units: Vec<Vec<u8>> = Vec::new();
    let mut current: Vec<u8> = Vec::new();
    let mut has_slice = false;
    for nal in nal_units(stream) {
        let t = nal_type(nal);
        let boundary = has_slice && (matches!(t, NAL_AUD | NAL_SPS | NAL_PPS | NAL_SEI) || starts_picture(nal));
        if boundary {
            units.push(std::mem::take(&mut current));
            has_slice = false;
        }
        current.extend_from_slice(&[0, 0, 0, 1]);
        current.extend_from_slice(nal);
        if matches!(t, NAL_SLICE | NAL_IDR) {
            has_slice = true;
        }
    }
    if has_slice {
        units.push(current);
    }
    units
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPS: &[u8] = &[0x67, 0x64, 0x00, 0x28, 0xAC];
    const PPS: &[u8] = &[0x68, 0xEE, 0x3C, 0x80];
    const IDR: &[u8] = &[0x65, 0x88, 0x84, 0x00, 0x33];
    const IDR_PART2: &[u8] = &[0x65, 0x40, 0x11]; // first_mb_in_slice ≠ 0
    const P: &[u8] = &[0x41, 0x9A, 0x02];

    fn annexb(nals: &[&[u8]], long: bool) -> Vec<u8> {
        let mut v = Vec::new();
        for n in nals {
            v.extend_from_slice(if long { &[0, 0, 0, 1][..] } else { &[0, 0, 1][..] });
            v.extend_from_slice(n);
        }
        v
    }

    #[test]
    fn splits_nal_units_with_both_start_codes() {
        let mut data = annexb(&[SPS, PPS], true);
        data.extend(annexb(&[IDR], false));
        data.extend_from_slice(&[0, 0]); // remplissage
        assert_eq!(nal_units(&data), vec![SPS, PPS, IDR]);
        assert_eq!(nal_units(&[]), Vec::<&[u8]>::new());
        assert_eq!(nal_units(&[0x41, 0x01]), vec![&[0x41u8, 0x01][..]], "sans code de départ");
        assert_eq!(nal_units(&[0, 0, 1]), Vec::<&[u8]>::new(), "unité vide ignorée");
    }

    #[test]
    fn keyframes_and_parameter_sets() {
        let key = annexb(&[SPS, PPS, IDR], true);
        assert!(is_keyframe(&key) && has_parameter_sets(&key));
        let p = annexb(&[P], true);
        assert!(!is_keyframe(&p) && !has_parameter_sets(&p));
        assert_eq!(parameter_sets(&key), annexb(&[SPS, PPS], true));
        assert!(parameter_sets(&annexb(&[SPS, IDR], true)).is_empty(), "PPS manquant");
    }

    #[test]
    fn idr_gets_cached_parameter_sets() {
        let cached = annexb(&[SPS, PPS], true);
        let bare = annexb(&[IDR], true);
        assert_eq!(with_parameter_sets(bare.clone(), &cached), annexb(&[SPS, PPS, IDR], true));
        let full = annexb(&[SPS, PPS, IDR], false);
        assert_eq!(with_parameter_sets(full.clone(), &cached), full, "déjà complet");
        let p = annexb(&[P], true);
        assert_eq!(with_parameter_sets(p.clone(), &cached), p, "image P inchangée");
        assert_eq!(with_parameter_sets(bare.clone(), &[]), bare, "rien en cache");
    }

    #[test]
    fn sequence_header_formats() {
        let annex = annexb(&[SPS, PPS], false);
        assert_eq!(sequence_header_to_annexb(&annex), annexb(&[SPS, PPS], true));
        // avcC : version, profil, compat, niveau, 0xFF, 0xE1 (1 SPS), longueur, SPS, 1 PPS, longueur, PPS
        let mut avcc = vec![1, 0x64, 0x00, 0x28, 0xFF, 0xE1, 0, SPS.len() as u8];
        avcc.extend_from_slice(SPS);
        avcc.extend_from_slice(&[1, 0, PPS.len() as u8]);
        avcc.extend_from_slice(PPS);
        assert_eq!(sequence_header_to_annexb(&avcc), annexb(&[SPS, PPS], true));
        assert!(sequence_header_to_annexb(&avcc[..avcc.len() - 2]).is_empty(), "tronqué");
        assert!(sequence_header_to_annexb(&[]).is_empty());
        assert!(sequence_header_to_annexb(&[2, 0, 0, 0, 0, 0, 0]).is_empty());
    }

    #[test]
    fn groups_access_units() {
        let stream = [
            annexb(&[SPS, PPS, IDR, IDR_PART2], true),
            annexb(&[P], true),
            annexb(&[P], false),
            annexb(&[SPS, PPS, IDR], true),
        ]
        .concat();
        let aus = access_units(&stream);
        assert_eq!(aus.len(), 4);
        assert_eq!(aus[0], annexb(&[SPS, PPS, IDR, IDR_PART2], true), "deux tranches, une image");
        assert_eq!(aus[1], annexb(&[P], true));
        assert!(is_keyframe(&aus[3]) && has_parameter_sets(&aus[3]));
        assert!(access_units(&annexb(&[SPS, PPS], true)).is_empty(), "pas d'image sans tranche");
    }
}
