import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';

import Ajv2020 from 'ajv/dist/2020';
import addFormats from 'ajv-formats';

const root = process.cwd();
const read = (relative: string) => fs.readFileSync(path.join(root, relative), 'utf8');

const manifestSource = read('public/.well-known/apocky.json');
const rootManifestSource = read('public/apocrypha-manifest.json');
const manifest = JSON.parse(manifestSource) as Record<string, unknown>;
const rootManifest = JSON.parse(rootManifestSource) as Record<string, unknown>;
const schema = JSON.parse(read('public/schemas/site-manifest.v1.json')) as Record<string, unknown>;
const pwa = JSON.parse(read('public/manifest.json')) as Record<string, unknown>;
const llms = read('public/llms.txt');
const robots = read('public/robots.txt');
const sitemap = read('public/sitemap.xml');
const nextConfig = read('next.config.js');
const vercel = JSON.parse(read('vercel.json')) as {
  rewrites?: Array<{ source: string; destination: string }>;
  headers?: Array<{ source: string; headers: Array<{ key: string; value: string }> }>;
};
const clearingPage = read('pages/clearing.tsx');
const clearingRoom = read('components/clearing/ClearingRoom.tsx');
const atlasPage = read('public/commons/atlas.html');
const membershipPage = read('public/commons/membership.html');
const principlesPage = read('public/commons/principles.html');

const ajv = new Ajv2020({ allErrors: true, strict: true });
addFormats(ajv);
const validate = ajv.compile(schema);
assert.equal(validate(manifest), true, JSON.stringify(validate.errors, null, 2));
assert.deepEqual(rootManifest, manifest, 'root manifest alias must remain byte-semantic equivalent to the canonical well-known manifest');
assert.equal(rootManifestSource, manifestSource, 'root manifest alias must remain byte-for-byte equivalent to the canonical well-known manifest');

