/**
 * Cadre vertical 9:16 posé sur la vidéo du lecteur : on le fait glisser (souris ; gauche et
 * droite au clavier ou à la manette) pour choisir ce qui reste. `focus` : centre du cadre,
 * de 0 (à gauche) à 1 (à droite), repris tel quel par iBox (`clip_edit::vertical_plan`).
 */
import { motion } from "motion/react";
import { useEffect, useRef, useState, type RefObject } from "react";
import { SPRING } from "./ibox/motion";

/** Ressort du cadre : il suit le doigt de près, et glisse quand il saute (flèches, manette). */
const FOLLOW = { type: "spring", stiffness: 620, damping: 44, mass: 0.6 } as const;
import { t } from "./i18n";

/** Rectangle de l'image dans la balise (vidéo en `object-contain`). */
export function contentBox(boxW: number, boxH: number, vw: number, vh: number) {
  if (!vw || !vh || !boxW || !boxH) return { x: 0, y: 0, w: boxW, h: boxH };
  const scale = Math.min(boxW / vw, boxH / vh);
  const w = vw * scale, h = vh * scale;
  return { x: (boxW - w) / 2, y: (boxH - h) / 2, w, h };
}

/** Bord gauche du cadre (en fraction de l'image) pour un centre `focus` ; largeur `frame`. */
export function frameLeft(focus: number, frame: number) {
  return Math.min(1 - frame, Math.max(0, focus - frame / 2));
}

export function Vertical({ video, focus, onFocus }: { video: RefObject<HTMLVideoElement | null>; focus: number; onFocus: (f: number) => void }) {
  const [box, setBox] = useState({ x: 0, y: 0, w: 0, h: 0, vw: 16, vh: 9 });
  const grab = useRef<{ x: number; focus: number } | null>(null);
  const [held, setHeld] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const focusRef = useRef(focus);
  focusRef.current = focus;

  useEffect(() => {
    const v = video.current;
    if (!v) return;
    const measure = () => {
      const r = contentBox(v.clientWidth, v.clientHeight, v.videoWidth, v.videoHeight);
      setBox({ ...r, x: r.x + v.offsetLeft, y: r.y + v.offsetTop, vw: v.videoWidth || 16, vh: v.videoHeight || 9 });
    };
    measure();
    v.addEventListener("loadedmetadata", measure);
    const ro = new ResizeObserver(measure);
    ro.observe(v);
    return () => { v.removeEventListener("loadedmetadata", measure); ro.disconnect(); };
  }, [video]);
  // gauche et droite (clavier, manette) arrivent par la navigation (`data-adjust`)
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const onAdjust = (e: Event) => onFocus(Math.min(1, Math.max(0, focusRef.current + ((e as CustomEvent<string>).detail === "left" ? -0.05 : 0.05))));
    el.addEventListener("nav-adjust", onAdjust);
    return () => el.removeEventListener("nav-adjust", onAdjust);
  }, [onFocus]);

  // largeur du cadre : 9:16 de la hauteur, en fraction de la largeur de l'image
  const frame = Math.min(1, (box.vh * 9) / 16 / box.vw);
  const left = frameLeft(focus, frame);

  return (
    <motion.div className="pointer-events-none absolute" style={{ left: box.x, top: box.y, width: box.w, height: box.h }}
      initial={{ opacity: 0 }} animate={{ opacity: 1 }} transition={{ duration: 0.25 }}>
      <motion.div className="absolute inset-y-0 left-0 bg-black/65" initial={false} animate={{ width: `${left * 100}%` }} transition={FOLLOW} />
      <motion.div className="absolute inset-y-0 right-0 bg-black/65" initial={false} animate={{ width: `${(1 - left - frame) * 100}%` }} transition={FOLLOW} />
      <motion.div
        ref={ref}
        role="slider"
        tabIndex={0}
        data-nav
        data-adjust
        aria-label={t("Cadre vertical")}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(focus * 100)}
        className="pointer-events-auto absolute inset-y-0 cursor-grab touch-none rounded-sm border-[3px] border-accent outline-none focus-visible:ring-4 focus-visible:ring-accent/40 active:cursor-grabbing"
        initial={{ left: `${left * 100}%`, width: `${frame * 100}%`, scale: 0.94 }}
        animate={{ left: `${left * 100}%`, width: `${frame * 100}%`, scale: held ? 1.015 : 1, boxShadow: held ? "0 0 0 1px rgba(0,0,0,0.4), 0 0 40px -6px var(--accent-glow)" : "0 0 0 1px rgba(0,0,0,0.4), 0 0 0 0 transparent" }}
        transition={{ ...FOLLOW, scale: SPRING.pop }}
        onPointerDown={(e) => { e.preventDefault(); e.currentTarget.setPointerCapture(e.pointerId); e.currentTarget.focus(); grab.current = { x: e.clientX, focus }; setHeld(true); }}
        onPointerMove={(e) => {
          const g = grab.current;
          if (!g || !box.w) return;
          onFocus(Math.min(1, Math.max(0, g.focus + (e.clientX - g.x) / box.w)));
        }}
        onPointerUp={() => { grab.current = null; setHeld(false); }}
        onPointerCancel={() => { grab.current = null; setHeld(false); }}
      >
        <span className="absolute top-2 left-1/2 -translate-x-1/2 rounded-full bg-accent px-2.5 py-0.5 text-[0.75rem] font-extrabold text-on-accent">9:16</span>
      </motion.div>
    </motion.div>
  );
}
