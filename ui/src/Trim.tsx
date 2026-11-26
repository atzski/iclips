/**
 * Coupe d'un clip, dans le lecteur : une bande d'images du clip et deux poignées à faire glisser
 * pour choisir le passage (souris ; au clavier et à la manette, gauche et droite sur une
 * poignée ; I et O). La vidéo montre l'image sous la poignée tirée ; un clic sur la bande y
 * place la lecture. On relit le passage, puis on crée un nouveau clip ou un GIF.
 *
 * Tout glisse sur des ressorts : poignées, passage, zones assombries, repères de temps ; la tête
 * de lecture suit la vidéo image par image ; une bulle donne l'instant sous la poignée tenue et
 * sous le pointeur.
 *
 * Studio 0.9 : zoom de la frise sur le passage (Z), pour placer les poignées au dixième ; case
 * « Remplacer l'original » (le clip coupé prend sa place, l'original n'est effacé qu'une fois le
 * rendu réussi, dans `Studio.tsx`).
 */
import { AnimatePresence, motion, useMotionValue, useTransform } from "motion/react";
import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent, type RefObject } from "react";
import { clampSpan, GIF_MAX, setEdge, zoomView, type Edge, type Span } from "./edit";
import { instant } from "./format";
import { t } from "./i18n";
import { SPRING } from "./ibox/motion";
import { filmstrip } from "./thumbs";
import { Button, Icon } from "./ui";
import { hint, useKeys } from "./keys";

/** Pas d'une poignée au clavier ou à la manette (secondes). */
const STEP = 0.2;
/** Pas en zoom. */
const FINE = 0.1;
/** Case « Remplacer l'original », gardée d'une fois sur l'autre. */
const REPLACE_KEY = "iclips.trim.replace";
function readReplace() { try { return localStorage.getItem(REPLACE_KEY) === "1"; } catch { return false; } }
function writeReplace(on: boolean) { try { localStorage.setItem(REPLACE_KEY, on ? "1" : "0"); } catch { /* sans stockage : non gardée */ } }
const FRAMES = 12;
/** Ressort des poignées : vif sous le doigt, doux quand elles sautent (I, O, flèches). */
const FOLLOW = { type: "spring", stiffness: 620, damping: 44, mass: 0.6 } as const;

