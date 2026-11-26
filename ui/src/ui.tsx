/** Icônes et boutons dessinés comme ceux d'iBox (src/components/ui/), sans dépendre d'iBox. */
import type { ComponentProps, SVGProps } from "react";

const PATHS = {
  star: <path d="m12 3.6 2.6 5.3 5.8.8-4.2 4.1 1 5.8L12 16.9l-5.2 2.7 1-5.8-4.2-4.1 5.8-.8z" />,
  back: <path d="M14.5 5.5 8 12l6.5 6.5" />,
  play: <path d="M8 5.5v13l10-6.5z" fill="currentColor" />,
  pause: <path d="M7.5 5.5h3v13h-3zM13.5 5.5h3v13h-3z" fill="currentColor" />,
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
  cut: (
    <>
      <circle cx="6.5" cy="7" r="2.5" />
      <circle cx="6.5" cy="17" r="2.5" />
      <path d="M8.7 8.3 19 16.5M8.7 15.7 19 7.5" />
    </>
  ),
  sound: (
    <>
      <path d="M4.5 9.5h3l4.5-4v13l-4.5-4h-3z" />
      <path d="M15.5 9a4 4 0 0 1 0 6M18 6.5a7.5 7.5 0 0 1 0 11" />
    </>
  ),
  camera: (
    <>
      <path d="M4.5 8.5A1.5 1.5 0 0 1 6 7h2l1.5-2h5L16 7h2a1.5 1.5 0 0 1 1.5 1.5v9A1.5 1.5 0 0 1 18 19H6a1.5 1.5 0 0 1-1.5-1.5z" />
      <circle cx="12" cy="12.8" r="3.3" />
    </>
  ),
  globe: (
    <>
      <circle cx="12" cy="12" r="8" />
      <path d="M4 12h16M12 4a12 12 0 0 1 0 16M12 4a12 12 0 0 0 0 16" />
    </>
  ),
  gear: (
    <>
      <circle cx="12" cy="12" r="3" />
      <path d="M12 3.5v2.2M12 18.3v2.2M3.5 12h2.2M18.3 12h2.2M6 6l1.6 1.6M16.4 16.4 18 18M6 18l1.6-1.6M16.4 7.6 18 6" />
    </>
  ),
  copy: (
    <>
      <rect x="8.5" y="8.5" width="11" height="11" rx="2.2" />
      <path d="M15.5 8.5V6.2a1.7 1.7 0 0 0-1.7-1.7H6.2a1.7 1.7 0 0 0-1.7 1.7v7.6a1.7 1.7 0 0 0 1.7 1.7h2.3" />
    </>
  ),
  link: (
    <>
      <path d="M10.5 13.5a3.5 3.5 0 0 0 5 0l3-3a3.5 3.5 0 0 0-5-5l-1 1" />
      <path d="M13.5 10.5a3.5 3.5 0 0 0-5 0l-3 3a3.5 3.5 0 0 0 5 5l1-1" />
    </>
  ),
  user: (
    <>
      <circle cx="12" cy="8.5" r="3.5" />
      <path d="M5.5 19.5a6.5 6.5 0 0 1 13 0" />
    </>
  ),
  phone: (
    <>
      <rect x="7" y="3.5" width="10" height="17" rx="2.2" />
      <path d="M11 17.5h2" />
    </>
  ),
  download: (
    <>
      <path d="M12 4.5v10M7.5 10.5 12 15l4.5-4.5" />
      <path d="M5 19.5h14" />
    </>
  ),
  folder: <path d="M3.8 7.2A1.7 1.7 0 0 1 5.5 5.5h3.6l2 2h7.4a1.7 1.7 0 0 1 1.7 1.7v8.6a1.7 1.7 0 0 1-1.7 1.7h-13a1.7 1.7 0 0 1-1.7-1.7z" />,
  share: (
    <>
      <path d="M12 15V4.5M8 8.5l4-4 4 4" />
      <path d="M7 11.5H6a1.5 1.5 0 0 0-1.5 1.5v5A1.5 1.5 0 0 0 6 19.5h12a1.5 1.5 0 0 0 1.5-1.5v-5a1.5 1.5 0 0 0-1.5-1.5h-1" />
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
  // Studio 0.9 : lecteur de studio, coupe zoomée, galerie
  up: <path d="M6.5 14.5 12 9l5.5 5.5" />,
  down: <path d="M6.5 9.5 12 15l5.5-5.5" />,
  expand: <path d="M4.5 9V4.5H9M15 4.5h4.5V9M19.5 15v4.5H15M9 19.5H4.5V15" />,
  shrink: <path d="M9 4.5V9H4.5M19.5 9H15V4.5M15 19.5V15h4.5M4.5 15H9v4.5" />,
  zoom: (
    <>
      <circle cx="10.5" cy="10.5" r="6" />
      <path d="M15 15l4.5 4.5M8 10.5h5M10.5 8v5" />
    </>
  ),
  unzoom: (
    <>
      <circle cx="10.5" cy="10.5" r="6" />
      <path d="M15 15l4.5 4.5M8 10.5h5" />
    </>
  ),
  grid: (
    <>
      <rect x="4.5" y="4.5" width="6" height="6" rx="1.4" />
      <rect x="13.5" y="4.5" width="6" height="6" rx="1.4" />
      <rect x="4.5" y="13.5" width="6" height="6" rx="1.4" />
      <rect x="13.5" y="13.5" width="6" height="6" rx="1.4" />
    </>
  ),
  stepBack: <path d="M16.5 6v12M13 6.5 7 12l6 5.5" />,
  stepFwd: <path d="M7.5 6v12M11 6.5l6 5.5-6 5.5" />,
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
/** Teintes du code couleur (styles.css) ; `accent` : le violet d'iClips. */
export type Tone = "accent" | "blue" | "green" | "yellow" | "orange" | "red" | "pink" | "purple" | "teal";
const VARIANTS: Record<Variant, string> = {
  primary: "ibox-btn-primary",
  soft: "ibox-btn-soft",
  ghost: "ibox-btn-ghost",
  danger: "ibox-btn-primary",
};

export function Button({ variant = "soft", tone, icon, filled, className = "", children, ...rest }: { variant?: Variant; tone?: Tone; icon?: IconName; filled?: boolean } & ComponentProps<"button">) {
  const t = tone ?? (variant === "danger" ? "red" : "accent");
  return (
    <button
      type="button"
      data-nav
      className={`ibox-btn tone-${t} inline-flex h-11 shrink-0 items-center justify-center gap-2 rounded-full px-5 text-[0.95rem] font-bold whitespace-nowrap disabled:opacity-55 ${VARIANTS[variant]} ${className}`}
      {...rest}
    >
      {icon && <Icon name={icon} filled={filled} />}
      {children}
    </button>
  );
}

/** Pastille d'icône teintée. */
export function Pill({ icon, tone, size = "h-11 w-11", iconSize = "1.35em" }: { icon: IconName; tone: Tone; size?: string; iconSize?: string }) {
  return (
    <span className={`ibox-pill tone-${tone} grid ${size} shrink-0 place-items-center rounded-full`}>
      <Icon name={icon} size={iconSize} />
    </span>
  );
}
