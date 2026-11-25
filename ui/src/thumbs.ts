/**
 * Vignette d'un clip, faite ici : WebView2 décode déjà la vidéo (H.264 matériel), le socle
 * d'iBox n'a qu'à garder le JPEG (`clips.thumb`). Image prise à 10 % du clip (une seconde au
 * plus), 480 × 270, recadrée au centre. La vidéo reste muette.
 */

export const THUMB = { w: 480, h: 270 };

/** Rectangle source pour remplir la vignette sans la déformer (recadrage au centre). */
export function cover(vw: number, vh: number, tw = THUMB.w, th = THUMB.h): { sx: number; sy: number; sw: number; sh: number } {
  const scale = Math.max(tw / vw, th / vh);
  const sw = tw / scale, sh = th / scale;
  return { sx: (vw - sw) / 2, sy: (vh - sh) / 2, sw, sh };
}

export async function makeThumb(src: string): Promise<{ jpeg: string; duration: number } | null> {
  if (!src) return null;
  const v = document.createElement("video");
  v.muted = true;
  v.preload = "auto";
  v.src = src;
  const once = (ev: string) => new Promise<void>((ok, ko) => {
    const timer = setTimeout(() => ko(new Error("délai")), 10_000);
    v.addEventListener(ev, () => { clearTimeout(timer); ok(); }, { once: true });
    v.addEventListener("error", () => { clearTimeout(timer); ko(new Error("vidéo illisible")); }, { once: true });
  });
  try {
    await once("loadedmetadata");
    const seeked = once("seeked");
    v.currentTime = Math.min(1, v.duration * 0.1 || 0);
    await seeked;
    const canvas = document.createElement("canvas");
    canvas.width = THUMB.w;
    canvas.height = THUMB.h;
    const { sx, sy, sw, sh } = cover(v.videoWidth, v.videoHeight);
    canvas.getContext("2d")!.drawImage(v, sx, sy, sw, sh, 0, 0, THUMB.w, THUMB.h);
    const jpeg = canvas.toDataURL("image/jpeg", 0.8).split(",")[1] ?? "";
    return jpeg ? { jpeg, duration: v.duration } : null;
  } finally {
    v.removeAttribute("src");
    v.load();
  }
}
