/**
 * Commandes du lecteur (0.8.8), à la place de celles du navigateur : un clic sur l'image lit ou
 * met en pause, un gros bouton rond au centre quand la vidéo est en pause, et un signe qui éclot
 * au centre à chaque bascule. En bas, une barre visible au survol ou en pause : progression à
 * saisir, lecture, image par image, temps, vitesse et plein écran (Studio 0.9, comme les
 * lecteurs de clips des studios de jeu). Le son se règle dans le panneau Son (`Sound.tsx`,
 * `mix.ts`).
 */
import { AnimatePresence, motion, useMotionValue, useTransform } from "motion/react";
import { useEffect, useRef, useState, type RefObject } from "react";
import { duration } from "./format";
import { locale, t } from "./i18n";
import { SPRING } from "./ibox/motion";
import { Icon, type IconName } from "./ui";
import { hint } from "./keys";

/** Vitesses proposées (ralenti pour revoir une action). */
export const SPEEDS = [0.25, 0.5, 1, 1.5, 2] as const;

/** Vitesse voisine : `dir` -1 plus lente, +1 plus rapide, bornée aux vitesses proposées. */
export function nextSpeed(speed: number, dir: -1 | 1): number {
  const i = SPEEDS.findIndex((s) => s >= speed - 1e-6);
  const at = i < 0 ? SPEEDS.length - 1 : i;
  return SPEEDS[Math.min(SPEEDS.length - 1, Math.max(0, at + dir))];
}

/** « 0,5× », « 2× ». */
export const speedLabel = (s: number) => `${String(s).replace(".", locale === "fr-FR" ? "," : ".")}×`;

/** Lit ou met en pause. */
export function toggle(v: HTMLVideoElement | null) {
  if (!v) return;
  if (v.paused || v.ended) void v.play().catch(() => {});
  else v.pause();
}

/** Images par seconde mesurées pendant la lecture (30 ou 60 selon la qualité des clips). */
const rates = new WeakMap<HTMLVideoElement, number>();

/** Recule ou avance d'une image, vidéo en pause. */
export function frameStep(v: HTMLVideoElement | null, dir: -1 | 1) {
  if (!v) return;
  v.pause();
  const fps = rates.get(v) ?? 60;
  const end = Number.isFinite(v.duration) ? v.duration : v.currentTime + 1;
  v.currentTime = Math.min(end, Math.max(0, v.currentTime + dir / fps));
}

/**
 * Plein écran du lecteur : l'élément (vidéo et commandes) couvre la fenêtre (`ibox-theater`,
 * styles.css), et la page passe en plein écran quand la WebView le permet. Échap en sort.
 */
let theater: HTMLElement | null = null;
const watchers = new Set<() => void>();
export const isFullscreen = () => theater !== null;
export function exitFullscreen() {
  if (!theater) return;
  theater.classList.remove("ibox-theater");
  theater = null;
  watchers.forEach((w) => w());
  if (document.fullscreenElement) void document.exitFullscreen().catch(() => {});
}
export function toggleFullscreen(el: HTMLElement | null) {
  if (theater) { exitFullscreen(); return; }
  if (!el) return;
  theater = el;
  el.classList.add("ibox-theater");
  watchers.forEach((w) => w());
  void document.documentElement.requestFullscreen?.().catch(() => {});
}
// plein écran natif quitté (Échap, F11) : le lecteur reprend sa place
if (typeof document !== "undefined") document.addEventListener("fullscreenchange", () => { if (!document.fullscreenElement) exitFullscreen(); });

export function useFullscreen(): boolean {
  const [on, setOn] = useState(isFullscreen);
  useEffect(() => {
    const w = () => setOn(isFullscreen());
    watchers.add(w);
    return () => { watchers.delete(w); };
  }, []);
  return on;
}

