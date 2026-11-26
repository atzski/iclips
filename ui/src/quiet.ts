/**
 * « Clip ça ! » sourd pendant la lecture d'un clip (0.7.1) : un clip rejoué avec le son, qui sort
 * des haut-parleurs, ne doit pas en créer un autre. Tant qu'une vidéo joue avec le son, iClips
 * demande à iBox une courte pause de la voix (`auto.pause`, iBox 1.0.9) et la renouvelle ; à la
 * fin, la voix revient (après une seconde et demie de grâce côté iBox). Les réglages enregistrés
 * ne changent pas. Sur iBox 1.0.8, l'appel n'existe pas : iClips s'en passe sans erreur.
 */
import { useEffect, type RefObject } from "react";
import { host } from "./host";

/** Durée de chaque pause demandée, et rythme de son renouvellement (ms). */
export const LEASE_MS = 4_000;
export const RENEW_MS = 1_500;

/** iBox connaît-il `auto.pause` ? (faux après un premier refus : iBox 1.0.8) */
let supported = true;
/** Vidéos qui jouent en ce moment avec le son (Studio, panneau). */
const loud = new Set<HTMLMediaElement>();
let timer: number | undefined;

/** Une vidéo s'entend-elle ? */
export function audible(v: Pick<HTMLMediaElement, "paused" | "ended" | "muted" | "volume">): boolean {
  return !v.paused && !v.ended && !v.muted && v.volume > 0;
}

function ask(ms: number) {
  if (!supported) return;
  host("auto.pause", { ms }).catch(() => {
    // iBox 1.0.8 : appel inconnu (ou permission absente) ; on n'insiste pas
    supported = false;
    window.clearInterval(timer);
    timer = undefined;
  });
}

function update(v: HTMLMediaElement) {
  const was = loud.size > 0;
  if (v.isConnected && audible(v)) loud.add(v);
  else loud.delete(v);
  const now = loud.size > 0;
  if (now && !was) {
    ask(LEASE_MS);
    timer = window.setInterval(() => ask(LEASE_MS), RENEW_MS);
  } else if (!now && was) {
    window.clearInterval(timer);
    timer = undefined;
    ask(0);
  }
}

const EVENTS = ["play", "playing", "pause", "ended", "volumechange", "emptied"] as const;

/** Surveille une vidéo : la voix se tait tant qu'elle joue avec le son. `key` : la vidéo changée. */
export function useQuietVoice(video: RefObject<HTMLMediaElement | null>, key?: unknown): void {
  useEffect(() => {
    const v = video.current;
    return v ? watchQuiet(v) : undefined;
  }, [video, key]);
}

/** Même surveillance pour un son créé hors de React (pistes du lecteur, `mix.ts`). */
export function watchQuiet(v: HTMLMediaElement): () => void {
  const on = () => update(v);
  for (const e of EVENTS) v.addEventListener(e, on);
  on();
  return () => {
    for (const e of EVENTS) v.removeEventListener(e, on);
    // lecteur fermé en pleine lecture : la voix revient tout de suite (après la grâce)
    if (loud.delete(v) && loud.size === 0) {
      window.clearInterval(timer);
      timer = undefined;
      ask(0);
    }
  };
}
