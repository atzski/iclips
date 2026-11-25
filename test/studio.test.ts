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
