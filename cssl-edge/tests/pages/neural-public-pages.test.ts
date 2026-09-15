import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import Ajv2020 from 'ajv/dist/2020';
import addFormats from 'ajv-formats';

import { PUBLIC_SURFACE_EDGES, PUBLIC_SURFACE_NODES } from '@/lib/public-surface-graph';

function read(path: string): string {
  return readFileSync(resolve(process.cwd(), path), 'utf8');
}

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

const home = read('pages/index.tsx');
const start = read('pages/start.tsx');
const membership = read('pages/membership.tsx');
const quests = read('pages/quests.tsx');
const status = read('pages/status.tsx');
const now = read('pages/now.tsx');
const labs = read('pages/labs.tsx');
const memoryTools = read('pages/memory-tools.tsx');
const divination = read('pages/divination.tsx');
const oracle = read('pages/oracle.tsx');
const spellcraft = read('pages/spellcraft.tsx');
const sigils = read('pages/sigils.tsx');
const spellbook = read('pages/spellbook.tsx');
const theory = read('pages/theory-of-everything.tsx');
const principles = read('pages/principles.tsx');
const shell = read('components/SiteShell.tsx');
const synapses = read('components/site/ContextualSynapses.tsx');
const command = read('components/site/CommandPalette.tsx');
const nextConfig = read('next.config.js');
const errorBoundary = read('lib/akashic-telemetry/error-boundary.tsx');
const sitemap = read('public/sitemap.xml');
const llms = read('public/llms.txt');

for (const [name, source, canonical] of [
  ['home', home, 'https://www.apocky.com/'],
  ['start', start, 'https://www.apocky.com/start'],
  ['membership', membership, 'https://www.apocky.com/membership'],
  ['quests', quests, 'https://www.apocky.com/quests'],
  ['status', status, 'https://www.apocky.com/status'],
  ['now', now, 'https://www.apocky.com/now'],
  ['labs', labs, 'https://www.apocky.com/labs'],
  ['memory-tools', memoryTools, 'https://www.apocky.com/memory-tools'],
  ['divination', divination, 'https://www.apocky.com/divination'],
  ['oracle', oracle, 'https://www.apocky.com/oracle'],
  ['spellcraft', spellcraft, 'https://www.apocky.com/spellcraft'],
  ['sigils', sigils, 'https://www.apocky.com/sigils'],
  ['spellbook', spellbook, 'https://www.apocky.com/spellbook'],
  ['theory', theory, 'https://www.apocky.com/theory-of-everything'],
  ['principles', principles, 'https://www.apocky.com/principles'],
] as const) {
  assert(source.includes(`rel="canonical" href="${canonical}"`), `${name} must self-canonicalize on the www origin`);
}

for (const route of ['/tools', '/codex-apockalypsis', '/start', '/divination', '/spellcraft', '/sigils', '/spellbook', '/theory-of-everything', '/principles', '/membership', '/quests', '/status', '/now', '/labs', '/memory-tools', '/infinity-engine']) {
  assert(sitemap.includes(`<loc>https://www.apocky.com${route}</loc>`), `sitemap missing ${route}`);
  assert(llms.includes(`https://www.apocky.com${route}`), `llms.txt missing ${route}`);
}
assert(llms.includes('https://chaos-tarot.com/yes-no') && llms.includes('external sign-in required'), 'discovery text must disclose the actual Oracle destination and sign-in requirement');

assert(!home.includes('const CREATIVE_WORK'), 'home must not duplicate the complete project index beneath its primary paths');
assert(!home.includes("access === 'owner'"), 'account chat entry must not be restricted to the owner');
assert(!home.includes('public relay remains closed'), 'home must not retain the superseded owner-only copy');
assert(!home.includes('runtime ready') && !home.includes('release verified'), 'home must not make unsupported runtime or release claims');
assert(home.includes('consumeAuthCallbackFromLocation'), 'home simplification must preserve auth callback consumption');
assert(home.includes('location.replace(returnTo)'), 'home simplification must preserve the normalized post-auth return path');
assert(membership.includes('https://chaos-tarot.com/pricing'), 'membership must expose the usable Chaos product path');
assert(membership.includes('SUPPORT_LINKS'), 'membership must reuse canonical support links');
assert(!membership.includes('Enrollment not open'), 'native membership must not retain the retired mock-enrollment state');

assert(quests.includes("apocky.public-quests.v1"), 'quests must use a versioned device-local state key');
assert(quests.includes('window.localStorage'), 'quests must persist progress locally');
assert(!quests.includes('fetch(') && !quests.includes('sendBeacon'), 'quest progress must not be transmitted');

assert(status.includes("fetch('/api/health'"), 'status page must probe the same-origin health route');
assert(status.includes('APX-STATUS-UNAVAILABLE'), 'status failures must carry a stable recovery code');
assert(status.includes('health API version'), 'status page must label the health contract version distinctly from product releases');
assert(!status.includes('public version'), 'status page must not present the health contract version as a product release');
assert(status.includes('Configuration flags mean a connection is present; they do not prove every user flow succeeds'), 'status page must state its evidence boundary');