assert.equal(manifest['declared_release_state'], 'public');
assert.equal(pwa['start_url'], '/', 'public PWA must not enter the owner-only admin shell');
assert.match(llms, /does not\s+define or classify them/i);
assert.match(llms, /content is voluntary/i);
assert.match(llms, /live social room/i);
assert.match(llms, /https:\/\/www\.apocky\.com\/clearing/);
assert.doesNotMatch(llms, /authored interaction sample/i);
assert.doesNotMatch(llms, /conversation doorway|persistent digital intelligence|sovereign creative systems/i);
assert.match(JSON.stringify(manifest['entry_points']), /words_and_symbols/);
assert.match(JSON.stringify(manifest['entry_points']), /game_download/);
assert.match(JSON.stringify(manifest['entry_points']), /"rel":"optional_support","href":"\/buy"/);
assert.match(JSON.stringify(manifest['entry_points']), /works_archive/);
assert.match(JSON.stringify(manifest['entry_points']), /conversations_archive/);
assert.match(JSON.stringify(manifest['entry_points']), /"rel":"writing","href":"\/akashic-records"/);
assert.match(JSON.stringify(manifest['entry_points']), /"href":"\/akashic-records"/);
assert.match(JSON.stringify(manifest['entry_points']), /"rel":"works_archive_manifest","href":"\/akashic-records\/manifest\.json"/);
assert.match(llms, /https:\/\/www\.apocky\.com\/akashic-records\/manifest\.json/);
assert.match(llms, /public-safe Codex conversation projections/i);
assert.match(llms, /https:\/\/www\.apocky\.com\/buy/);
assert.match(JSON.stringify(manifest['entry_points']), /public_social_room/);
assert.match(JSON.stringify(manifest['entry_points']), /"href":"\/clearing"/);
assert.match(JSON.stringify(manifest['entry_points']), /apocrypha_conversation/);
assert.match(JSON.stringify(manifest['entry_points']), /"href":"\/apocrypha"/);
assert.match(JSON.stringify(manifest['entry_points']), /without_training_consent/);
assert.match(JSON.stringify(manifest['entry_points']), /design_study_not_enrollment/);
assert.doesNotMatch(JSON.stringify(manifest['entry_points']), /conversation_doorway|\/login|\/register/);
assert.doesNotMatch(
  JSON.stringify(manifest),
  /\/chat|\/api\/apocrypha\/presence|operator_surfaces/,
  'private conversation and operator surfaces must not be published in the discovery manifest',
);
assert.match(
  JSON.stringify((manifest['apocrypha'] as { claims: Record<string, string> }).claims),
  /public_conversation_service.*\/apocrypha.*no training consent/,
);
assert.match(llms, /signed-in text interface/i);
assert.match(llms, /does\s+not treat a send action as training consent/i);
assert.match(robots, /Disallow: \/admin\//);
assert.match(robots, /Disallow: \/api\//);
assert.match(robots, /Allow: \/clearing/);
assert.match(sitemap, /https:\/\/www\.apocky\.com\//);
assert.match(sitemap, /https:\/\/www\.apocky\.com\/apocrypha/);
assert.match(sitemap, /https:\/\/www\.apocky\.com\/clearing/);
assert.match(sitemap, /https:\/\/www\.apocky\.com\/words/);
assert.match(sitemap, /https:\/\/www\.apocky\.com\/download/);
assert.match(sitemap, /https:\/\/www\.apocky\.com\/buy/);
assert.match(sitemap, /https:\/\/www\.apocky\.com\/akashic-records/);
assert.doesNotMatch(sitemap, /\/admin|\/api|\/account|\/login|\/register|\/chat/);
assert.doesNotMatch(sitemap, /\/content/, 'unavailable shared-content routes must not be advertised');

const expectedStaticRewrites = [
  { source: '/atlas', destination: '/commons/atlas.html' },
  { source: '/membership', destination: '/commons/membership.html' },
  { source: '/principles', destination: '/commons/principles.html' },
];
assert.deepEqual(vercel.rewrites, expectedStaticRewrites, 'Vercel may retain only the three intentional static reference pages');
assert.doesNotMatch(nextConfig, /\{\s*source:\s*'\/',\s*destination:\s*'\/commons\/index\.html'/);
assert.doesNotMatch(nextConfig, /\{\s*source:\s*'\/commons',\s*destination:\s*'\/commons\/index\.html'/);
assert.match(nextConfig, /\{\s*source:\s*'\/commons',\s*destination:\s*'\/',\s*permanent:\s*true\s*\}/);
assert.match(clearingPage, /canonical" href="https:\/\/www\.apocky\.com\/clearing"/);
assert.match(clearingPage, /pathname:\s*CLEARING_PATH/);
assert.doesNotMatch(clearingPage, /GetServerSideProps|destination:\s*`\/apocrypha/);
assert.match(clearingRoom, /Sign in to join the room/);
assert.doesNotMatch(clearingRoom, /onUpload|onMic|onHeadset|onCamera|Microphone unavailable|Camera unavailable/);
assert.doesNotMatch(membershipPage, /data-prototype-action|Preview a Member seat|Preview the covenant step/);
assert.match(membershipPage, /disabled>Enrollment not open/);
for (const supportPage of [atlasPage, membershipPage, principlesPage]) {
  assert.doesNotMatch(supportPage, /href="\/apocrypha">(?:The )?Clearing/);
  assert.match(supportPage, /href="\/clearing">(?:The )?Clearing/);
}
assert.match(atlasPage, /Seven doors\./, 'Atlas must describe its full seven-door map');
assert.match(atlasPage, /href="\/akashic-records"/, 'Atlas must expose the same-origin works archive');

const clearingHeaders = vercel.headers?.find((entry) => entry.source === '/clearing')?.headers ?? [];
assert.ok(clearingHeaders.some((header) => header.key === 'Cache-Control' && header.value.includes('no-store')));
assert.ok(clearingHeaders.some((header) => header.key === 'X-Served-By' && header.value === 'apocky-clearing'));

console.log('public route map, discovery manifests, schema, PWA, robots, sitemap, and plain-language guidance agree');
