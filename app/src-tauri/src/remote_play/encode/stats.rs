//! Mesures de la chaîne capture → encodage : uniquement des valeurs observées, jamais estimées.

use std::fmt;
use std::time::Duration;

/// Résumé d'une série de durées, en millisecondes.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Summary {
    pub count: usize,
    pub min: f64,
    pub mean: f64,
    pub p50: f64,
    pub p95: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Default)]
pub struct Series(Vec<f64>);

impl Series {
    pub fn push(&mut self, d: Duration) {
        self.0.push(d.as_secs_f64() * 1000.0);
    }

    /// Valeur signée, en millisecondes.
    pub fn push_ms(&mut self, ms: f64) {
        if ms.is_finite() {
            self.0.push(ms);
        }
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn summary(&self) -> Summary {
        if self.0.is_empty() {
            return Summary::default();
        }
        let mut v = self.0.clone();
        v.sort_by(|a, b| a.total_cmp(b));
        // rang le plus proche (nearest-rank)
        let rank = |p: f64| v[((p * v.len() as f64).ceil() as usize).clamp(1, v.len()) - 1];
        Summary {
            count: v.len(),
            min: v[0],
            mean: v.iter().sum::<f64>() / v.len() as f64,
            p50: rank(0.50),
            p95: rank(0.95),
            max: v[v.len() - 1],
        }
    }
}

impl fmt::Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.count == 0 {
            return write!(f, "aucune mesure");
        }
        write!(
            f,
            "moy {:.2} ms · médiane {:.2} · p95 {:.2} · min {:.2} · max {:.2} ({} mesures)",
            self.mean, self.p50, self.p95, self.min, self.max, self.count
        )
    }
}

/// Bilan d'un enregistrement capture → encodage.
#[derive(Debug, Clone, Default)]
pub struct Measurements {
    pub adapter: String,
    pub encoder: String,
    pub hardware: bool,
    pub source: (u32, u32),
    pub output: (u32, u32),
    pub target_fps: u32,
    pub bitrate_kbps: u32,
    pub elapsed: Duration,
    /// Images nouvelles reçues de WGC (WGC n'en livre que si l'écran change).
    pub captured: u32,
    /// Images rendues à WGC sans traitement (une plus récente attendait).
    pub skipped: u64,
    pub encoded: u32,
    /// Images réencodées faute d'image nouvelle (le flux garde sa cadence).
    pub repeated: u32,
    pub keyframes: u32,
    pub bytes: u64,
    pub resizes: u32,
    /// Changements de débit acceptés par l'encodeur en cours de flux (adaptation au réseau).
    pub bitrate_changes: u32,
    pub borderless: bool,
    /// La source (fenêtre, écran) a disparu pendant l'enregistrement.
    pub source_closed: bool,
    /// Intervalle entre deux images nouvelles reçues de WGC.
    pub capture_interval: Series,
    /// Horodatage WGC − instant de réception (signé ; voir `wgc::Frame`).
    pub wgc_timestamp_offset: Series,
    /// Copie + conversion BGRA→NV12 (soumission au GPU, côté processeur).
    pub convert: Series,
    /// Envoi à l'encodeur → paquet H.264 disponible (inclut l'attente de la conversion GPU).
    pub encode: Series,
    /// Image nouvelle reçue de WGC → envoi à l'encodeur (attente d'un jeton de cadence).
    pub wait: Series,
    /// Images nouvelles jamais encodées parce que la file d'envoi du réseau n'était pas vide.
    pub network_skipped: u64,
}

pub fn per_second(count: u64, elapsed: Duration) -> f64 {
    let s = elapsed.as_secs_f64();
    if s > 0.0 {
        count as f64 / s
    } else {
        0.0
    }
}

impl Measurements {
    pub fn capture_fps(&self) -> f64 {
        per_second(self.captured as u64, self.elapsed)
    }

    pub fn encode_fps(&self) -> f64 {
        per_second(self.encoded as u64, self.elapsed)
    }

    pub fn actual_kbps(&self) -> f64 {
        per_second(self.bytes * 8, self.elapsed) / 1000.0
    }
}

impl fmt::Display for Measurements {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "GPU            : {}", self.adapter)?;
        writeln!(f, "encodeur       : {} ({})", self.encoder, if self.hardware { "matériel" } else { "logiciel" })?;
        writeln!(f, "source → sortie: {}×{} → {}×{} · cible {} i/s · CBR {} kbit/s", self.source.0, self.source.1, self.output.0, self.output.1, self.target_fps, self.bitrate_kbps)?;
        writeln!(f, "durée          : {:.2} s · bordure retirée : {} · redimensionnements : {}", self.elapsed.as_secs_f64(), self.borderless, self.resizes)?;
        writeln!(f, "capture        : {} images nouvelles ({:.1} i/s), {} sautées", self.captured, self.capture_fps(), self.skipped)?;
        writeln!(f, "encodage       : {} images ({:.1} i/s, dont {} répétées), {} IDR, {} octets ({:.0} kbit/s réels)", self.encoded, self.encode_fps(), self.repeated, self.keyframes, self.bytes, self.actual_kbps())?;
        if self.source_closed {
            writeln!(f, "source fermée  : capture arrêtée")?;
        }
        writeln!(f, "intervalle WGC : {}", self.capture_interval.summary())?;
        writeln!(f, "horodatage WGC : {} (horodatage − réception)", self.wgc_timestamp_offset.summary())?;
        writeln!(f, "conversion     : {}", self.convert.summary())?;
        write!(f, "encodage       : {}", self.encode.summary())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(v: &[u64]) -> Series {
        let mut s = Series::default();
        for &x in v {
            s.push(Duration::from_millis(x));
        }
        s
    }

    #[test]
    fn summary_values() {
        let s = ms(&[5, 1, 3, 2, 4]).summary();
        assert_eq!((s.count, s.min, s.p50, s.max), (5, 1.0, 3.0, 5.0));
        assert!((s.mean - 3.0).abs() < 1e-9);
        assert_eq!(s.p95, 5.0);
        let s = ms(&(1..=100).collect::<Vec<_>>()).summary();
        assert_eq!((s.p50, s.p95), (50.0, 95.0));
        assert_eq!(Series::default().summary(), Summary::default());
        assert_eq!(Series::default().summary().to_string(), "aucune mesure");
        let mut signed = Series::default();
        signed.push_ms(-2.5);
        signed.push_ms(f64::NAN);
        signed.push_ms(1.5);
        assert_eq!((signed.len(), signed.summary().min, signed.summary().max), (2, -2.5, 1.5));
    }

    #[test]
    fn rates() {
        let m = Measurements { captured: 120, encoded: 300, bytes: 1_250_000, elapsed: Duration::from_secs(5), ..Default::default() };
        assert_eq!(m.capture_fps(), 24.0);
        assert_eq!(m.encode_fps(), 60.0);
        assert_eq!(m.actual_kbps(), 2000.0);
        assert_eq!(per_second(10, Duration::ZERO), 0.0);
    }
}
