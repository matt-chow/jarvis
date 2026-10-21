// Generates the PNG app icons (iOS needs PNG apple-touch-icons) without any
// image dependency. Mirrors apps/web/public/icons/icon.svg. Run:
//   node scripts/generate-icons.mjs
import { deflateSync } from "node:zlib";
import { writeFileSync, mkdirSync } from "node:fs";

const OUT = new URL("../apps/web/public/icons/", import.meta.url);
const BG = [0x10, 0x12, 0x17];
const FG = [0xf2, 0xa5, 0x41];

const CRC_TABLE = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
const crc32 = (buf) => {
  let c = 0xffffffff;
  for (const b of buf) c = CRC_TABLE[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}

/** Distance from point p to segment ab, all in 64-unit icon space. */
function segDist(px, py, ax, ay, bx, by) {
  const dx = bx - ax, dy = by - ay;
  const t = Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)));
  return Math.hypot(px - (ax + t * dx), py - (ay + t * dy));
}

function inGlyph(x, y) {
  const chevron = Math.min(segDist(x, y, 17, 20, 31, 32), segDist(x, y, 31, 32, 17, 44)) <= 3;
  const underscore = x >= 34 && x <= 48 && y >= 40 && y <= 45;
  return chevron || underscore;
}

/** `rounded`: transparent corners (favicon style); otherwise full bleed (iOS/maskable). */
function render(size, rounded) {
  const SS = 4; // supersampling for anti-aliasing
  const raw = Buffer.alloc(size * (size * 4 + 1));
  for (let py = 0; py < size; py++) {
    raw[py * (size * 4 + 1)] = 0; // filter: none
    for (let px = 0; px < size; px++) {
      let fg = 0, inside = 0;
      for (let sy = 0; sy < SS; sy++) {
        for (let sx = 0; sx < SS; sx++) {
          const x = ((px + (sx + 0.5) / SS) / size) * 64;
          const y = ((py + (sy + 0.5) / SS) / size) * 64;
          const r = 14, cx = Math.min(Math.max(x, r), 64 - r), cy = Math.min(Math.max(y, r), 64 - r);
          if (rounded && Math.hypot(x - cx, y - cy) > r) continue;
          inside++;
          if (inGlyph(x, y)) fg++;
        }
      }
      const n = SS * SS;
      const t = inside ? fg / inside : 0;
      const o = py * (size * 4 + 1) + 1 + px * 4;
      for (let i = 0; i < 3; i++) raw[o + i] = Math.round(BG[i] + (FG[i] - BG[i]) * t);
      raw[o + 3] = Math.round((inside / n) * 255);
    }
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

mkdirSync(OUT, { recursive: true });
for (const [name, size, rounded] of [
  ["icon-192.png", 192, true],
  ["icon-512.png", 512, false],
  ["apple-touch-icon.png", 180, false],
]) {
  writeFileSync(new URL(name, OUT), render(size, rounded));
  console.log(`wrote ${name} (${size}px)`);
}
