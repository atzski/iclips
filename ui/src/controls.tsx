/** Interrupteur et glissière de volume, utilisables à la souris, au clavier et à la manette. */
import { motion } from "motion/react";
import { useEffect, useRef } from "react";
import { SPRING } from "./ibox/motion";

export function Switch({ label, on, onChange, disabled }: { label: string; on: boolean; onChange: (v: boolean) => void; disabled?: boolean }) {
  return (
    <button type="button" role="switch" data-nav aria-checked={on} aria-label={label} disabled={disabled} onClick={() => onChange(!on)}
      className={`relative h-8 w-14 shrink-0 rounded-full transition disabled:opacity-55 ${on ? "bg-accent" : "bg-line"}`}>
      <motion.span className="absolute top-1 left-1 h-6 w-6 rounded-full bg-white shadow" animate={{ x: on ? 24 : 0 }} transition={SPRING.snappy} />
    </button>
  );
}

/** Volume de 0 à 200 % (1 : tel quel). Gauche et droite à la manette : par 5 %. */
export function Slider({ label, value, onChange }: { label: string; value: number; onChange: (v: number) => void }) {
  const ref = useRef<HTMLInputElement>(null);
  const current = useRef(value);
  current.current = value;
  const change = useRef(onChange);
  change.current = onChange;
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const onAdjust = (e: Event) => change.current(Math.min(2, Math.max(0, Math.round((current.current + ((e as CustomEvent<string>).detail === "left" ? -0.05 : 0.05)) * 100) / 100)));
    el.addEventListener("nav-adjust", onAdjust);
    return () => el.removeEventListener("nav-adjust", onAdjust);
  }, []);
  const pct = Math.round(value * 100);
  return (
    <div className="flex items-center gap-3">
      <input
        ref={ref}
        type="range"
        data-nav
        data-adjust
        min={0}
        max={200}
        step={5}
        value={pct}
        aria-label={label}
        aria-valuetext={`${pct} %`}
        onChange={(e) => onChange(Number(e.target.value) / 100)}
        className="h-2 min-w-0 flex-1 cursor-pointer accent-[var(--color-accent)]"
      />
      <span className="w-14 text-right text-[0.88rem] font-extrabold text-ink tabular-nums">{pct} %</span>
    </div>
  );
}
