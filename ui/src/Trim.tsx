/**
 * Coupe d'un clip, dans le lecteur : une bande d'images du clip et deux poignées à faire glisser
 * pour choisir le passage (souris ; au clavier et à la manette, gauche et droite sur une
 * poignée). La vidéo montre l'image sous la poignée tirée ; un clic sur la bande y place la
 * lecture. On relit le passage, puis on crée un nouveau clip ou un GIF.
 */
import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent, type RefObject } from "react";
import { clampSpan, GIF_MAX, setEdge, type Edge, type Span } from "./edit";
import { instant } from "./format";
import { t } from "./i18n";
import { filmstrip } from "./thumbs";
import { Button } from "./ui";

/** Pas d'une poignée au clavier ou à la manette (secondes). */
const STEP = 0.2;
const FRAMES = 12;

export function Trim({ video, src, length, busy, vertical, onRender }: {
  video: RefObject<HTMLVideoElement | null>;
  src: string;
  length: number;
  busy: boolean;
  /** Format vertical : un seul bouton, « Créer en vertical ». */
  vertical?: boolean;
  onRender: (kind: "mp4" | "gif" | "vertical", span: Span) => void;
}) {
  const [span, setSpan] = useState<Span>(() => clampSpan({ start: 0, end: length }, length));
  // tant que rien n'est tiré, le passage couvre tout le clip (sa durée arrive avec la vidéo)
  const [touched, setTouched] = useState(false);
  const [now, setNow] = useState(0);
  const [frames, setFrames] = useState<string[]>([]);
  const track = useRef<HTMLDivElement>(null);
  const dragging = useRef<Edge | "head" | null>(null);

  useEffect(() => setSpan((s) => clampSpan(touched ? s : { start: 0, end: length }, length)), [length, touched]);
  useEffect(() => {
    const v = video.current;
    if (!v) return;
    const tick = () => setNow(v.currentTime);
    v.addEventListener("timeupdate", tick);
    return () => v.removeEventListener("timeupdate", tick);
  }, [video]);
  // bande d'images, faite une fois la durée connue
  useEffect(() => {
    if (!src || !(length > 0)) return;
    let stop = false;
    setFrames([]);
    void filmstrip(src, FRAMES, (i, url) => setFrames((f) => { const n = [...f]; n[i] = url; return n; }), () => stop).catch(() => undefined);
    return () => { stop = true; };
  }, [src, length]);

  const len = length > 0 ? length : 1;
  const pct = (s: number) => `${Math.min(100, Math.max(0, (s / len) * 100))}%`;
  const seek = (s: number) => {
    const v = video.current;
    if (!v) return;
    v.pause();
    v.currentTime = s;
    setNow(s);
  };
  const timeAt = (x: number) => {
    const r = track.current?.getBoundingClientRect();
    return r && r.width > 0 ? Math.min(1, Math.max(0, (x - r.left) / r.width)) * len : 0;
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

  const grab = (e: ReactPointerEvent<HTMLElement>, what: Edge | "head") => {
    e.preventDefault();
    e.stopPropagation();
    e.currentTarget.setPointerCapture(e.pointerId);
    dragging.current = what;
    if (what === "head") seek(timeAt(e.clientX));
    else e.currentTarget.focus();
  };
  const drag = (e: ReactPointerEvent<HTMLElement>) => {
    const what = dragging.current;
    if (!what) return;
    if (what === "head") seek(timeAt(e.clientX));
    else moveEdge(what, timeAt(e.clientX));
  };
  const drop = () => { dragging.current = null; };

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
    <div className="flex flex-col gap-2 border-t border-line px-6 pt-4 pb-3">
      <div className="flex flex-wrap items-center gap-2">
        <span className="min-w-[11rem] text-[0.9rem] font-bold text-ink">
          {t("Passage : {0} à {1} ({2})", instant(span.start), instant(span.end), instant(spanLen))}
        </span>
        <span className="flex-1" />
        <Button tone="blue" variant="ghost" icon="play" onClick={playSpan}>{t("Lire le passage")}</Button>
        {vertical ? (
          <Button tone="pink" variant="primary" icon="phone" disabled={busy} onClick={() => onRender("vertical", span)}>{t("Créer en vertical")}</Button>
        ) : (
          <>
            <Button variant="primary" icon="cut" disabled={busy} onClick={() => onRender("mp4", span)}>{t("Créer le clip")}</Button>
            <Button tone="pink" icon="camera" disabled={busy || gifTooLong} onClick={() => onRender("gif", span)}>{t("Créer un GIF")}</Button>
          </>
        )}
      </div>

      <div
        ref={track}
        className="relative mx-5 mt-1 h-16 cursor-pointer touch-none select-none rounded-lg bg-cream-deep"
        onPointerDown={(e) => grab(e, "head")}
        onPointerMove={drag}
        onPointerUp={drop}
        onPointerCancel={drop}
      >
        <div className="absolute inset-0 flex overflow-hidden rounded-lg" aria-hidden="true">
          {Array.from({ length: FRAMES }, (_, i) => (
            <div key={i} className="h-full min-w-0 flex-1 bg-black/20">
              {frames[i] && <img src={frames[i]} alt="" draggable={false} className="h-full w-full object-cover" />}
            </div>
          ))}
        </div>
        {/* hors du passage : assombri */}
        <div className="pointer-events-none absolute inset-y-0 left-0 rounded-l-lg bg-black/60" style={{ width: pct(span.start) }} />
        <div className="pointer-events-none absolute inset-y-0 right-0 rounded-r-lg bg-black/60" style={{ width: `calc(100% - ${pct(span.end)})` }} />
        <div className="pointer-events-none absolute inset-y-0 border-y-[3px] border-accent" style={{ left: pct(span.start), width: `calc(${pct(span.end)} - ${pct(span.start)})` }} />
        <Handle edge="start" at={span.start} len={len} label={t("Début du passage")} style={{ left: `calc(${pct(span.start)} - 1.25rem)` }}
          onGrab={(e) => grab(e, "start")} onStep={(d) => moveEdge("start", span.start + d)} />
        <Handle edge="end" at={span.end} len={len} label={t("Fin du passage")} style={{ left: pct(span.end) }}
          onGrab={(e) => grab(e, "end")} onStep={(d) => moveEdge("end", span.end + d)} />
        <div className="pointer-events-none absolute -inset-y-1.5 w-[3px] -translate-x-1/2 rounded-full bg-white shadow-md" style={{ left: pct(now) }} />
      </div>

      <div className="relative mx-5 h-5 text-[0.78rem] font-extrabold text-ink-soft" aria-hidden="true">
        <span className="absolute -translate-x-1/2" style={{ left: pct(span.start) }}>{instant(span.start)}</span>
        <span className="absolute -translate-x-1/2" style={{ left: pct(span.end) }}>{instant(span.end)}</span>
      </div>
      <p className="text-[0.78rem] font-semibold text-ink-faint">
        {vertical ? t("Faites glisser le cadre sur l’image pour choisir ce qui reste, et les poignées pour le passage. La vidéo est réencodée en 9:16, coupée à l’image près.") : gifTooLong ? t("Un GIF dure 10 secondes au plus : raccourcissez le passage.") : t("Faites glisser les poignées pour choisir le passage (à la manette : gauche et droite sur une poignée). Le clip commence sur l’image clé qui précède le début (une par seconde).")}
      </p>
    </div>
  );
}

/** Poignée : la saisie part d'ici, le glissement remonte à la bande (`onPointerMove`). */
function Handle({ edge, at, len, label, style, onGrab, onStep }: {
  edge: Edge; at: number; len: number; label: string; style: React.CSSProperties;
  onGrab: (e: ReactPointerEvent<HTMLElement>) => void; onStep: (delta: number) => void;
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
    <button
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
      style={style}
      className={`absolute inset-y-0 z-[1] grid w-5 cursor-ew-resize touch-none place-items-center bg-accent shadow-md outline-none transition-[filter] hover:brightness-110 focus-visible:ring-4 focus-visible:ring-accent/40 ${edge === "start" ? "rounded-l-lg" : "rounded-r-lg"}`}
    >
      <span className="flex gap-[3px]" aria-hidden="true">
        <span className="h-5 w-[2px] rounded-full bg-on-accent/80" />
        <span className="h-5 w-[2px] rounded-full bg-on-accent/80" />
      </span>
    </button>
  );
}