export function Trim({ video, src, length, busy, vertical, onRender }: {
  video: RefObject<HTMLVideoElement | null>;
  src: string;
  length: number;
  busy: boolean;
  /** Format vertical : un seul bouton, « Créer en vertical ». */
  vertical?: boolean;
  /** `replace` : le clip coupé remplace l'original (« Créer le clip » seulement). */
  onRender: (kind: "mp4" | "gif" | "vertical", span: Span, replace?: boolean) => void;
}) {
  const [replace, setReplace] = useState(readReplace);
  /** Frise zoomée : la partie du clip montrée (null : tout le clip). */
  const [view, setView] = useState<Span | null>(null);
  const [span, setSpan] = useState<Span>(() => clampSpan({ start: 0, end: length }, length));
  // tant que rien n'est tiré, le passage couvre tout le clip (sa durée arrive avec la vidéo)
  const [touched, setTouched] = useState(false);
  const [frames, setFrames] = useState<string[]>([]);
  const [held, setHeld] = useState<Edge | null>(null);
  const [focused, setFocused] = useState<Edge | null>(null);
  const [hover, setHover] = useState<number | null>(null);
  const track = useRef<HTMLDivElement>(null);
  const dragging = useRef<Edge | "head" | null>(null);

  const len = length > 0 ? length : 1;
  const v0 = view?.start ?? 0;
  const vw = view ? Math.max(0.1, view.end - view.start) : len;
  const viewRef = useRef({ v0, vw });
  viewRef.current = { v0, vw };
  const frac = (s: number) => Math.min(100, Math.max(0, ((s - v0) / vw) * 100));
  const pct = (s: number) => `${frac(s)}%`;
  const inView = (s: number) => s >= v0 - 0.01 && s <= v0 + vw + 0.01;

  useEffect(() => setSpan((s) => clampSpan(touched ? s : { start: 0, end: length }, length)), [length, touched]);
  // tête de lecture : suit la vidéo à chaque image (plus de saut toutes les 250 ms)
  const head = useMotionValue(0);
  const headLeft = useTransform(head, (v) => `${Math.min(100, Math.max(0, ((v - viewRef.current.v0) / viewRef.current.vw) * 100))}%`);
  useEffect(() => {
    let raf = 0;
    const loop = () => {
      const v = video.current;
      if (v) head.set(v.currentTime);
      raf = requestAnimationFrame(loop);
    };
    loop();
    return () => cancelAnimationFrame(raf);
  }, [video, head]);
  // bande d'images, faite une fois la durée connue
  useEffect(() => {
    if (!src || !(length > 0)) return;
    let stop = false;
    setFrames([]);
    void filmstrip(src, FRAMES, (i, url) => setFrames((f) => { const n = [...f]; n[i] = url; return n; }), () => stop, view ?? undefined).catch(() => undefined);
    return () => { stop = true; };
  }, [src, length, view]);

  const seek = (s: number) => {
    const v = video.current;
    if (!v) return;
    v.pause();
    v.currentTime = s;
  };
  const timeAt = (x: number) => {
    const r = track.current?.getBoundingClientRect();
    return r && r.width > 0 ? v0 + Math.min(1, Math.max(0, (x - r.left) / r.width)) * vw : v0;
  };
  // passage à jour entre deux rendus (glissement rapide)
  const current = useRef(span);
  current.current = span;
  const moveEdge = (edge: Edge, at: number) => {
    const next = setEdge(current.current, edge, at, length);
    current.current = next;
    setTouched(true);
    setSpan(next);
    seek(edge === "start" ? next.start : next.end);
  };
  const zoom = () => setView((cur) => (cur ? null : zoomView(current.current, video.current?.currentTime ?? 0, length)));

  const grab = (e: ReactPointerEvent<HTMLElement>, what: Edge | "head") => {
    e.preventDefault();
    e.stopPropagation();
    e.currentTarget.setPointerCapture(e.pointerId);
    dragging.current = what;
    if (what === "head") seek(timeAt(e.clientX));
    else {
      setHeld(what);
      e.currentTarget.focus();
    }
  };
  const drag = (e: ReactPointerEvent<HTMLElement>) => {
    const what = dragging.current;
    if (!what) {
      setHover(timeAt(e.clientX));
      return;
    }
    if (what === "head") seek(timeAt(e.clientX));
    else moveEdge(what, timeAt(e.clientX));
  };
  const drop = () => {
    dragging.current = null;
    setHeld(null);
  };

  // I et O : début et fin à l'endroit où l'on est
  useKeys(true, (k) => {
    const at = video.current?.currentTime;
    if (at === undefined) return false;
    if (k === "i") { moveEdge("start", at); return true; }
    if (k === "o") { moveEdge("end", at); return true; }
    if (k === "z") { zoom(); return true; }
    return false;
  });

  const playSpan = () => {
    const v = video.current;
    if (!v) return;
    v.currentTime = span.start;
    void v.play();
    const stop = () => { if (v.currentTime >= span.end) { v.pause(); v.removeEventListener("timeupdate", stop); } };
    v.addEventListener("timeupdate", stop);
  };

  const spanLen = span.end - span.start;
  const gifTooLong = spanLen > GIF_MAX + 0.01;

  return (
    <div className="flex flex-col gap-2 px-6 pt-4 pb-3">
      <div className="flex flex-wrap items-center gap-2">
        <span className="flex min-w-[11rem] items-center gap-2 text-[0.9rem] font-bold text-ink">
          {t("Passage : {0} à {1}", instant(span.start), instant(span.end))}
          {/* la durée rebondit à chaque changement */}
          <motion.span key={spanLen.toFixed(1)} initial={{ scale: 1.25, opacity: 0.6 }} animate={{ scale: 1, opacity: 1 }} transition={SPRING.pop}
            className={`rounded-full px-2.5 py-0.5 text-[0.82rem] font-extrabold ${gifTooLong && !vertical ? "tone-pink" : "tone-accent"} bg-[color-mix(in_srgb,var(--tone)_20%,var(--ibox-white))] text-[var(--tone-ink)]`}>
            {instant(spanLen)}
          </motion.span>
        </span>
        <span className="flex-1" />
        <Button tone="blue" variant={view ? "primary" : "soft"} icon={view ? "unzoom" : "zoom"} aria-pressed={!!view} title={hint(view ? t("Voir tout le clip") : t("Zoom sur le passage"), "Z")} onClick={zoom}>{view ? t("Tout le clip") : t("Zoom")}</Button>
        <Button tone="blue" variant="ghost" icon="play" onClick={playSpan}>{t("Lire le passage")}</Button>
        {vertical ? (
          <Button tone="pink" variant="primary" icon="phone" disabled={busy} onClick={() => onRender("vertical", span)}>{t("Créer en vertical")}</Button>
        ) : (
          <>
            <label className="flex cursor-pointer items-center gap-2 rounded-full px-3 py-2 text-[0.86rem] font-bold text-ink-soft transition-colors hover:text-ink" title={t("Le clip coupé prend la place de l’original, effacé une fois le clip prêt.")}>
              <input type="checkbox" data-nav checked={replace} onChange={(e) => { setReplace(e.target.checked); writeReplace(e.target.checked); }} className="peer sr-only" />
              <span aria-hidden="true" className="grid h-5 w-5 place-items-center rounded-md border-2 border-[color-mix(in_srgb,var(--accent)_55%,transparent)] text-transparent transition-colors peer-checked:border-accent peer-checked:bg-accent peer-checked:text-[#fff] peer-focus-visible:ring-4 peer-focus-visible:ring-accent/40"><Icon name="check" size="0.85em" /></span>
              {t("Remplacer l’original")}
            </label>
            <Button variant="primary" icon="cut" disabled={busy} onClick={() => onRender("mp4", span, replace)}>{replace ? t("Couper le clip") : t("Créer le clip")}</Button>
            <Button tone="pink" icon="camera" disabled={busy || gifTooLong} onClick={() => onRender("gif", span)}>{t("Créer un GIF")}</Button>
          </>
        )}
      </div>

      <div
        ref={track}
        className={`relative mx-5 mt-7 h-16 cursor-pointer touch-none select-none rounded-lg bg-cream-deep transition-shadow ${view ? "shadow-[0_0_0_2px_var(--tone-blue)]" : ""}`}
        onPointerDown={(e) => grab(e, "head")}
        onPointerMove={drag}
        onPointerUp={drop}
        onPointerCancel={drop}
        onPointerLeave={() => setHover(null)}
      >
        <div className="absolute inset-0 flex overflow-hidden rounded-lg" aria-hidden="true">
          {Array.from({ length: FRAMES }, (_, i) => (
            <div key={i} className="h-full min-w-0 flex-1 bg-black/20">
              {frames[i] && (
                <motion.img src={frames[i]} alt="" draggable={false} className="h-full w-full object-cover"
                  initial={{ opacity: 0, scale: 1.12 }} animate={{ opacity: 1, scale: 1 }} transition={{ ...SPRING.smooth, delay: i * 0.03 }} />
              )}
            </div>
          ))}
        </div>
        {/* hors du passage : assombri */}
        <motion.div className="pointer-events-none absolute inset-y-0 left-0 rounded-l-lg bg-black/60" initial={false} animate={{ width: pct(span.start) }} transition={FOLLOW} />
        <motion.div className="pointer-events-none absolute inset-y-0 right-0 rounded-r-lg bg-black/60" initial={false} animate={{ width: `${100 - frac(span.end)}%` }} transition={FOLLOW} />
        {/* le passage : cadre violet qui respire quand on tient une poignée */}
        <motion.div className="pointer-events-none absolute inset-y-0 border-y-[3px] border-accent" initial={false}
          animate={{ left: pct(span.start), width: `${frac(span.end) - frac(span.start)}%`, boxShadow: held ? "0 0 22px -2px var(--accent-glow)" : "0 0 0 0 transparent" }} transition={FOLLOW} />
        <Handle edge="start" at={span.start} left={pct(span.start)} len={len} label={t("Début du passage")} held={held === "start"} dim={!inView(span.start)}
          onGrab={(e) => grab(e, "start")} onStep={(d) => moveEdge("start", span.start + d * (view ? FINE / STEP : 1))} onFocusChange={(f) => setFocused(f ? "start" : null)} />
        <Handle edge="end" at={span.end} left={pct(span.end)} len={len} label={t("Fin du passage")} held={held === "end"} dim={!inView(span.end)}
          onGrab={(e) => grab(e, "end")} onStep={(d) => moveEdge("end", span.end + d * (view ? FINE / STEP : 1))} onFocusChange={(f) => setFocused(f ? "end" : null)} />
        {/* l'instant sous la poignée tenue (ou qui a le focus), au-dessus de la bande */}
        <AnimatePresence>
          {(held ?? focused) && (
            <motion.span key="bulle" initial={{ opacity: 0, y: 6, scale: 0.7, left: pct((held ?? focused) === "start" ? span.start : span.end) }}
              animate={{ opacity: 1, y: 0, scale: 1, left: pct((held ?? focused) === "start" ? span.start : span.end) }} exit={{ opacity: 0, y: 4, scale: 0.8 }}
              transition={{ ...FOLLOW, opacity: { duration: 0.15 }, scale: SPRING.pop }}
              className="pointer-events-none absolute bottom-full z-[5] mb-2 -translate-x-1/2 rounded-full bg-accent px-2.5 py-1 text-[0.75rem] font-extrabold whitespace-nowrap text-[#fff] shadow-[0_6px_16px_-6px_var(--accent)]">
              {instant((held ?? focused) === "start" ? span.start : span.end)}
            </motion.span>
          )}
        </AnimatePresence>
        {/* repère sous le pointeur */}
        <AnimatePresence>
          {hover !== null && !held && (
            <motion.div className="pointer-events-none absolute -top-1 bottom-0 z-[2] w-px bg-white/60" style={{ left: pct(hover) }}
              initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={{ duration: 0.12 }}>
              <span className="absolute bottom-full left-1/2 mb-1 -translate-x-1/2 rounded-full bg-ink px-2 py-0.5 text-[0.72rem] font-extrabold whitespace-nowrap text-cream">{instant(hover)}</span>
            </motion.div>
          )}
        </AnimatePresence>
        {/* tête de lecture, image par image */}
        <motion.div className="pointer-events-none absolute -inset-y-2 z-[3] w-[3px] -translate-x-1/2 rounded-full bg-[#fff] shadow-[0_0_10px_rgba(255,255,255,0.7)]" style={{ left: headLeft }}>
          <span className="absolute -top-1 left-1/2 h-2.5 w-2.5 -translate-x-1/2 rounded-full bg-[#fff] shadow-md" />
        </motion.div>
      </div>

      <div className="relative mx-5 h-5 text-[0.78rem] font-extrabold text-ink-soft" aria-hidden="true">
        <motion.span className="absolute -translate-x-1/2" initial={false} animate={{ left: pct(span.start) }} transition={FOLLOW}>{instant(span.start)}</motion.span>
        <motion.span className="absolute -translate-x-1/2" initial={false} animate={{ left: pct(span.end) }} transition={FOLLOW}>{instant(span.end)}</motion.span>
      </div>
      <p className="text-[0.78rem] font-semibold text-ink-faint">
        {vertical ? t("Faites glisser le cadre sur l’image pour choisir ce qui reste, et les poignées pour le passage. La vidéo est réencodée en 9:16, coupée à l’image près.") : view ? t("Zoom : la frise montre {0} à {1}, les poignées avancent d’un dixième de seconde. Z revient à tout le clip.", instant(view.start), instant(view.end)) : gifTooLong ? t("Un GIF dure 10 secondes au plus : raccourcissez le passage.") : t("Faites glisser les poignées pour choisir le passage (I et O au clavier ; à la manette : gauche et droite sur une poignée ; Z : zoom). Le clip commence sur l’image clé qui précède le début (une par seconde).")}
      </p>
    </div>
  );
}

