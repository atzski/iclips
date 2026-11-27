import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { EN_KEYS, lang, langOf, t, tm } from "../ui/src/i18n";

/**
 * L'anglais est la langue par défaut d'iClips : chaque texte passé à `t("…")` doit avoir sa
 * traduction (sinon une page en anglais montre du français).
 */
const dir = join(import.meta.dirname, "..", "ui", "src");
const files = readdirSync(dir).filter((f) => /\.tsx?$/.test(f)).map((f) => join(dir, f));

function literals(): { text: string; file: string }[] {
  const out: { text: string; file: string }[] = [];
  for (const file of files) {
    const src = readFileSync(file, "utf8");
    for (const m of src.matchAll(/\bt\(\s*"((?:[^"\\]|\\.)*)"/g)) out.push({ text: JSON.parse(`"${m[1]}"`), file });
  }
  return out;
}

describe("traductions d'iClips", () => {
  it("anglais par défaut, français seulement demandé", () => {
    expect(langOf("")).toBe("en");
    expect(langOf("?lang=de")).toBe("en");
    expect(langOf("?lang=fr&scheme=dark")).toBe("fr");
    expect(lang).toBe("en");
  });

  it("chaque t(\"…\") a sa traduction anglaise", () => {
    const missing = literals().filter((l) => !EN_KEYS.has(l.text)).map((l) => `${l.text}  ← ${l.file.split(/[\\/]/).pop()}`);
    expect([...new Set(missing)]).toEqual([]);
    expect(t("Fermer")).toBe("Close");
  });

  it("les messages du natif sont traduits, gabarits compris", () => {
    expect(tm("Vidéo illisible.")).toBe("Unreadable video.");
    expect(tm("Clip impossible à écrire : Vidéo illisible.")).toBe("Could not write the clip: Unreadable video.");
    expect(tm("Litterbox a refusé l'envoi (413).")).toBe("Litterbox refused the upload (413).");
    // inconnu : tel quel
    expect(tm("Hollow Knight")).toBe("Hollow Knight");
    expect(tm("Vidéo illisible.", "fr")).toBe("Vidéo illisible.");
  });
});
