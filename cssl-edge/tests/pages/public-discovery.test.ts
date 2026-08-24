import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';

import Ajv2020 from 'ajv/dist/2020';
import addFormats from 'ajv-formats';

const root = process.cwd();
const read = (relative: string) => fs.readFileSync(path.join(root, relative), 'utf8');
const exists = (relative: string) => fs.existsSync(path.join(root, relative));

const manifestSource = read('public/.well-known/apocky.json');
const manifest = JSON.parse(manifestSource) as Record<string, unknown>;
const schemaSource = read('public/schemas/site-manifest.v1.json');
const schema = JSON.parse(schemaSource) as Record<string, unknown>;
const pwa = JSON.parse(read('public/manifest.json')) as Record<string, unknown>;
const llms = read('public/llms.txt');
const robots = read('public/robots.txt');
const sitemap = read('public/sitemap.xml');
const nextConfig = read('next.config.js');
const contentPage = read('pages/content/index.tsx');
const vercel = JSON.parse(read('vercel.json')) as {
  rewrites?: Array<{ source: string; destination: string }>;
  headers?: Array<{ source: string; headers: Array<{ key: string; value: string }> }>;
  functions?: Record<string, unknown>;
  crons?: Array<{ path: string; schedule: string }>;
};
const clearingPage = read('pages/clearing.tsx');
const clearingRoom = read('components/clearing/ClearingRoom.tsx');
const atlasPage = read('public/commons/atlas.html');
const membershipPage = read('public/commons/membership.html');
const principlesPage = read('public/commons/principles.html');
const homePage = read('pages/index.tsx');
const siteShell = read('components/SiteShell.tsx');

const ajv = new Ajv2020({ allErrors: true, strict: true });
addFormats(ajv);
const validate = ajv.compile(schema);
assert.equal(validate(manifest), true, JSON.stringify(validate.errors, null, 2));

assert.equal(manifest['declared_release_state'], 'public');
assert.equal(pwa['start_url'], '/', 'public PWA must not enter the owner-only admin shell');
assert.equal('apocrypha' in manifest, false, 'retired service must not have a discovery object');
assert.equal(exists('public/apocrypha-manifest.json'), false, 'retired manifest alias must not ship');

for (const retiredPage of ['pages/apocrypha.tsx', 'pages/apoc.tsx', 'pages/apx.tsx', 'pages/chat.tsx']) {
  assert.equal(exists(retiredPage), false, `${retiredPage} must not be built as a page`);
}

