/**
 * Raccourcis clavier du Studio et du panneau (0.7), et la fiche d'aide (touche ?).
 * Rien ne part pendant la saisie d'un texte ; Alt et Windows sont laissés au système.
 */
import { motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { SPRING } from "./ibox/motion";
import { host, standalone, type CaptureSettings } from "./host";
import { t } from "./i18n";
import { Button, Pill, type IconName, type Tone } from "./ui";

/** Nom d'une touche : `space`, `j`, `ctrl+c`, `Delete`, `?`… */
export function keyName(e: Pick<KeyboardEvent, "key" | "ctrlKey">): string {
  const k = e.key === " " ? "space" : e.key.length === 1 ? e.key.toLowerCase() : e.key;
  return (e.ctrlKey ? "ctrl+" : "") + k;
}

/** Écoute le clavier tant que `enabled` ; le gestionnaire rend vrai s'il a pris la touche. */
export function useKeys(enabled: boolean, handler: (key: string) => boolean | void) {
  const h = useRef(handler);
  h.current = handler;
  useEffect(() => {
    if (!enabled) return;
    const on = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement | null;
      const typing = el?.tagName === "TEXTAREA" || (el instanceof HTMLInputElement && el.type !== "range");
      if (typing || e.altKey || e.metaKey || e.repeat && e.key !== "j" && e.key !== "l") return;
      if (h.current(keyName(e)) === true) {
        e.preventDefault();
        e.stopPropagation();
      }
    };
    window.addEventListener("keydown", on);
    return () => window.removeEventListener("keydown", on);
  }, [enabled]);
}

/** Bulle d'un bouton : son nom et sa touche. */
export const hint = (label: string, key: string) => `${label} (${key})`;

type Group = { title: string; icon: IconName; tone: Tone; items: [string, string][] };

export function groups(overlayKeys: string | null | undefined): Group[] {
  return [
    { title: t("Partout"), icon: "auto", tone: "accent", items: [
      [overlayKeys ?? "Alt+X", standalone ? t("Ouvrir le panneau en jeu") : t("Ouvrir le panneau en jeu (raccourci de l’overlay d’iBox)")],
      [t("Vue + X"), t("Ouvrir le panneau à la manette")],
      [t("Vue + A"), t("Garder le clip tout de suite (Automatique)")],
      [t("« Clip ça ! »"), t("Garder le clip à la voix (Automatique)")],
    ] },
    { title: t("Galerie"), icon: "play", tone: "blue", items: [
      ["1 · 2 · 3 · 4", t("Tous, Clips, Captures, Favoris")],
      ["Ctrl+A", t("Assembler")],
      ["S", t("Sélectionner plusieurs clips (Ctrl+A : tous, Suppr : supprimer)")],
      ["+ · -", t("Taille des vignettes")],
      [t("Ctrl + clic · Maj + clic"), t("Cocher un clip, une plage de clips")],
      ["A", t("Automatique")],
      ["R", t("Réglages")],
      ["?", t("Cette aide")],
    ] },
    { title: t("Lecteur"), icon: "cut", tone: "orange", items: [
      [t("Espace") + " · K", t("Lecture, pause")],
      ["J · L", t("Reculer, avancer de 5 secondes")],
      [", · .", t("Image précédente, suivante")],
      ["- · +", t("Vitesse (ralenti jusqu’à 0,25×)")],
      [t("Entrée"), t("Plein écran")],
      [t("Page préc. · Page suiv."), t("Clip précédent, suivant (LB, RB à la manette)")],
      ["C", t("Couper")],
      ["I · O", t("Début, fin du passage (dans Couper)")],
      ["Z", t("Zoom de la frise (dans Couper)")],
      ["V", t("Vertical")],
      ["S", t("Son")],
      ["P", t("Partager")],
      ["F", t("Favori")],
      ["Ctrl+C", t("Copier le clip, à coller dans Discord")],
      [t("Suppr"), t("Supprimer")],
      [t("Échap"), t("Revenir")],
    ] },
    { title: t("Panneau en jeu"), icon: "camera", tone: "green", items: [
      [t("Entrée"), t("Garder le clip")],
      ["C", t("Garder la capture")],
      [t("Échap"), t("Fermer")],
    ] },
  ];
}

export function Kbd({ children }: { children: string }) {
  return <kbd className="shrink-0 rounded-md bg-white px-2 py-0.5 text-[0.8rem] font-extrabold whitespace-nowrap text-[var(--tone-ink,var(--ibox-ink))] shadow-[inset_0_-2px_0_color-mix(in_srgb,var(--tone,var(--accent))_30%,transparent)]">{children}</kbd>;
}

export function KeysSheet({ onClose }: { onClose: () => void }) {
  const [keys, setKeys] = useState<string | null | undefined>(undefined);
  const back = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    back.current?.focus();
    host<CaptureSettings>("capture.get").then((s) => setKeys(s.overlayKeys)).catch(() => setKeys(null));
  }, []);
  return (
    <motion.div data-nav-scope className="fixed inset-0 z-30 flex items-center justify-center bg-black/60 p-8" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={{ duration: 0.18 }} onClick={(e) => e.target === e.currentTarget && onClose()}>
      <motion.div role="dialog" aria-modal="true" aria-label={t("Raccourcis")} className="flex max-h-full w-full max-w-[54rem] flex-col gap-4 overflow-y-auto rounded-xl bg-white p-6 shadow-lg" initial={{ scale: 0.96, y: 12 }} animate={{ scale: 1, y: 0 }} exit={{ scale: 0.97, y: 8, transition: { duration: 0.15 } }} transition={SPRING.smooth}>
        <header className="flex items-center gap-3">
          <Button ref={back} tone="blue" icon="back" variant="ghost" onClick={onClose} aria-label={t("Retour")} className="px-3" />
          <h2 className="flex-1 font-display text-[1.35rem] leading-tight text-ink">{t("Raccourcis")}</h2>
        </header>
        <div className="grid grid-cols-2 gap-4">
          {groups(keys).map((g, i) => (
            <motion.section key={g.title} initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }} transition={{ ...SPRING.smooth, delay: i * 0.05 }}
              className={`tone-${g.tone} flex flex-col gap-2 rounded-xl border-l-4 border-[var(--tone)] bg-[color-mix(in_srgb,var(--tone)_6%,transparent)] py-3 pr-3 pl-4`}>
              <h3 className="mb-1 flex items-center gap-2.5 text-[0.8rem] font-extrabold tracking-[0.14em] text-[var(--tone-ink)] uppercase"><Pill icon={g.icon} tone={g.tone} size="h-8 w-8" iconSize="1.05em" />{g.title}</h3>
              {g.items.map(([k, what]) => (
                <div key={k + what} className="flex items-center gap-3 text-[0.88rem] font-semibold text-ink-soft">
                  <span className="w-28 shrink-0"><Kbd>{k}</Kbd></span>
                  <span className="min-w-0">{what}</span>
                </div>
              ))}
            </motion.section>
          ))}
        </div>
      </motion.div>
    </motion.div>
  );
}
