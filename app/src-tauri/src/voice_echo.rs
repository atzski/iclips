//! « Clip ça ! » : la phrase vient-elle du joueur ou des enceintes ?
//!
//! Le moteur vocal de Windows range sous « Clip ça ! » des bouts de musique, de jeu ou de
//! notification que le micro entend sortir des enceintes : dans le journal d'un vrai joueur
//! (26/11/2026), ces faux « Clip ça ! » ont les mêmes scores (0,5 à 0,78) et les mêmes durées
//! (0,3 à 3,4 s) que sa vraie voix. Aucun seuil de score ne les sépare : sept versions de
//! réglages l'ont montré.
//!
//! Ce module écoute donc, en même temps que le moteur vocal, le micro et ce que Windows envoie
//! aux enceintes (loopback), en énergie par tranche de 10 ms dans la bande de la voix (300 à
//! 3 400 Hz). Quand une phrase est reconnue, il apprend sur les 10 s d'avant ce que les enceintes
//! mettent dans le micro (décalage et gain, traîne de la pièce comprise), prédit l'écho pendant
//! la phrase et mesure de combien le micro le dépasse. La voix du joueur, tout près du micro,
//! le dépasse nettement ; un son des enceintes, non. Rien n'est enregistré : seules des
//! énergies des 20 dernières secondes restent en mémoire.

use std::collections::VecDeque;

/// Tranche d'analyse : 10 ms.
pub const FRAME_MS: f64 = 10.0;
/// En dessous (dB, bande de la voix), les enceintes sont muettes.
pub const SILENT_DB: f32 = -70.0;
/// La phrase doit dépasser l'écho prédit (et le bruit de fond) d'au moins tant de dB. Mesures
/// réelles (26/11/2026, micro Yeti) : la voix du joueur qui dit « Clip ça ! », de +24 à +42 dB ;
/// une phrase jouée par des haut-parleurs d'écran collés au micro, de -26 à +19 dB (une fois +25).
pub const VOICE_DB: f32 = 20.0;
/// « Clip ça ! » est une commande isolée : le micro doit être plus calme d'au moins tant de dB
/// dans les 0,5 s d'avant et d'après (sinon : une conversation, une vidéo, une chanson, qui ne
/// laissent presque pas de calme). Mesure : les phrases du joueur, 6 à 44 dB.
pub const ISOLATION_DB: f32 = 5.0;
/// Tranches gardées après la phrase pour juger de l'isolement (0,8 s).
pub const TAIL_FRAMES: usize = 80;
/// Décalage cherché entre les enceintes et le micro : 0 à 500 ms.
const MAX_LAG: usize = 50;
/// Traîne de la pièce : l'écho d'un son décroît d'au plus 1,5 dB par tranche (60 dB en 0,4 s).
const DECAY: f32 = 0.707_945_8; // 10^(-1,5/10)
/// Apprentissage : 10 s avant la phrase, sans les 300 dernières ms (le début de la phrase).
pub const REF_FRAMES: usize = 1_000;
const GUARD: usize = 30;
/// Il faut au moins 1 s d'enceintes actives pour apprendre le gain.
const MIN_ACTIVE: usize = 100;
/// Marge autour de la phrase (l'horloge du moteur vocal n'est pas exacte à la tranche près).
pub const PAD_FRAMES: usize = 15;

fn lin(db: f32) -> f32 {
    10f32.powf(db / 10.0)
}

fn db(x: f32) -> f32 {
    10.0 * x.max(1e-14).log10()
}

/// Énergie par tranche de 10 ms dans la bande de la voix : passe-haut 300 Hz puis passe-bas
/// 3 400 Hz (biquads de 2e ordre), son mono à `rate` Hz.
pub struct Band {
    hp: Biquad,
    lp: Biquad,
    size: usize,
    acc: f64,
    n: usize,
}

#[derive(Clone, Copy)]
struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    z: [f32; 2],
}

