/**
 * Menu d'un clip dans la galerie du Studio (0.7.1) : clic droit sur une carte, touche Menu ou
 * Maj+F10 au clavier, Y ou Menu à la manette. « Modifier le clip » ouvre le montage ;
 * « Sélectionner plusieurs clips » ouvre la sélection ; « Supprimer » demande confirmation dans le
 * menu même (Annuler revient à la liste).
 * Flèches, Entrée et Échap au clavier ; croix, A et B à la manette (nav.ts).
 */
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { SPRING } from "./ibox/motion";
import type { Clip } from "./host";
import { t } from "./i18n";
import { Button, Icon, Pill, type IconName, type Tone } from "./ui";

const isImage = (c: Clip) => c.kind === "gif" || c.kind === "image";
const MARGIN = 12;

/** Place du menu : au pointeur, retourné vers la gauche ou le haut s'il déborde de la fenêtre. */
export function place(at: { x: number; y: number }, size: { w: number; h: number }, view: { w: number; h: number }) {
  const left = at.x + size.w + MARGIN > view.w ? Math.max(MARGIN, at.x - size.w) : at.x;
  const top = at.y + size.h + MARGIN > view.h ? Math.max(MARGIN, at.y - size.h) : at.y;
  return { left, top, origin: `${left < at.x ? "right" : "left"} ${top < at.y ? "bottom" : "top"}` };
}

function deleteLabel(c: Clip) {
  return c.kind === "image" ? t("Supprimer la capture") : c.kind === "gif" ? t("Supprimer le GIF") : t("Supprimer le clip");
}

function deleteQuestion(c: Clip) {
  return c.kind === "image" ? t("Supprimer cette capture ? Elle est effacée de ce PC.") : c.kind === "gif" ? t("Supprimer ce GIF ? Il est effacé de ce PC.") : t("Supprimer ce clip ? Il est effacé de ce PC.");
}

