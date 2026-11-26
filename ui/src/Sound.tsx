/**
 * Son d'un clip, dans le lecteur (0.6) : volume du jeu et du micro quand le clip a ses pistes
 * séparées (Réglages › Pistes séparées), sinon un volume pour tout le son. Le réglage s'entend
 * tout de suite dans le lecteur (`mix.ts`, 0.8.8) ; « Créer avec ce son » refait le son dans un
 * nouveau clip (`edit.render` `mix`) ; l'image ne change pas, l'original non plus.
 */
import { motion } from "motion/react";
import { SPRING } from "./ibox/motion";
import type { Clip } from "./host";
import { t } from "./i18n";
import { Button } from "./ui";
import { Bars, Slider } from "./controls";
import { Pill, type IconName, type Tone } from "./ui";

export function SoundMix({ clip: c, game, mic, onChange, busy, onMix, onSettings }: {
  clip: Clip; game: number; mic: number; onChange: (v: { game: number; mic: number }) => void;
  busy: boolean; onMix: (game: number, mic: number) => void; onSettings?: () => void;
}) {
  const tracks = c.tracks ?? [];
  const separate = tracks.includes("game") || tracks.includes("mic");
  const setGame = (g: number) => onChange({ game: g, mic });
  const setMic = (m: number) => onChange({ game, mic: m });

  const changed = game !== 1 || mic !== 1;
  return (
    <div className="flex flex-col gap-3 px-6 py-4">
      {separate ? (
        <>
          {tracks.includes("game") && <Row tone="orange" icon="sound" label={t("Son du jeu")} value={game} onChange={setGame} />}
          {tracks.includes("mic") && <Row tone="purple" icon="mic" label={t("Micro")} value={mic} onChange={setMic} />}
        </>
      ) : (
        <Row tone="orange" icon="sound" label={t("Volume du clip")} value={game} onChange={setGame} />
      )}
      <div className="flex flex-wrap items-center gap-3">
        <p className="min-w-0 flex-1 text-[0.78rem] font-semibold text-ink-faint">
          {separate
            ? t("Le lecteur fait entendre ces volumes ; « Créer avec ce son » les garde dans un nouveau clip.")
            : t("Ce clip n’a qu’une piste de son. Activez « Pistes séparées » dans Réglages : le jeu et le micro se régleront à part sur vos prochains clips.")}
        </p>
        {/* ce réglage ne vaut que pour ce clip : les volumes des prochains se règlent à la capture */}
        {onSettings && <Button tone="purple" variant="soft" icon="mic" onClick={onSettings}>{t("Volumes des prochains clips")}</Button>}
        <Button tone="blue" variant="ghost" disabled={!changed} onClick={() => onChange({ game: 1, mic: 1 })}>{t("Rétablir")}</Button>
        <Button tone="orange" variant="primary" icon="sound" disabled={busy || !changed} onClick={() => onMix(game, mic)}>{t("Créer avec ce son")}</Button>
      </div>
    </div>
  );
}

function Row({ tone, icon, label, value, onChange }: { tone: Tone; icon: IconName; label: string; value: number; onChange: (v: number) => void }) {
  return (
    <motion.div layout initial={{ opacity: 0, x: -12 }} animate={{ opacity: 1, x: 0 }} transition={SPRING.smooth}
      className={`tone-${tone} ibox-tile flex items-center gap-4 rounded-lg px-4 py-2`}>
      <Pill icon={icon} tone={tone} size="h-9 w-9" iconSize="1.1em" />
      <span className="w-28 shrink-0 text-[0.92rem] font-extrabold text-ink">{label}</span>
      <Bars level={value} />
      <div className="min-w-0 flex-1"><Slider label={label} value={value} onChange={onChange} /></div>
    </motion.div>
  );
}
