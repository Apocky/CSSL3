import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import sharp from 'sharp';

const root = process.cwd();
const publicDir = join(root, 'public');
const brandDir = join(publicDir, 'brand');
const iconDir = join(publicDir, 'icons');
const ogDir = join(publicDir, 'og');

await Promise.all([
  mkdir(iconDir, { recursive: true }),
  mkdir(ogDir, { recursive: true }),
]);

const MASTER_NAME = 'apocky-neural-mark-v3.png';
const MASTER_SHA256 = '68a7b0dfe1f24bacbe171d6b2c95aa4c424d8121900b79d9f67ff48581bc25f7';
const masterPng = await readFile(join(brandDir, MASTER_NAME));
const masterDigest = createHash('sha256').update(masterPng).digest('hex');
if (masterDigest !== MASTER_SHA256) {
  throw new Error(`${MASTER_NAME} digest mismatch: expected ${MASTER_SHA256}, observed ${masterDigest}`);
}

const masterMetadata = await sharp(masterPng).metadata();
if (masterMetadata.width !== 1254 || masterMetadata.height !== 1254 || !masterMetadata.hasAlpha) {
  throw new Error(`${MASTER_NAME} must remain the 1254x1254 transparent production master`);
}

async function renderSquare(source, size, destination) {
  await sharp(source, { density: 384 })
    .resize(size, size, { fit: 'fill' })
    .png({ compressionLevel: 9, adaptiveFiltering: true })
    .toFile(destination);
}

function iconField(size) {
  return Buffer.from(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${size} ${size}" width="${size}" height="${size}">
    <defs>
      <radialGradient id="field" cx="50%" cy="43%" r="72%">
        <stop offset="0" stop-color="#171044"/>
        <stop offset="0.52" stop-color="#08051b"/>
        <stop offset="1" stop-color="#000000"/>
      </radialGradient>
    </defs>
    <rect width="${size}" height="${size}" fill="url(#field)"/>
  </svg>`);
}

async function iconSurface(size, artScale) {
  const artSize = Math.max(1, Math.round(size * artScale));
  const artwork = await sharp(masterPng)
    .resize(artSize, artSize, { fit: 'contain', kernel: sharp.kernel.lanczos3 })
    .png()
    .toBuffer();
  let pipeline = sharp(iconField(size)).composite([{ input: artwork, gravity: 'centre' }]);
  if (size <= 48) pipeline = pipeline.sharpen();
  return pipeline.png({ compressionLevel: 9, adaptiveFiltering: true }).toBuffer();
}

async function renderIconSurface(size, artScale, destination) {
  await writeFile(destination, await iconSurface(size, artScale));
}

async function writeIco(sizes, destination) {
  const images = await Promise.all(sizes.map((size) => iconSurface(size, size <= 48 ? 1 : 0.94)));
  const directory = Buffer.alloc(6 + (16 * images.length));
  directory.writeUInt16LE(0, 0);
  directory.writeUInt16LE(1, 2);
  directory.writeUInt16LE(images.length, 4);
  let offset = directory.length;
  images.forEach((image, index) => {
    const entry = 6 + (index * 16);
    const size = sizes[index];
    directory[entry] = size === 256 ? 0 : size;
    directory[entry + 1] = size === 256 ? 0 : size;
    directory[entry + 2] = 0;
    directory[entry + 3] = 0;
    directory.writeUInt16LE(1, entry + 4);
    directory.writeUInt16LE(32, entry + 6);
    directory.writeUInt32LE(image.length, entry + 8);
    directory.writeUInt32LE(offset, entry + 12);
    offset += image.length;
  });
  await writeFile(destination, Buffer.concat([directory, ...images]));
}

async function renderSocialCard(destination) {
  const artwork = await sharp(masterPng)
    .resize(510, 510, { fit: 'contain', kernel: sharp.kernel.lanczos3 })
    .png()
    .toBuffer();
  const field = Buffer.from(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 630" width="1200" height="630">
    <defs>
      <radialGradient id="field" cx="28%" cy="48%" r="74%">
        <stop offset="0" stop-color="#171044"/><stop offset="0.52" stop-color="#08051b"/><stop offset="1" stop-color="#000000"/>
      </radialGradient>
      <linearGradient id="word" x1="585" y1="210" x2="1055" y2="390" gradientUnits="userSpaceOnUse">
        <stop stop-color="#f8faff"/><stop offset="0.55" stop-color="#93c5fd"/><stop offset="1" stop-color="#d8b4fe"/>
      </linearGradient>
    </defs>
    <rect width="1200" height="630" fill="url(#field)"/>
    <rect x="36" y="36" width="1128" height="558" rx="64" fill="none" stroke="#312e81" stroke-width="3"/>
    <text x="585" y="284" fill="url(#word)" font-family="Inter, Arial, sans-serif" font-size="92" font-weight="800" letter-spacing="11">APOCKY</text>
    <text x="591" y="340" fill="#b7c0e8" font-family="Inter, Arial, sans-serif" font-size="19" font-weight="600" letter-spacing="1.4">A LIVING CONSTELLATION OF IDEAS + TOOLS</text>
    <path d="M591 389H1050" stroke="#6366f1" stroke-width="4" stroke-linecap="round"/>
    <text x="591" y="444" fill="#8993c1" font-family="Inter, Arial, sans-serif" font-size="23">Conversation, memory, creation, divination.</text>
  </svg>`);
  await sharp(field)
    .composite([{ input: artwork, left: 36, top: 60 }])
    .png({ compressionLevel: 9, adaptiveFiltering: true })
    .toFile(destination);
}

