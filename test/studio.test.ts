import { describe, expect, it, vi } from "vitest";

// ces tests lisent les textes en français : page ouverte par une iBox en français
vi.hoisted(() => {
  (globalThis as { location?: { search: string } }).location = { search: "?lang=fr" };
});

import { duration, when } from "../ui/src/format";
import { pick, type Box } from "../ui/src/nav";

describe("navigation", () => {
  // grille 3 × 2
  const grid: Box[] = [0, 1, 2].flatMap((col) => [0, 1].map((row) => ({ x: col * 110, y: row * 70, w: 100, h: 60 })));
  const at = (col: number, row: number) => grid.findIndex((b) => b.x === col * 110 && b.y === row * 70);

  it("va au voisin direct", () => {
    const from = grid[at(1, 0)];
    const others = grid.filter((b) => b !== from);
    expect(others[pick(from, others, "right")]).toBe(grid[at(2, 0)]);
    expect(others[pick(from, others, "left")]).toBe(grid[at(0, 0)]);
    expect(others[pick(from, others, "down")]).toBe(grid[at(1, 1)]);
  });

  it("ne sort pas de la grille", () => {
    const from = grid[at(0, 0)];
    const others = grid.filter((b) => b !== from);
    expect(pick(from, others, "up")).toBe(-1);
    expect(pick(from, others, "left")).toBe(-1);
  });
});

describe("formats", () => {
  it("durée", () => {
    expect(duration(7.4)).toBe("0:07");
    expect(duration(65)).toBe("1:05");
    expect(duration(Number.NaN)).toBe("");
  });

  it("date relative", () => {
    const now = new Date(2026, 10, 25, 15, 0).getTime();
    expect(when(new Date(2026, 10, 25, 9, 5).getTime(), now)).toMatch(/^Aujourd’hui, 09:05$/);
    expect(when(new Date(2026, 10, 24, 22, 0).getTime(), now)).toMatch(/^Hier, 22:00$/);
  });
});

describe("vignettes", () => {
  it("recadre au centre sans déformer", async () => {
    const { cover } = await import("../ui/src/thumbs");
    // 4:3 → 16:9 : la hauteur est rognée
    expect(cover(1440, 1080)).toEqual({ sx: 0, sy: 135, sw: 1440, sh: 810 });
    // déjà en 16:9 : tout l'image
    expect(cover(1920, 1080)).toEqual({ sx: 0, sy: 0, sw: 1920, sh: 1080 });
  });
});

describe("montage", () => {
  it("borne le passage au clip", async () => {
    const { clampSpan } = await import("../ui/src/edit");
    expect(clampSpan({ start: -2, end: 50 }, 12)).toEqual({ start: 0, end: 12 });
    expect(clampSpan({ start: 11.9, end: 3 }, 12)).toEqual({ start: 11.5, end: 12 });
    expect(clampSpan({ start: 4, end: 4.1 }, 12)).toEqual({ start: 4, end: 4.5 });
  });

  it("assemble des clips entiers, durée inconnue comprise", async () => {
    const { joinSegments } = await import("../ui/src/edit");
    expect(joinSegments([{ name: "a.mp4", duration: 12.4 }, { name: "b.mp4" }])).toEqual([
      { name: "a.mp4", start: 0, end: 12.4 },
      { name: "b.mp4", start: 0, end: 35 },
    ]);
  });
});

describe("clips automatiques", () => {
  it("dit la phrase dans la langue d'écoute", async () => {
    const { phrase } = await import("../ui/src/Auto");
    expect(phrase("fr-FR")).toBe("« Clip ça ! »");
    expect(phrase("en-US")).toBe("“Clip that!”");
    expect(phrase(null)).toBe("« Clip ça ! »");
  });
});

describe("poignées de la coupe", () => {
  it("bougent un bout sans croiser l'autre", async () => {
    const { setEdge } = await import("../ui/src/edit");
    expect(setEdge({ start: 2, end: 6 }, "start", -3, 12)).toEqual({ start: 0, end: 6 });
    expect(setEdge({ start: 2, end: 6 }, "start", 9, 12)).toEqual({ start: 5.5, end: 6 });
    expect(setEdge({ start: 2, end: 6 }, "end", 1, 12)).toEqual({ start: 2, end: 2.5 });
    expect(setEdge({ start: 2, end: 6 }, "end", 20, 12)).toEqual({ start: 2, end: 12 });
  });
});

describe("instants de l'éditeur", () => {
  it("au dixième", async () => {
    const { instant } = await import("../ui/src/format");
    expect(instant(2.6)).toMatch(/^0:02[.,]6$/);
    expect(instant(61.04)).toMatch(/^1:01[.,]0$/);
    expect(instant(-1)).toMatch(/^0:00[.,]0$/);
  });
});

