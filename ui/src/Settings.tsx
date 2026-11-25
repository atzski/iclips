/**
 * Réglages d'iClips (0.6, permission `clips-settings`) : la capture des clips, réglée ici comme
 * dans iBox (Paramètres › Captures suit) : durée, qualité, son de Windows et micro avec leur
 * volume, pistes séparées ; raccourcis ; place prise sur le disque.
 */
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { SPRING } from "./ibox/motion";
import { host, type CaptureSettings, type Quality } from "./host";
import { size } from "./format";
import { t } from "./i18n";
import { Button, Pill, type IconName, type Tone } from "./ui";
import { DevicePicker, Slider, Switch, type Device } from "./controls";
import { groups, Kbd } from "./keys";

const SECONDS = [15, 30, 60, 120];
const QUALITIES: { value: Quality; label: string; hint: string }[] = [
  { value: "low", label: "Légère", hint: "480p · 30 i/s" },
  { value: "medium", label: "Standard", hint: "720p · 30 i/s" },
  { value: "high", label: "Haute", hint: "1080p · 60 i/s" },
  { value: "max", label: "Maximale", hint: "1440p · 60 i/s" },
];

export function SettingsSheet({ onClose }: { onClose: () => void }) {
  const [s, setS] = useState<CaptureSettings | null>(null);
  const [error, setError] = useState<string | null>(null);
  const back = useRef<HTMLButtonElement>(null);
  const timer = useRef<number | undefined>(undefined);

  useEffect(() => {
    back.current?.focus();
    host<CaptureSettings>("capture.get").then(setS).catch((e: Error) => setError(e.message));
  }, []);

  // tout de suite à l'écran, envoyé à iBox après un court temps (glissières)
  const set = (patch: Partial<CaptureSettings>, wait = 0) => {
    if (!s) return;
    setS({ ...s, ...patch });
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      host<CaptureSettings>("capture.set", patch).then((r) => setS((cur) => (cur ? { ...r, systemVolume: cur.systemVolume, micVolume: cur.micVolume } : r))).catch((e: Error) => setError(e.message));
    }, wait);
  };

  return (
    <motion.div data-nav-scope className="fixed inset-0 z-10 flex items-center justify-center bg-black/60 p-8" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={{ duration: 0.18 }} onClick={(e) => e.target === e.currentTarget && onClose()}>
      <motion.div role="dialog" aria-modal="true" aria-label={t("Réglages")} className="flex max-h-full w-full max-w-[48rem] flex-col gap-4 overflow-y-auto rounded-xl bg-white p-6 shadow-lg" initial={{ scale: 0.96, y: 12 }} animate={{ scale: 1, y: 0 }} exit={{ scale: 0.97, y: 8, transition: { duration: 0.15 } }} transition={SPRING.smooth}>
        <header className="flex items-center gap-3">
          <Button ref={back} tone="blue" icon="back" variant="ghost" onClick={onClose} aria-label={t("Retour")} className="px-3" />
          <h2 className="flex-1 font-display text-[1.35rem] leading-tight text-ink">{t("Réglages")}</h2>
        </header>

        <Section tone="accent" icon="play" title={t("Capture")}>
          <div className={`flex items-center gap-3 rounded-lg px-4 py-3 ${s && !s.enabled ? "tone-red bg-[color-mix(in_srgb,var(--tone)_14%,var(--ibox-white))]" : "bg-cream-deep/60"}`}>
            <div className="min-w-0 flex-1">
              <div className="text-[0.95rem] font-extrabold text-ink">{t("Mémoire des clips")}</div>
              <div className="text-[0.82rem] font-semibold text-ink-soft">
                {s && !s.enabled ? t("Coupée : aucun clip ne peut être gardé pendant vos parties.") : t("Pendant vos parties, iBox garde les dernières secondes pour en faire un clip.")}
              </div>
            </div>
            {s && <Switch label={t("Mémoire des clips")} on={s.enabled} onChange={(v) => set({ enabled: v })} />}
          </div>
          <Label text={t("Durée d’un clip")} hint={s ? t("Mémoire tenue en jeu : environ {0}", size(s.memory)) : ""} />
          <div className="flex flex-wrap gap-2">
            {SECONDS.map((n) => (
              <Chip key={n} on={s?.seconds === n} onClick={() => set({ seconds: n })}>{n >= 60 ? t("{0} min", n / 60) : t("{0} s", n)}</Chip>
            ))}
          </div>
          <Label text={t("Qualité")} hint={t("La carte graphique encode l’image, sans ralentir le jeu sur un PC récent.")} />
          <div className="grid grid-cols-4 gap-2">
            {QUALITIES.map((q) => (
              <button key={q.value} type="button" data-nav aria-pressed={s?.quality === q.value} onClick={() => set({ quality: q.value })}
                className="tone-accent ibox-tile flex flex-col items-start rounded-lg px-4 py-3 text-left text-ink">
                <span className="text-[0.95rem] font-extrabold">{t(q.label)}</span>
                <span className={`text-[0.78rem] font-semibold ${s?.quality === q.value ? "opacity-85" : "text-ink-soft"}`}>{q.hint}</span>
              </button>
            ))}
          </div>
        </Section>

        <Section tone="orange" icon="sound" title={t("Son")}>
          <Track label={t("Son de Windows")} hint={t("Le jeu, et tout ce que joue la sortie choisie (casque, haut-parleurs)")} on={!!s?.systemAudio} volume={s?.systemVolume ?? 1}
            onToggle={(v) => set({ systemAudio: v })} onVolume={(v) => set({ systemVolume: v }, 300)}
            devices={s?.outputs ?? []} device={s?.outputDevice} onDevice={(id) => set({ outputDevice: id })} deviceLabel={t("Sortie écoutée")} />
          <Track label={t("Micro")} hint={t("Votre voix, par le micro choisi")} on={!!s?.microphone} volume={s?.micVolume ?? 1}
            onToggle={(v) => set({ microphone: v })} onVolume={(v) => set({ micVolume: v }, 300)}
            devices={s?.inputs ?? []} device={s?.micDevice} onDevice={(id) => set({ micDevice: id })} deviceLabel={t("Micro utilisé")} />
          <div className="flex items-center gap-3 rounded-lg bg-cream-deep/60 px-4 py-3">
            <div className="min-w-0 flex-1">
              <div className="text-[0.95rem] font-extrabold text-ink">{t("Pistes séparées")}</div>
              <div className="text-[0.82rem] font-semibold text-ink-soft">{t("Le jeu et le micro gardés chacun à part : leur volume se règle après coup, dans le lecteur (Son).")}</div>
            </div>
            <Switch label={t("Pistes séparées")} on={!!s?.tracks} onChange={(v) => set({ tracks: v })} />
          </div>
        </Section>

        <Section tone="blue" icon="pad" title={t("Raccourcis")}>
          <ul className="grid grid-cols-2 gap-2 text-[0.88rem] font-semibold text-ink-soft">
            {groups(s?.overlayKeys)[0].items.map(([k, what]) => <Key key={what} keys={k} what={what} />)}
          </ul>
          <p className="text-[0.8rem] font-semibold text-ink-faint">{s && !s.overlayKeys ? t("Le raccourci de l’overlay est coupé dans iBox (Paramètres › Système).") : t("Il se change dans iBox, Paramètres › Système. Touche ? : tous les raccourcis du Studio.")}</p>
        </Section>

        <Section tone="teal" icon="folder" title={t("Stockage")}>
          <div className="flex items-center gap-3">
            <span className="flex-1 text-[0.92rem] font-bold text-ink">{s ? t("{0} fichiers · {1}", s.count, size(s.bytes)) : "…"}</span>
            <Button tone="teal" icon="folder" onClick={() => void host("capture.folder").catch((e: Error) => setError(e.message))}>{t("Ouvrir le dossier")}</Button>
          </div>
        </Section>
        {error && <p role="alert" className="text-[0.85rem] font-bold text-peach-deep">{t("Action impossible : {0}", error)}</p>}
      </motion.div>
    </motion.div>
  );
}

