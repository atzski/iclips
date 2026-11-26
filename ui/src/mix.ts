/**
 * Volumes entendus dans le lecteur (0.8.8) : les barres du panneau Son s'entendent pendant la
 * lecture, au lieu de ne changer que le clip créé. Le son du clip est rejoué à côté de la vidéo
 * (muette) : ses pistes séparées (`<nom>.game.m4a`, `<nom>.mic.m4a`) quand il en a, sinon son
 * propre son, chacun avec son volume, jusqu'à 200 % (Web Audio). Les sons suivent la vidéo :
 * lecture, pause, déplacement, vitesse, et se recalent s'ils dérivent.
 *
 * Si un son ne se charge pas (iBox qui ne sert pas encore les pistes) ou ne peut pas jouer, la
 * vidéo garde son propre son, comme avant.
 */
import { useEffect, useRef, type RefObject } from "react";
import { watchQuiet } from "./quiet";

/** Écart toléré entre la vidéo et un son avant de le recaler (secondes). */
export const DRIFT = 0.12;

/** Adresse d'une piste séparée à côté du clip (`__clips/a.mp4` → `__clips/a.game.m4a`). */
export function trackUrl(src: string, track: string): string {
  return src.replace(/\.mp4$/, `.${track}.m4a`);
}

/** Sons à rejouer : les pistes séparées présentes, sinon le son du clip lui-même. */
export function sources(src: string, tracks: string[] | undefined): { key: "game" | "mic"; url: string }[] {
  const t = tracks ?? [];
  const separate = (["game", "mic"] as const).filter((k) => t.includes(k));
  return separate.length ? separate.map((k) => ({ key: k, url: trackUrl(src, k) })) : [{ key: "game", url: src }];
}

/**
 * Volume de la vidéo quand ses sons à part ne jouent pas (pistes absentes) : le son du clip
 * mélange jeu et micro, il suit la plus forte des deux barres, 100 % au plus.
 */
export function fallbackVolume(volumes: { game: number; mic: number }): number {
  return Math.min(1, Math.max(0, volumes.game, volumes.mic));
}

/** Un son a-t-il dérivé de la vidéo au point d'être recalé ? */
export function drifted(sound: number, video: number): boolean {
  return Math.abs(sound - video) > DRIFT;
}

/**
 * Fait entendre `volumes` (jeu, micro ; 1 : tel quel) pendant la lecture de `video`. Inactif sans
 * `src` (image, vidéo illisible).
 */
export function useLiveMix(video: RefObject<HTMLVideoElement | null>, src: string, tracks: string[] | undefined, volumes: { game: number; mic: number }): void {
  const gains = useRef<Map<string, { node?: GainNode; el: HTMLAudioElement }>>(new Map());
  const wanted = useRef(volumes);
  wanted.current = volumes;
  /** Applique les volumes voulus (sons à part, ou la vidéo quand ils ne jouent pas). */
  const applyRef = useRef<() => void>(() => {});
  const key = `${src}|${(tracks ?? []).join(",")}`;

  useEffect(() => {
    const v = video.current;
    if (!v || !src) return;
    const list = sources(src, tracks);
    const ctx: AudioContext | null = typeof AudioContext === "function" ? new AudioContext() : null;
    const els = list.map(({ key: k, url }) => {
      const el = document.createElement("audio");
      el.src = url;
      el.preload = "auto";
      el.hidden = true;
      document.body.appendChild(el);
      const entry: { node?: GainNode; el: HTMLAudioElement } = { el };
      gains.current.set(k, entry);
      return { k, el, entry };
    });
    const stops = els.map(({ el }) => watchQuiet(el));
    /** Les sons jouent-ils à la place de celui de la vidéo ? */
    let live = false;
    let broken = false;
    const level = (k: string) => (k === "mic" ? wanted.current.mic : wanted.current.game);
    const apply = () => {
      // sons injouables : la vidéo garde son propre son, à ce volume (100 % au plus)
      if (broken) { v.volume = fallbackVolume(wanted.current); return; }
      for (const { k, el, entry } of els) {
        if (entry.node) entry.node.gain.value = level(k);
        else el.volume = Math.min(1, level(k));
      }
    };
    applyRef.current = apply;
    const fallBack = () => {
      if (broken) return;
      broken = true;
      live = false;
      for (const { el } of els) el.pause();
      v.muted = false;
      apply();
    };
    const route = () => {
      if (!ctx || ctx.state !== "running") return;
      for (const { el, entry } of els) {
        if (entry.node) continue;
        try {
          const node = ctx.createGain();
          ctx.createMediaElementSource(el).connect(node).connect(ctx.destination);
          entry.node = node;
        } catch { /* le son reste sur le volume de l'élément (100 % au plus) */ }
      }
      apply();
    };
    const sync = (force = false) => {
      for (const { el } of els) {
        if (el.playbackRate !== v.playbackRate) el.playbackRate = v.playbackRate;
        if (force || drifted(el.currentTime, v.currentTime)) el.currentTime = v.currentTime;
      }
    };
    const play = async () => {
      if (broken) return;
      try {
        await ctx?.resume();
      } catch { /* sans Web Audio : volume de l'élément */ }
      route();
      apply();
      sync(true);
      try {
        await Promise.all(els.map(({ el }) => el.play()));
        if (v.paused) { for (const { el } of els) el.pause(); return; }
        live = true;
        v.muted = true;
      } catch {
        fallBack();
      }
    };
    const pause = () => { for (const { el } of els) el.pause(); };
    const seeked = () => sync(true);
    const tick = () => { if (live && !v.paused) sync(); };
    for (const { el } of els) el.addEventListener("error", fallBack);
    v.addEventListener("play", play);
    v.addEventListener("pause", pause);
    v.addEventListener("ended", pause);
    v.addEventListener("seeked", seeked);
    v.addEventListener("ratechange", seeked);
    v.addEventListener("timeupdate", tick);
    if (!v.paused) void play();
    return () => {
      v.removeEventListener("play", play);
      v.removeEventListener("pause", pause);
      v.removeEventListener("ended", pause);
      v.removeEventListener("seeked", seeked);
      v.removeEventListener("ratechange", seeked);
      v.removeEventListener("timeupdate", tick);
      for (const s of stops) s();
      for (const { el } of els) { el.removeEventListener("error", fallBack); el.pause(); el.removeAttribute("src"); el.load(); el.remove(); }
      gains.current.clear();
      applyRef.current = () => {};
      v.muted = false;
      v.volume = 1;
      void ctx?.close();
    };
    // `key` résume src et tracks
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [video, key]);

  // volume changé : appliqué tout de suite, sans recréer les sons
  useEffect(() => applyRef.current(), [volumes.game, volumes.mic]);
}
