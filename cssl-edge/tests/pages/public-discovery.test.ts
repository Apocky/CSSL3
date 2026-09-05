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
const homePage = read('pages/index.tsx');
const siteShell = read('components/SiteShell.tsx');
const apocryphaPage = read('pages/apocrypha.tsx');

const homePanels = renderSiteDirectory();
const panels = [...homePanels.matchAll(/<article\b([^>]*\bdata-destination="([^"]+)"[^>]*)>([\s\S]*?)<\/article>/g)]
  .map((match) => ({ attributes: match[1]!, id: match[2]!, body: match[3]! }));
const publicDestinations = PUBLIC_SURFACE_NODES.filter((node) => node.id !== 'home');
const sortedIds = (nodes: ReadonlyArray<{ id: string }>) => nodes.map((node) => node.id).sort();
const escapeHtml = (value: string) => value.replace(/[&<>"']/g, (character) => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#x27;',
})[character]!);
assert.match(homePage, /<SiteDirectory\s*\/>/, 'home renders the shared destination panels');
assert.deepEqual(sortedIds(DIRECTORY_NODES), sortedIds(publicDestinations), 'every non-home public destination enters the home directory');
assert.deepEqual(sortedIds(findDirectoryItems('')), sortedIds(publicDestinations), 'default search includes every destination');
assert.deepEqual(sortedIds(panels), sortedIds(publicDestinations), 'actual default rendering gives every destination exactly one full panel');
assert.equal(new Set(panels.map((panel) => panel.id)).size, panels.length, 'panels must not duplicate destinations');
assert.deepEqual(panels.slice(0, 4).map((panel) => panel.id), ['codex-apockalypsis', 'apocrypha', 'atlas', 'chaos-tarot'], 'the four requested destinations lead the page');
assert.doesNotMatch(homePanels, /<details\b|\shidden(?:=|\s|>)|More of Apocky/i, 'default panels must not sit behind a disclosure or hidden state');
for (const node of publicDestinations) {
  const panel = panels.find((item) => item.id === node.id)!;
  assert.match(panel.attributes, /class="destinationPanel"/, `${node.id} uses the same full panel presentation`);
  assert.doesNotMatch(panel.attributes, /aria-hidden="true"|style="[^"]*(?:display:\s*none|visibility:\s*hidden)/, `${node.id} remains exposed`);
  assert.match(panel.body, /<h3>[^<]+<\/h3>/, `${node.id} has a visible destination heading`);
  assert.ok(panel.body.includes(`<p class="panelDescription">${escapeHtml(node.summary)}</p>`), `${node.id} explains its use`);
  assert.ok(panel.body.includes(`href="${escapeHtml(node.href)}"`), `${node.id} links directly to its registered destination`);
  assert.match(panel.body, /<a class="panelBody"[^>]*>/, `${node.id} exposes the full panel as an action`);
  assert.match(panel.body, /class="panelAction">[^<]+/, `${node.id} names the action`);
  assert.ok(DIRECTORY_GROUPS.includes(directoryGroup(node)), `${node.id} belongs to a rendered group`);
  assert.ok(findDirectoryItems(node.id).some((item) => item.id === node.id), `${node.id} remains searchable`);
  if (node.external) {
    assert.match(panel.body, /target="_blank" rel="noopener noreferrer"/, `${node.id} safely opens its external destination`);
    assert.match(panel.body, /Opens another website in a new tab\./, `${node.id} announces the external handoff`);
  }
}
assert.equal(findDirectoryItems('no-such-destination-7f849e').length, 0, 'a missing query produces the empty state');
assert.ok(homePanels.includes('href="/codex-apockalypsis/library/novel-volume-01-01-before-anyone-asked"'), 'Codex opening chapter is one direct action from home');

const ajv = new Ajv2020({ allErrors: true, strict: true });
addFormats(ajv);
const validate = ajv.compile(schema);
assert.equal(validate(manifest), true, JSON.stringify(validate.errors, null, 2));

assert.equal(manifest['declared_release_state'], 'public');
assert.equal(pwa['start_url'], '/', 'public PWA must not enter the owner-only admin shell');
assert.equal('apocrypha' in manifest, false, 'retired service must not have a discovery object');
assert.equal(exists('public/apocrypha-manifest.json'), false, 'retired manifest alias must not ship');

assert.match(apocryphaPage, /BrainExperience/, 'exact /apocrypha retains the owner conversation');
assert.match(apocryphaPage, /requireBrainOwner/, 'owner conversation selection retains authorization');
assert.match(apocryphaPage, /AccountChat/, 'other accounts retain their existing account conversation');
assert.doesNotMatch(apocryphaPage, /PublicChat|ClearingRoom/, 'exact /apocrypha must not revive the retired public chat');
for (const retiredPage of ['pages/apoc.tsx', 'pages/apx.tsx', 'pages/chat.tsx']) {
  assert.equal(exists(retiredPage), false, `${retiredPage} must not be built as a page`);
}

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
assert.match(homePanels, /href="\/apocrypha"/, 'rendered home links the exact existing account conversation route');
assert.match(siteShell, /href:\s*'\/apocrypha'/, 'shell links the exact existing account conversation route');
assert.doesNotMatch(`${homePage}\n${homePanels}\n${siteShell}`, /href=["']\/apocrypha\//, 'public navigation must not revive an Apocrypha descendant');
assert.doesNotMatch(homePanels, /href="\/(?:apoc|apx|chat)(?:[?"/])|href="\/(?:admin|api|content|shawn)(?:[?"/])/, 'home panels preserve route retirement and private publication boundaries');

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
assert.match(sitemap, /https:\/\/www\.apocky\.com\/download/);
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

assert.equal(vercel.crons?.some((cron) => /apocrypha/i.test(cron.path)), false, 'retired worker must not be scheduled');
assert.equal(
  Object.keys(vercel.functions ?? {}).some((route) => /apocrypha/i.test(route) && route !== 'pages/api/admin/apocrypha/inspect.ts'),
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
assert.match(homePanels, /href="\/codex-apockalypsis"/, 'Codex is directly reachable from the rendered hub');
assert.match(siteShell, /href: '\/codex-apockalypsis'/, 'Codex is directly reachable from global navigation');
assert.match(atlasGraph, /"href": "\/codex-apockalypsis"/, 'Codex is in the shared searchable directory');
assert.match(atlasGraph, /href: '\/akashic-records'/, 'Atlas must expose the same-origin works archive');
assert.match(atlasGraph, /href: '\/clearing'/, 'Atlas must expose the public social room');
assert.doesNotMatch(`${atlasPage}\n${atlasComponent}\n${atlasGraph}`, /(?:from|import\()[^\n]*\/shawn/i);

const clearingHeaders = vercel.headers?.find((entry) => entry.source === '/clearing')?.headers ?? [];
assert.ok(clearingHeaders.some((header) => header.key === 'Cache-Control' && header.value.includes('no-store')));
assert.ok(clearingHeaders.some((header) => header.key === 'X-Served-By' && header.value === 'apocky-clearing'));

console.log(`public discovery: ${panels.length} full home panels, direct links, registry coverage, privacy and retirement gates passed`);