impl Biquad {
    fn new(rate: f32, freq: f32, high: bool) -> Self {
        let w = std::f32::consts::TAU * freq / rate;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
        let a0 = 1.0 + alpha;
        let b = if high { [(1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0] } else { [(1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0] };
        Biquad { b: [b[0] / a0, b[1] / a0, b[2] / a0], a: [-2.0 * cos / a0, (1.0 - alpha) / a0], z: [0.0; 2] }
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

impl Band {
    pub fn new(rate: u32) -> Self {
        let r = rate as f32;
        Band { hp: Biquad::new(r, 300.0, true), lp: Biquad::new(r, 3_400.0, false), size: (rate as usize / 100).max(1), acc: 0.0, n: 0 }
    }

    /// Ajoute des échantillons mono ; rend (indice de l'échantillon de fin, dB) de chaque tranche finie.
    pub fn push(&mut self, mono: &[f32], out: &mut Vec<(usize, f32)>) {
        for (i, &x) in mono.iter().enumerate() {
            let y = self.lp.run(self.hp.run(x));
            self.acc += (y * y) as f64;
            self.n += 1;
            if self.n == self.size {
                out.push((i, db((self.acc / self.size as f64) as f32)));
                self.acc = 0.0;
                self.n = 0;
            }
        }
    }
}

/// Énergies (dB) d'un son entier, tranche par tranche (essais sur fichiers).
#[allow(dead_code)]
pub fn frames(mono: &[f32], rate: u32) -> Vec<f32> {
    let mut b = Band::new(rate);
    let mut out = Vec::new();
    b.push(mono, &mut out);
    out.into_iter().map(|(_, d)| d).collect()
}

/// Ce que le micro a entendu pendant la phrase, comparé aux enceintes.
#[derive(Clone, Debug, PartialEq)]
pub struct Verdict {
    /// Sortie de Windows retenue (celle qui explique le mieux le micro), vide si aucune ne joue.
    pub output: String,
    /// Les enceintes jouaient pendant la phrase (ou juste avant : traîne).
    pub speakers: bool,
    /// Décalage enceintes → micro retenu (ms).
    pub lag_ms: u32,
    /// Gain enceintes → micro appris (dB), `None` : enceintes muettes, rien à apprendre.
    pub gain_db: Option<f32>,
    /// De combien la phrase dépasse l'écho prédit et le bruit de fond (dB).
    pub voice_db: f32,
    /// De combien la crête de la phrase dépasse le micro juste avant et juste après (dB).
    pub isolation_db: f32,
}

impl Verdict {
    /// La phrase vient-elle du joueur ?
    pub fn voice(&self) -> bool {
        self.voice_db >= VOICE_DB
    }

    /// La phrase est-elle dite seule (calme autour) ?
    pub fn isolated(&self) -> bool {
        self.isolation_db >= ISOLATION_DB
    }

    /// Ligne du journal.
    pub fn describe(&self) -> String {
        let room = match (self.speakers, self.gain_db) {
            (false, _) => "enceintes muettes".to_string(),
            (true, Some(g)) => format!("son sur « {} » (écho {g:.0} dB, décalage {} ms)", self.output, self.lag_ms),
            (true, None) => format!("son sur « {} » (écho pas encore mesuré)", self.output),
        };
        format!(
            "{room}, voix {:+.1} dB au-dessus {} (il faut {VOICE_DB:.0}), {:.1} dB au-dessus des 0,5 s autour (il faut {ISOLATION_DB:.0})",
            self.voice_db,
            if self.speakers { "de l'écho" } else { "du bruit de fond" },
            self.isolation_db
        )
    }
}

fn percentile(mut v: Vec<f32>, p: f32) -> Option<f32> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    Some(v[((v.len() - 1) as f32 * p).round() as usize])
}

/// Écho que les enceintes peuvent mettre dans le micro à chaque tranche (puissance, avant gain) :
/// le son des enceintes décalé de `lag`, avec la traîne de la pièce.
fn smear(spk: &[f32], lag: usize) -> Vec<f32> {
    let mut out = vec![0f32; spk.len()];
    let mut prev = 0f32;
    for i in 0..spk.len() {
        let x = if i >= lag { lin(spk[i - lag]) } else { 0.0 };
        prev = x.max(prev * DECAY);
        out[i] = prev;
    }
    out
}

/// Corrélation des énergies (dB) du micro et des enceintes décalées de `lag`, sur les tranches
/// où les enceintes jouent.
fn correlation(mic: &[f32], spk: &[f32], lag: usize, end: usize) -> Option<f32> {
    let pairs: Vec<(f32, f32)> = (lag..end).filter(|&i| spk[i - lag] > SILENT_DB).map(|i| (mic[i], spk[i - lag])).collect();
    if pairs.len() < MIN_ACTIVE {
        return None;
    }
    let n = pairs.len() as f32;
    let (mx, my) = (pairs.iter().map(|p| p.0).sum::<f32>() / n, pairs.iter().map(|p| p.1).sum::<f32>() / n);
    let (mut sxy, mut sxx, mut syy) = (0f32, 0f32, 0f32);
    for (x, y) in pairs {
        sxy += (x - mx) * (y - my);
        sxx += (x - mx) * (x - mx);
        syy += (y - my) * (y - my);
    }
    (sxx > 0.0 && syy > 0.0).then(|| sxy / (sxx * syy).sqrt())
}

/// Jugement d'une phrase. `mic` et `spk` : énergies (dB) sur la même grille de 10 ms, des
/// 10 s d'avant jusqu'à 0,8 s après la phrase ; `phrase` : ses tranches (marge comprise) ;
/// `known_gain` : dernier gain appris (enceintes muettes pendant les 10 s d'avant).
pub fn judge(mic: &[f32], spk: &[f32], phrase: std::ops::Range<usize>, known_gain: Option<f32>) -> Verdict {
    let n = mic.len().min(spk.len());
    let phrase = phrase.start.min(n)..phrase.end.min(n);
    // isolement : crête de la phrase contre le micro de 0,55 à 0,1 s avant et de 0,1 à 0,55 s après
    let peak = mic[phrase.clone()].iter().fold(f32::MIN, |a, &b| a.max(b));
    let before = phrase.start.saturating_sub(55)..phrase.start.saturating_sub(10);
    let after = (phrase.end + 10).min(n)..(phrase.end + 55).min(n);
    let calm = mic[before].iter().chain(&mic[after]).fold(f32::MIN, |a, &b| a.max(b));
    let isolation_db = if calm == f32::MIN { 99.0 } else { (peak - calm).min(99.0) };
    let ref_end = phrase.start.saturating_sub(GUARD);
    // enceintes actives pendant la phrase, ou juste avant (traîne de la pièce, décalage)
    let around = phrase.start.saturating_sub(MAX_LAG + GUARD)..phrase.end;
    let speakers = spk[around].iter().any(|&s| s > SILENT_DB);
    // décalage : la meilleure corrélation sur tout ce qui est connu (sinon 60 ms, valeur courante)
    let lag = (0..=MAX_LAG)
        .filter_map(|l| correlation(mic, spk, l, n).map(|c| (l, c)))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(l, _)| l)
        .unwrap_or(6);
    let echo = smear(&spk[..n], lag);
    let active = |i: &usize| echo[*i] > lin(SILENT_DB);
    // gain : médiane du rapport micro / écho sur les 10 s d'avant (la voix du joueur, s'il parle
    // par moments, ne déplace pas la médiane)
    let ratios: Vec<f32> = (0..ref_end).filter(active).map(|i| mic[i] - db(echo[i])).collect();
    let learned = if ratios.len() >= MIN_ACTIVE { percentile(ratios, 0.5) } else { None };
    // jamais appris (l'écoute vient de démarrer) : pas d'écho prédit, la phrase n'est comparée
    // qu'au bruit de fond. Deviner le gain sur la phrase même refusait la vraie voix (essai réel
    // du 26/11/2026 : « Clip ça ! » dit 4 s après le début de l'écoute, refusé)
    let gain = learned.or(known_gain);
    // bruit de fond : le bas du micro quand les enceintes se taisent (sinon, prudemment, le bas tout court)
    let quiet: Vec<f32> = (0..ref_end).filter(|i| !active(i)).map(|i| lin(mic[i])).collect();
    let floor = if quiet.len() >= 50 {
        percentile(quiet, 0.1).unwrap_or(0.0)
    } else {
        percentile((0..ref_end).map(|i| lin(mic[i])).collect(), 0.05).unwrap_or(0.0) * 0.5
    };
    let g = gain.map(lin).unwrap_or(0.0);
    let (mut heard, mut expected) = (0f64, 0f64);
    for i in phrase.clone() {
        heard += lin(mic[i]) as f64;
        expected += (g * echo[i] + floor) as f64;
    }
    let voice_db = if expected > 0.0 { db((heard / expected) as f32) } else { 99.0 };
    Verdict { output: String::new(), speakers, lag_ms: (lag as f64 * FRAME_MS) as u32, gain_db: if speakers { gain } else { learned.or(known_gain) }, voice_db: voice_db.min(99.0), isolation_db }
}

/// Énergies d'un son capté en continu, datées (ms de l'horloge du module), 20 s au plus.
#[derive(Default)]
pub struct Track {
    frames: VecDeque<(f64, f32)>,
}

/// 20 s gardées (tout, pendant les essais qui exportent les pistes : `IBOX_VOICE_DUMP`).
fn keep_ms() -> f64 {
    static KEEP: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *KEEP.get_or_init(|| if cfg!(test) && std::env::var_os("IBOX_VOICE_DUMP").is_some() { 1e12 } else { 20_000.0 })
}

impl Track {
    pub fn push(&mut self, at: f64, db: f32) {
        self.frames.push_back((at, db));
        while self.frames.front().is_some_and(|f| at - f.0 > keep_ms()) {
            self.frames.pop_front();
        }
    }

    /// Dernière tranche connue.
    pub fn last(&self) -> Option<f64> {
        self.frames.back().map(|f| f.0)
    }

    /// Énergies sur une grille de `n` tranches à partir de `t0` ; `None` aux trous.
    pub fn grid(&self, t0: f64, n: usize) -> Vec<Option<f32>> {
        (0..n)
            .map(|k| {
                let t = t0 + k as f64 * FRAME_MS;
                let i = self.frames.partition_point(|f| f.0 < t);
                [i.checked_sub(1), Some(i)]
                    .into_iter()
                    .flatten()
                    .filter_map(|j| self.frames.get(j))
                    .min_by(|a, b| (a.0 - t).abs().total_cmp(&(b.0 - t).abs()))
                    .filter(|f| (f.0 - t).abs() <= 1.5 * FRAME_MS)
                    .map(|f| f.1)
            })
            .collect()
    }
}

/// Énergies du micro et des enceintes autour de la phrase qui commence à `start` (ms de
/// l'horloge du module) et dure `ms` : des 10 s d'avant à 0,8 s après sa fin, et ses tranches.
/// `None` : le micro n'a pas été capté assez (rien à juger).
pub fn window(mic: &Track, spk: &Track, start: f64, ms: u32) -> Option<(Vec<f32>, Vec<f32>, std::ops::Range<usize>)> {
    let lead = (REF_FRAMES + MAX_LAG + GUARD) as f64 * FRAME_MS;
    let t0 = start - PAD_FRAMES as f64 * FRAME_MS - lead;
    let end = ((start + ms as f64 + PAD_FRAMES as f64 * FRAME_MS - t0) / FRAME_MS).ceil() as usize;
    let first = ((start - PAD_FRAMES as f64 * FRAME_MS - t0) / FRAME_MS) as usize;
    let total = end + TAIL_FRAMES;
    let m = mic.grid(t0, total);
    let s = spk.grid(t0, total);
    let missing = m[first..end].iter().filter(|x| x.is_none()).count();
    if missing * 5 > end - first {
        return None;
    }
    // après la fin de la capture (fichiers d'essai) : rien autour
    let last = m.iter().rposition(|x| x.is_some()).unwrap_or(0) + 1;
    // avant le début de la capture : le bas du micro, enceintes muettes
    let floor = m.iter().flatten().fold(f32::MAX, |a, &b| a.min(b));
    let mic: Vec<f32> = m.into_iter().take(last.max(end)).map(|x| x.unwrap_or(floor)).collect();
    let spk: Vec<f32> = s.into_iter().take(last.max(end)).map(|x| x.unwrap_or(-120.0)).collect();
    Some((mic, spk, first..end))
}

/// Horloge du module (ms), commune aux deux captures et au moteur vocal.
pub fn clock() -> f64 {
    static ORIGIN: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    ORIGIN.get_or_init(std::time::Instant::now).elapsed().as_secs_f64() * 1_000.0
}

#[cfg(windows)]
pub use monitor::Monitor;

#[cfg(windows)]
mod monitor {
    use super::{clock, Band, Track, Verdict};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;

    /// Une sortie de Windows écoutée (loopback) : son nom, ses énergies, son dernier gain appris.
    struct Output {
        name: String,
        track: Arc<Mutex<Track>>,
        gain: Mutex<Option<f32>>,
    }

    /// Capture en continu du micro et de **toutes** les sorties de Windows (enceintes, casque,
    /// écran…) : un jeu ou une app peut jouer ailleurs que sur la sortie par défaut. S'arrête
    /// quand la valeur est lâchée.
    pub struct Monitor {
        mic: Arc<Mutex<Track>>,
        outputs: Vec<Output>,
        stop: Arc<AtomicBool>,
        threads: Vec<JoinHandle<()>>,
    }

    fn capture(microphone: bool, device: Option<String>, name: String, track: Arc<Mutex<Track>>, stop: Arc<AtomicBool>) -> JoinHandle<()> {
        std::thread::spawn(move || {
            let rate = crate::remote_play::audio::SAMPLE_RATE;
            let mut band = Band::new(rate);
            let mut mono = Vec::new();
            let mut done = Vec::new();
            let r = crate::remote_play::audio::pcm(&stop, microphone, device.as_deref(), &mut |chunk| {
                // le morceau vient d'être lu : sa fin, c'est maintenant
                let now = clock();
                mono.clear();
                mono.extend(chunk.chunks_exact(2).map(|s| (s[0] + s[1]) * 0.5));
                done.clear();
                band.push(&mono, &mut done);
                if let Ok(mut t) = track.lock() {
                    for &(end, d) in &done {
                        let after = (mono.len() - 1 - end) as f64 * 1_000.0 / rate as f64;
                        t.push(now - after - super::FRAME_MS / 2.0, d);
                    }
                }
                !stop.load(Ordering::Relaxed)
            });
            if let Err(e) = r {
                if !stop.load(Ordering::Relaxed) {
                    crate::voice::journal::write(&format!("contrôle des enceintes : « {name} » non écouté ({e})"));
                }
            }
        })
    }

    impl Monitor {
        /// `mic` : identifiant Windows du micro (`None` : celui par défaut).
        pub fn start(mic: Option<String>) -> Self {
            let stop = Arc::new(AtomicBool::new(false));
            let m = Arc::new(Mutex::new(Track::default()));
            let mut threads = vec![capture(true, mic, "micro".into(), m.clone(), stop.clone())];
            let mut outputs = Vec::new();
            for e in crate::remote_play::audio::endpoints(false) {
                let track = Arc::new(Mutex::new(Track::default()));
                threads.push(capture(false, Some(e.id), e.name.clone(), track.clone(), stop.clone()));
                outputs.push(Output { name: e.name, track, gain: Mutex::new(None) });
            }
            Monitor { mic: m, outputs, stop, threads }
        }

        /// Noms des sorties écoutées.
        pub fn outputs(&self) -> Vec<String> {
            self.outputs.iter().map(|o| o.name.clone()).collect()
        }

        /// Juge la phrase qui a commencé à `start` (horloge du module) et duré `ms`, contre
        /// chaque sortie : on garde la sortie qui explique le mieux le micro (le plus petit
        /// dépassement). Attend au plus 0,6 s que le micro ait dépassé la fin de la phrase.
        pub fn judge(&self, start: f64, ms: u32) -> Option<Verdict> {
            let end = start + ms as f64 + (super::PAD_FRAMES + super::TAIL_FRAMES) as f64 * super::FRAME_MS;
            let ready = |t: &Arc<Mutex<Track>>| t.lock().ok().and_then(|t| t.last()).is_some_and(|l| l >= end);
            let t0 = clock();
            while clock() - t0 < 1_000.0 && !ready(&self.mic) {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            let mut best: Option<Verdict> = None;
            let empty = Track::default();
            for o in self.outputs.iter().map(Some).chain(std::iter::once(None)) {
                // `None` : aucune sortie, le micro contre son seul bruit de fond
                let (mic, spk, phrase) = {
                    let m = self.mic.lock().ok()?;
                    match o {
                        Some(o) => super::window(&m, &*o.track.lock().ok()?, start, ms)?,
                        None => super::window(&m, &empty, start, ms)?,
                    }
                };
                // sortie restée muette : rien à en apprendre, le cas « aucune sortie » suffit
                if o.is_some() && spk.iter().all(|&x| x <= super::SILENT_DB) {
                    continue;
                }
                let known = o.and_then(|o| o.gain.lock().ok().and_then(|g| *g));
                let mut v = super::judge(&mic, &spk, phrase.clone(), known);
                if let Some(o) = o {
                    if let (Some(g), Ok(mut k)) = (v.gain_db, o.gain.lock()) {
                        *k = Some(g);
                    }
                    v.output = o.name.clone();
                }
                // essais (`IBOX_VOICE_DUMP` : dossier) : énergies gardées pour les rejouer
                if let Some(dir) = std::env::var_os("IBOX_VOICE_DUMP") {
                    let line = |v: &[f32]| v.iter().map(|x| format!("{x:.1}")).collect::<Vec<_>>().join(" ");
                    let text = format!("{} {}\n{}\n{}\n", phrase.start, phrase.end, line(&mic), line(&spk));
                    let tag: String = o.map(|o| o.name.chars().filter(|c| c.is_alphanumeric()).collect()).unwrap_or_else(|| "aucune".into());
                    let _ = std::fs::write(std::path::Path::new(&dir).join(format!("{:.0}-{tag}.txt", start)), text);
                }
                if best.as_ref().is_none_or(|b| v.voice_db < b.voice_db) {
                    best = Some(v);
                }
            }
            best
        }
    }

    #[cfg(test)]
    impl Monitor {
        /// Même jugement, contre un son de sortie donné (banc d'essai `essai_enceintes`).
        fn judge_against(&self, out: &Track, start: f64, ms: u32) -> Option<Verdict> {
            let (mic, spk, phrase) = super::window(&*self.mic.lock().ok()?, out, start, ms)?;
            Some(super::judge(&mic, &spk, phrase, None))
        }
    }

    /// Banc d'essai acoustique réel. La musique qui joue sur `IBOX_ESSAI_SOURCE` (loopback) est
    /// relayée vers les enceintes `IBOX_ESSAI_SORTIE`, avec, toutes les `IBOX_ESSAI_TOUS` s, la
    /// phrase `IBOX_ESSAI_PHRASE` (WAV 48 kHz 16 bits mono, « Clip ça ! » en synthèse). Le vrai
    /// moteur vocal écoute le micro `IBOX_VOICE_MIC` pendant `IBOX_VOICE_SECS` s. Chaque son
    /// reconnu est jugé deux fois : contre ce qui sort vraiment (la phrase vient des enceintes :
    /// doit être refusée) et contre la musique seule (comme si une personne dans la pièce disait
    /// la phrase : doit passer).
    /// `cargo test --lib essai_enceintes -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn essai_enceintes() {
        use std::collections::VecDeque;
        let env = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("{k}"));
        let find = |capture: bool, part: &str| crate::remote_play::audio::endpoints(capture).into_iter().find(|e| e.name.contains(part)).unwrap_or_else(|| panic!("{part}"));
        let mic = find(true, &env("IBOX_VOICE_MIC"));
        let source = find(false, &env("IBOX_ESSAI_SOURCE"));
        let sink = find(false, &env("IBOX_ESSAI_SORTIE"));
        let every: f64 = std::env::var("IBOX_ESSAI_TOUS").ok().and_then(|s| s.parse().ok()).unwrap_or(20.0);
        let secs: u64 = std::env::var("IBOX_VOICE_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(120);
        let wav = std::fs::read(env("IBOX_ESSAI_PHRASE")).unwrap();
        let data = wav.windows(4).position(|w| w == b"data").unwrap() + 8;
        let loud: f32 = std::env::var("IBOX_ESSAI_GAIN").ok().and_then(|s| s.parse().ok()).unwrap_or(1.0);
        let soft: f32 = std::env::var("IBOX_ESSAI_MUSIQUE").ok().and_then(|s| s.parse().ok()).unwrap_or(1.0);
        let phrase: Vec<f32> = wav[data..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0 * loud).collect();
        println!("micro {} ; musique de {} relayée vers {} ; phrase de {:.2} s toutes les {every} s", mic.name, source.name, sink.name, phrase.len() as f32 / 48_000.0);

        let stop = Arc::new(AtomicBool::new(false));
        let music: Arc<Mutex<VecDeque<f32>>> = Arc::default();
        let only = Arc::new(Mutex::new(Track::default()));
        let said: Arc<Mutex<Vec<f64>>> = Arc::default();
        // relais : loopback de la source
        let (m, st) = (music.clone(), stop.clone());
        let relay = std::thread::spawn(move || {
            let _ = crate::remote_play::audio::pcm(&st, false, Some(&source.id), &mut |c| {
                let mut q = m.lock().unwrap();
                q.extend(c.iter().copied());
                while q.len() > 48_000 * 2 {
                    q.pop_front();
                }
                !st.load(Ordering::Relaxed)
            });
        });
        // rendu vers les enceintes : musique + phrase ; énergies de la musique seule
        let (m, st, o, sd) = (music.clone(), stop.clone(), only.clone(), said.clone());
        let player = std::thread::spawn(move || unsafe {
            use windows::Win32::Media::Audio::*;
            use windows::Win32::System::Com::*;
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).unwrap();
            let device = devices.GetDevice(&windows::core::HSTRING::from(sink.id.as_str())).unwrap();
            let client: IAudioClient = device.Activate(CLSCTX_ALL, None).unwrap();
            let fmt = client.GetMixFormat().unwrap();
            let f = std::ptr::read_unaligned(fmt);
            let (hz, bits) = (f.nSamplesPerSec, f.wBitsPerSample);
            assert!(hz == 48_000 && bits == 32, "format de sortie {hz} Hz {bits} bits");
            let ch = f.nChannels as usize;
            client.Initialize(AUDCLNT_SHAREMODE_SHARED, 0, 1_000_000, 0, fmt, None).unwrap();
            let render: IAudioRenderClient = client.GetService().unwrap();
            let size = client.GetBufferSize().unwrap();
            client.Start().unwrap();
            let mut band = Band::new(48_000);
            let (mut next, mut pos) = (clock() + 15_000.0, usize::MAX);
            let mut mono = Vec::new();
            let mut done = Vec::new();
            while !st.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(10));
                let pad = client.GetCurrentPadding().unwrap();
                let free = (size - pad) as usize;
                if free == 0 {
                    continue;
                }
                let buf = render.GetBuffer(free as u32).unwrap() as *mut f32;
                let ahead = pad as f64 / 48.0;
                mono.clear();
                let mut q = m.lock().unwrap();
                for i in 0..free {
                    let (l, r) = if q.len() >= 2 { (q.pop_front().unwrap() * soft, q.pop_front().unwrap() * soft) } else { (0.0, 0.0) };
                    mono.push((l + r) * 0.5);
                    if pos == usize::MAX && clock() + ahead + i as f64 / 48.0 >= next {
                        pos = 0;
                        sd.lock().unwrap().push(clock() + ahead + i as f64 / 48.0);
                    }
                    let v = if pos < phrase.len() { pos += 1; phrase[pos - 1] } else { if pos != usize::MAX { pos = usize::MAX; next += every * 1_000.0; } 0.0 };
                    for c in 0..ch {
                        *buf.add(i * ch + c) = (if c % 2 == 0 { l } else { r }) + v;
                    }
                }
                drop(q);
                render.ReleaseBuffer(free as u32, 0).unwrap();
                done.clear();
                band.push(&mono, &mut done);
                let now = clock();
                let mut t = o.lock().unwrap();
                for &(end, d) in &done {
                    t.push(now + ahead + end as f64 / 48.0 - super::FRAME_MS / 2.0, d);
                }
            }
            let _ = client.Stop();
        });

