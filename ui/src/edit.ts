/**
 * Montage (iClips 0.3, permission `clips-edit`) : iBox fait le rendu dans son socle
 * (`edit.render`) et prévient par `edit://progress` puis `edit://done`. Les originaux ne
 * changent jamais : chaque rendu est un nouveau clip.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { host, onHost } from "./host";

export const GIF_MAX = 10;
/** Durée prise pour un clip dont la durée n'est pas encore connue (le rendu s'arrête à la fin). */
const UNKNOWN = 35;

export interface Span { start: number; end: number }
export type Edge = "start" | "end";

/** Bornes d'un passage dans un clip de `length` s : début avant fin, une demi-seconde au moins. */
export function clampSpan(span: Span, length: number): Span {
  const len = Number.isFinite(length) && length > 0 ? length : UNKNOWN;
  const start = Math.min(Math.max(0, span.start), Math.max(0, len - 0.5));
  const end = Math.min(len, Math.max(start + 0.5, span.end));
  return { start: round(start), end: round(end) };
}

const round = (s: number) => Math.round(s * 10) / 10;

/** Poignée tirée à `at` : l'autre bout ne bouge pas, une demi-seconde reste entre les deux. */
export function setEdge(span: Span, edge: Edge, at: number, length: number): Span {
  return edge === "start"
    ? clampSpan({ start: Math.min(at, span.end - 0.5), end: span.end }, length)
    : clampSpan({ start: span.start, end: Math.max(at, span.start + 0.5) }, length);
}

/** Assemblage de clips entiers, dans l'ordre choisi. */
export function joinSegments(clips: { name: string; duration?: number | null }[]) {
  return clips.map((c) => ({ name: c.name, start: 0, end: c.duration && c.duration > 0 ? c.duration : UNKNOWN }));
}

export interface Job { id: number; ratio: number; label: string }

/** Rendu en cours (un à la fois) et sa fin. */
export function useRender(onDone: (name: string) => void, onError: (message: string) => void) {
  const [job, setJob] = useState<Job | null>(null);
  const current = useRef<number | null>(null);
  // fin arrivée avant la réponse d'`edit.render` (rendu très court)
  const early = useRef(new Map<number, { name?: string; error?: string }>());
  const done = useRef({ onDone, onError });
  done.current = { onDone, onError };

  const finish = useCallback((r: { name?: string; error?: string }) => {
    current.current = null;
    setJob(null);
    if (r.name) done.current.onDone(r.name);
    else done.current.onError(r.error ?? "?");
  }, []);

  useEffect(
    () =>
      onHost((event, payload) => {
        const p = payload as { job?: number; ratio?: number; name?: string; error?: string } | null;
        if (!p || typeof p.job !== "number") return;
        if (event === "edit://progress" && p.job === current.current) setJob((j) => (j ? { ...j, ratio: p.ratio ?? j.ratio } : j));
        else if (event === "edit://done") {
          if (p.job === current.current) finish(p);
          else early.current.set(p.job, p);
        }
      }),
    [finish],
  );

  const render = useCallback(
    async (arg: Record<string, unknown>, label: string) => {
      const { job: id } = await host<{ job: number }>("edit.render", arg);
      const already = early.current.get(id);
      if (already) {
        early.current.delete(id);
        finish(already);
        return;
      }
      current.current = id;
      setJob({ id, ratio: 0, label });
    },
    [finish],
  );

  return { job, render };
}
