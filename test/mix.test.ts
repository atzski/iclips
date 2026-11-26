import { describe, expect, it } from "vitest";
import { drifted, sources, trackUrl } from "../ui/src/mix";

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
