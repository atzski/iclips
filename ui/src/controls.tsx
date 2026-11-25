/** Interrupteur, glissière de volume et petit égaliseur : souris, clavier et manette. */
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { SPRING } from "./ibox/motion";

export function Switch({ label, on, onChange, disabled }: { label: string; on: boolean; onChange: (v: boolean) => void; disabled?: boolean }) {
  return (
    <button type="button" role="switch" data-nav aria-checked={on} aria-label={label} disabled={disabled} onClick={() => onChange(!on)}
      className={`relative h-8 w-14 shrink-0 rounded-full transition-colors duration-200 disabled:opacity-55 ${on ? "bg-[var(--tone,var(--accent))] shadow-[0_4px_14px_-6px_var(--tone,var(--accent))]" : "bg-line"}`}>
      <motion.span className="absolute top-1 left-1 h-6 w-6 rounded-full bg-white shadow" animate={{ x: on ? 24 : 0 }} transition={SPRING.snappy} />
    </button>
  );
}

/** Ressort du remplissage : collé au doigt pendant le glissement, souple quand la valeur saute. */
const HELD = { type: "spring", stiffness: 900, damping: 55, mass: 0.5 } as const;

/**
 * Volume de 0 à 200 % (1 : tel quel, repère au milieu). La piste se remplit sur un ressort, le
 * bouton grossit sous le doigt et une bulle donne la valeur. Gauche et droite au clavier ou à la
 * manette : par 5 %.
 */
export function Slider({ label, value, onChange }: { label: string; value: number; onChange: (v: number) => void }) {
  const track = useRef<HTMLDivElement>(null);
  const current = useRef(value);
  current.current = value;
  const change = useRef(onChange);
  change.current = onChange;
  const [held, setHeld] = useState(false);
  const [near, setNear] = useState(false);
  const [focus, setFocus] = useState(false);

  useEffect(() => {
    const el = track.current;
    if (!el) return;
    const onAdjust = (e: Event) => change.current(Math.min(2, Math.max(0, Math.round((current.current + ((e as CustomEvent<string>).detail === "left" ? -0.05 : 0.05)) * 100) / 100)));
    el.addEventListener("nav-adjust", onAdjust);
    return () => el.removeEventListener("nav-adjust", onAdjust);
  }, []);

  const at = (x: number) => {
    const r = track.current?.getBoundingClientRect();
    if (!r || r.width <= 0) return current.current;
    return Math.round(Math.min(1, Math.max(0, (x - r.left) / r.width)) * 40) / 20;
  };
  const pct = Math.round(value * 100);
  const fill = `${Math.min(100, (value / 2) * 100)}%`;
  const muted = value === 0;

  return (
    <div className="flex items-center gap-3">
      <div
        ref={track}
        role="slider"
        tabIndex={0}
        data-nav
        data-adjust
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={200}
        aria-valuenow={pct}
        aria-valuetext={`${pct} %`}
        onPointerDown={(e) => { e.preventDefault(); e.currentTarget.setPointerCapture(e.pointerId); e.currentTarget.focus(); setHeld(true); onChange(at(e.clientX)); }}
        onPointerMove={(e) => { if (held) onChange(at(e.clientX)); }}
        onPointerUp={() => setHeld(false)}
        onPointerCancel={() => setHeld(false)}
        onPointerEnter={() => setNear(true)}
        onPointerLeave={() => setNear(false)}
        onFocus={() => setFocus(true)}
        onBlur={() => setFocus(false)}
        className="group relative h-8 min-w-0 flex-1 cursor-pointer touch-none rounded-full outline-none"
      >
        {/* piste, remplissage en dégradé, repère « tel quel » au milieu */}
        <motion.div className="absolute inset-x-0 top-1/2 -translate-y-1/2 overflow-hidden rounded-full bg-[color-mix(in_srgb,var(--tone,var(--accent))_16%,var(--ibox-cream-deep))]"
          animate={{ height: held || near ? 10 : 8 }} transition={SPRING.snappy}>
          <motion.div className="h-full rounded-full bg-[linear-gradient(90deg,color-mix(in_srgb,var(--tone,var(--accent))_55%,#fff),var(--tone,var(--accent)))]"
            initial={false} animate={{ width: fill, opacity: muted ? 0.4 : 1 }} transition={held ? HELD : SPRING.smooth} />
        </motion.div>
        <span className="pointer-events-none absolute top-1/2 left-1/2 h-3.5 w-0.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-[var(--ibox-ink)]/25" aria-hidden="true" />
        {/* bouton rond : il grossit sous le doigt, une bulle donne la valeur */}
        <motion.div className="pointer-events-none absolute top-1/2 h-5 w-5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-[#fff]"
          initial={false}
          animate={{ left: fill, scale: held ? 1.3 : near || focus ? 1.12 : 1, boxShadow: held ? "0 0 0 4px var(--tone, var(--accent)), 0 8px 20px -4px var(--tone, var(--accent))" : "0 0 0 3px var(--tone, var(--accent)), 0 4px 10px -4px var(--tone, var(--accent))" }}
          transition={{ left: held ? HELD : SPRING.smooth, scale: SPRING.pop, boxShadow: { duration: 0.2 } }}>
          <AnimatePresence>
            {(held || focus) && (
              <motion.span initial={{ opacity: 0, y: 6, scale: 0.6 }} animate={{ opacity: 1, y: 0, scale: 0.77 }} exit={{ opacity: 0, y: 4, scale: 0.6 }} transition={SPRING.pop}
                className="absolute bottom-full left-1/2 mb-2 -translate-x-1/2 rounded-full bg-[var(--tone,var(--accent))] px-2.5 py-1 text-[0.95rem] font-extrabold whitespace-nowrap text-[#fff] shadow-md">
                {pct} %
              </motion.span>
            )}
          </AnimatePresence>
        </motion.div>
      </div>
      <motion.span key={muted ? "muet" : "son"} initial={{ scale: 1.2 }} animate={{ scale: 1 }} transition={SPRING.pop}
        className={`w-14 text-right text-[0.88rem] font-extrabold tabular-nums ${muted ? "text-ink-faint" : "text-ink"}`}>{pct} %</motion.span>
    </div>
  );
}

