/**
 * Appels au socle d'iBox. La fenêtre du plugin n'a aucune commande Tauri : tout passe par
 * `POST /iclips/__host/<appel>` sur le protocole du plugin, vérifié par iBox contre les
 * permissions accordées (src-tauri/src/plugins/host.rs). Les vidéos se lisent par
 * `GET /iclips/__clips/<nom>`.
 */

export const PLUGIN_ID = "iclips";

export interface Clip {
  name: string;
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
}

/** État de la mémoire des clips (permission `clips-capture`). */
export interface ClipStatus {
  game: string | null;
  platformId: string | null;
  /** Clips activés dans Paramètres › Captures. */
  enabled: boolean;
  /** Secondes déjà en mémoire, et durée réglée. */
  seconds: number;
  max: number;
}

/** Hors d'iBox (aperçu `npm run dev`), des clips d'exemple sans vidéo. */
const inIbox = location.hostname === "iboxplugin.localhost" || location.protocol === "iboxplugin:";

let sample: Clip[] = [
  { name: "1764060000000-hollow-knight.mp4", game: "Hollow Knight", platformId: "pc", at: Date.now() - 3_600_000, size: 18_400_000, title: "Mante sans un coup", favorite: true, src: "" },
  { name: "1763990000000-celeste.mp4", game: "Celeste", platformId: "pc", at: Date.now() - 86_400_000, size: 9_100_000, title: null, favorite: false, src: "" },
  { name: "1763900000000-mario-kart-8-deluxe.mp4", game: "Mario Kart 8 Deluxe", platformId: "switch", at: Date.now() - 3 * 86_400_000, size: 31_000_000, title: null, favorite: false, src: "" },
];

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
  if (call === "clips.status") return { game: "Hollow Knight", platformId: "pc", enabled: true, seconds: 30, max: 30 } satisfies ClipStatus;
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