assert(command.includes('aria-label="Search Apocky"'), 'mobile command trigger must retain an accessible name');
assert(shell.includes('<ContextualSynapses pathname={pathname} />'), 'global shell must expose contextual graph edges');
const primaryNav = shell.match(/const NAV:[\s\S]*?= \[([\s\S]*?)\];/)?.[1] ?? '';
// Was pinned at exactly 5 when the shell fronted five different projects. The site is Apocrypha
// now, so the ceiling is what matters, not the count: a nav that grows back into a directory is
// the regression worth catching.
assert((primaryNav.match(/href:/g) ?? []).length <= 3, 'global shell must stay focused: at most three primary destinations');
// The nav used to front five projects. It now fronts one, so the assertion inverts: the four other
// labels must be GONE, and the ways into Apocrypha must be present.
for (const gone of ['Tools', 'Words', 'Thoughts', 'Codex']) {
  assert(!primaryNav.includes(gone), `primary navigation still advertises ${gone}`);
}
for (const kept of ['/apocrypha', '/download/apocrypha']) {
  assert(primaryNav.includes(kept), `primary navigation missing the Apocrypha surface ${kept}`);
}
assert(!primaryNav.includes('/start'), 'home now owns orientation; Start must not remain a primary destination');
assert(!shell.includes("href: '/oracle'"), 'global navigation and footer must not advertise the retired local Oracle');
assert(!shell.includes('const WORK') && !shell.includes('const ABOUT'), 'footer must not restore the two long link inventories');
assert(synapses.includes("pathname === '/'"), 'contextual synapses must stay hidden on the home page');
assert(synapses.includes("pathname === '/download/apocrypha'"), 'native app download must not inherit Labyrinth links');
assert(synapses.includes("relation.neighbor.id !== 'oracle'"), 'contextual synapses must not re-advertise the retired local Oracle');
assert(synapses.includes('.slice(0, 2)'), 'contextual synapses must expose at most two related destinations');
assert(nextConfig.includes("source: '/oracle', destination: 'https://chaos-tarot.com/yes-no?source=apocky-oracle', permanent: true"), 'legacy Oracle requests must permanently hand off to Chaos Tarot');
assert(!nextConfig.includes("source: '/auth/callback'"), 'the Oracle handoff must not intercept the auth callback route');
assert(!shell.includes('https://cssl.dev'), 'global shell must not send visitors to the observed-unavailable CSSL host');

assert(errorBoundary.includes('publicErrorCode'), 'render failures must derive a stable public error code');
assert(!errorBoundary.includes('{err.message}'), 'default public fallback must not render raw exception messages');

assert(divination.includes('Cross-system synthesis') && divination.includes('without pretending they are interchangeable'), 'divination guide must preserve the distinctions between symbolic traditions');
assert(divination.includes('FAQPage'), 'divination guide must include structured FAQ data');
assert(theory.includes('rather than a proven physical theory'), 'Theory of Everything guide must preserve the evidence boundary');
assert(theory.includes('FAQPage'), 'Theory of Everything guide must include structured FAQ data');
assert(principles.includes('Four commitments') && (principles.match(/title: '/g) ?? []).length === 4, 'principles must preserve all four commitments');
assert(principles.includes('Equivalent interface views'), 'principles must expose the graph-equivalence visual aid');
assert(now.includes('Still unwired.'), 'current-state ledger must name capabilities that are not connected yet');
assert(labs.includes('“Experimental” can mean missing data, changing behavior, or an unavailable dependency'), 'labs must explain the limits of experimental availability');
assert(memoryTools.includes('Private stays private.'), 'memory directory must preserve the private memory boundary');
assert(memoryTools.includes('Saved work in this browser stays on this device'), 'memory directory must name local persistence semantics');
assert(!memoryTools.includes('/api/mneme/'), 'public memory directory must not expose an unbrokered Mneme endpoint');

const manifest = JSON.parse(read('public/.well-known/apocky.json')) as Record<string, unknown>;
const manifestSchema = JSON.parse(read('public/schemas/site-manifest.v1.json')) as Record<string, unknown>;
const ajv = new Ajv2020({ allErrors: true, strict: false });
addFormats(ajv);
const validate = ajv.compile(manifestSchema);
assert(validate(manifest), `site manifest must validate: ${ajv.errorsText(validate.errors)}`);

const entryPoints = manifest.entry_points as Array<{ href?: string }>;
for (const href of ['/tools', '/codex-apockalypsis', '/start', '/divination', 'https://chaos-tarot.com/yes-no?source=apocky-oracle', '/spellcraft', '/sigils', '/spellbook', '/theory-of-everything', '/membership', '/quests', '/status', '/now', '/labs', '/memory-tools', '/docs', '/infinity-engine', 'https://chaos-tarot.com/']) {
  assert(entryPoints.some((entry) => entry.href === href), `site manifest missing ${href}`);
}
assert(!entryPoints.some((entry) => entry.href?.startsWith('https://cssl.dev')), 'site manifest must use the local CSSL recovery rail while cssl.dev is unavailable');

const constellation = JSON.parse(read('public/constellation.json')) as {
  nodes: Array<{ id: string }>;
  edges: Array<{ source: string; target: string }>;
};
const constellationSchema = JSON.parse(read('public/schemas/public-surface.v1.json')) as Record<string, unknown>;
const validateConstellation = ajv.compile(constellationSchema);
assert(validateConstellation(constellation), `public constellation must validate: ${ajv.errorsText(validateConstellation.errors)}`);
assert(constellation.nodes.length === PUBLIC_SURFACE_NODES.length, 'public constellation must project every source node');
assert(constellation.edges.length === PUBLIC_SURFACE_EDGES.length, 'public constellation must project every source edge');
assert(new Set(constellation.nodes.map((node) => node.id)).size === constellation.nodes.length, 'public constellation node IDs must be unique');
const constellationIds = new Set(constellation.nodes.map((node) => node.id));
assert(constellation.edges.every((edge) => constellationIds.has(edge.source) && constellationIds.has(edge.target)), 'every public constellation edge must resolve to published nodes');

const sitemapUrls = [...sitemap.matchAll(/<loc>([^<]+)<\/loc>/g)].map((match) => match[1]);
assert(new Set(sitemapUrls).size === sitemapUrls.length, 'sitemap URLs must remain unique');

// eslint-disable-next-line no-console
console.log('neural-public-pages.test : OK');