        let monitor = Arc::new(Monitor::start(Some(mic.id.clone())));
        let (mo, o, sd) = (monitor.clone(), only.clone(), said.clone());
        let gate: crate::voice_sapi::Gate = Box::new(move |start, ms| {
            let real = mo.judge(start, ms);
            let sim = mo.judge_against(&o.lock().unwrap(), start, ms);
            let near = sd.lock().unwrap().iter().any(|&t| (start - t).abs() < 1_500.0);
            println!(
                "  >> {} | réel : {} -> {} | musique seule : {} -> {}",
                if near { "PHRASE JOUÉE" } else { "musique" },
                real.as_ref().map(|v| v.describe()).unwrap_or_default(),
                if real.as_ref().is_none_or(|v| v.voice()) { "voix" } else { "écho" },
                sim.as_ref().map(|v| v.describe()).unwrap_or_default(),
                if sim.as_ref().is_none_or(|v| v.voice()) { "voix" } else { "écho" },
            );
            real
        });
        let l = crate::voice_sapi::Listener::start("fr-FR", crate::voice::phrases("fr-FR"), crate::voice_sapi::Input::Device(mic.name.clone()), Some(gate), || {}).expect("moteur");
        std::thread::sleep(std::time::Duration::from_secs(secs));
        drop(l);
        if let Some(dir) = std::env::var_os("IBOX_VOICE_DUMP") {
            let save = |name: &str, t: &Track| {
                let text: String = t.frames.iter().map(|f| format!("{:.1} {:.1}
", f.0, f.1)).collect();
                let _ = std::fs::write(std::path::Path::new(&dir).join(format!("piste-{name}.txt")), text);
            };
            save("micro", &monitor.mic.lock().unwrap());
            save("musique-seule", &only.lock().unwrap());
            for o in &monitor.outputs {
                save(&o.name.chars().filter(|c| c.is_alphanumeric()).collect::<String>(), &o.track.lock().unwrap());
            }
            let _ = std::fs::write(std::path::Path::new(&dir).join("phrases.txt"), said.lock().unwrap().iter().map(|t| format!("{t:.1}
")).collect::<String>());
        }
        stop.store(true, Ordering::Relaxed);
        let _ = (relay.join(), player.join());
        println!("phrases jouées : {}", said.lock().unwrap().len());
    }

    /// Enregistrement silencieux (rien n'est joué) : le vrai moteur vocal écoute `IBOX_VOICE_MIC`
    /// pendant `IBOX_VOICE_SECS` s avec le contrôle des sorties ; le son du micro est gardé dans
    /// `IBOX_VOICE_DUMP\micro.wav` (48 kHz 16 bits mono) avec les pistes d'énergie, pour rejouer
    /// une vraie voix hors ligne. `cargo test --lib enregistrer_voix -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn enregistrer_voix() {
        let env = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("{k}"));
        let mic = crate::remote_play::audio::endpoints(true).into_iter().find(|e| e.name.contains(&env("IBOX_VOICE_MIC"))).expect("micro");
        let secs: u64 = env("IBOX_VOICE_SECS").parse().unwrap();
        let dir = std::path::PathBuf::from(env("IBOX_VOICE_DUMP"));
        let stop = Arc::new(AtomicBool::new(false));
        let raw: Arc<Mutex<Vec<i16>>> = Arc::default();
        let (st, r, id) = (stop.clone(), raw.clone(), mic.id.clone());
        let t0 = clock();
        let rec = std::thread::spawn(move || {
            let _ = crate::remote_play::audio::pcm(&st, true, Some(&id), &mut |c| {
                r.lock().unwrap().extend(c.chunks_exact(2).map(|s| (((s[0] + s[1]) * 0.5).clamp(-1.0, 1.0) * 32767.0) as i16));
                !st.load(Ordering::Relaxed)
            });
        });
        let monitor = Arc::new(Monitor::start(Some(mic.id.clone())));
        let mo = monitor.clone();
        let gate: crate::voice_sapi::Gate = Box::new(move |start, ms| mo.judge(start, ms));
        let l = crate::voice_sapi::Listener::start("fr-FR", crate::voice::phrases("fr-FR"), crate::voice_sapi::Input::Device(mic.name.clone()), Some(gate), || {}).expect("moteur");
        println!("ENREGISTREMENT {secs} s (début du WAV à {:.2} s)", t0 / 1000.0);
        std::thread::sleep(std::time::Duration::from_secs(secs));
        drop(l);
        stop.store(true, Ordering::Relaxed);
        let _ = rec.join();
        let pcm = raw.lock().unwrap();
        let mut wav = Vec::with_capacity(44 + pcm.len() * 2);
        let data = (pcm.len() * 2) as u32;
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        for v in [16u32.to_le_bytes(), [1, 0, 1, 0], 48_000u32.to_le_bytes(), 96_000u32.to_le_bytes(), [2, 0, 16, 0]] {
            wav.extend_from_slice(&v);
        }
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data.to_le_bytes());
        for s in pcm.iter() {
            wav.extend_from_slice(&s.to_le_bytes());
        }
        std::fs::write(dir.join("micro.wav"), wav).unwrap();
        let save = |name: &str, t: &Track| {
            let text: String = t.frames.iter().map(|f| format!("{:.1} {:.1}\n", f.0, f.1)).collect();
            let _ = std::fs::write(dir.join(format!("piste-{name}.txt")), text);
        };
        save("micro", &monitor.mic.lock().unwrap());
        for o in &monitor.outputs {
            save(&o.name.chars().filter(|c| c.is_alphanumeric()).collect::<String>(), &o.track.lock().unwrap());
        }
        println!("fini : {:.1} s de micro", pcm.len() as f32 / 48_000.0);
    }

    fn write_wav(path: &std::path::Path, pcm: &[i16]) {
        let mut wav = Vec::with_capacity(44 + pcm.len() * 2);
        let data = (pcm.len() * 2) as u32;
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        for v in [16u32.to_le_bytes(), [1, 0, 1, 0], 48_000u32.to_le_bytes(), 96_000u32.to_le_bytes(), [2, 0, 16, 0]] {
            wav.extend_from_slice(&v);
        }
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data.to_le_bytes());
        for s in pcm {
            wav.extend_from_slice(&s.to_le_bytes());
        }
        std::fs::write(path, wav).unwrap();
    }

