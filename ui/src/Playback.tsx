/**
 * Commandes du lecteur (0.8.8), à la place de celles du navigateur (volume, plein écran, menu,
 * qui n'avaient rien à faire là) : un clic sur l'image lit ou met en pause, un gros bouton
 * rond au centre quand la vidéo est en pause, et un signe qui éclot au centre à chaque bascule.
 * En bas, une fine barre de progression à saisir et le temps, visibles au survol ou en pause.
 * Le son se règle dans le panneau Son (`Sound.tsx`, `mix.ts`).
 */
import { AnimatePresence, motion, useMotionValue, useTransform } from "motion/react";
import { useEffect, useRef, useState, type RefObject } from "react";
import { duration } from "./format";
import { t } from "./i18n";
import { SPRING } from "./ibox/motion";
import { Icon } from "./ui";

/** Lit ou met en pause. */
export function toggle(v: HTMLVideoElement | null) {
  if (!v) return;
  if (v.paused || v.ended) void v.play().catch(() => {});
  else v.pause();
}

/** À poser dans un conteneur `relative group/player` qui tient la vidéo. */
export function Playback({ video, length, bar = true }: { video: RefObject<HTMLVideoElement | null>; length: number; bar?: boolean }) {
  const [paused, setPaused] = useState(true);
  /** Signe qui éclot au centre à chaque bascule (lecture ou pause), comme sur les lecteurs de clips. */
  const [burst, setBurst] = useState<{ n: number; icon: "play" | "pause" } | null>(null);
  const [now, setNow] = useState(0);
  const [scrubbing, setScrubbing] = useState(false);
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
    const sec = () => setNow(v.currentTime);
    v.addEventListener("play", state);
    v.addEventListener("pause", state);
    v.addEventListener("ended", state);
    v.addEventListener("timeupdate", sec);
    return () => {
      cancelAnimationFrame(raf);
      v.removeEventListener("play", state);
      v.removeEventListener("pause", state);
      v.removeEventListener("ended", state);
      v.removeEventListener("timeupdate", sec);
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
      {bar && (
        <div className={`group/bar pointer-events-auto absolute inset-x-0 bottom-0 flex items-end gap-3 bg-gradient-to-t from-black/65 to-transparent px-4 pt-10 pb-3 transition-opacity duration-200 ${paused || scrubbing ? "opacity-100" : "opacity-0 group-hover/player:opacity-100"}`}>
          <div ref={track} role="slider" aria-label={t("Position dans le clip")} aria-valuemin={0} aria-valuemax={Math.round(len)} aria-valuenow={Math.round(now)} aria-valuetext={`${duration(now) || "0:00"} / ${duration(len)}`} tabIndex={-1}
            onPointerDown={(e) => { e.preventDefault(); e.currentTarget.setPointerCapture(e.pointerId); setScrubbing(true); seek(e.clientX); }}
            onPointerMove={(e) => { if (scrubbing) seek(e.clientX); }}
            onPointerUp={() => setScrubbing(false)}
            onPointerCancel={() => setScrubbing(false)}
            className="relative flex h-5 min-w-0 flex-1 cursor-pointer touch-none items-center">
            <div className="relative h-1.5 w-full overflow-hidden rounded-full bg-[#fff]/25 transition-[height] duration-150 group-hover/bar:h-2">
              <motion.div className="absolute inset-y-0 left-0 rounded-full bg-[var(--accent)]" style={{ width }} />
            </div>
            <motion.span className="pointer-events-none absolute top-1/2 h-3.5 w-3.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-[#fff] opacity-0 shadow transition-opacity group-hover/bar:opacity-100" style={{ left: width }} />
          </div>
          <span className="shrink-0 pb-0.5 text-[0.8rem] font-bold tabular-nums text-[#fff]/90">{duration(now) || "0:00"} / {duration(len)}</span>
        </div>
      )}
    </div>
  );
}
