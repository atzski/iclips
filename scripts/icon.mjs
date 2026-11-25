// Logo source (fond blanc) → icon.png (256 px, catalogue et paquet) et logo.png (512 px,
// Studio et README), fond transparent. Le blanc n'est retiré que depuis les bords : les
// rayures blanches du clap, dans le contour sombre, restent.
//   node scripts/icon.mjs "<logo source>.png"
import sharp from "sharp";
import { copyFileSync, statSync } from "node:fs";
import { join } from "node:path";

const root = join(import.meta.dirname, "..");
const src = process.argv[2];
if (!src) throw new Error("Chemin du logo source manquant.");

const { data, info } = await sharp(src).ensureAlpha().raw().toBuffer({ resolveWithObject: true });
const { width: w, height: h } = info;
const white = (i) => data[i] > 225 && data[i + 1] > 225 && data[i + 2] > 225;
const seen = new Uint8Array(w * h);
const stack = [];
for (let x = 0; x < w; x++) stack.push(x, (h - 1) * w + x);
for (let y = 0; y < h; y++) stack.push(y * w, y * w + w - 1);
while (stack.length) {
  const p = stack.pop();
  if (seen[p]) continue;
  seen[p] = 1;
  if (!white(p * 4)) continue;
  data[p * 4 + 3] = 0;
  const x = p % w;
  if (x > 0) stack.push(p - 1);
  if (x < w - 1) stack.push(p + 1);
  if (p >= w) stack.push(p - w);
  if (p < w * (h - 1)) stack.push(p + w);
}

const clean = await sharp(data, { raw: { width: w, height: h, channels: 4 } }).png().toBuffer();
const trimmed = await sharp(clean).trim({ background: { r: 0, g: 0, b: 0, alpha: 0 } }).toBuffer({ resolveWithObject: true });
const side = Math.max(trimmed.info.width, trimmed.info.height);
const pad = Math.round(side * 0.04);
const square = await sharp(trimmed.data)
  .extend({
    top: pad + Math.floor((side - trimmed.info.height) / 2), bottom: pad + Math.ceil((side - trimmed.info.height) / 2),
    left: pad + Math.floor((side - trimmed.info.width) / 2), right: pad + Math.ceil((side - trimmed.info.width) / 2),
    background: { r: 0, g: 0, b: 0, alpha: 0 },
  })
  .png()
  .toBuffer();
for (const [size, name] of [[256, "icon.png"], [512, "logo.png"]]) {
  await sharp(square).resize(size, size).png({ compressionLevel: 9, palette: size === 256 }).toFile(join(root, name));
  console.log(`${name} : ${Math.round(statSync(join(root, name)).size / 1024)} Ko`);
}
// l’interface prend l’icône (256 px suffisent pour 6 rem, et 24 Ko au lieu de 200)
copyFileSync(join(root, "icon.png"), join(root, "ui", "src", "logo.png"));
