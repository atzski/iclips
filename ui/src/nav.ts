/**
 * Navigation au clavier et à la manette, comme dans iBox : flèches ou croix et stick gauche
 * pour aller d'un élément `[data-nav]` à son voisin, Entrée ou A pour valider, Échap ou B
 * pour revenir. La fenêtre du Studio a le focus : la Gamepad API du navigateur suffit.
 * Y ou Menu (0.7.1) ouvrent le menu de l'élément choisi, comme un clic droit.
 */
import { useEffect, useRef } from "react";

export type Dir = "up" | "down" | "left" | "right";
export interface Box { x: number; y: number; w: number; h: number }

/** Voisin le plus proche dans une direction (centre à centre, l'écart de côté compte double). */
export function pick(from: Box, boxes: Box[], dir: Dir): number {
  const cx = from.x + from.w / 2, cy = from.y + from.h / 2;
  let best = -1, score = Infinity;
  boxes.forEach((b, i) => {
    const bx = b.x + b.w / 2, by = b.y + b.h / 2;
    const dx = bx - cx, dy = by - cy;
    const along = dir === "left" ? -dx : dir === "right" ? dx : dir === "up" ? -dy : dy;
    const side = dir === "left" || dir === "right" ? Math.abs(dy) : Math.abs(dx);
    if (along <= 1) return;
    const s = along + side * 2;
    if (s < score) { score = s; best = i; }
  });
  return best;
}

function targets(): HTMLElement[] {
  // une modale ouverte garde la navigation pour elle
  const scope = [...document.querySelectorAll<HTMLElement>("[data-nav-scope]")].pop() ?? document.body;
  return [...scope.querySelectorAll<HTMLElement>("[data-nav]")].filter((el) => !el.hasAttribute("disabled") && el.offsetParent !== null);
}

export function move(dir: Dir): void {
  const list = targets();
  if (!list.length) return;
  const current = document.activeElement as HTMLElement | null;
  if (!current || !list.includes(current)) { list[0].focus(); return; }
  // réglage (poignée de la coupe) : gauche et droite le changent au lieu de quitter l'élément
  if (current.hasAttribute("data-adjust") && (dir === "left" || dir === "right")) {
    current.dispatchEvent(new CustomEvent("nav-adjust", { detail: dir }));
    return;
  }
  const box = (el: HTMLElement): Box => { const r = el.getBoundingClientRect(); return { x: r.left, y: r.top, w: r.width, h: r.height }; };
  const others = list.filter((el) => el !== current);
  const i = pick(box(current), others.map(box), dir);
  if (i >= 0) { others[i].focus(); others[i].scrollIntoView({ block: "nearest", behavior: "smooth" }); }
}

/** Menu de l'élément choisi (manette : Y ou Menu) : un clic droit au centre de l'élément. */
export function openMenu(el: Element | null = document.activeElement): void {
  if (!(el instanceof HTMLElement) || !el.hasAttribute("data-nav")) return;
  const r = el.getBoundingClientRect();
  el.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, button: 0, clientX: r.left + r.width / 2, clientY: r.top + r.height / 2 }));
}

const KEYS: Record<string, Dir> = { ArrowUp: "up", ArrowDown: "down", ArrowLeft: "left", ArrowRight: "right" };

/** Panneau en jeu : la manette arrive du socle (XInput natif, événement `pad`). */
export function padAction(action: string, onBack: () => void): void {
  if (action === "up" || action === "down" || action === "left" || action === "right") move(action);
  else if (action === "ok") (document.activeElement as HTMLElement | null)?.click();
  else if (action === "back") onBack();
}

/** Clavier et manette. `onBack` : Échap ou B. */
export function useNav(onBack: () => void): void {
  const back = useRef(onBack);
  back.current = onBack;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = e.target instanceof HTMLInputElement;
      if (KEYS[e.key] && !(typing && (e.key === "ArrowLeft" || e.key === "ArrowRight"))) { e.preventDefault(); move(KEYS[e.key]); }
      else if (e.key === "Escape") { e.preventDefault(); back.current(); }
    };
    window.addEventListener("keydown", onKey);
    // manette : croix, stick gauche (répétition après 380 ms, puis toutes les 120 ms), A, B, Y et Menu
    let raf = 0, held: string | null = null, next = 0;
    const pressed = new Set<string>();
    const loop = (now: number) => {
      const pad = [...(navigator.getGamepads?.() ?? [])].find((p) => p?.connected);
      if (pad) {
        const b = (i: number) => !!pad.buttons[i]?.pressed;
        const [ax, ay] = [pad.axes[0] ?? 0, pad.axes[1] ?? 0];
        const dir: Dir | null = b(12) || ay < -0.6 ? "up" : b(13) || ay > 0.6 ? "down" : b(14) || ax < -0.6 ? "left" : b(15) || ax > 0.6 ? "right" : null;
        if (dir !== held) { held = dir; if (dir) { move(dir); next = now + 380; } }
        else if (dir && now >= next) { move(dir); next = now + 120; }
        for (const [i, act] of [[0, "a"], [1, "b"], [3, "y"], [9, "menu"]] as const) {
          if (b(i) && !pressed.has(act)) {
            pressed.add(act);
            if (act === "a") (document.activeElement as HTMLElement | null)?.click();
            else if (act === "b") back.current();
            else openMenu();
          } else if (!b(i)) pressed.delete(act);
        }
      }
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => { window.removeEventListener("keydown", onKey); cancelAnimationFrame(raf); };
  }, []);
}
