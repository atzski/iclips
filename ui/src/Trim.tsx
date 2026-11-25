/**
 * Coupe d'un clip, dans le lecteur : on pose le début et la fin à l'endroit où l'on est
 * (boutons, ou I et O au clavier), on relit le passage, puis on crée un nouveau clip ou un GIF.
 */
import { useEffect, useState, type RefObject } from "react";
import { clampSpan, GIF_MAX, type Span } from "./edit";
import { duration } from "./format";
import { t } from "./i18n";
import { Button } from "./ui";

export function Trim({ video, length, busy, onRender }: {
  video: RefObject<HTMLVideoElement | null>;
  length: number;
  busy: boolean;
  onRender: (kind: "mp4" | "gif", span: Span) => void;
}) {
  const [span, setSpan] = useState<Span>(() => clampSpan({ start: 0, end: length }, length));
  // tant que rien n'est posé, le passage couvre tout le clip (sa durée arrive avec la vidéo)
  const [touched, setTouched] = useState(false);
  const [now, setNow] = useState(0);
  useEffect(() => setSpan((s) => clampSpan(touched ? s : { start: 0, end: length }, length)), [length, touched]);
  useEffect(() => {
    const v = video.current;
    if (!v) return;
    const tick = () => setNow(v.currentTime);
    v.addEventListener("timeupdate", tick);
    return () => v.removeEventListener("timeupdate", tick);
  }, [video]);

  const at = () => video.current?.currentTime ?? 0;
  const setStart = () => { setTouched(true); setSpan((s) => clampSpan({ start: at(), end: Math.max(s.end, at() + 0.5) }, length)); };
  const setEnd = () => { setTouched(true); setSpan((s) => clampSpan({ start: Math.min(s.start, at() - 0.5), end: at() }, length)); };
  const playSpan = () => {
    const v = video.current;
    if (!v) return;
    v.currentTime = span.start;
    void v.play();
    const stop = () => { if (v.currentTime >= span.end) { v.pause(); v.removeEventListener("timeupdate", stop); } };
    v.addEventListener("timeupdate", stop);
  };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement) return;
      if (e.key === "i" || e.key === "I") setStart();
      else if (e.key === "o" || e.key === "O") setEnd();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const len = length > 0 ? length : 1;
  const pct = (s: number) => `${Math.min(100, Math.max(0, (s / len) * 100))}%`;
  const spanLen = span.end - span.start;
  const gifTooLong = spanLen > GIF_MAX + 0.01;

  return (
    <div className="flex flex-col gap-3 border-t border-line px-6 py-4">
      <div className="relative h-3 rounded-full bg-cream-deep" aria-hidden="true">
        <div className="absolute inset-y-0 rounded-full bg-accent/70" style={{ left: pct(span.start), width: `calc(${pct(span.end)} - ${pct(span.start)})` }} />
        <div className="absolute -inset-y-1 w-0.5 rounded-full bg-ink" style={{ left: pct(now) }} />
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <span className="min-w-[11rem] text-[0.9rem] font-bold text-ink">
          {t("Passage : {0} à {1} ({2})", duration(span.start) || "0:00", duration(span.end) || "0:00", duration(spanLen) || "0:00")}
        </span>
        <Button onClick={setStart}>{t("Début ici")}</Button>
        <Button onClick={setEnd}>{t("Fin ici")}</Button>
        <Button variant="ghost" icon="play" onClick={playSpan}>{t("Lire le passage")}</Button>
        <span className="flex-1" />
        <Button variant="primary" disabled={busy} onClick={() => onRender("mp4", span)}>{t("Créer le clip")}</Button>
        <Button disabled={busy || gifTooLong} onClick={() => onRender("gif", span)}>{t("Créer un GIF")}</Button>
      </div>
      <p className="text-[0.78rem] font-semibold text-ink-faint">
        {gifTooLong ? t("Un GIF dure 10 secondes au plus : raccourcissez le passage.") : t("I et O posent le début et la fin. Le clip commence sur l’image clé qui précède le début (une par seconde).")}
      </p>
    </div>
  );
}
