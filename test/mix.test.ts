import { describe, expect, it } from "vitest";
import { drifted, fallbackVolume, sources, trackUrl } from "../ui/src/mix";

describe("volumes entendus dans le lecteur", () => {
  it("pistes séparées à côté du clip", () => {
    expect(trackUrl("/iclips/__clips/a-1.mp4", "game")).toBe("/iclips/__clips/a-1.game.m4a");
    expect(sources("/iclips/__clips/a-1.mp4", ["mix", "game", "mic"]).map((s) => s.url)).toEqual(["/iclips/__clips/a-1.game.m4a", "/iclips/__clips/a-1.mic.m4a"]);
  });

  it("sans pistes séparées : le son du clip lui-même", () => {
    expect(sources("/iclips/__clips/a-1.mp4", undefined)).toEqual([{ key: "game", url: "/iclips/__clips/a-1.mp4" }]);
    expect(sources("/iclips/__clips/a-1.mp4", ["mix"])).toEqual([{ key: "game", url: "/iclips/__clips/a-1.mp4" }]);
  });

  it("recale un son qui dérive", () => {
    expect(drifted(10.05, 10)).toBe(false);
    expect(drifted(10.3, 10)).toBe(true);
  });
});

describe("volume sans pistes à part", () => {
  it("suit la plus forte des deux barres, 100 % au plus", () => {
    expect(fallbackVolume({ game: 0.5, mic: 0.5 })).toBe(0.5);
    expect(fallbackVolume({ game: 0.2, mic: 0.7 })).toBe(0.7);
    expect(fallbackVolume({ game: 2, mic: 1 })).toBe(1);
    expect(fallbackVolume({ game: 0, mic: 0 })).toBe(0);
  });
});