    /// Enregistrement silencieux (rien n'est joué) du micro `IBOX_VOICE_MIC` et de chaque sortie
    /// de Windows, en WAV 48 kHz mono alignés (même début), pendant `IBOX_VOICE_SECS` s, dans
    /// `IBOX_VOICE_DUMP` : de quoi rejouer hors ligne le moteur vocal et le contrôle des sorties.
    /// `cargo test --lib enregistrer_sons -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn enregistrer_sons() {
        let env = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("{k}"));
        let mic = crate::remote_play::audio::endpoints(true).into_iter().find(|e| e.name.contains(&env("IBOX_VOICE_MIC"))).expect("micro");
        let secs: u64 = env("IBOX_VOICE_SECS").parse().unwrap();
        let dir = std::path::PathBuf::from(env("IBOX_VOICE_DUMP"));
        let stop = Arc::new(AtomicBool::new(false));
        let mut sources = vec![("micro".to_string(), true, mic.id.clone())];
        for e in crate::remote_play::audio::endpoints(false) {
            sources.push((e.name.chars().filter(|c| c.is_alphanumeric()).collect(), false, e.id));
        }
        let t0 = std::time::Instant::now() + std::time::Duration::from_millis(1500);
        let threads: Vec<_> = sources
            .into_iter()
            .map(|(name, capture, id)| {
                let st = stop.clone();
                std::thread::spawn(move || {
                    // horloge commune : on jette ce qui arrive avant `t0`, chaque piste commence au même instant
                    let mut pcm: Vec<i16> = Vec::new();
                    let mut started = false;
                    let _ = crate::remote_play::audio::pcm(&st, capture, Some(&id), &mut |c| {
                        let now = std::time::Instant::now();
                        if !started {
                            if now < t0 {
                                return true;
                            }
                            started = true;
                            let late = ((now - t0).as_secs_f64() * 48_000.0) as usize;
                            pcm.extend(std::iter::repeat_n(0, late.saturating_sub(c.len() / 2)));
                        }
                        pcm.extend(c.chunks_exact(2).map(|s| (((s[0] + s[1]) * 0.5).clamp(-1.0, 1.0) * 32767.0) as i16));
                        !st.load(Ordering::Relaxed)
                    });
                    (name, pcm)
                })
            })
            .collect();
        println!("ENREGISTREMENT {secs} s");
        std::thread::sleep(std::time::Duration::from_secs(secs) + std::time::Duration::from_millis(1500));
        stop.store(true, Ordering::Relaxed);
        for t in threads {
            let (name, pcm) = t.join().unwrap();
            let peak = pcm.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
            if name == "micro" || peak > 30 {
                write_wav(&dir.join(format!("{name}.wav")), &pcm);
                println!("{name} : {:.1} s, crête {peak}", pcm.len() as f32 / 48_000.0);
            }
        }
    }

    /// Niveaux réels (`IBOX_VOICE_MIC` : micro, sinon celui par défaut), seconde par seconde :
    /// `cargo test --lib niveaux -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn niveaux() {
        let id = std::env::var("IBOX_VOICE_MIC").ok().and_then(|n| crate::remote_play::audio::endpoints(true).into_iter().find(|e| crate::voice_sapi::same_device(&e.name, &n)).map(|e| e.id));
        let out = crate::remote_play::audio::endpoints(false).into_iter().find(|e| e.default).map(|e| e.name);
        println!("micro {id:?}, sortie par défaut {out:?}");
        let m = Monitor::start(id);
        println!("sorties : {:?}", m.outputs());
        for _ in 0..10 {
            std::thread::sleep(std::time::Duration::from_secs(1));
            let now = clock();
            let med = |t: &Arc<Mutex<Track>>| {
                let t = t.lock().unwrap();
                let mut v: Vec<f32> = t.frames.iter().filter(|f| now - f.0 < 1_000.0).map(|f| f.1).collect();
                v.sort_by(|a, b| a.total_cmp(b));
                (v.len(), v.get(v.len() / 2).copied(), v.last().copied())
            };
            let outs: Vec<String> = m.outputs.iter().map(|o| format!("{}={:?}", o.name.chars().take(14).collect::<String>(), med(&o.track).1.map(|x| x.round()))).collect();
            println!("micro {:?}  {}", med(&m.mic), outs.join(" "));
        }
    }

    impl Drop for Monitor {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            for t in self.threads.drain(..) {
                let _ = t.join();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bruit pseudo-aléatoire reproductible.
    fn noise(n: usize, seed: u32) -> Vec<f32> {
        let mut s = seed.wrapping_mul(2_654_435_761).max(1);
        (0..n)
            .map(|_| {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                (s as f32 / u32::MAX as f32) * 2.0 - 1.0
            })
            .collect()
    }

    /// « Musique » : bruit modulé (rythme irrégulier), 48 kHz.
    fn music(secs: f32, seed: u32) -> Vec<f32> {
        let n = (secs * 48_000.0) as usize;
        let env = noise(n / 4_800 + 2, seed ^ 77);
        noise(n, seed).into_iter().enumerate().map(|(i, x)| x * 0.3 * (0.2 + env[i / 4_800].abs())).collect()
    }

    /// Son des enceintes vu par le micro : retard, gain, traîne de la pièce.
    fn room(spk: &[f32], delay: usize, gain: f32) -> Vec<f32> {
        let mut out = vec![0f32; spk.len()];
        let mut tail = 0f32;
        for i in delay..spk.len() {
            tail = tail * 0.9995 + spk[i - delay] * 0.02;
            out[i] = gain * (spk[i - delay] + tail);
        }
        out
    }

    fn add(a: &mut [f32], b: &[f32], at: usize, k: f32) {
        for (i, x) in b.iter().enumerate() {
            if let Some(y) = a.get_mut(at + i) {
                *y += x * k;
            }
        }
    }

    /// « Voix » : bourdonnement modulé dans la bande de la parole.
    fn voice(secs: f32) -> Vec<f32> {
        let n = (secs * 48_000.0) as usize;
        (0..n)
            .map(|i| {
                let t = i as f32 / 48_000.0;
                let syll = (t * std::f32::consts::TAU * 4.0).sin().abs();
                syll * ((t * 140.0 * std::f32::consts::TAU).sin() + 0.6 * (t * 700.0 * std::f32::consts::TAU).sin() + 0.4 * (t * 1_400.0 * std::f32::consts::TAU).sin()) * 0.25
            })
            .collect()
    }

    fn case(mic: &[f32], spk: &[f32], at_s: f32, secs: f32) -> Verdict {
        let (m, s) = (frames(mic, 48_000), frames(spk, 48_000));
        let a = (at_s * 100.0) as usize - PAD_FRAMES;
        let b = ((at_s + secs) * 100.0) as usize + PAD_FRAMES;
        judge(&m[..b], &s[..b], a..b, None)
    }

    #[test]
    fn la_musique_des_enceintes_n_est_pas_une_voix() {
        for (seed, delay, gain) in [(1, 2_400, 0.05), (2, 9_600, 0.2), (3, 14_400, 0.02), (4, 4_800, 0.5)] {
            let spk = music(14.0, seed);
            let mut mic = room(&spk, delay, gain);
            let hiss = noise(mic.len(), 99);
            add(&mut mic, &hiss, 0, 0.0005);
            for at in [11.0, 12.0, 12.6] {
                let v = case(&mic, &spk, at, 0.8);
                assert!(v.speakers && !v.voice(), "{seed} {at} {v:?}");
            }
        }
    }

    #[test]
    fn la_voix_par_dessus_la_musique_passe() {
        // voix du joueur 24 dB et plus au-dessus de l'écho (mesures réelles)
        for (seed, delay, gain, level) in [(1, 2_400, 0.05, 1.0), (2, 9_600, 0.2, 3.0), (4, 4_800, 0.5, 8.0)] {
            let spk = music(14.0, seed);
            let mut mic = room(&spk, delay, gain);
            let hiss = noise(mic.len(), 99);
            add(&mut mic, &hiss, 0, 0.0005);
            add(&mut mic, &voice(0.8), (12.0 * 48_000.0) as usize, level);
            let v = case(&mic, &spk, 12.0, 0.8);
            assert!(v.speakers && v.voice(), "{seed} {v:?}");
        }
    }

    #[test]
    fn enceintes_muettes_la_voix_passe_le_souffle_non() {
        let spk = vec![0f32; 14 * 48_000];
        let mut mic = noise(spk.len(), 5).into_iter().map(|x| x * 0.001).collect::<Vec<_>>();
        let quiet = case(&mic, &spk, 12.0, 0.8);
        assert!(!quiet.speakers && !quiet.voice(), "{quiet:?}");
        add(&mut mic, &voice(0.8), 12 * 48_000, 0.2);
        let v = case(&mic, &spk, 12.0, 0.8);
        assert!(!v.speakers && v.voice(), "{v:?}");
    }

    #[test]
    fn un_son_qui_vient_de_demarrer_aux_enceintes() {
        // muettes pendant 10 s, puis une notification pile pendant la phrase : écho jamais appris
        let mut spk = vec![0f32; 14 * 48_000];
        add(&mut spk, &music(1.0, 8), 12 * 48_000 - 2_400, 1.0);
        let mut mic = room(&spk, 4_800, 0.1);
        let hiss = noise(mic.len(), 3);
        add(&mut mic, &hiss, 0, 0.0005);
        // la notification seule, écho jamais appris : rien ne la sépare d'une voix (Vosk ne la
        // prend pas pour la phrase) ; écho appris : refusée
        let (m, sp) = (frames(&mic, 48_000), frames(&spk, 48_000));
        assert!(!judge(&m, &sp, 1_185..1_295, Some(-20.0)).voice());
        add(&mut mic, &voice(0.8), 12 * 48_000, 3.0);
        // écho jamais appris : la voix passe (comparée au bruit de fond) ; appris, elle passe aussi
        let v = case(&mic, &spk, 12.0, 0.8);
        assert!(v.voice(), "{v:?}");
        let (m, s) = (frames(&mic, 48_000), frames(&spk, 48_000));
        let v = judge(&m, &s, 1_185..1_295, Some(-20.0));
        assert!(v.voice(), "{v:?}");
    }

    fn load(path: &std::path::Path) -> Track {
        let mut t = Track::default();
        for l in std::fs::read_to_string(path).unwrap().lines() {
            let mut it = l.split_whitespace().map(|x| x.parse::<f64>().unwrap());
            if let (Some(a), Some(b)) = (it.next(), it.next()) {
                t.frames.push_back((a, b as f32));
            }
        }
        t
    }

    /// Jugement le plus prudent sur toutes les sorties (comme `Monitor::judge`).
    fn judge_all(mic: &Track, outs: &[(String, Track)], start: f64, ms: u32) -> Option<Verdict> {
        let empty = Track::default();
        let mut best: Option<Verdict> = None;
        for (name, t) in outs.iter().map(|(n, t)| (n.as_str(), t)).chain(std::iter::once(("", &empty))) {
            let (m, s, p) = window(mic, t, start, ms)?;
            if !name.is_empty() && s.iter().all(|&x| x <= SILENT_DB) {
                continue;
            }
            let mut v = judge(&m, &s, p, None);
            v.output = name.into();
            if best.as_ref().is_none_or(|b| v.voice_db < b.voice_db) {
                best = Some(v);
            }
        }
        best
    }

    fn read_wav(path: &std::path::Path) -> Vec<f32> {
        let wav = std::fs::read(path).unwrap();
        let data = wav.windows(4).position(|w| w == b"data").unwrap() + 8;
        wav[data..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect()
    }

    fn track_of(samples: &[f32]) -> Track {
        let mut t = Track::default();
        for (i, d) in frames(samples, 48_000).into_iter().enumerate() {
            t.frames.push_back((i as f64 * FRAME_MS + FRAME_MS / 2.0, d));
        }
        t
    }

    /// Rejoue de vrais enregistrements (`enregistrer_sons`) dans le vrai moteur vocal puis le
    /// contrôle des sorties. `IBOX_VOICE_MUSIQUE` : dossier (micro.wav + une piste par sortie)
    /// enregistré avec de la musique aux enceintes et personne qui parle : chaque « Clip ça ! »
    /// reconnu doit être refusé. `IBOX_VOICE_VOIX` (facultatif) : vraie voix enregistrée, ajoutée
    /// au micro de la musique : ses phrases doivent passer.
    /// `cargo test --lib rejouer_wav -- --ignored --nocapture`
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn rejouer_wav() {
        let dir = std::path::PathBuf::from(std::env::var("IBOX_VOICE_MUSIQUE").expect("IBOX_VOICE_MUSIQUE"));
        let mic = read_wav(&dir.join("micro.wav"));
        let mut outs = vec![];
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.ends_with(".wav") && n != "micro.wav" && !n.starts_with("mix") {
                outs.push((n, track_of(&read_wav(&e.path()))));
            }
        }
        let run = |label: &str, path: &std::path::Path, mic: &Track, voice: &[(u64, u32)]| {
            let (mut ok_voice, mut n_voice, mut false_ok, mut n_other) = (0, 0, 0, 0);
            for h in crate::voice_sapi::recognize_file(path).into_iter().filter(|h| h.ours || crate::voice_sapi::dictated(&h.words)) {
                let v = judge_all(mic, &outs, h.start as f64, h.ms);
                let decision = crate::voice_sapi::decide(h.confidence, h.engine, true, h.ms, v.as_ref());
                let is_voice = voice.iter().any(|&(s, d)| h.start + 300 >= s && h.start <= s + d as u64 + 300);
                if is_voice { n_voice += 1; if decision.is_ok() { ok_voice += 1; } } else { n_other += 1; if decision.is_ok() { false_ok += 1; } }
                println!("  {label} {:6.1} s {} conf {} score {:.2} {} ms ; {} ; {:?}", h.start as f32 / 1000.0, if is_voice { "VOIX " } else { "autre" }, h.confidence, h.engine, h.ms, v.map(|v| v.describe()).unwrap_or_default(), decision);
            }
            println!("{label} : voix acceptées {ok_voice}/{n_voice}, autres sons acceptés {false_ok}/{n_other}");
        };
        run("musique", &dir.join("micro.wav"), &track_of(&mic), &[]);
        if let Ok(v) = std::env::var("IBOX_VOICE_VOIX") {
            let voice = read_wav(std::path::Path::new(&v));
            let spoken: Vec<(u64, u32)> = crate::voice_sapi::recognize_file(std::path::Path::new(&v)).into_iter().map(|h| (h.start, h.ms)).collect();
            let mix: Vec<f32> = mic.iter().enumerate().map(|(i, &m)| m + voice.get(i).copied().unwrap_or(0.0)).collect();
            let path = dir.join("mix.wav");
            write_test_wav(&path, &mix);
            run("musique+voix", &path, &track_of(&mix), &spoken);
        }
    }

    fn write_test_wav(path: &std::path::Path, s: &[f32]) {
        let pcm: Vec<u8> = s.iter().flat_map(|x| ((x.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        for v in [16u32.to_le_bytes(), [1, 0, 1, 0], 48_000u32.to_le_bytes(), 96_000u32.to_le_bytes(), [2, 0, 16, 0]] {
            wav.extend_from_slice(&v);
        }
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
        wav.extend_from_slice(&pcm);
        std::fs::write(path, wav).unwrap();
    }

    /// Rejoue des pistes réelles du banc `essai_enceintes` (`IBOX_VOICE_PISTES` : dossier) :
    /// chaque phrase jouée aux enceintes doit être jugée « écho » contre ce qui sortait vraiment,
    /// « voix » contre la musique seule ; puis toute la musique, tranche de 0,8 s par tranche.
    #[test]
    #[ignore]
    fn rejouer_pistes() {
        let dir = std::path::PathBuf::from(std::env::var("IBOX_VOICE_PISTES").expect("IBOX_VOICE_PISTES"));
        let mic = load(&dir.join("piste-micro.txt"));
        let only = load(&dir.join("piste-musique-seule.txt"));
        let mut outs = vec![];
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.starts_with("piste-") && n != "piste-micro.txt" && n != "piste-musique-seule.txt" {
                outs.push((n, load(&e.path())));
            }
        }
        let said: Vec<f64> = std::fs::read_to_string(dir.join("phrases.txt")).unwrap().lines().map(|l| l.parse().unwrap()).collect();
        let (first, last) = (mic.frames.front().unwrap().0 + 11_000.0, mic.frames.back().unwrap().0 - 2_000.0);
        let (mut bad_echo, mut bad_voice, mut n) = (vec![], vec![], 0);
        for &t in said.iter().filter(|&&t| t > first && t < last) {
            n += 1;
            let real = judge_all(&mic, &outs, t, 1_600).unwrap();
            let sim = judge_all(&mic, &[("musique seule".into(), Track { frames: only.frames.clone() })], t, 1_600).unwrap();
            println!("phrase {:.1} s : réel {} | musique seule {}", t / 1000.0, real.describe(), sim.describe());
            if real.voice() { bad_echo.push(t); }
            if !sim.voice() { bad_voice.push(t); }
        }
        // musique seule, hors phrases : la plus haute « voix » trouvée
        let mut music = vec![];
        let mut t = first;
        while t < last {
            if said.iter().all(|&s| (t - s).abs() > 3_000.0) {
                if let Some(v) = judge_all(&mic, &outs, t, 800) {
                    music.push((v.voice_db, t, v.speakers));
                }
            }
            t += 400.0;
        }
        music.sort_by(|a, b| b.0.total_cmp(&a.0));
        let loud = music.iter().filter(|m| m.2 && m.0 >= VOICE_DB).count();
        println!("{n} phrases jouées : {} prises pour une voix (doit être 0), {} refusées en simulation de voix", bad_echo.len(), bad_voice.len());
        println!("{} tranches de musique : {} au-dessus du seuil avec son aux sorties ; les plus hautes : {:?}", music.len(), loud, &music[..music.len().min(8)]);
    }

    #[test]
    fn grille_datee() {
        let mut t = Track::default();
        for k in 0..100 {
            t.push(1_000.0 + k as f64 * 10.0 + 2.0, k as f32);
        }
        let g = t.grid(1_000.0, 5);
        assert_eq!(g, vec![Some(0.0), Some(1.0), Some(2.0), Some(3.0), Some(4.0)]);
        assert_eq!(t.grid(5_000.0, 1), vec![None]);
    }
}
