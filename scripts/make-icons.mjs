// Erzeugt das App-Icon (source.png) und die Tray-Icons aus SVG.
// Aufruf: bun run icons  (danach erzeugt `tauri icon` alle Größen + icon.ico)
import sharp from "sharp";
import { mkdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const iconsDir = join(root, "src-tauri", "icons");
mkdirSync(iconsDir, { recursive: true });

// Farben aus src/birdy-tokens.css
const SURFACE = "#323436";
const BORDER = "#4B4F4C";
const DANGER = "#E6AAA5";
const WARNING = "#E9C493";

// Der Birdy-Eisvogel (wie im Tray von Birdy Credit Status), 256er-Raster.
// Inhalt ohne <svg>-Hülle, Titel und Beschreibung, damit er sich einbetten lässt.
const kingfisher = readFileSync(join(root, "src", "assets", "birdy-kingfisher.svg"), "utf8")
  .replace(/^[\s\S]*?<svg[^>]*>/, "")
  .replace(/<\/svg>\s*$/, "")
  .replace(/<title>[\s\S]*?<\/title>|<desc>[\s\S]*?<\/desc>/g, "");
// Mitte der Vogelgrafik im 256er-Raster; (cx, cy) ist die Zielmitte, s der Maßstab.
const bird = (cx, cy, s) => `<g transform="translate(${cx - 132.5 * s} ${cy - 127.5 * s}) scale(${s})">${kingfisher}</g>`;

const appIcon = `
<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
  <rect x="68" y="68" width="888" height="888" rx="216" fill="${SURFACE}" stroke="${BORDER}" stroke-width="8"/>
  ${bird(512, 512, 3.1)}
</svg>`;

// Graphitkreis grenzt das Tray-Icon vom freistehenden Vogel in Birdy Credit
// Status ab. Statuspunkt oben links – rechts oben sitzt der Schnabel.
const trayIcon = (dot) => `
<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 1024 1024">
  <circle cx="512" cy="512" r="500" fill="${SURFACE}"/>
  ${bird(512, 512, 3.2)}
  ${dot ? `<circle cx="210" cy="210" r="170" fill="${dot}" stroke="${SURFACE}" stroke-width="56"/>` : ""}
</svg>`;

await sharp(Buffer.from(appIcon)).png().toFile(join(iconsDir, "source.png"));
await sharp(Buffer.from(trayIcon(null))).png().toFile(join(iconsDir, "tray-idle.png"));
await sharp(Buffer.from(trayIcon(DANGER))).png().toFile(join(iconsDir, "tray-recording.png"));
await sharp(Buffer.from(trayIcon(WARNING))).png().toFile(join(iconsDir, "tray-processing.png"));
console.log("Icons erzeugt in", iconsDir);