function Section({ tone, icon, title, children }: { tone: Tone; icon: IconName; title: string; children: React.ReactNode }) {
  return (
    <section className={`tone-${tone} flex flex-col gap-3 rounded-xl border-l-4 border-[var(--tone)] bg-[color-mix(in_srgb,var(--tone)_5%,transparent)] py-3 pr-1 pl-4`}>
      <h3 className="flex items-center gap-2.5 text-[0.8rem] font-extrabold tracking-[0.14em] text-[var(--tone-ink)] uppercase"><Pill icon={icon} tone={tone} size="h-8 w-8" iconSize="1.05em" />{title}</h3>
      {children}
    </section>
  );
}

function Label({ text, hint }: { text: string; hint: string }) {
  return (
    <div className="flex items-baseline gap-3">
      <span className="text-[0.95rem] font-extrabold text-ink">{text}</span>
      <span className="text-[0.8rem] font-semibold text-ink-faint">{hint}</span>
    </div>
  );
}

function Chip({ on, onClick, children }: { on: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button type="button" data-nav aria-pressed={on} onClick={onClick}
      className="ibox-btn ibox-btn-soft tone-accent h-10 rounded-full px-5 text-[0.92rem] font-bold">
      {children}
    </button>
  );
}

function Track({ label, hint, on, volume, onToggle, onVolume, devices, device, onDevice, deviceLabel }: {
  label: string; hint: string; on: boolean; volume: number; onToggle: (v: boolean) => void; onVolume: (v: number) => void;
  devices: Device[]; device: string | null | undefined; onDevice: (id: string | null) => void; deviceLabel: string;
}) {
  return (
    <div className="flex flex-col gap-2 rounded-lg bg-cream-deep/60 px-4 py-3">
      <div className="flex items-center gap-3">
        <div className="min-w-0 flex-1">
          <div className="text-[0.95rem] font-extrabold text-ink">{label}</div>
          <div className="text-[0.82rem] font-semibold text-ink-soft">{hint}</div>
        </div>
        <Switch label={label} on={on} onChange={onToggle} />
      </div>
      <AnimatePresence initial={false}>
        {on && (
          <motion.div className="flex flex-col gap-2" initial={{ height: 0, opacity: 0 }} animate={{ height: "auto", opacity: 1 }} exit={{ height: 0, opacity: 0, transition: { duration: 0.16 } }} transition={{ height: SPRING.smooth, opacity: { duration: 0.18 } }}>
            <DevicePicker label={deviceLabel} devices={devices} value={device} onChange={onDevice}
              defaultLabel={t("Par défaut de Windows")} missingLabel={t("Appareil débranché : celui par défaut est utilisé")} />
            <Slider label={t("Volume : {0}", label)} value={volume} onChange={onVolume} />
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

function Key({ keys, what }: { keys: string; what: string }) {
  return (
    <li className="flex items-center gap-3 rounded-lg bg-cream-deep/60 px-3 py-2">
      <Kbd>{keys}</Kbd>
      <span className="min-w-0">{what}</span>
    </li>
  );
}
