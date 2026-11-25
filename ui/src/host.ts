/**
 * Appels au socle d'iBox. La fenêtre du plugin n'a aucune commande Tauri : tout passe par
 * `POST /iclips/__host/<appel>` sur le protocole du plugin, vérifié par iBox contre les
 * permissions accordées (src-tauri/src/plugins/host.rs). Les vidéos se lisent par
 * `GET /iclips/__clips/<nom>`.
 */

export const PLUGIN_ID = "iclips";

export interface Clip {
  name: string;
  /** `gif` : fait par le montage ; `image` : capture d'écran. Les deux s'affichent comme une image. */
  kind?: "video" | "gif" | "image";
  /** Pistes de son (`mix`, puis `game`, `mic` quand elles sont gardées à part). */
  tracks?: string[];
  game: string | null;
  platformId: string | null;
  /** Horodatage (ms). */
  at: number;
  size: number;
  title: string | null;
  favorite: boolean;
  /** Chemin relatif au plugin (`__clips/<nom>`). */
  src: string;
  /** Vignette faite par le Studio (`__thumbs/<nom>`), et durée en secondes. */
  thumb?: string | null;
  duration?: number | null;
  /** Clip automatique : « Clip ça ! », Vue + A ou fin de partie. */
  auto?: "voice" | "pad" | "end" | null;
  /** Lien de partage (Litterbox) et sa fin de validité (ms). */
  link?: string | null;
  linkUntil?: number | null;
}

/** Ami iNetbox (partage d'un lien). */
export interface Friend { id: string; name: string; online: boolean; game: string | null }

/** État de la mémoire des clips (permission `clips-capture`). */
export interface ClipStatus {
  game: string | null;
  platformId: string | null;
  /** Clips activés dans Paramètres › Captures. */
  enabled: boolean;
  /** Secondes déjà en mémoire, et durée réglée. */
  seconds: number;
  max: number;
  /** Le micro écoute « Clip ça ! » (clips automatiques). */
  listening?: boolean;
  /** Image du jeu figée à l'ouverture du panneau (`__shot`), à garder en capture d'écran. */
  hasShot?: boolean;
}

export type Quality = "low" | "medium" | "high" | "max";

/** Réglages de la capture des clips (permission `clips-settings`). */
export interface CaptureSettings {
  enabled: boolean;
  quality: Quality;
  seconds: number;
  systemAudio: boolean;
  microphone: boolean;
  systemVolume: number;
  micVolume: number;
  tracks: boolean;
  /** Octets tenus en mémoire pendant une partie, nombre et poids des captures. */
  memory: number;
  count: number;
  bytes: number;
  /** Raccourci de l'overlay d'iBox (il ouvre le panneau d'iClips), null s'il est coupé. */
  overlayKeys?: string | null;
}

/** Clips automatiques (permission `clips-auto`). */
export interface AutoStatus {
  voice: boolean;
  pad: boolean;
  end: boolean;
  listening: boolean;
  /** Langue d'écoute (`fr-FR`…), null si Windows n'en a aucune. */
  language: string | null;
  error: string | null;
}

/** Hors d'iBox (aperçu `npm run dev`), des clips d'exemple sans vidéo. */
const inIbox = typeof location !== "undefined" && (location.hostname === "iboxplugin.localhost" || location.protocol === "iboxplugin:");

let sample: Clip[] = [
  { name: "1764060000000-hollow-knight.mp4", game: "Hollow Knight", platformId: "pc", at: Date.now() - 3_600_000, size: 18_400_000, title: "Mante sans un coup", favorite: true, src: "" },
  { name: "1763990000000-celeste.mp4", game: "Celeste", platformId: "pc", at: Date.now() - 86_400_000, size: 9_100_000, title: null, favorite: false, src: "", auto: "voice" },
  { name: "1763900000000-mario-kart-8-deluxe.mp4", game: "Mario Kart 8 Deluxe", platformId: "switch", at: Date.now() - 3 * 86_400_000, size: 31_000_000, title: null, favorite: false, src: "" },
];

let capture: CaptureSettings = { enabled: true, quality: "high", seconds: 30, systemAudio: true, microphone: true, systemVolume: 1, micVolume: 0.8, tracks: true, memory: 52_000_000, count: 14, bytes: 312_000_000, overlayKeys: "Alt+X" };
let auto: AutoStatus = { voice: false, pad: true, end: false, listening: false, language: "fr-FR", error: null };

