// Erzeugt das App-Icon (source.png) und die Tray-Icons aus SVG.
// Aufruf: bun run icons  (danach erzeugt `tauri icon` alle Größen + icon.ico)
import sharp from "sharp";
import { mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const iconsDir = join(root, "src-tauri", "icons");
mkdirSync(iconsDir, { recursive: true });

const bars = (cx, cy, scale, color) => {
  const heights = [0.32, 0.62, 1, 0.7, 0.4];
  const w = 54 * scale;
  const gap = 34 * scale;
  const maxH = 300 * scale;
  const total = heights.length * w + (heights.length - 1) * gap;
  return heights
    .map((h, i) => {
      const x = cx - total / 2 + i * (w + gap);
      const hh = maxH * h;
      return `<rect x="${x}" y="${cy - hh / 2}" width="${w}" height="${hh}" rx="${w / 2}" fill="${color}"/>`;
    })
    .join("");
};

const appIcon = `
<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#4F46E5"/>
      <stop offset="0.55" stop-color="#7C3AED"/>
      <stop offset="1" stop-color="#06B6D4"/>
    </linearGradient>
    <linearGradient id="pill" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#0B1020"/>
      <stop offset="1" stop-color="#151B33"/>
    </linearGradient>
  </defs>
  <rect x="64" y="64" width="896" height="896" rx="220" fill="url(#bg)"/>
  <rect x="172" y="332" width="680" height="360" rx="180" fill="url(#pill)" opacity="0.92"/>
  ${bars(512, 512, 1, "#FFFFFF")}
</svg>`;

const trayIcon = (dot) => `
<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 1024 1024">
  <rect x="40" y="200" width="944" height="624" rx="312" fill="#111827"/>
  ${bars(512, 512, 1.15, "#FFFFFF")}
  ${dot ? `<circle cx="850" cy="230" r="170" fill="${dot}" stroke="#111827" stroke-width="50"/>` : ""}
</svg>`;

await sharp(Buffer.from(appIcon)).png().toFile(join(iconsDir, "source.png"));
await sharp(Buffer.from(trayIcon(null))).png().toFile(join(iconsDir, "tray-idle.png"));
await sharp(Buffer.from(trayIcon("#EF4444"))).png().toFile(join(iconsDir, "tray-recording.png"));
await sharp(Buffer.from(trayIcon("#F59E0B"))).png().toFile(join(iconsDir, "tray-processing.png"));
console.log("Icons erzeugt in", iconsDir);
