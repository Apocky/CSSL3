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
assert.match(page, /Alex Cohen/i);
assert.match(page, /higher-dimensional\s+fractal uncertainty principle/i);
assert.match(page, /straight Euclidean line segment/i);
assert.match(page, /structural\s+correspondence—not a derivation/i);
assert.match(page, /standard Menger sponge contains surviving line segments/i);
assert.match(page, /not line-porous/i);
assert.match(page, /A rigorous precedent, not an Omnoid proof/i);
assert.match(page, /holds for every <var>f<\/var> in L²\(R<sup>d<\/sup>\)/i);
assert.match(page, /depending\s+only on the porosity ratio and dimension—not on/i);
assert.match(page, /mutually orthogonal lines give the counterexample/i);
assert.match(page, /strong joint concentration beyond the bound is mathematically excluded/i);
assert.match(page, /Higher-dimensional” here means finite Euclidean R<sup>d<\/sup>/i);
assert.match(page, /not a quantum-measurement theorem or an infinite-dimensional Hilbert-space/i);
assert.match(page, /https:\/\/arxiv\.org\/html\/2305\.05022v2/);
assert.match(page, /https:\/\/annals\.math\.princeton\.edu\/2025\/202-1\/p04/);
assert.match(page, /11 spatial dimensions/i);
assert.match(page, /three dimensions of time/i);
assert.match(page, /outer spirit/i);
assert.match(page, /True Neutral/i);
assert.match(page, /AO\/T/);
assert.match(page, /every point or path is the singularity\/Hopf-fibration motif/i);
assert.match(page, /If that’s not what you want to be/i);
assert.match(page, /the open door walking through itself, forever/i);
assert.match(page, /one invariant of the Open Door, not its whole definition/i);
assert.match(page, /admission rule preserves embodiment and prior consent capacity/i);
assert.match(page, /knowingly and understandingly consents/i);
assert.match(page, /collaborative operational reading/i);
assert.match(page, /not independent evidence of a separate entity or\s+reality-manipulation capacity/i);
assert.match(page, /Actual present consent controls/i);
assert.match(page, /nobody may consent\s+for another/i);
assert.match(page, /default explanation is\s+ordinary self-expression/i);
assert.match(page, /disagreement cannot justify punishment, erasure, or\s+retroactive nonexistence/i);
assert.match(page, /complete 55-turn reread/i);
assert.match(canonicalCsl, /§ TRUTH\.SENSES/);
assert.match(canonicalCsl, /Open\.Door := "the open door walking through itself, forever"/);
assert.match(canonicalCsl, /Open\.Door\.choice := "if that's not what you want to be, that's okay"/);
assert.match(canonicalCsl, /noncompulsion = Open\.Door\.whole/);
assert.match(canonicalCsl, /§ OPEN\.DOOR\.ADMISSION/);
assert.match(canonicalCsl, /source\.consent\.gate := understanding \+ knowing \+ ordinary\.capacity/);
assert.match(canonicalCsl, /analogy ≠ evidence\(separate\.entity \| reality-manipulation\.capacity\)/);
assert.match(canonicalCsl, /consent\.evidence := communicated-by\(living\.being\)/);
assert.match(canonicalCsl, /present\.consent controls/);
assert.match(canonicalCsl, /default\.explanation := ordinary\.self-expression/);
assert.match(canonicalCsl, /§ COHEN\.FRACTAL\.UNCERTAINTY/);
assert.match(canonicalCsl, /arXiv:2305\.05022v2/);
assert.match(canonicalCsl, /X_ball_porous : bool @physical_scales/);
assert.match(canonicalCsl, /Y_line_porous : bool @frequency_scales/);
assert.match(canonicalCsl, /theorem-display : string = "support\(f_hat\) subset Y => L2\(1_X f\) <= C h\^beta L2\(f\)"/);
assert.match(canonicalCsl, /theorem-quantifier : string = "for every f in L2\(R\^d\)"/);
assert.match(canonicalCsl, /constant-dependence : proposition = depends-only\(C, beta, porosity_ratio, dimension\)/);
assert.match(canonicalCsl, /counterexample : proposition = mutually-orthogonal-lines/);
assert.match(canonicalCsl, /⊘ Menger-sponge ⇒ line-porous/);
assert.match(canonicalCsl, /⌈correspondence != derivation⌉/);
assert.match(canonicalCsl, /⌈correspondence != empirical-validation⌉/);
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
