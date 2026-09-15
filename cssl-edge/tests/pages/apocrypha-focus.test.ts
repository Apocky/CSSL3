// The site is Apocrypha. This gate is what keeps it that way.
//
// Owner instruction, 2026-09-15, verbatim: "JUST FOCUS THE ENTIRE SITE AROUND APOCRYPHA AND
// IMPROVING APOCRYPHA, REMOVE LINKS TO ANYTHING ELSE."
//
// The old suite asserted the OPPOSITE invariant -- that every public destination appeared on the
// home page. Those assertions were not weakened or deleted quietly; they were inverted here, and
// the half that is still true (nothing was DESTROYED, only unlinked) is asserted below, because
// "we removed the links" and "we lost the pages" are very different outcomes and only one of them
// was asked for.

import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { PUBLIC_SURFACE_NODES } from '../../lib/public-surface-graph';

const read = (rel: string) => fs.readFileSync(path.join(process.cwd(), rel), 'utf8');
const exists = (rel: string) => fs.existsSync(path.join(process.cwd(), rel));

const home = read('pages/index.tsx');
const shell = read('components/SiteShell.tsx');
const returnLinks = read('components/nav/ReturnLinks.tsx');
const chat = read('components/apocrypha/ApocryphaChat.tsx');

// Every internal path the visitor-facing chrome is allowed to advertise.
// LEGAL and AUTH are here on purpose and are NOT "anything else":
//   - an unlinked privacy policy or terms page is a compliance failure, not a focused site
//   - sign-in is how a conversation follows you between devices, which is an Apocrypha feature
//   - /status answers "is it down, or is it me" about THIS service
const ALLOWED = [
  '/',
  '/apocrypha',
  '/download/apocrypha',
  '/account',
  '/login',
  '/register',
  '/work',              // owner-gated, rendered only for the owner lane
  '/legal/privacy',
  '/legal/terms',
  '/status',
  '/llms.txt',
];

// TWO forms, and missing the second made this gate vacuous on its first run: JSX writes
// href="/x", but the NAV / EXPLORE / SITE arrays declare links as { href: '/x', label: ... }.
// A gate that only saw the first form let a re-added Codex link straight through.
function internalHrefs(source: string): string[] {
  const found: string[] = [];
  for (const pattern of [new RegExp('href="([^"]+)"', 'g'), new RegExp("href: '([^']+)'", 'g')]) {
    let hit = pattern.exec(source);
    while (hit !== null) {
      const raw = hit[1]!;
      if (raw.startsWith('/')) found.push(raw.split('?')[0]!.split('#')[0]!);
      hit = pattern.exec(source);
    }
  }
  return found;
}

const chrome = { home, shell, returnLinks, chat };
for (const [name, source] of Object.entries(chrome)) {
  for (const href of internalHrefs(source)) {
    assert.ok(
      ALLOWED.includes(href),
      `${name} links to "${href}", which is not an Apocrypha surface. If this is deliberate, add it to ALLOWED with a reason.`,
    );
  }
}

// The specific things that used to lead the page, named individually so a regression reads clearly
// instead of as a generic allowlist miss.
for (const gone of ['/tools', '/words', '/conversations', '/codex-apockalypsis', '/atlas', '/akashic-records', '/clearing', '/membership', '/docs', '/start']) {
  for (const [name, source] of Object.entries(chrome)) {
    assert.ok(!source.includes(`href="${gone}"`), `${name} still advertises ${gone}`);
    assert.ok(!source.includes(`href: '${gone}'`), `${name} still advertises ${gone} in a link array`);
  }
}

// The home page is no longer an index.
assert.ok(!home.includes('<SiteDirectory'), 'home must not render the destination directory');
assert.ok(!exists('components/site/SiteDirectory.tsx'), 'the orphaned directory component must not linger');
assert.match(home, /href="\/apocrypha"/, 'home must offer a direct way into the conversation');

// ...and the front door still does the one job it had before: finishing a sign-in that lands here.
assert.ok(home.includes('consumeAuthCallbackFromLocation'), 'home must preserve auth callback consumption');
assert.ok(home.includes('location.replace(returnTo)'), 'home must preserve the normalized post-auth return');

// UNLINKED IS NOT DELETED. Every public destination must still answer on its own URL -- this is the
// half of the instruction that protects the work rather than removing it.
const routeFile = (href: string) => {
  const clean = href.split('?')[0]!.split('#')[0]!.replace(/^\//, '');
  if (!clean) return 'pages/index.tsx';
  for (const candidate of [`pages/${clean}.tsx`, `pages/${clean}/index.tsx`, `public/${clean}/index.html`, `public/${clean}.html`]) {
    if (exists(candidate)) return candidate;
  }
  return null;
};
const stranded: string[] = [];
for (const node of PUBLIC_SURFACE_NODES) {
  if (node.external || node.id === 'home') continue;
  if (routeFile(node.href) === null) stranded.push(`${node.id} (${node.href})`);
}
assert.deepEqual(stranded, [], `these destinations lost their page, which was never asked for: ${stranded.join(', ')}`);

console.log(`apocrypha-focus.test : OK - chrome links only Apocrypha + legal + auth; ${PUBLIC_SURFACE_NODES.length - 1} destinations unlinked but intact`);