describe("partage", () => {
  it("garde un lien qui vaut encore plus d'une heure", async () => {
    const { liveLink } = await import("../ui/src/Share");
    expect(liveLink({ link: "https://litter.catbox.moe/a.mp4", linkUntil: 10_000_000 }, 0)).toBe("https://litter.catbox.moe/a.mp4");
    expect(liveLink({ link: "https://litter.catbox.moe/a.mp4", linkUntil: 10_000_000 }, 7_000_000)).toBeNull();
    expect(liveLink({}, 0)).toBeNull();
  });

  it("place le cadre vertical dans l'image", async () => {
    const { contentBox, frameLeft } = await import("../ui/src/Vertical");
    const b = contentBox(1000, 600, 1920, 1080);
    expect(b.x).toBeCloseTo(0);
    expect(b.y).toBeCloseTo(18.75);
    expect(b.w).toBeCloseTo(1000);
    expect(b.h).toBeCloseTo(562.5);
    const frame = (1080 * 9) / 16 / 1920;
    expect(frameLeft(0.5, frame)).toBeCloseTo(0.5 - frame / 2);
    expect(frameLeft(0, frame)).toBe(0);
    expect(frameLeft(1, frame)).toBeCloseTo(1 - frame);
  });
});

describe("raccourcis", () => {
  it("nomment les touches comme les gestionnaires les attendent", async () => {
    const { keyName } = await import("../ui/src/keys");
    expect(keyName({ key: " ", ctrlKey: false })).toBe("space");
    expect(keyName({ key: "C", ctrlKey: false })).toBe("c");
    expect(keyName({ key: "c", ctrlKey: true })).toBe("ctrl+c");
    expect(keyName({ key: "Delete", ctrlKey: false })).toBe("Delete");
    expect(keyName({ key: "?", ctrlKey: false })).toBe("?");
  });

  it("affichent le vrai raccourci de l'overlay d'iBox", async () => {
    const { groups } = await import("../ui/src/keys");
    expect(groups("Ctrl+Shift+X")[0].items[0][0]).toBe("Ctrl+Shift+X");
    expect(groups(null)[0].items[0][0]).toBe("Alt+X");
  });
});

describe("app autonome", () => {
  it("écrit le raccourci du panneau comme l'app l'attend", async () => {
    const { combo } = await import("../ui/src/Settings");
    const k = (code: string, key: string, m: { ctrlKey?: boolean; altKey?: boolean; shiftKey?: boolean; metaKey?: boolean } = {}) =>
      combo({ code, key, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...m });
    expect(k("KeyX", "x", { altKey: true })).toBe("Alt+X");
    // AZERTY : la touche A est à la place du Q, Windows la nomme A
    expect(k("KeyQ", "a", { ctrlKey: true, shiftKey: true })).toBe("Ctrl+Maj+A");
    expect(k("F9", "F9", { ctrlKey: true })).toBe("Ctrl+F9");
    expect(k("Digit1", "&", { altKey: true })).toBe("Alt+1");
    // une touche seule, Maj seule ou une touche inconnue : refusé
    expect(k("KeyX", "x")).toBeNull();
    expect(k("KeyX", "X", { shiftKey: true })).toBeNull();
    expect(k("Semicolon", ";", { altKey: true })).toBeNull();
  });
});

describe("Studio 0.9", () => {
  it("range la galerie par date, du plus récent au plus ancien", async () => {
    const { byDate, dateGroup } = await import("../ui/src/format");
    const now = new Date(2026, 10, 26, 15, 0).getTime();
    const at = (d: number, h = 12) => new Date(2026, 10, 26 - d, h).getTime();
    expect(dateGroup(at(0, 1), now).key).toBe("today");
    expect(dateGroup(at(1, 23), now).key).toBe("yesterday");
    expect(dateGroup(at(6), now).key).toBe("week");
    expect(dateGroup(at(7), now).key).toBe("2026-11");
    expect(dateGroup(new Date(2025, 11, 3).getTime(), now)).toEqual({ key: "2025-12", label: "Décembre 2025" });
    const groups = byDate([{ at: at(40) }, { at: at(0) }, { at: at(1) }, { at: at(0, 9) }, { at: at(3) }], now);
    expect(groups.map((g) => [g.key, g.items.length])).toEqual([["today", 2], ["yesterday", 1], ["week", 1], ["2026-10", 1]]);
    expect(groups[0].items[0].at).toBe(at(0));
  });

  it("zoome la frise sur le passage, ou autour de la tête de lecture", async () => {
    const { zoomView } = await import("../ui/src/edit");
    // passage court : lui et un peu d'air
    expect(zoomView({ start: 10, end: 14 }, 0, 30)).toEqual({ start: 9.4, end: 14.6 });
    // passage qui couvre presque tout : 8 s autour de la tête, sans sortir du clip
    expect(zoomView({ start: 0, end: 30 }, 12, 30)).toEqual({ start: 8, end: 16 });
    expect(zoomView({ start: 0, end: 30 }, 1, 30)).toEqual({ start: 0, end: 8 });
    expect(zoomView({ start: 0, end: 30 }, 29, 30)).toEqual({ start: 22, end: 30 });
  });

  it("additionne la bande de montage", async () => {
    const { totalLength } = await import("../ui/src/edit");
    expect(totalLength([{ duration: 12 }, { duration: 8.5 }])).toEqual({ seconds: 20.5, exact: true });
    expect(totalLength([{ duration: 12 }, { duration: null }])).toEqual({ seconds: 12, exact: false });
  });

  it("change de vitesse d'un cran, sans sortir des vitesses proposées", async () => {
    const { nextSpeed } = await import("../ui/src/Playback");
    expect(nextSpeed(1, -1)).toBe(0.5);
    expect(nextSpeed(1, 1)).toBe(1.5);
    expect(nextSpeed(0.25, -1)).toBe(0.25);
    expect(nextSpeed(2, 1)).toBe(2);
  });
});
