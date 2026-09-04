import { mkdir } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import sharp from 'sharp';

const root = process.cwd();
const output = process.argv[2] ?? join(root, 'test-results', 'apocky-icons-v3-surface-preview.png');

function labelSvg(width, height, lines) {
  return Buffer.from(`<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">
    <rect width="${width}" height="${height}" rx="18" fill="#11121a" stroke="#292c42"/>
    ${lines.map((line, index) => `<text x="18" y="${27 + (index * 21)}" fill="${index === 0 ? '#f8faff' : '#9299bd'}" font-family="Arial, sans-serif" font-size="${index === 0 ? 15 : 12}" font-weight="${index === 0 ? 700 : 500}">${line}</text>`).join('')}
  </svg>`);
}

async function displayIcon(relative, displaySize, options = {}) {
  let image = sharp(join(root, 'public', relative)).resize(displaySize, displaySize, { kernel: options.pixelated ? sharp.kernel.nearest : sharp.kernel.lanczos3 });
  if (options.mask === 'circle') {
    image = image.composite([{ input: Buffer.from(`<svg xmlns="http://www.w3.org/2000/svg" width="${displaySize}" height="${displaySize}"><circle cx="50%" cy="50%" r="50%" fill="white"/></svg>`), blend: 'dest-in' }]);
  } else if (options.mask === 'rounded') {
    const radius = Math.round(displaySize * 0.225);
    image = image.composite([{ input: Buffer.from(`<svg xmlns="http://www.w3.org/2000/svg" width="${displaySize}" height="${displaySize}"><rect width="${displaySize}" height="${displaySize}" rx="${radius}" fill="white"/></svg>`), blend: 'dest-in' }]);
  }
  return image.png().toBuffer();
}

const panels = [
  { x: 40, y: 44, w: 250, h: 250, iconX: 66, iconY: 118, size: 16, source: 'icons/apocky-v3-16.png', pixelated: false, title: 'Browser favicon · 16 px', note: 'Rendered at physical size' },
  { x: 310, y: 44, w: 250, h: 250, iconX: 344, iconY: 110, size: 32, source: 'icons/apocky-v3-32.png', pixelated: false, title: 'Browser favicon · 32 px', note: 'Rendered at physical size' },
  { x: 580, y: 44, w: 250, h: 250, iconX: 603, iconY: 102, size: 128, source: 'icons/apocky-v3-16.png', pixelated: true, title: '16 px inspection · 8×', note: 'Nearest-neighbor magnification' },
  { x: 850, y: 44, w: 250, h: 250, iconX: 873, iconY: 102, size: 128, source: 'icons/apocky-v3-32.png', pixelated: true, title: '32 px inspection · 4×', note: 'Nearest-neighbor magnification' },
  { x: 40, y: 320, w: 330, h: 390, iconX: 75, iconY: 405, size: 260, source: 'apple-touch-icon.png', mask: 'rounded', title: 'iOS home screen · 180 px', note: 'Rounded platform mask' },
  { x: 390, y: 320, w: 330, h: 390, iconX: 425, iconY: 405, size: 260, source: 'icons/apocky-maskable-v3-192.png', mask: 'circle', title: 'Android maskable · 192 px', note: 'Circular launcher mask' },
  { x: 740, y: 320, w: 330, h: 390, iconX: 775, iconY: 405, size: 260, source: 'icons/apocky-maskable-v3-512.png', mask: 'rounded', title: 'Android maskable · 512 px', note: 'Squircle-style launcher mask' },
  { x: 1090, y: 320, w: 310, h: 390, iconX: 1115, iconY: 405, size: 260, source: 'icons/apocky-v3-512.png', title: 'Desktop install · 512 px', note: 'Ordinary square surface' },
];

const composites = [];
for (const panel of panels) {
  composites.push({ input: labelSvg(panel.w, panel.h, [panel.title, panel.note]), left: panel.x, top: panel.y });
  composites.push({
    input: await displayIcon(panel.source, panel.size, { mask: panel.mask, pixelated: panel.pixelated }),
    left: panel.iconX,
    top: panel.iconY,
  });
}

await mkdir(dirname(output), { recursive: true });
await sharp({ create: { width: 1440, height: 760, channels: 4, background: '#08090e' } })
  .composite(composites)
  .png({ compressionLevel: 9, adaptiveFiltering: true })
  .toFile(output);

console.log(output);