/** Petit égaliseur qui danse au volume donné (0 : à plat). */
export function Bars({ level, active = true }: { level: number; active?: boolean }) {
  const amp = Math.min(1, level / 2);
  return (
    <span className="flex h-5 items-end gap-[3px]" aria-hidden="true">
      {[0.55, 1, 0.75, 0.9].map((h, i) => (
        <motion.span key={i} className="w-[3px] rounded-full bg-[var(--tone,var(--accent))]"
          animate={active && amp > 0 ? { height: [`${20 + h * amp * 30}%`, `${30 + h * amp * 70}%`, `${15 + h * amp * 40}%`] } : { height: "15%" }}
          transition={active && amp > 0 ? { duration: 0.9 - i * 0.08, repeat: Infinity, repeatType: "mirror", ease: "easeInOut", delay: i * 0.12 } : SPRING.smooth} />
      ))}
    </span>
  );
}

export interface Device { id: string; name: string; default: boolean }

/**
 * Choix d'un appareil de son : le choisi (ou « Par défaut de Windows ») ; un clic déplie la
 * liste des appareils branchés, sur un ressort. `null` : celui par défaut.
 */
export function DevicePicker({ label, devices, value, onChange, defaultLabel, missingLabel }: {
  label: string; devices: Device[]; value: string | null | undefined; onChange: (id: string | null) => void;
  defaultLabel: string; missingLabel: string;
}) {
  const [open, setOpen] = useState(false);
  const fallback = devices.find((d) => d.default);
  const chosen = value ? devices.find((d) => d.id === value) : null;
  const shown = value ? chosen?.name ?? missingLabel : defaultLabel;
  const options: { id: string | null; name: string; sub?: string }[] = [
    { id: null, name: defaultLabel, sub: fallback?.name },
    ...devices.map((d) => ({ id: d.id, name: d.name })),
  ];
  const pick = (id: string | null) => { onChange(id); setOpen(false); };
  return (
    <div className="flex flex-col gap-1.5">
      <button type="button" data-nav aria-expanded={open} aria-label={label} onClick={() => setOpen((o) => !o)}
        className="ibox-btn-soft flex h-11 w-full items-center gap-3 rounded-lg px-4 text-left text-[0.9rem] font-bold transition-[background-color,transform] duration-200 active:scale-[0.98]">
        <span className="min-w-0 flex-1 truncate">
          {shown}
          {!value && fallback && <span className="font-semibold text-ink-soft"> · {fallback.name}</span>}
        </span>
        <motion.svg viewBox="0 0 24 24" width="1.1em" height="1.1em" fill="none" stroke="currentColor" strokeWidth={2.2} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"
          animate={{ rotate: open ? 180 : 0 }} transition={SPRING.snappy} className="shrink-0 text-[var(--tone-ink,var(--accent-text))]">
          <path d="M6.5 9.5 12 15l5.5-5.5" />
        </motion.svg>
      </button>
      <AnimatePresence initial={false}>
        {open && (
          <motion.ul role="listbox" aria-label={label} className="flex flex-col gap-1 overflow-hidden"
            initial={{ height: 0, opacity: 0 }} animate={{ height: "auto", opacity: 1 }} exit={{ height: 0, opacity: 0, transition: { duration: 0.16 } }} transition={{ height: SPRING.smooth, opacity: { duration: 0.18 } }}>
            {options.map((o, i) => {
              const selected = (value ?? null) === o.id;
              return (
                <motion.li key={o.id ?? "*"} initial={{ opacity: 0, x: -10 }} animate={{ opacity: 1, x: 0 }} transition={{ ...SPRING.smooth, delay: i * 0.03 }}>
                  <button type="button" role="option" data-nav aria-selected={selected} onClick={() => pick(o.id)}
                    className={`flex w-full items-center gap-3 rounded-lg px-4 py-2 text-left text-[0.88rem] font-bold transition-colors ${selected ? "bg-[color-mix(in_srgb,var(--tone,var(--accent))_22%,var(--ibox-white))] text-ink" : "text-ink-soft hover:bg-[color-mix(in_srgb,var(--tone,var(--accent))_10%,var(--ibox-white))] hover:text-ink"}`}>
                    <span className={`grid h-5 w-5 shrink-0 place-items-center rounded-full border-2 ${selected ? "border-[var(--tone,var(--accent))] bg-[var(--tone,var(--accent))]" : "border-line"}`}>
                      <AnimatePresence>{selected && <motion.span className="h-2 w-2 rounded-full bg-[#fff]" initial={{ scale: 0 }} animate={{ scale: 1 }} exit={{ scale: 0 }} transition={SPRING.pop} />}</AnimatePresence>
                    </span>
                    <span className="min-w-0 flex-1 truncate">
                      {o.name}
                      {o.sub && <span className="font-semibold text-ink-faint"> · {o.sub}</span>}
                    </span>
                  </button>
                </motion.li>
              );
            })}
          </motion.ul>
        )}
      </AnimatePresence>
    </div>
  );
}
