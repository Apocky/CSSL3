import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';

const root = process.cwd();
const read = (relative: string): string => fs.readFileSync(path.join(root, relative), 'utf8');
const digest = (relative: string): string => createHash('sha256').update(fs.readFileSync(path.join(root, relative))).digest('hex').toUpperCase();

const page = read('pages/showcase.tsx');
const player = read('components/showcase/ShowcaseVideo.tsx');
const styles = read('styles/Showcase.module.css');
const graph = read('lib/public-surface-graph.ts');
const sitemap = read('public/sitemap.xml');
const manifest = read('public/.well-known/apocky.json');
const llms = read('public/llms.txt');
const vtt = read('public/showcase/promo-apocky-chaos-23s-en-v1.vtt');
const transcript = read('public/showcase/promo-apocky-chaos-23s-transcript-v1.txt');

assert.match(page, /rel="canonical" href="https:\/\/www\.apocky\.com\/showcase"/);
assert.match(page, /VideoObject/);
assert.match(page, /duration: 'PT23S'/);
assert.match(page, /max-video-preview:-1/);
assert.match(page, /Illustrative concept art, not product photography/);
assert.match(page, /href="\/atlas"/);
assert.match(page, /href="https:\/\/chaos-tarot\.com\/yes-no"/);
assert.match(page, /requires a separate sign-in/, 'external reading access must be explained before the handoff');
assert.doesNotMatch(page, /private, device-local symbolic signal/, 'the external reading must not be described as a local tool');
assert.match(page, /https:\/\/chaos-tarot\.com\/free-reading\?source=apocky-showcase/);
assert.match(page, /href="\/membership"/);
assert.doesNotMatch(page, /buy (?:the )?deck|pre-?order|limited time|guaranteed/i);

const openingVideoTag = player.match(/<video[\s\S]*?>/)?.[0] ?? '';
assert.doesNotMatch(openingVideoTag, /\scontrols(?:\s|>)/);
assert.match(player, /playsInline/);
assert.match(player, /preload="metadata"/);
assert.match(player, /kind="captions"/);
assert.match(player, /promo-apocky-chaos-23s-en-v1\.vtt/);
assert.match(player, /matchMedia\(PORTRAIT_QUERY\)/);
assert.match(player, /aria-label="Video controls"/);
assert.match(player, /aria-label=\{playing \? 'Pause video' : 'Play video'\}/);
assert.match(player, /webkitEnterFullscreen/);
assert.doesNotMatch(player, /autoPlay|autoplay/);
assert.doesNotMatch(player, /fetch\(|sendBeacon|XMLHttpRequest/);

assert.match(styles, /env\(safe-area-inset-left\)/);
assert.match(styles, /env\(safe-area-inset-bottom\)/);
assert.match(styles, /min-height: 44px/);
assert.match(styles, /@media \(max-width: 660px\)/);
assert.match(styles, /@media \(prefers-reduced-motion: reduce\)/);

assert.match(graph, /id: 'showcase'/);
assert.match(graph, /href: '\/showcase'/);
assert.match(sitemap, /https:\/\/www\.apocky\.com\/showcase/);
assert.match(manifest, /"rel": "media_showcase"/);
assert.match(manifest, /"href": "\/showcase"/);
assert.match(llms, /Connected-worlds video showcase/);
assert.match(vtt, /^WEBVTT/);
assert.match(transcript, /Persistent disclosure: Illustrative concept art/);

const expectedHashes: Record<string, string> = {
  'public/showcase/promo-apocky-chaos-landscape-23s-v1.mp4': 'D631F52D806FF2C30802E06DDFFB89E03BC0CF6722C5355157A2DCB605C64927',
  'public/showcase/promo-apocky-chaos-vertical-23s-v1.mp4': 'BE035C0927DD395A445152EC1AA8E6F910DC0366206FBF04404EB5D90A2674E5',
  'public/showcase/promo-apocky-chaos-landscape-cover-v1.png': '9D4E9EEED6CF2E14EF2ADB5CD2623C5A4631545731150A33F28547569F330F19',
  'public/showcase/promo-apocky-chaos-vertical-cover-v1.png': 'E1720A5C65A591DE16C98F8D3FDAE6C991C4449CE9C6FAAFB26E7727C54D52FB',
};

for (const [relative, expected] of Object.entries(expectedHashes)) {
  assert.equal(digest(relative), expected, `${relative} must remain byte-identical to the approved campaign asset`);
}

console.log('showcase page keeps its media, claims, iOS controls, discovery, and support handoffs explicit');
