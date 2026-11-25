/**
 * Son « clip » du panneau en jeu (même idée que le son d'iBox pour les clips automatiques) :
 * le déclencheur d'un appareil photo, puis une quinte qui s'ouvre. Synthétisé (Web Audio),
 * aucun fichier ; un seul à la fois.
 */
let ctx: AudioContext | null = null;
let last = 0;

export function playClip(): void {
  const now = performance.now();
  if (now - last < 450) return;
  last = now;
  try {
    ctx ??= new AudioContext();
    const c = ctx;
    const t0 = c.currentTime + 0.01;
    const out = c.createGain();
    out.gain.value = 0.55;
    out.connect(c.destination);
    // bruit blanc court (rideau de l'obturateur)
    const noise = c.createBuffer(1, c.sampleRate / 10, c.sampleRate);
    const data = noise.getChannelData(0);
    for (let i = 0; i < data.length; i++) data[i] = Math.random() * 2 - 1;
    const burst = (at: number, dur: number, gain: number, freq: number) => {
      const src = c.createBufferSource();
      src.buffer = noise;
      const f = c.createBiquadFilter();
      f.type = "highpass";
      f.frequency.value = freq;
      const g = c.createGain();
      g.gain.setValueAtTime(0, t0 + at);
      g.gain.linearRampToValueAtTime(gain, t0 + at + 0.002);
      g.gain.exponentialRampToValueAtTime(0.0001, t0 + at + dur);
      src.connect(f).connect(g).connect(out);
      src.start(t0 + at);
      src.stop(t0 + at + dur + 0.02);
    };
    const tone = (at: number, freq: number, dur: number, gain: number) => {
      for (const [ratio, level] of [[1, 1], [2, 0.18], [1.004, 0.4]] as const) {
        const o = c.createOscillator();
        o.frequency.value = freq * ratio;
        const g = c.createGain();
        g.gain.setValueAtTime(0, t0 + at);
        g.gain.linearRampToValueAtTime(gain * level, t0 + at + 0.004);
        g.gain.exponentialRampToValueAtTime(0.0001, t0 + at + dur);
        o.connect(g).connect(out);
        o.start(t0 + at);
        o.stop(t0 + at + dur + 0.02);
      }
    };
    burst(0, 0.03, 0.22, 3200);
    burst(0.03, 0.06, 0.08, 1800);
    burst(0.075, 0.025, 0.18, 2600);
    tone(0.13, 880, 0.22, 0.07);
    tone(0.2, 1174.66, 0.3, 0.08);
    tone(0.27, 1760, 0.6, 0.06);
  } catch {
    // pas de son : rien de grave
  }
}
