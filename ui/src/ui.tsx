/** Icônes et boutons dessinés comme ceux d'iBox (src/components/ui/), sans dépendre d'iBox. */
import type { ComponentProps, SVGProps } from "react";

const PATHS = {
  star: <path d="m12 3.6 2.6 5.3 5.8.8-4.2 4.1 1 5.8L12 16.9l-5.2 2.7 1-5.8-4.2-4.1 5.8-.8z" />,
  back: <path d="M14.5 5.5 8 12l6.5 6.5" />,
  play: <path d="M8 5.5v13l10-6.5z" fill="currentColor" />,
  x: <path d="M6.5 6.5l11 11M17.5 6.5l-11 11" />,
  check: <path d="M5 12.5l4.5 4.5L19 7.5" />,
  mic: (
    <>
      <rect x="9" y="3.5" width="6" height="11" rx="3" />
      <path d="M6 11.5a6 6 0 0 0 12 0M12 17.5v3" />
    </>
  ),
  pad: (
    <>
      <path d="M7.5 8h9a4.5 4.5 0 0 1 4.4 5.4l-.8 3.6a2.3 2.3 0 0 1-4 .9L14.5 16h-5l-1.6 1.9a2.3 2.3 0 0 1-4-.9l-.8-3.6A4.5 4.5 0 0 1 7.5 8z" />
      <path d="M8.5 10.5v3M7 12h3" />
    </>
  ),
  flag: (
    <>
      <path d="M6 20.5V4" />
      <path d="M6 4.5h10.5l-2 3.5 2 3.5H6" />
    </>
  ),
  auto: (
    <>
      <path d="M12 3.5v3M12 17.5v3M3.5 12h3M17.5 12h3" />
      <circle cx="12" cy="12" r="4" />
    </>
  ),
  trash: (
    <>
      <path d="M4.5 7h15" />
      <path d="M9.5 7V5.2A1.2 1.2 0 0 1 10.7 4h2.6a1.2 1.2 0 0 1 1.2 1.2V7" />
      <path d="M6.5 7l.9 11.6A1.6 1.6 0 0 0 9 20h6a1.6 1.6 0 0 0 1.6-1.4L17.5 7" />
      <path d="M10.2 11v5M13.8 11v5" />
    </>
  ),
};
export type IconName = keyof typeof PATHS;

export function Icon({ name, size = "1.25em", filled, ...rest }: { name: IconName; size?: string | number; filled?: boolean } & SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill={filled ? "currentColor" : "none"} stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" {...rest}>
      {PATHS[name]}
    </svg>
  );
}

type Variant = "primary" | "soft" | "ghost" | "danger";
const VARIANTS: Record<Variant, string> = {
  primary: "bg-accent text-on-accent shadow-sm hover:bg-accent-deep",
  soft: "bg-white text-ink shadow-sm hover:bg-accent-soft",
  ghost: "bg-transparent text-ink-soft hover:bg-accent-soft hover:text-ink",
  danger: "bg-peach-deep text-on-accent shadow-sm hover:brightness-110",
};

export function Button({ variant = "soft", icon, filled, className = "", children, ...rest }: { variant?: Variant; icon?: IconName; filled?: boolean } & ComponentProps<"button">) {
  return (
    <button
      type="button"
      data-nav
      className={`inline-flex h-11 shrink-0 items-center justify-center gap-2 rounded-full px-5 text-[0.95rem] font-bold whitespace-nowrap transition active:scale-[0.97] disabled:opacity-55 ${VARIANTS[variant]} ${className}`}
      {...rest}
    >
      {icon && <Icon name={icon} filled={filled} />}
      {children}
    </button>
  );
}