function shortcutSvg(glyph) {
  return Buffer.from(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96" fill="none">
    <defs>
      <radialGradient id="field" cx="48" cy="43" r="54" gradientUnits="userSpaceOnUse">
        <stop stop-color="#171044"/><stop offset="1" stop-color="#000000"/>
      </radialGradient>
      <linearGradient id="spectrum" x1="20" y1="18" x2="78" y2="80" gradientUnits="userSpaceOnUse">
        <stop stop-color="#38BDF8"/><stop offset=".5" stop-color="#6366F1"/><stop offset="1" stop-color="#A855F7"/>
      </linearGradient>
    </defs>
    <path d="M0 0H96V96H0Z" fill="url(#field)"/>
    <path d="M9 26C9 16.6 16.6 9 26 9H70C79.4 9 87 16.6 87 26V70C87 79.4 79.4 87 70 87H26C16.6 87 9 79.4 9 70Z" stroke="#312E81" stroke-width="2"/>
    ${glyph}
  </svg>`);
}

const shortcuts = {
  atlas: '<path d="M24 65L35 32L50 57L64 25L73 65M29 55L66 51" stroke="url(#spectrum)" stroke-width="5" stroke-linecap="round" stroke-linejoin="round"/><path d="M35 26L41 32L35 38L29 32ZM64 19L70 25L64 31L58 25ZM24 59L30 65L24 71L18 65ZM73 59L79 65L73 71L67 65Z" fill="#F8FAFF"/>',
  clearing: '<path d="M27 68C19 58 18 42 25 31C32 20 46 16 58 20M69 29C78 39 79 55 72 66C65 77 51 81 39 77" stroke="url(#spectrum)" stroke-width="6" stroke-linecap="round"/><path d="M48 31L64 48L48 65L32 48Z" fill="#000" stroke="#F8FAFF" stroke-width="4"/><path d="M43 48H53" stroke="#38BDF8" stroke-width="4" stroke-linecap="round"/>',
  memory: '<path d="M48 18L69 30L48 42L27 30ZM48 36L69 48L48 60L27 48ZM48 54L69 66L48 78L27 66Z" stroke="url(#spectrum)" stroke-width="4" stroke-linejoin="round"/><path d="M48 27L53 30L48 33L43 30ZM48 63L53 66L48 69L43 66Z" fill="#F8FAFF"/>',
  divination: '<path d="M16 48C25 31 36 23 48 23C60 23 71 31 80 48C71 65 60 73 48 73C36 73 25 65 16 48Z" stroke="url(#spectrum)" stroke-width="5" stroke-linejoin="round"/><path d="M48 31L54 42L65 48L54 54L48 65L42 54L31 48L42 42Z" fill="#F8FAFF"/><path d="M48 42L54 48L48 54L42 48Z" fill="#6366F1"/>',
  oracle: '<path d="M23 28C30 20 40 17 48 17C61 17 72 25 77 36M73 68C66 76 56 79 48 79C35 79 24 71 19 60" stroke="url(#spectrum)" stroke-width="5" stroke-linecap="round"/><path d="M24 48H72" stroke="#312E81" stroke-width="3"/><path d="M34 36L47 48L34 60M62 36L49 48L62 60" stroke="#F8FAFF" stroke-width="4" stroke-linecap="round" stroke-linejoin="round"/>',
  spellcraft: '<circle cx="48" cy="48" r="27" stroke="url(#spectrum)" stroke-width="4"/><path d="M48 18V78M18 48H78M27 27L69 69M69 27L27 69" stroke="#6366F1" stroke-width="2" opacity=".7"/><path d="M48 29L54 42L68 48L54 54L48 68L42 54L28 48L42 42Z" fill="#09061A" stroke="#F8FAFF" stroke-width="4"/>',
};

await Promise.all([
  renderIconSurface(16, 1, join(iconDir, 'apocky-v3-16.png')),
  renderIconSurface(32, 1, join(iconDir, 'apocky-v3-32.png')),
  renderIconSurface(192, 0.94, join(iconDir, 'apocky-v3-192.png')),
  renderIconSurface(512, 0.94, join(iconDir, 'apocky-v3-512.png')),
  // Maskable/PWA and Apple masks keep the meaningful alpha bounds inside the
  // central 80% safe region. The field deliberately fills the whole canvas.
  renderIconSurface(192, 0.84, join(iconDir, 'apocky-maskable-v3-192.png')),
  renderIconSurface(512, 0.84, join(iconDir, 'apocky-maskable-v3-512.png')),
  renderIconSurface(152, 0.84, join(publicDir, 'apple-touch-icon-152x152.png')),
  renderIconSurface(167, 0.84, join(publicDir, 'apple-touch-icon-167x167.png')),
  renderIconSurface(180, 0.84, join(publicDir, 'apple-touch-icon.png')),
  ...Object.entries(shortcuts).map(([name, glyph]) =>
    renderSquare(shortcutSvg(glyph), 96, join(iconDir, `shortcut-${name}-v2-96.png`))),
  renderSocialCard(join(ogDir, 'apocky-default-v3.png')),
  writeIco([16, 32, 48, 256], join(publicDir, 'favicon.ico')),
]);

console.log(`Generated Apocky v3 icon family from SHA-256 ${MASTER_SHA256}.`);