export function ClipMenu({ clip: c, at, onEdit, onOpen, onDelete, onSelect, onClose, onFavorite, onCopy, onShare }: {
  clip: Clip; at: { x: number; y: number }; onEdit: () => void; onOpen: () => void; onDelete: () => void; onSelect: () => void; onClose: () => void;
  /** Studio 0.9 : les actions rapides des cartes, aussi au clavier et à la manette. */
  onFavorite?: () => void; onCopy?: () => void; onShare?: () => void;
}) {
  const box = useRef<HTMLDivElement>(null);
  const [confirm, setConfirm] = useState(false);
  const [pos, setPos] = useState<{ left: number; top: number; origin: string } | null>(null);

  useLayoutEffect(() => {
    const r = box.current?.getBoundingClientRect();
    if (r) setPos(place(at, { w: r.width, h: r.height }, { w: window.innerWidth, h: window.innerHeight }));
  }, [at]);
  // le premier choix (ou « Annuler » quand on confirme) prend le focus
  useEffect(() => {
    const id = requestAnimationFrame(() => box.current?.querySelector<HTMLElement>(confirm ? "[data-cancel]" : "[role=menuitem]")?.focus());
    return () => cancelAnimationFrame(id);
  }, [confirm]);
  // la fenêtre perd le focus ou change de taille : le menu se ferme
  useEffect(() => {
    window.addEventListener("blur", onClose);
    window.addEventListener("resize", onClose);
    return () => { window.removeEventListener("blur", onClose); window.removeEventListener("resize", onClose); };
  }, [onClose]);

  const onKey = (e: React.KeyboardEvent) => {
    // Tab reste dans le menu
    if (e.key === "Tab") {
      const items = [...(box.current?.querySelectorAll<HTMLElement>("[data-nav]") ?? [])];
      const i = items.indexOf(document.activeElement as HTMLElement);
      e.preventDefault();
      items[(i + (e.shiftKey ? -1 : 1) + items.length) % items.length]?.focus();
    } else if (e.key === "Delete" && !confirm) {
      e.preventDefault();
      setConfirm(true);
    }
  };

  const image = isImage(c);
  return (
    <div className="fixed inset-0 z-30" onPointerDown={(e) => e.target === e.currentTarget && onClose()} onContextMenu={(e) => { e.preventDefault(); if (e.target === e.currentTarget) onClose(); }}>
      <motion.div
        ref={box}
        data-nav-scope
        role={confirm ? "alertdialog" : "menu"}
        aria-label={confirm ? deleteLabel(c) : t("Actions : {0}", c.title ?? c.game ?? t("Hors jeu"))}
        onKeyDown={onKey}
        style={{ left: pos?.left ?? at.x, top: pos?.top ?? at.y, transformOrigin: pos?.origin ?? "left top", visibility: pos ? "visible" : "hidden" }}
        className="absolute w-[17.5rem] overflow-hidden rounded-xl bg-[color-mix(in_srgb,var(--ibox-white)_90%,#fff)] p-1.5 shadow-[0_18px_48px_-12px_rgba(0,0,0,0.45),0_0_0_1px_var(--ibox-line)]"
        initial={{ opacity: 0, scale: 0.9, y: -4 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        exit={{ opacity: 0, scale: 0.95, transition: { duration: 0.12 } }}
        transition={SPRING.pop}
        layout
      >
        <AnimatePresence mode="popLayout" initial={false}>
          {confirm ? (
            <motion.div key="confirm" className="flex flex-col gap-3 p-2.5" initial={{ opacity: 0, x: 16 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: 16 }} transition={SPRING.smooth}>
              <div className="flex items-start gap-3">
                <Pill icon="trash" tone="red" size="h-10 w-10" iconSize="1.15em" />
                <p className="min-w-0 flex-1 text-[0.9rem] leading-snug font-bold text-ink">{deleteQuestion(c)}</p>
              </div>
              <div className="flex gap-2">
                <Button variant="danger" tone="red" icon="trash" className="flex-1" onClick={onDelete}>{t("Supprimer")}</Button>
                <Button data-cancel tone="blue" variant="ghost" className="flex-1" onClick={() => setConfirm(false)}>{t("Annuler")}</Button>
              </div>
            </motion.div>
          ) : (
            <motion.ul key="list" className="flex flex-col gap-0.5" initial={{ opacity: 0, x: -16 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -16 }} transition={SPRING.smooth}>
              <li className="truncate px-3 pt-1.5 pb-1 text-[0.72rem] font-extrabold tracking-[0.12em] text-ink-faint uppercase">{c.title ?? c.game ?? t("Hors jeu")}</li>
              {image
                ? <Item icon="play" tone="accent" label={t("Ouvrir")} index={0} onClick={onOpen} />
                : <Item icon="cut" tone="orange" label={t("Modifier le clip")} index={0} onClick={onEdit} />}
              {onFavorite && <Item icon="star" tone="yellow" label={c.favorite ? t("Retirer des favoris") : t("Ajouter aux favoris")} index={1} onClick={onFavorite} />}
              {onCopy && <Item icon="copy" tone="blue" label={t("Copier le clip")} index={2} onClick={onCopy} />}
              {onShare && <Item icon="share" tone="green" label={t("Partager")} index={3} onClick={onShare} />}
              <Item icon="check" tone="blue" label={t("Sélectionner plusieurs clips")} hint="S" index={4} onClick={onSelect} />
              <Item icon="trash" tone="red" label={deleteLabel(c)} hint={t("Suppr")} index={5} onClick={() => setConfirm(true)} />
            </motion.ul>
          )}
        </AnimatePresence>
      </motion.div>
    </div>
  );
}

function Item({ icon, tone, label, hint, index, onClick }: { icon: IconName; tone: Tone; label: string; hint?: string; index: number; onClick: () => void }) {
  return (
    <motion.li initial={{ opacity: 0, x: -8 }} animate={{ opacity: 1, x: 0 }} transition={{ ...SPRING.smooth, delay: 0.03 + index * 0.035 }}>
      <button type="button" role="menuitem" data-nav onClick={onClick}
        className={`tone-${tone} group flex w-full items-center gap-3 rounded-lg px-2.5 py-2 text-left text-[0.92rem] font-bold text-ink outline-none transition-colors duration-150 hover:bg-[color-mix(in_srgb,var(--tone)_14%,var(--ibox-white))] focus-visible:bg-[color-mix(in_srgb,var(--tone)_14%,var(--ibox-white))]`}>
        <span className="grid h-8 w-8 shrink-0 place-items-center rounded-full bg-[color-mix(in_srgb,var(--tone)_18%,var(--ibox-white))] text-[var(--tone-ink)] transition-transform duration-200 ease-[cubic-bezier(0.34,1.56,0.64,1)] group-hover:scale-110 group-focus-visible:scale-110">
          <Icon name={icon} size="1.05em" />
        </span>
        <span className="min-w-0 flex-1 truncate">{label}</span>
        {hint && <span className="shrink-0 rounded-md bg-cream-deep px-1.5 py-0.5 text-[0.72rem] font-extrabold text-ink-faint">{hint}</span>}
      </button>
    </motion.li>
  );
}
