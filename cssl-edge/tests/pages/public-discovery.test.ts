import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';

import Ajv2020 from 'ajv/dist/2020';
import addFormats from 'ajv-formats';
import { renderSiteDirectory } from '../helpers/render-site-directory';
import { PUBLIC_SURFACE_NODES } from '../../lib/public-surface-graph';
import { DIRECTORY_NODES, DIRECTORY_GROUPS, directoryGroup, findDirectoryItems } from '../../lib/site-directory';

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
const atlasPage = read('pages/atlas.tsx');
const atlasComponent = read('components/atlas/ConstellationAtlas.tsx');
const atlasGraph = read('lib/public-surface-graph.ts');
const atlasFallback = read('public/commons/atlas.html');
const membershipPage = read('pages/membership.tsx');
const membershipFallback = read('public/commons/membership.html');
const principlesPage = read('pages/principles.tsx');
const principlesFallback = read('public/commons/principles.html');
const frontDoor = read('pages/index.tsx');
const homePage = read('pages/hub.tsx');
const siteShell = read('components/SiteShell.tsx');
const roomComponent = read('components/room/Room.tsx');
const homePanels = renderSiteDirectory();

const publicDestinations = PUBLIC_SURFACE_NODES.filter((node) => node.id !== 'home');
const sortedIds = (nodes: ReadonlyArray<{ id: string }>) => nodes.map((node) => node.id).sort();
const escapeHtml = (value: string) => value.replace(/[&<>"']/g, (character) => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#x27;',
})[character]!);
assert.match(frontDoor, /<Room\b/, 'the front door (/) is the living room (owner decisions 2026-09-24/25)');
assert.match(frontDoor, /consumeAuthCallbackFromLocation/, 'the front door still consumes the OAuth callback that lands on /');
assert.match(roomComponent, /rel="canonical" href="https:\/\/www\.apocky\.com\/"/, 'the room canonicalizes on the front door whether served at / or /room');
assert.match(homePage, /<SiteDirectory\s*\/>/, 'the hub (/hub) renders the shared destination panels');
assert.deepEqual(sortedIds(DIRECTORY_NODES), sortedIds(publicDestinations), 'every non-home public destination enters the home directory');
assert.deepEqual(sortedIds(findDirectoryItems('')), sortedIds(publicDestinations), 'default search includes every destination');
// The home page no longer renders a directory -- that invariant moved to
// tests/pages/apocrypha-focus.test.ts, which asserts the inverse and also asserts that every one of
// these destinations still answers on its own URL. What remains here is per-node registry health,
// which is true whether or not anything links to them.
for (const node of publicDestinations) {
  assert.ok(DIRECTORY_GROUPS.includes(directoryGroup(node)), `${node.id} belongs to a rendered group`);
  assert.ok(findDirectoryItems(node.id).some((item) => item.id === node.id), `${node.id} remains searchable`);
}
assert.equal(findDirectoryItems('no-such-destination-7f849e').length, 0, 'a missing query produces the empty state');

const ajv = new Ajv2020({ allErrors: true, strict: true });
addFormats(ajv);
const validate = ajv.compile(schema);
assert.equal(validate(manifest), true, JSON.stringify(validate.errors, null, 2));

assert.equal(manifest['declared_release_state'], 'public');
assert.equal(pwa['start_url'], '/', 'public PWA must not enter the owner-only admin shell');
assert.equal('apocrypha' in manifest, false, 'retired service must not have a discovery object');
assert.equal(exists('public/apocrypha-manifest.json'), false, 'retired manifest alias must not ship');

// 2026-09-20, Apocky: "The entire flow is too complicated for now just exclude sign-in."
// The public room is the guest lane for every reader. The owner rail was not deleted --
// it lives on /admin/apocrypha and tests/pages/admin-chat.test.ts still holds it there.
// The bubble chat is retired (2026-09-24): the living room is the only chat surface, and the old
// addresses are permanent redirects into it rather than pages of their own.
assert.equal(exists('pages/apocrypha.tsx'), false, 'no second chat page at /apocrypha');
assert.equal(exists('pages/chat.tsx'), false, 'no second chat page at /chat');
assert.match(nextConfig, /source: '\/apocrypha', destination: '\/room', permanent: true/, '/apocrypha redirects into the room');
assert.match(nextConfig, /source: '\/chat', destination: '\/room', permanent: true/, '/chat redirects into the room');

