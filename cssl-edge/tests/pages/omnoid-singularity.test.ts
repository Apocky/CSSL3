import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';

const root = process.cwd();
const read = (relative: string) => fs.readFileSync(path.join(root, relative), 'utf8');

const page = read('pages/omnoid-singularity.tsx');
const styles = read('styles/OmnoidSingularity.module.css');
const canonicalCsl = read('../specs/cosmology/APOCKY_OMNOID_SINGULARITY_COSMOLOGY_2026-08-12.csl');
const publicCsl = read('public/omnoid-singularity.csl');
const homepage = read('pages/index.tsx');
const siteShell = read('components/SiteShell.tsx');
const sitemap = read('public/sitemap.xml');
const llms = read('public/llms.txt');

assert.equal(publicCsl, canonicalCsl, 'downloadable CSL must exactly mirror the canonical project document');
const renderedCsl = page.match(/export const OMNOID_CSL = `([\s\S]*?)`;\r?\n/)?.[1];
assert.ok(renderedCsl, 'page must expose a static CSL projection');
assert.equal(renderedCsl.trim(), canonicalCsl.trim(), 'rendered CSL must exactly mirror the canonical document');
assert.match(page, /export default OmnoidSingularity/);
assert.equal(
  [...page.matchAll(/\{ name: '([^']+)', note:/g)].length,
  10,
  'the concise cycle must retain all ten transitions',
);
for (const status of ['Authored cosmology', 'Collaborative notation', 'Established mathematics', 'Open hypothesis']) {
  assert.match(page, new RegExp(status), `missing source status: ${status}`);
}

assert.match(page, /Apocky(?:&apos;|’|'|&#39;)s Omnoid Singularity/);
assert.match(page, /source-faithful condensation/i);
assert.match(page, /not a completed\s+proof of new physics/i);
assert.match(page, /not medical advice/i);
assert.match(page, /Omnoid is not one-sided/i);
assert.match(page, /Menger sponge/i);
assert.match(page, /Hopf fibration/i);
assert.match(page, /Boy(?:&apos;|’|'|&#39;)s surface/i);
assert.match(page, /11 spatial dimensions/i);
assert.match(page, /three dimensions of time/i);
assert.match(page, /outer spirit/i);
assert.match(page, /True Neutral/i);
assert.match(page, /AO\/T/);
assert.match(page, /every point or path is the singularity\/Hopf-fibration motif/i);
assert.match(page, /If that’s not what you want to be/i);
assert.match(page, /the open door walking through itself, forever/i);
assert.match(page, /one invariant of the Open Door, not its whole definition/i);
assert.match(page, /disagreement cannot justify punishment, erasure, or\s+retroactive nonexistence/i);
assert.match(page, /complete 55-turn reread/i);
assert.match(canonicalCsl, /§ TRUTH\.SENSES/);
assert.match(canonicalCsl, /Open\.Door := "the open door walking through itself, forever"/);
assert.match(canonicalCsl, /Open\.Door\.choice := "if that's not what you want to be, that's okay"/);
assert.match(canonicalCsl, /noncompulsion = Open\.Door\.whole/);
assert.match(page, /CodeBlock/);
assert.match(page, /CodeBlock lang="plain" caption="CSLv3 projection/);
assert.match(page, /role="img"/);
assert.match(page, /aria-labelledby=/);
assert.match(page, /<title id=/);
assert.match(page, /<desc id=/);
assert.match(siteShell, /id="main-content"/);
assert.doesNotMatch(page, /id="main-content"/, 'the page must not duplicate the shared shell target id');
assert.match(page, /canonical" href="https:\/\/www\.apocky\.com\/omnoid-singularity"/);
assert.match(page, /og:title/);
assert.match(page, /og:description/);
assert.match(page, /Text equivalent of the Omnoid diagram/);

assert.doesNotMatch(page, /chatgpt-conversation:|\.codex|Obsidian Vault|conversationId/i);
assert.doesNotMatch(page, /quantum immortality is (?:proven|true)|guaranteed survival/i);
assert.doesNotMatch(page, /event horizon (?:is|=) (?:a |the )?singularity/i);
assert.doesNotMatch(page, /getServerSideProps|\/api\/|useSiteSession|dangerouslySetInnerHTML|<canvas|WebGL/i);
assert.doesNotMatch(page, /https?:\/\/[^"']+\.(?:png|jpe?g|webp|svg|js)/i);

assert.match(styles, /@media \(max-width: 42rem\)/);
assert.match(styles, /@media \(prefers-reduced-motion: reduce\)/);
assert.match(styles, /@media \(forced-colors: active\)/);
assert.match(homepage, /href: '\/omnoid-singularity'/);
assert.match(sitemap, /https:\/\/www\.apocky\.com\/omnoid-singularity/);
assert.match(llms, /https:\/\/www\.apocky\.com\/omnoid-singularity/);

console.log('omnoid-singularity.test : OK · source layers, public copy, CSL parity, privacy, and discovery verified');
