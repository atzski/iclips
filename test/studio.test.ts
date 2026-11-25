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
