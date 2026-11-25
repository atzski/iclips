/**
 * Son d'un clip, dans le lecteur (0.6) : volume du jeu et du micro quand le clip a ses pistes
 * séparées (Réglages › Pistes séparées), sinon un volume pour tout le son. iBox refait le son
 * dans un nouveau clip (`edit.render` `mix`) ; l'image ne change pas, l'original non plus.
 */
import { useEffect, useState, type RefObject } from "react";
import type { Clip } from "./host";
import { t } from "./i18n";
import { Button } from "./ui";
import { Slider } from "./controls";

export function SoundMix({ clip: c, video, busy, onMix }: { clip: Clip; video: RefObject<HTMLVideoElement | null>; busy: boolean; onMix: (game: number, mic: number) => void }) {
  const tracks = c.tracks ?? [];
  const separate = tracks.includes("game") || tracks.includes("mic");
  const [game, setGame] = useState(1);
  const [mic, setMic] = useState(1);
  // sans pistes séparées, le lecteur fait entendre le réglage (jusqu'à 100 %)
  useEffect(() => {
    const v = video.current;
    if (!v || separate) return;
    v.volume = Math.min(1, game);
    return () => { v.volume = 1; };
  }, [game, separate, video]);

  const changed = game !== 1 || mic !== 1;
  return (
    <div className="flex flex-col gap-3 border-t border-line px-6 py-4">
      {separate ? (
        <>
          {tracks.includes("game") && <Row label={t("Son du jeu")} value={game} onChange={setGame} />}
          {tracks.includes("mic") && <Row label={t("Micro")} value={mic} onChange={setMic} />}
        </>
      ) : (
        <Row label={t("Volume du clip")} value={game} onChange={setGame} />
      )}
      <div className="flex flex-wrap items-center gap-3">
        <p className="min-w-0 flex-1 text-[0.78rem] font-semibold text-ink-faint">
          {separate
            ? t("Le lecteur joue le mélange d’origine : le nouveau s’entend dans le clip créé.")
            : t("Ce clip n’a qu’une piste de son. Activez « Pistes séparées » dans Réglages : le jeu et le micro se régleront à part sur vos prochains clips.")}
        </p>
        <Button tone="blue" variant="ghost" disabled={!changed} onClick={() => { setGame(1); setMic(1); }}>{t("Rétablir")}</Button>
        <Button tone="orange" variant="primary" icon="sound" disabled={busy || !changed} onClick={() => onMix(game, mic)}>{t("Créer avec ce son")}</Button>
      </div>
    </div>
  );
}

function Row({ label, value, onChange }: { label: string; value: number; onChange: (v: number) => void }) {
  return (
    <div className="flex items-center gap-4">
      <span className="w-32 shrink-0 text-[0.92rem] font-extrabold text-ink">{label}</span>
      <div className="min-w-0 flex-1"><Slider label={label} value={value} onChange={onChange} /></div>
    </div>
  );
}
