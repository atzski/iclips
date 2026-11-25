import { describe, expect, it } from "vitest";
import { place } from "../ui/src/Menu";
import { audible } from "../ui/src/quiet";

describe("menu d'un clip", () => {
  const view = { w: 1280, h: 800 };
  const size = { w: 280, h: 140 };
  it("s'ouvre au pointeur", () => {
    expect(place({ x: 100, y: 100 }, size, view)).toEqual({ left: 100, top: 100, origin: "left top" });
  });
  it("se retourne au bord droit et en bas", () => {
    expect(place({ x: 1200, y: 760 }, size, view)).toEqual({ left: 920, top: 620, origin: "right bottom" });
  });
  it("ne sort jamais par le haut ou la gauche", () => {
    const p = place({ x: 100, y: 100 }, { w: 280, h: 790 }, view);
    expect(p.top).toBe(12);
  });
});

describe("« Clip ça ! » pendant la lecture", () => {
  const v = { paused: false, ended: false, muted: false, volume: 1 };
  it("se tait seulement si la vidéo s'entend", () => {
    expect(audible(v)).toBe(true);
    expect(audible({ ...v, paused: true })).toBe(false);
    expect(audible({ ...v, ended: true })).toBe(false);
    expect(audible({ ...v, muted: true })).toBe(false);
    expect(audible({ ...v, volume: 0 })).toBe(false);
  });
});
