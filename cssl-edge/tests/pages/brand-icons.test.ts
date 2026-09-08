import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import sharp from 'sharp';

const root = process.cwd();
const read = (relative: string): string => fs.readFileSync(path.join(root, relative), 'utf8');
const bytes = (relative: string): Buffer => fs.readFileSync(path.join(root, relative));
const exists = (relative: string): boolean => fs.existsSync(path.join(root, relative));
const fromPublicUrl = (url: string): string => path.join('public', url.replace(/^\//, ''));

function pngDimensions(relative: string): { width: number; height: number } {
  const buffer = bytes(relative);
  assert.equal(buffer.subarray(0, 8).toString('hex'), '89504e470d0a1a0a', `${relative} must be a PNG`);
  return { width: buffer.readUInt32BE(16), height: buffer.readUInt32BE(20) };
}

function icoDimensions(relative: string): Array<[number, number]> {
  const buffer = bytes(relative);
  assert.equal(buffer.readUInt16LE(0), 0, `${relative} ICO reserved field`);
  assert.equal(buffer.readUInt16LE(2), 1, `${relative} must contain icons`);
  const count = buffer.readUInt16LE(4);
  return Array.from({ length: count }, (_, index) => {
    const offset = 6 + (index * 16);
    return [buffer[offset] || 256, buffer[offset + 1] || 256];
  });
}

const manifest = JSON.parse(read('public/manifest.json')) as {
  id?: string;
  background_color?: string;
  theme_color?: string;
  icons?: Array<{ src: string; sizes: string; purpose?: string }>;
  shortcuts?: Array<{ url: string; icons?: Array<{ src: string; sizes: string }> }>;
};

assert.equal(manifest.id, '/', 'installed identity must remain stable');
assert.equal(manifest.background_color, '#000000', 'install splash must be AMOLED black');
assert.equal(manifest.theme_color, '#000000', 'browser chrome must be AMOLED black');

const icons = manifest.icons ?? [];
assert(icons.some((icon) => icon.purpose === 'any' && icon.sizes === '192x192'));
assert(icons.some((icon) => icon.purpose === 'any' && icon.sizes === '512x512'));
assert(icons.some((icon) => icon.purpose === 'maskable' && icon.sizes === '192x192'));
assert(icons.some((icon) => icon.purpose === 'maskable' && icon.sizes === '512x512'));
assert(icons.some((icon) => icon.purpose === 'monochrome'));
assert(icons.filter((icon) => icon.purpose !== 'monochrome').every((icon) => /apocky-(?:maskable-)?v3-/.test(icon.src)));
assert(icons.every((icon) => !icon.src.includes('-v2-')), 'the install manifest must not advertise retired v2 icons');

for (const icon of icons) {
  const relative = fromPublicUrl(icon.src);
  assert(exists(relative), `manifest icon must exist: ${relative}`);
  const match = icon.sizes.match(/^(\d+)x(\d+)$/);
  if (match && relative.endsWith('.png')) {
    assert.deepEqual(
      pngDimensions(relative),
      { width: Number(match[1]), height: Number(match[2]) },
      `${relative} pixels must match its manifest declaration`,
    );
  }
}

const shortcutIcons = (manifest.shortcuts ?? []).flatMap((shortcut) => shortcut.icons ?? []);
assert.equal(manifest.shortcuts?.length, 6, 'Atlas, Clearing, Memory, Divination, Oracle, and Spellcraft shortcuts must ship');
assert.equal(new Set(shortcutIcons.map((icon) => icon.src)).size, 6, 'each shortcut needs a distinct glyph');
const shortcutByUrl = new Map((manifest.shortcuts ?? []).map((shortcut) => [shortcut.url, shortcut.icons?.[0]?.src]));
assert.equal(shortcutByUrl.get('/divination'), '/icons/shortcut-divination-v2-96.png');
assert.equal(shortcutByUrl.get('/oracle'), '/icons/shortcut-oracle-v2-96.png');
for (const icon of shortcutIcons) {
  const relative = fromPublicUrl(icon.src);
  assert(exists(relative), `shortcut icon must exist: ${relative}`);
  assert.deepEqual(pngDimensions(relative), { width: 96, height: 96 });
}

for (const [relative, size] of [
  ['public/icons/apocky-v3-16.png', 16],
  ['public/icons/apocky-v3-32.png', 32],
  ['public/icons/apocky-v3-192.png', 192],
  ['public/icons/apocky-v3-512.png', 512],
  ['public/icons/apocky-maskable-v3-192.png', 192],
  ['public/icons/apocky-maskable-v3-512.png', 512],
  ['public/apple-touch-icon.png', 180],
  ['public/apple-touch-icon-167x167.png', 167],
  ['public/apple-touch-icon-152x152.png', 152],
] as const) {
  assert.deepEqual(pngDimensions(relative), { width: size, height: size });
}

const master = bytes('public/brand/apocky-neural-mark-v3.png');
assert.deepEqual(pngDimensions('public/brand/apocky-neural-mark-v3.png'), { width: 1254, height: 1254 });
assert.equal(
  createHash('sha256').update(master).digest('hex'),
  '68a7b0dfe1f24bacbe171d6b2c95aa4c424d8121900b79d9f67ff48581bc25f7',
  'derived icons must remain bound to the approved production master',
);

const opaqueSurfaces = [
  'public/icons/apocky-maskable-v3-192.png',
  'public/icons/apocky-maskable-v3-512.png',
  'public/apple-touch-icon.png',
] as const;

assert.deepEqual(icoDimensions('public/favicon.ico'), [[16, 16], [32, 32], [48, 48], [256, 256]]);
assert.deepEqual(pngDimensions('public/og/apocky-default-v3.png'), { width: 1200, height: 630 });

for (const relative of [
  'public/brand/apocky-mark.svg',
  'public/brand/apocky-icon.svg',
  'public/brand/apocky-maskable.svg',
  'public/brand/apocky-favicon.svg',
  'public/brand/apocky-monochrome.svg',
]) {
  const svg = read(relative);
  assert.doesNotMatch(svg, /<text\b/i, `${relative} must not depend on a font glyph`);
  assert.doesNotMatch(svg, /(?:#39ff14|\bgreen\b)/i, `${relative} must not restore the retired green accent`);
}

const documentSource = read('pages/_document.tsx');
assert.match(documentSource, /href="\/icons\/apocky-v3-32\.png"/);
assert.match(documentSource, /href="\/icons\/apocky-v3-16\.png"/);
assert.match(documentSource, /href="\/favicon\.ico"/);
assert.match(documentSource, /rel="mask-icon" href="\/brand\/apocky-monochrome\.svg" color="#6366f1"/);
assert.match(documentSource, /href="\/apple-touch-icon\.png"/);
assert.match(documentSource, /name="apple-mobile-web-app-capable" content="yes"/);
assert.match(documentSource, /name="apple-mobile-web-app-status-bar-style" content="black-translucent"/);
assert.match(documentSource, /name="apple-mobile-web-app-title" content="Apocky"/);
assert.match(documentSource, /msapplication-TileImage" content="\/icons\/apocky-v3-192\.png"/);
assert.match(documentSource, /apocky-default-v3\.png/);
assert.doesNotMatch(documentSource, /href="\/favicon\.svg"/);
assert.doesNotMatch(documentSource, /apple-touch-icon" href="\/icon-192\.svg"/);
assert.match(read('styles/apocky-system.css'), /url\('\/brand\/apocky-mark\.svg'\)/);

for (const relative of [
  'pages/content/index.tsx',
  'pages/content/feed.tsx',
  'pages/content/subscribed.tsx',
  'pages/content/search.tsx',
  'pages/content/trending.tsx',
  'pages/content/[slug].tsx',
  'pages/docs/[slug].tsx',
  'pages/download.tsx',
]) {
  assert.doesNotMatch(read(relative), /theme-color" content="#0a0a0f"/);
  assert.match(read(relative), /theme-color" content="#000000"/);
}

Promise.all(opaqueSurfaces.map(async (relative) => ({ relative, stats: await sharp(path.join(root, relative)).stats() })))
  .then((surfaces) => {
    for (const { relative, stats } of surfaces) {
      assert.equal(stats.isOpaque, true, `${relative} must fill every platform mask with the AMOLED field`);
    }
    console.log('Apocky v3 brand icon contract passed.');
  })
  .catch((error: unknown) => {
    console.error(error);
    process.exitCode = 1;
  });