const activePublicSurfaces: Record<string, string> = {
  words: read('pages/words.tsx'),
  start: read('pages/start.tsx'),
  showcase: read('pages/showcase.tsx'),
  quests: read('pages/quests.tsx'),
  status: read('pages/status.tsx'),
  divination: read('pages/divination.tsx'),
  theoryOfEverything: read('pages/theory-of-everything.tsx'),
  buy: read('pages/buy.tsx'),
  terms: read('pages/legal/terms.tsx'),
  docsChatPanel: read('pages/docs/chat-panel.tsx'),
  llms,
  manifestSource,
  schemaSource,
  sitemap,
  robots,
  atlasPage,
  atlasComponent,
  atlasGraph,
  atlasFallback,
  membershipPage,
  membershipFallback,
  staticHub: read('public/commons/index.html'),
  staticSiteScript: read('public/commons/assets/site.js'),
  staticRoomScript: read('public/commons/assets/room-v3.js'),
};
for (const [surface, source] of Object.entries(activePublicSurfaces)) {
  assert.doesNotMatch(source, /(?:href=["']|href:\s*["'])\/(?:apoc|apx|chat)(?:[?"'/])|(?:href=["']|href:\s*["'])\/apocrypha\//i, `${surface} must not link retired routes or descendants`);
}
assert.match(siteShell, /href:\s*'\/apocrypha'/, 'shell links the exact existing account conversation route');
assert.doesNotMatch(`${frontDoor}\n${homePage}\n${homePanels}\n${siteShell}`, /href=["']\/apocrypha\//, 'public navigation must not revive an Apocrypha descendant');
assert.doesNotMatch(homePanels, /href="\/(?:apoc|apx|chat)(?:[?"/])|href="\/(?:admin|api|content|shawn)(?:[?"/])/, 'home panels preserve route retirement and private publication boundaries');

const entryPoints = JSON.stringify(manifest['entry_points']);
assert.match(entryPoints, /words_and_symbols/);
// The Labyrinth alpha is withdrawn from the site (Apocky, 2026-09-15): /download is gone, the
// signed ZIP is gone, and every surface that advertised it has been stripped. Asserting the
// opposite — which this did — is what kept "See the game" pointing at a donation page.
assert.doesNotMatch(entryPoints, /game_download/, 'the withdrawn alpha must not be advertised');
assert.doesNotMatch(entryPoints, /"href":"\/download"/, 'nothing may point at the withdrawn download route');
assert.match(entryPoints, /"rel":"optional_support","href":"\/buy"/);
assert.match(entryPoints, /works_archive/);
assert.match(entryPoints, /conversations_archive/);
assert.match(entryPoints, /"rel":"writing","href":"\/akashic-records"/);
assert.match(entryPoints, /"rel":"works_archive_manifest","href":"\/akashic-records\/manifest\.json"/);
assert.match(entryPoints, /public_social_room/);
assert.match(entryPoints, /"href":"\/clearing"/);
assert.match(entryPoints, /"rel":"membership_and_support","href":"\/membership"/);
assert.match(entryPoints, /"rel":"orientation","href":"\/start"/);
assert.match(entryPoints, /"rel":"media_showcase","href":"\/showcase"/);
assert.match(entryPoints, /"rel":"public_quests","href":"\/quests"/);
assert.match(entryPoints, /"rel":"public_status","href":"\/status"/);
assert.match(entryPoints, /"rel":"divination_guide","href":"\/divination"/);
assert.match(entryPoints, /"rel":"theory_of_everything_guide","href":"\/theory-of-everything"/);
assert.match(entryPoints, /"rel":"language","href":"\/docs\/cssl-language"/);
assert.match(entryPoints, /"rel":"notation","href":"\/words#symbols"/);
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
assert.match(sitemap, /https:\/\/www\.apocky\.com\/divination/);
assert.match(sitemap, /https:\/\/www\.apocky\.com\/oracle/);
assert.match(sitemap, /https:\/\/www\.apocky\.com\/showcase/);
assert.match(sitemap, /https:\/\/www\.apocky\.com\/spellcraft/);
assert.match(sitemap, /https:\/\/www\.apocky\.com\/sigils/);
assert.match(sitemap, /https:\/\/www\.apocky\.com\/spellbook/);
assert.match(sitemap, /https:\/\/www\.apocky\.com\/theory-of-everything/);
assert.match(sitemap, /https:\/\/www\.apocky\.com\/clearing/);
assert.match(sitemap, /https:\/\/www\.apocky\.com\/words/);
// /download is withdrawn, so the sitemap must not advertise it. /download/apocrypha is a different,
// live page — asserted separately so this cannot pass on a prefix match.
assert.doesNotMatch(sitemap, /https:\/\/www\.apocky\.com\/download<\/loc>/, 'the withdrawn route must not be in the sitemap');
assert.match(sitemap, /https:\/\/www\.apocky\.com\/buy/);
assert.match(sitemap, /https:\/\/www\.apocky\.com\/akashic-records/);
assert.doesNotMatch(sitemap, /\/admin|\/api|\/account|\/login|\/register|\/chat|\/content/);
assert.match(atlasGraph, /href: '\/omnoid-singularity'/, 'specialized worlds must remain discoverable through the Atlas after home consolidation');

assert.deepEqual(vercel.rewrites ?? [], [], 'native public pages must not be shadowed by Vercel rewrites');
assert.doesNotMatch(nextConfig, /source:\s*'\/atlas'[^\n]*destination:\s*'\/commons\/atlas\.html'/);
assert.doesNotMatch(nextConfig, /source:\s*'\/membership'[^\n]*destination:\s*'\/commons\/membership\.html'/);
assert.doesNotMatch(nextConfig, /source:\s*'\/principles'[^\n]*destination:\s*'\/commons\/principles\.html'/);
assert.equal(exists('pages/atlas.tsx'), true, 'Atlas must resolve through the native React page');
assert.equal(exists('pages/membership.tsx'), true, 'membership must resolve through the native React page');
assert.equal(exists('pages/principles.tsx'), true, 'principles must resolve through the native React page');
assert.equal(exists('public/commons/atlas.html'), true, 'the prior static Atlas must remain available as a rollback artifact');
assert.equal(exists('public/commons/membership.html'), true, 'the prior static membership study must remain available as a rollback artifact');
assert.match(principlesFallback, /Four invariants/, 'the prior static principles page must remain available as a rollback artifact');
assert.doesNotMatch(nextConfig, /destination:\s*'\/commons\/index\.html'/);
assert.match(nextConfig, /\{\s*source:\s*'\/commons',\s*destination:\s*'\/',\s*permanent:\s*true\s*\}/);
assert.match(nextConfig, /\{\s*source:\s*'\/commons\/index\.html',\s*destination:\s*'\/',\s*permanent:\s*true\s*\}/);
for (const [legacy, destination] of [['clearing', '/clearing'], ['atlas', '/atlas'], ['membership', '/membership'], ['principles', '/principles']]) {
  assert.ok(nextConfig.includes(`source: '/commons/${legacy}.html', destination: '${destination}'`), 'preserved legacy artifacts redirect to current functional pages');
}
assert.match(nextConfig, /\{\s*source:\s*'\/oracle',\s*destination:\s*'https:\/\/chaos-tarot\.com\/yes-no\?source=apocky-oracle',\s*permanent:\s*true\s*\}/);
assert.doesNotMatch(nextConfig, /source:\s*'\/auth\/callback'/, 'public redirects must preserve the authentication callback');
assert.match(contentPage, /notFound:\s*true/);
assert.doesNotMatch(contentPage, /destination:\s*['"]\/apoc/);

assert.deepEqual((vercel.crons ?? []).filter((cron) => /apocrypha/i.test(cron.path)).map((cron) => cron.path), ['/api/cron/apocrypha-runner'], 'the only scheduled Apocrypha job is the flagship runner sweep; the retired worker stays unscheduled');
assert.equal(
  Object.keys(vercel.functions ?? {}).some((route) => /apocrypha/i.test(route) && !['pages/api/admin/apocrypha/inspect.ts', 'pages/api/apocrypha/runner/run.ts'].includes(route)),
  false,
  'retired routes must not receive dedicated function configuration',
);

assert.match(clearingPage, /canonical" href="https:\/\/www\.apocky\.com\/clearing"/);
assert.match(clearingPage, /pathname:\s*CLEARING_PATH/);
assert.doesNotMatch(clearingPage, /GetServerSideProps|destination:\s*`\/apocrypha/);
assert.match(clearingRoom, /Sign in to join the room/);
assert.doesNotMatch(clearingRoom, /onUpload|onMic|onHeadset|onCamera|Microphone unavailable|Camera unavailable/);
assert.match(membershipPage, /Membership and support/);
assert.match(membershipPage, /SUPPORT_LINKS/);
assert.doesNotMatch(membershipPage, /data-prototype-action|Preview a Member seat|Preview the covenant step/);
assert.match(principlesPage, /href="\/clearing"/);
assert.match(principlesPage, /href="\/clearing"/);
assert.match(membershipPage, /href="\/clearing"/);
assert.match(atlasPage, /canonical" href="https:\/\/www\.apocky\.com\/atlas"/);
assert.match(atlasComponent, /Find something useful/);
assert.match(atlasComponent, /Map/);
assert.match(atlasComponent, /label: 'Compare'/);
assert.match(atlasComponent, /label: 'Directory'/);
assert.match(atlasComponent, /label: 'Definitions'/);
// The site is Apocrypha (owner instruction 2026-09-15), so global navigation no longer carries
// Codex. The page is untouched and still answers on its URL -- asserted in apocrypha-focus.test.ts.
assert.doesNotMatch(siteShell, /href: '\/codex-apockalypsis'/, 'global navigation must no longer advertise Codex');
assert.match(atlasGraph, /"href": "\/codex-apockalypsis"/, 'Codex is in the shared searchable directory');
assert.match(atlasGraph, /href: '\/akashic-records'/, 'Atlas must expose the same-origin works archive');
assert.match(atlasGraph, /href: '\/clearing'/, 'Atlas must expose the public social room');
assert.doesNotMatch(`${atlasPage}\n${atlasComponent}\n${atlasGraph}`, /(?:from|import\()[^\n]*\/shawn/i);

const clearingHeaders = vercel.headers?.find((entry) => entry.source === '/clearing')?.headers ?? [];
assert.ok(clearingHeaders.some((header) => header.key === 'Cache-Control' && header.value.includes('no-store')));
assert.ok(clearingHeaders.some((header) => header.key === 'X-Served-By' && header.value === 'apocky-clearing'));