/** Poignée : la saisie part d'ici, le glissement remonte à la bande (`onPointerMove`). */
function Handle({ edge, at, left, len, label, held, dim, onGrab, onStep, onFocusChange }: {
  edge: Edge; at: number; left: string; len: number; label: string; held: boolean; dim?: boolean;
  onGrab: (e: ReactPointerEvent<HTMLElement>) => void; onStep: (delta: number) => void; onFocusChange: (focused: boolean) => void;
}) {
  const ref = useRef<HTMLButtonElement>(null);
  const step = useRef(onStep);
  step.current = onStep;
  // gauche et droite (clavier, manette) arrivent par la navigation (`data-adjust`, nav.ts)
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const onAdjust = (e: Event) => step.current((e as CustomEvent<string>).detail === "left" ? -STEP : STEP);
    el.addEventListener("nav-adjust", onAdjust);
    return () => el.removeEventListener("nav-adjust", onAdjust);
  }, []);
  return (
    <motion.button
      ref={ref}
      type="button"
      role="slider"
      data-nav
      data-adjust
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={Math.round(len * 10) / 10}
      aria-valuenow={at}
      aria-valuetext={instant(at)}
      onPointerDown={onGrab}
      onFocus={() => onFocusChange(true)}
      onBlur={() => onFocusChange(false)}
      initial={false}
      animate={{ left, scaleY: held ? 1.14 : 1, scaleX: held ? 1.2 : 1, opacity: dim ? 0.35 : 1 }}
      whileHover={{ scaleX: 1.18 }}
      transition={FOLLOW}
      style={{ marginLeft: edge === "start" ? "-1.25rem" : 0, originX: edge === "start" ? 1 : 0 }}
      className={`absolute inset-y-0 z-[4] grid w-5 cursor-ew-resize touch-none place-items-center bg-[linear-gradient(180deg,color-mix(in_srgb,var(--accent)_75%,#fff),var(--accent))] shadow-[0_6px_16px_-6px_var(--accent)] outline-none focus-visible:ring-4 focus-visible:ring-accent/40 ${edge === "start" ? "rounded-l-lg" : "rounded-r-lg"}`}
    >
      <span className="flex gap-[3px]" aria-hidden="true">
        <span className="h-5 w-[2px] rounded-full bg-[#fff]/85" />
        <span className="h-5 w-[2px] rounded-full bg-[#fff]/85" />
      </span>
    </motion.button>
  );
}