const activePublicSurfaces: Record<string, string> = {
  homePage,
  siteShell,
  words: read('pages/words.tsx'),
  buy: read('pages/buy.tsx'),
  terms: read('pages/legal/terms.tsx'),
  docsChatPanel: read('pages/docs/chat-panel.tsx'),
  llms,
  manifestSource,
  schemaSource,
  sitemap,
  robots,
  atlasPage,
  membershipPage,
  staticClearing: read('public/commons/clearing.html'),
  staticHub: read('public/commons/index.html'),
  staticSiteScript: read('public/commons/assets/site.js'),
  staticRoomScript: read('public/commons/assets/room-v3.js'),
};
for (const [surface, source] of Object.entries(activePublicSurfaces)) {
  assert.doesNotMatch(source, /apocrypha/i, `${surface} must not advertise or name the retired service`);
  assert.doesNotMatch(source, /href=["']\/apoc(?:rypha)?(?:[?"'/])/i, `${surface} must not link a retired route`);
}

const entryPoints = JSON.stringify(manifest['entry_points']);
assert.match(entryPoints, /words_and_symbols/);
assert.match(entryPoints, /game_download/);
assert.match(entryPoints, /"rel":"optional_support","href":"\/buy"/);
assert.match(entryPoints, /works_archive/);
assert.match(entryPoints, /conversations_archive/);
assert.match(entryPoints, /"rel":"writing","href":"\/akashic-records"/);
assert.match(entryPoints, /"rel":"works_archive_manifest","href":"\/akashic-records\/manifest\.json"/);
assert.match(entryPoints, /public_social_room/);
assert.match(entryPoints, /"href":"\/clearing"/);
assert.match(entryPoints, /design_study_not_enrollment/);
assert.doesNotMatch(entryPoints, /conversation_doorway|\/login|\/register|\/chat/);
assert.match(llms, /live public social room/i);
assert.match(llms, /https:\/\/www\.apocky\.com\/clearing/);
assert.match(llms, /https:\/\/www\.apocky\.com\/akashic-records\/manifest\.json/);
assert.match(llms, /public-safe Codex conversation projections/i);
assert.match(llms, /https:\/\/www\.apocky\.com\/buy/);
assert.match(robots, /Disallow: \/admin\//);
assert.match(robots, /Disallow: \/api\//);
assert.match(robots, /Allow: \/clearing/);
assert.match(sitemap, /https:\/\/www\.apocky\.com\//);
assert.match(sitemap, /https:\/\/www\.apocky\.com\/omnoid-singularity/);
assert.match(sitemap, /https:\/\/www\.apocky\.com\/clearing/);
assert.match(sitemap, /https:\/\/www\.apocky\.com\/words/);
assert.match(sitemap, /https:\/\/www\.apocky\.com\/download/);
assert.match(sitemap, /https:\/\/www\.apocky\.com\/buy/);
assert.match(sitemap, /https:\/\/www\.apocky\.com\/akashic-records/);
assert.doesNotMatch(sitemap, /\/admin|\/api|\/account|\/login|\/register|\/chat|\/content/);
assert.match(homePage, /href: '\/omnoid-singularity'/);

const expectedStaticRewrites = [
  { source: '/atlas', destination: '/commons/atlas.html' },
  { source: '/membership', destination: '/commons/membership.html' },
  { source: '/principles', destination: '/commons/principles.html' },
];
assert.deepEqual(vercel.rewrites, expectedStaticRewrites, 'Vercel may retain only the three intentional static reference pages');
assert.doesNotMatch(nextConfig, /destination:\s*'\/commons\/index\.html'/);
assert.match(nextConfig, /\{\s*source:\s*'\/commons',\s*destination:\s*'\/',\s*permanent:\s*true\s*\}/);
assert.match(nextConfig, /\{\s*source:\s*'\/commons\/index\.html',\s*destination:\s*'\/',\s*permanent:\s*true\s*\}/);
assert.match(contentPage, /notFound:\s*true/);
assert.doesNotMatch(contentPage, /destination:\s*['"]\/apoc/);

assert.equal(vercel.crons?.some((cron) => /apocrypha/i.test(cron.path)), false, 'retired worker must not be scheduled');
assert.equal(
  Object.keys(vercel.functions ?? {}).some((route) => /apocrypha/i.test(route)),
  false,
  'retired routes must not receive dedicated function configuration',
);

assert.match(clearingPage, /canonical" href="https:\/\/www\.apocky\.com\/clearing"/);
assert.match(clearingPage, /pathname:\s*CLEARING_PATH/);
assert.doesNotMatch(clearingPage, /GetServerSideProps|destination:\s*`\/apocrypha/);
assert.match(clearingRoom, /Sign in to join the room/);
assert.doesNotMatch(clearingRoom, /onUpload|onMic|onHeadset|onCamera|Microphone unavailable|Camera unavailable/);
assert.doesNotMatch(membershipPage, /data-prototype-action|Preview a Member seat|Preview the covenant step/);
assert.match(membershipPage, /disabled>Enrollment not open/);
for (const supportPage of [atlasPage, membershipPage, principlesPage]) {
  assert.match(supportPage, /href="\/clearing">(?:The )?Clearing/);
}
assert.match(atlasPage, /Seven doors\./, 'Atlas must describe its full seven-door map');
assert.match(atlasPage, /href="\/akashic-records"/, 'Atlas must expose the same-origin works archive');

const clearingHeaders = vercel.headers?.find((entry) => entry.source === '/clearing')?.headers ?? [];
assert.ok(clearingHeaders.some((header) => header.key === 'Cache-Control' && header.value.includes('no-store')));
assert.ok(clearingHeaders.some((header) => header.key === 'X-Served-By' && header.value === 'apocky-clearing'));

console.log('public route map remains useful while retired service routes and discovery stay absent');
