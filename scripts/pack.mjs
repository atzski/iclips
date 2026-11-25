// Paquet signé du plugin : dist/iclips-<version>.iboxplugin + dist/catalog-entry.json.
// Construit l'interface (Vite) puis empaquette manifest.json, icon.png et ui/.
// Clé d'éditeur : IBOX_PLUGIN_KEY, sinon Documents\iBox-sauvegarde\signature\ibox-plugins.key
// (jamais dans ce dépôt). Message signé identique à iBox (src-tauri/src/plugins/trust.rs).
//   PACK_LOCAL=1 : paquet d'essai pour un catalogue local (adresse relative).
//   IBOX_MIN=1.0.7 (avec PACK_LOCAL) : version d'iBox demandée par le paquet d'essai, pour une
//   copie d'iBox pas encore numérotée comme la prochaine version.
import { createHash, createPrivateKey, sign } from "node:crypto";
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { execFileSync } from "node:child_process";

const root = join(import.meta.dirname, "..");
const manifest = JSON.parse(readFileSync(join(root, "manifest.json"), "utf8"));
if (process.env.IBOX_MIN) {
  if (process.env.PACK_LOCAL !== "1") throw new Error("IBOX_MIN est réservé aux paquets d'essai (PACK_LOCAL=1).");
  manifest.ibox = `>=${process.env.IBOX_MIN}`;
}
const { id, version } = manifest;
const local = process.env.PACK_LOCAL === "1";
if (!manifest.author?.username) throw new Error("manifest.json : author.username (compte iNetbox du créateur) manquant.");
const key = process.env.IBOX_PLUGIN_KEY ?? join(homedir(), "Documents", "iBox-sauvegarde", "signature", "ibox-plugins.key");
if (!existsSync(key)) throw new Error(`Clé de signature introuvable : ${key}`);

execFileSync("npm", ["run", "build"], { cwd: root, stdio: "inherit", shell: process.platform === "win32" });

const stage = join(root, "dist", "pkg");
rmSync(stage, { recursive: true, force: true });
mkdirSync(stage, { recursive: true });
writeFileSync(join(stage, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
cpSync(join(root, "icon.png"), join(stage, "icon.png"));
cpSync(join(root, "dist", "ui"), join(stage, "ui"), { recursive: true });

// zip aux chemins en « / » (Compress-Archive écrit des « \ », refusés par iBox)
const out = join(root, "dist", `${id}-${version}.iboxplugin`);
const zip = out.replace(/\.iboxplugin$/, ".zip");
rmSync(out, { force: true });
rmSync(zip, { force: true });
const tar = process.platform === "win32" ? join(process.env.SystemRoot ?? "C:\\Windows", "System32", "tar.exe") : "bsdtar";
execFileSync(tar, ["-a", "-c", "-f", zip, "manifest.json", "icon.png", "ui"], { cwd: stage });
cpSync(zip, out);
rmSync(zip);

const bytes = readFileSync(out);
const sha256 = createHash("sha256").update(bytes).digest("hex");
const signature = sign(null, Buffer.from(`ibox-plugin-v1\n${id}\n${version}\n${sha256}`), createPrivateKey(readFileSync(key))).toString("base64");
const icon = `data:image/png;base64,${readFileSync(join(root, "icon.png")).toString("base64")}`;
if (icon.length > 64 * 1024) throw new Error("icon.png trop lourde pour le catalogue (64 Ko en base64 au plus).");
const entry = {
  id, name: manifest.name, version, description: manifest.description, author: { name: manifest.author.name, username: manifest.author.username },
  url: local ? `${id}-${version}.iboxplugin` : `https://github.com/atzski/${id}/releases/download/v${version}/${id}-${version}.iboxplugin`,
  sha256, signature, publisher: "atzski", size: statSync(out).size,
  notes: process.env.NOTES ?? "", permissions: manifest.permissions, icon, ibox: manifest.ibox, platforms: manifest.platforms,
};
writeFileSync(join(root, "dist", "catalog-entry.json"), JSON.stringify(entry, null, 2));
if (local) writeFileSync(join(root, "dist", "catalog.json"), JSON.stringify({ plugins: [entry] }, null, 2));
console.log(`${out}\nsha256 ${sha256}\nentrée de catalogue : dist/catalog-entry.json${local ? "\ncatalogue local : dist/catalog.json (IBOX_PLUGINS_CATALOG)" : ""}`);