/** À poser dans un conteneur `relative group/player` qui tient la vidéo. */
export function Playback({ video, length, bar = true, speed = 1, onSpeed, stage }: {
  video: RefObject<HTMLVideoElement | null>; length: number; bar?: boolean;
  speed?: number; onSpeed?: (s: number) => void;
  /** Conteneur à mettre en plein écran (bouton de la barre). */
  stage?: RefObject<HTMLElement | null>;
}) {
  const [paused, setPaused] = useState(true);
  /** Signe qui éclot au centre à chaque bascule (lecture ou pause), comme sur les lecteurs de clips. */
  const [burst, setBurst] = useState<{ n: number; icon: "play" | "pause" } | null>(null);
  const [now, setNow] = useState(0);
  const [scrubbing, setScrubbing] = useState(false);
  const [speeds, setSpeeds] = useState(false);
  const full = useFullscreen();
  const time = useMotionValue(0);
  const len = length > 0 ? length : 1;
  const lenRef = useRef(len);
  lenRef.current = len;
  const width = useTransform(time, (s) => `${Math.min(100, Math.max(0, (s / lenRef.current) * 100))}%`);
  const track = useRef<HTMLDivElement>(null);
  const first = useRef(true);

  useEffect(() => {
    const v = video.current;
    if (!v) return;
    const state = () => {
      const p = v.paused || v.ended;
      setPaused(p);
      // pas de signe pour le départ automatique à l'ouverture
      if (first.current) { first.current = false; return; }
      setBurst((b) => ({ n: (b?.n ?? 0) + 1, icon: p ? "pause" : "play" }));
    };
    setPaused(v.paused || v.ended);
    if (!v.paused) first.current = false;
    let raf = 0;
    const loop = () => {
      time.set(v.currentTime);
      raf = requestAnimationFrame(loop);
    };
    loop();
    // cadence des images, pour avancer image par image
    let frame = 0, last = -1;
    const onFrame = (_: number, m: VideoFrameCallbackMetadata) => {
      const d = m.mediaTime - last;
      if (last >= 0 && d > 0.008 && d < 0.05) rates.set(v, d < 0.025 ? 60 : 30);
      last = m.mediaTime;
      frame = v.requestVideoFrameCallback(onFrame);
    };
    if (typeof v.requestVideoFrameCallback === "function") frame = v.requestVideoFrameCallback(onFrame);
    const sec = () => setNow(v.currentTime);
    v.addEventListener("play", state);
    v.addEventListener("pause", state);
    v.addEventListener("ended", state);
    v.addEventListener("timeupdate", sec);
    v.addEventListener("seeked", sec);
    return () => {
      cancelAnimationFrame(raf);
      if (frame && typeof v.cancelVideoFrameCallback === "function") v.cancelVideoFrameCallback(frame);
      v.removeEventListener("play", state);
      v.removeEventListener("pause", state);
      v.removeEventListener("ended", state);
      v.removeEventListener("timeupdate", sec);
      v.removeEventListener("seeked", sec);
    };
  }, [video, time]);

  useEffect(() => {
    if (!burst) return;
    const id = setTimeout(() => setBurst(null), 650);
    return () => clearTimeout(id);
  }, [burst]);

  const seek = (x: number) => {
    const v = video.current;
    const r = track.current?.getBoundingClientRect();
    if (!v || !r || r.width <= 0) return;
    v.currentTime = Math.min(1, Math.max(0, (x - r.left) / r.width)) * (Number.isFinite(v.duration) ? v.duration : len);
    time.set(v.currentTime);
    setNow(v.currentTime);
  };

  const shown = paused || scrubbing || speeds;
  return (
    <div className="pointer-events-none absolute inset-0">
      {/* gros bouton central en pause */}
      <AnimatePresence>
        {paused && !scrubbing && (
          <motion.button type="button" aria-label={t("Lire")} tabIndex={-1}
            initial={{ opacity: 0, scale: 0.7 }} animate={{ opacity: 1, scale: 1 }} exit={{ opacity: 0, scale: 0.85, transition: { duration: 0.12 } }} transition={SPRING.pop}
            whileHover={{ scale: 1.08 }} whileTap={{ scale: 0.94 }}
            onClick={() => toggle(video.current)}
            className="pointer-events-auto absolute top-1/2 left-1/2 -mt-11 -ml-11 grid h-22 w-22 place-items-center rounded-full bg-black/55 text-[#fff] shadow-[0_10px_40px_-8px_rgba(0,0,0,0.7)] ring-2 ring-[#fff]/25 backdrop-blur-md">
            <Icon name="play" size="2.6rem" className="translate-x-[3px]" />
          </motion.button>
        )}
      </AnimatePresence>
      {/* signe de bascule pendant la lecture */}
      <AnimatePresence>
        {burst && !paused && (
          <motion.span key={burst.n} aria-hidden="true"
            initial={{ opacity: 0.85, scale: 0.6 }} animate={{ opacity: 0, scale: 1.35 }} exit={{ opacity: 0 }} transition={{ duration: 0.6, ease: "easeOut" }}
            className="absolute top-1/2 left-1/2 -mt-9 -ml-9 grid h-18 w-18 place-items-center rounded-full bg-black/45 text-[#fff]">
            <Icon name={burst.icon} size="2rem" />
          </motion.span>
        )}
      </AnimatePresence>
      {/* vitesse autre que 1× : rappelée en haut à gauche, même barre cachée */}
      <AnimatePresence>
        {speed !== 1 && (
          <motion.span initial={{ opacity: 0, scale: 0.8 }} animate={{ opacity: 1, scale: 1 }} exit={{ opacity: 0, scale: 0.8 }} transition={SPRING.pop}
            className="absolute top-3 left-3 rounded-full bg-black/55 px-2.5 py-1 text-[0.78rem] font-extrabold text-[#fff] backdrop-blur-md">
            {speedLabel(speed)}
          </motion.span>
        )}
      </AnimatePresence>
      {bar && (
        <div className={`group/bar pointer-events-auto absolute inset-x-0 bottom-0 flex flex-col gap-1 bg-gradient-to-t from-black/70 to-transparent px-4 pt-12 pb-2.5 transition-opacity duration-200 ${shown ? "opacity-100" : "opacity-0 group-hover/player:opacity-100 group-focus-within/player:opacity-100"}`}>
          <div ref={track} role="slider" aria-label={t("Position dans le clip")} aria-valuemin={0} aria-valuemax={Math.round(len)} aria-valuenow={Math.round(now)} aria-valuetext={`${duration(now) || "0:00"} / ${duration(len)}`} tabIndex={-1}
            onPointerDown={(e) => { e.preventDefault(); e.currentTarget.setPointerCapture(e.pointerId); setScrubbing(true); seek(e.clientX); }}
            onPointerMove={(e) => { if (scrubbing) seek(e.clientX); }}
            onPointerUp={() => setScrubbing(false)}
            onPointerCancel={() => setScrubbing(false)}
            className="relative flex h-5 min-w-0 cursor-pointer touch-none items-center">
            <div className="relative h-1.5 w-full overflow-hidden rounded-full bg-[#fff]/25 transition-[height] duration-150 group-hover/bar:h-2">
              <motion.div className="absolute inset-y-0 left-0 rounded-full bg-[var(--accent)]" style={{ width }} />
            </div>
            <motion.span className="pointer-events-none absolute top-1/2 h-3.5 w-3.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-[#fff] opacity-0 shadow transition-opacity group-hover/bar:opacity-100" style={{ left: width }} />
          </div>
          {onSpeed ? (
            <div className="flex items-center gap-1 text-[#fff]">
              <BarButton icon={paused ? "play" : "pause"} label={hint(paused ? t("Lire") : t("Pause"), t("Espace"))} onClick={() => toggle(video.current)} />
              <BarButton icon="stepBack" label={hint(t("Image précédente"), ",")} onClick={() => frameStep(video.current, -1)} />
              <BarButton icon="stepFwd" label={hint(t("Image suivante"), ".")} onClick={() => frameStep(video.current, 1)} />
              <span className="ml-1.5 text-[0.82rem] font-bold tabular-nums text-[#fff]/90">{duration(now) || "0:00"} / {duration(len)}</span>
              <span className="flex-1" />
              <div className="relative">
                <AnimatePresence>
                  {speeds && (
                    <motion.div role="menu" aria-label={t("Vitesse")} initial={{ opacity: 0, y: 6, scale: 0.95 }} animate={{ opacity: 1, y: 0, scale: 1 }} exit={{ opacity: 0, y: 4, scale: 0.97, transition: { duration: 0.1 } }} transition={SPRING.snappy}
                      className="absolute right-0 bottom-full mb-2 flex flex-col rounded-xl bg-black/80 p-1 shadow-lg backdrop-blur-md">
                      {[...SPEEDS].reverse().map((s) => (
                        <button key={s} type="button" role="menuitemradio" aria-checked={s === speed} onClick={() => { onSpeed(s); setSpeeds(false); }}
                          className={`rounded-lg px-4 py-1.5 text-right text-[0.85rem] font-extrabold tabular-nums transition-colors ${s === speed ? "bg-[var(--accent)] text-[#fff]" : "text-[#fff]/85 hover:bg-[#fff]/15"}`}>
                          {s === 1 ? t("Normale") : speedLabel(s)}
                        </button>
                      ))}
                    </motion.div>
                  )}
                </AnimatePresence>
                <button type="button" title={hint(t("Vitesse"), "- +")} aria-haspopup="menu" aria-expanded={speeds} onClick={() => setSpeeds((o) => !o)}
                  className="h-8 min-w-[3.2rem] rounded-full px-2.5 text-[0.85rem] font-extrabold tabular-nums transition-colors hover:bg-[#fff]/15">
                  {speedLabel(speed)}
                </button>
              </div>
              {stage && <BarButton icon={full ? "shrink" : "expand"} label={hint(full ? t("Quitter le plein écran") : t("Plein écran"), t("Entrée"))} onClick={() => toggleFullscreen(stage.current)} />}
            </div>
          ) : (
            <span className="self-end text-[0.8rem] font-bold tabular-nums text-[#fff]/90">{duration(now) || "0:00"} / {duration(len)}</span>
          )}
        </div>
      )}
    </div>
  );
}

function BarButton({ icon, label, onClick }: { icon: IconName; label: string; onClick: () => void }) {
  return (
    <button type="button" title={label} aria-label={label} onClick={onClick}
      className="grid h-8 w-8 place-items-center rounded-full transition-[background-color,transform] duration-150 hover:bg-[#fff]/15 active:scale-90">
      <Icon name={icon} size="1.15em" />
    </button>
  );
}