function preview(call: string, arg: Record<string, unknown>): unknown {
  if (call === "clips.list") return sample;
  if (call === "clips.update") {
    sample = sample.map((c) => (c.name === arg.name ? { ...c, ...(arg.title !== undefined ? { title: String(arg.title).trim() || null } : {}), ...(arg.favorite !== undefined ? { favorite: !!arg.favorite } : {}) } : c));
    return null;
  }
  if (call === "clips.delete") {
    sample = sample.filter((c) => c.name !== arg.name);
    return null;
  }
  if (call === "screen.close") window.close();
  if (call === "edit.render") {
    // aperçu : un rendu imité, en une seconde et demie
    const job = Math.floor(Math.random() * 1e6);
    const emit = (event: string, payload: unknown) => window.dispatchEvent(new CustomEvent("ibox:host", { detail: { event, payload } }));
    [0.25, 0.6, 0.9].forEach((ratio, i) => setTimeout(() => emit("edit://progress", { job, ratio }), 300 * (i + 1)));
    setTimeout(() => {
      const c: Clip = { name: `${Date.now()}-montage.mp4`, game: sample[0]?.game ?? null, platformId: "pc", at: Date.now(), size: 9_000_000, title: (arg.title as string) ?? null, favorite: false, src: "" };
      sample = [c, ...sample];
      emit("edit://done", { job, name: c.name });
    }, 1500);
    return { job };
  }
  if (call === "capture.get") return capture;
  if (call === "capture.set") {
    capture = { ...capture, ...(arg as Partial<CaptureSettings>) };
    return capture;
  }
  if (call === "shot.save") return { name: `${Date.now()}-capture.png` };
  if (call === "share.friends") return [
    { id: "a", name: "Léa", online: true, game: "Celeste" },
    { id: "b", name: "Sam", online: false, game: null },
  ] satisfies Friend[];
  if (call === "share.link") {
    // aperçu : un envoi imité, en deux secondes
    const job = Math.floor(Math.random() * 1e6);
    const emit = (event: string, payload: unknown) => window.dispatchEvent(new CustomEvent("ibox:host", { detail: { event, payload } }));
    [0.2, 0.5, 0.8].forEach((ratio, i) => setTimeout(() => emit("share://progress", { job, ratio }), 400 * (i + 1)));
    setTimeout(() => emit("share://done", { job, name: arg.name, link: "https://litter.catbox.moe/apercu.mp4", until: Date.now() + 72 * 3_600_000 }), 2000);
    return { job };
  }
  if (call === "share.save") return { path: "C:\\Users\\apercu\\Videos\\clip.mp4" };
  if (call.startsWith("share.")) return null;
  if (call === "auto.get") return auto;
  if (call === "auto.set") {
    auto = { ...auto, ...(arg as Partial<AutoStatus>) };
    return auto;
  }
  if (call === "auto.test") {
    // aperçu : la phrase est « entendue » au bout de deux secondes
    setTimeout(() => window.dispatchEvent(new CustomEvent("ibox:host", { detail: { event: "voice://heard", payload: null } })), 2000);
    return { ms: 8000 };
  }
  if (call === "clips.status") return { game: "Hollow Knight", platformId: "pc", enabled: true, seconds: 30, max: 30, hasShot: false } satisfies ClipStatus;
  if (call === "clips.save") {
    const c: Clip = { name: `${Date.now()}-hollow-knight.mp4`, game: "Hollow Knight", platformId: "pc", at: Date.now(), size: 21_000_000, title: null, favorite: false, src: "" };
    sample = [c, ...sample];
    return { name: c.name, game: c.game, src: "" };
  }
  return null;
}

export async function host<T>(call: string, arg: Record<string, unknown> = {}): Promise<T> {
  if (!inIbox) return preview(call, arg) as T;
  const res = await fetch(`/${PLUGIN_ID}/__host/${call}`, { method: "POST", body: JSON.stringify(arg) });
  const out = (await res.json()) as { ok?: T; error?: string };
  if (out.error !== undefined) throw new Error(out.error);
  return out.ok as T;
}

/** Image figée à l'ouverture du panneau ; `stamp` change à chaque ouverture. */
export const shotUrl = (stamp: number) => (inIbox ? `/${PLUGIN_ID}/__shot?at=${stamp}` : "");
export const clipUrl = (c: Pick<Clip, "src">) => (c.src ? `/${PLUGIN_ID}/${c.src}` : "");
export const thumbUrl = (c: Clip) => (c.thumb ? `/${PLUGIN_ID}/${c.thumb}` : "");

/** Événements du socle (`clips://changed`…). */
export function onHost(handler: (event: string, payload: unknown) => void): () => void {
  const listener = (e: Event) => {
    const d = (e as CustomEvent<{ event: string; payload: unknown }>).detail;
    if (d && typeof d.event === "string") handler(d.event, d.payload);
  };
  window.addEventListener("ibox:host", listener);
  return () => window.removeEventListener("ibox:host", listener);
}
