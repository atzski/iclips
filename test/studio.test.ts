import { describe, expect, it } from "vitest";
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
