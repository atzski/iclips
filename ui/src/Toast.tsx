/**
 * Notification de l'app autonome (`ui/toast.html`), sur le modèle de celle d'iBox : une carte à la
 * fois en bas à droite de l'écran, par-dessus le jeu, avec le son « clip » quand un clip est
 * gardé sans ouvrir le panneau (« Clip ça ! », Vue + A, fin de partie). Dans iBox, c'est la
 * notification d'iBox qui s'en charge : cette page ne sert qu'à l'app.
 */
import { AnimatePresence, motion } from "motion/react";
import { useCallback, useEffect, useRef, useState } from "react";
import { host, onHost } from "./host";
import { t } from "./i18n";
import { SPRING } from "./ibox/motion";
import { playClip } from "./shutter";
import { Icon, type IconName } from "./ui";

interface Payload {
  kind: "clip" | "notice";
  name: string;
  body: string;
  gameTitle?: string | null;
  icon?: string;
  tone?: "info" | "ok" | "warn";
}

const DURATION = { clip: 4500, notice: 8000 } as const;
const LOOK = {
  clip: { from: "#ECE4FF", to: "#B9A2F4", ink: "#6A4FB3" },
  info: { from: "#DDF0FF", to: "#88CBF5", ink: "#2C6FA8" },
  ok: { from: "#DDF6EA", to: "#98DEBE", ink: "#2F7A57" },
  warn: { from: "#FFE1DA", to: "#F3AFA4", ink: "#B4513F" },
} as const;
const ICONS: Record<string, IconName> = { play: "play", sound: "mic", mic: "mic" };

export function ToastPage() {
  const [queue, setQueue] = useState<(Payload & { key: number })[]>([]);
  const counter = useRef(0);
  const current = queue[0];
  const dark = document.documentElement.dataset.theme !== "light";

  const pull = useCallback(async () => {
    const items = (await host<Payload[] | null>("toast.pull").catch(() => null)) ?? [];
    if (items.length) setQueue((q) => [...q, ...items.map((p) => ({ ...p, key: ++counter.current }))]);
  }, []);

  useEffect(() => {
    void pull();
    return onHost((event) => { if (event === "toast://push") void pull(); });
  }, [pull]);

  const next = useCallback(() => setQueue((q) => q.slice(1)), []);

  return (
    <div className="fixed inset-0 flex items-end justify-end px-5 pt-3 pb-5">
      <AnimatePresence onExitComplete={() => queue.length === 0 && void host("toast.done")}>
        {current && <Card key={current.key} toast={current} dark={dark} onDone={next} />}
      </AnimatePresence>
    </div>
  );
}

function Card({ toast, dark, onDone }: { toast: Payload; dark: boolean; onDone: () => void }) {
  const look = toast.kind === "clip" ? LOOK.clip : LOOK[toast.tone ?? "info"];
  const icon: IconName = toast.kind === "clip" ? "play" : (ICONS[toast.icon ?? ""] ?? "auto");
  const [paused, setPaused] = useState(false);
  const duration = DURATION[toast.kind] ?? 6000;
  const left = useRef(duration);
  const ink = dark ? look.to : look.ink;

  useEffect(() => { if (toast.kind === "clip") playClip(); }, []); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    if (paused) return;
    const start = performance.now();
    const timer = window.setTimeout(onDone, left.current);
    return () => { window.clearTimeout(timer); left.current -= performance.now() - start; };
  }, [paused, onDone]);

  const open = () => {
    if (toast.kind === "clip") void host("toast.open");
    onDone();
  };

  return (
    <motion.div
      role="status"
      initial={{ x: 60, y: 8, opacity: 0, scale: 0.94 }}
      animate={{ x: 0, y: 0, opacity: 1, scale: 1 }}
      exit={{ x: 70, opacity: 0, scale: 0.96, transition: { duration: 0.22, ease: [0.4, 0, 1, 1] } }}
      transition={SPRING.snappy}
      whileHover={{ y: -2 }}
      onMouseEnter={() => setPaused(true)}
      onMouseLeave={() => setPaused(false)}
      onClick={open}
      className="group relative w-[24rem] cursor-pointer rounded-[1.45rem] p-[1.5px] shadow-[0_18px_38px_-20px_rgba(20,30,60,0.6),0_4px_12px_-6px_rgba(20,30,60,0.22)]"
      style={{ background: `linear-gradient(135deg, ${look.to}, color-mix(in srgb, ${look.from} 40%, transparent) 45%, color-mix(in srgb, ${look.to} 55%, transparent))` }}
    >
      <div className="relative flex min-h-[6.6rem] items-center gap-3.5 overflow-hidden rounded-[calc(1.45rem-1.5px)] p-3 pr-5 pb-4" style={{ background: `color-mix(in srgb, var(--ibox-white) 95%, ${look.to})` }}>
        <span aria-hidden className="pointer-events-none absolute -top-14 -left-12 size-44 rounded-full opacity-50 blur-2xl" style={{ background: `radial-gradient(circle, ${look.to}, transparent 70%)` }} />
        <span
          className="relative grid size-[4.4rem] shrink-0 place-items-center overflow-hidden rounded-[1.15rem] shadow-[inset_0_-3px_0_rgba(0,0,0,0.08),inset_0_1px_0_rgba(255,255,255,0.7),0_8px_18px_-10px_rgba(20,30,60,0.5)]"
          style={{ background: `linear-gradient(150deg, ${look.from}, ${look.to})`, color: look.ink }}
        >
          <span aria-hidden className="absolute inset-x-0 top-0 h-1/2 bg-gradient-to-b from-white/45 to-transparent" />
          <motion.span className="relative" initial={{ y: -6, opacity: 0 }} animate={{ y: 0, opacity: 1 }} transition={SPRING.pop}>
            <Icon name={icon} size="2.1rem" filled={icon === "play"} />
          </motion.span>
        </span>
        <span className="relative min-w-0 flex-1">
          <span className="flex items-center gap-1.5 text-[0.66rem] font-extrabold tracking-[0.14em] uppercase" style={{ color: ink }}>
            <span className="inline-flex items-center gap-1 rounded-full px-2 py-0.5" style={{ background: `color-mix(in srgb, ${look.to} ${dark ? 18 : 30}%, transparent)` }}>
              <Icon name={icon} size="1.05em" filled={icon === "play"} />
              iClips
            </span>
          </span>
          <span data-no-i18n className="mt-1 block truncate font-display text-[1.15rem] leading-tight font-semibold text-ink">{t(toast.name)}</span>
          <span className="mt-0.5 line-clamp-2 text-[0.82rem] leading-snug font-semibold text-ink-soft">{t(toast.body)}</span>
        </span>
        <button
          aria-label={t("Fermer")}
          onClick={(e) => { e.stopPropagation(); onDone(); }}
          className="absolute top-2.5 right-2.5 grid size-6 place-items-center rounded-full text-ink-faint opacity-0 transition-opacity group-hover:opacity-100 hover:bg-[var(--ibox-ink)]/8"
        >
          <Icon name="x" size="0.8em" />
        </button>
        <span aria-hidden className="absolute inset-x-5 bottom-2 h-[3px] overflow-hidden rounded-full bg-[var(--ibox-ink)]/8">
          <motion.span
            className="block h-full origin-left rounded-full"
            style={{ background: `linear-gradient(90deg, ${look.to}, ${ink})` }}
            initial={{ scaleX: 1 }}
            animate={{ scaleX: paused ? undefined : 0 }}
            transition={{ duration: duration / 1000, ease: "linear" }}
          />
        </span>
      </div>
    </motion.div>
  );
}
