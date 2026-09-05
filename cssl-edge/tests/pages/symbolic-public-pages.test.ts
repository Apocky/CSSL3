import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { getPublicSurfaceNode } from '../../lib/public-surface-graph';

const root = process.cwd();
const read = (relative: string): string => fs.readFileSync(path.join(root, relative), 'utf8');

const oraclePage = read('pages/oracle.tsx');
const spellcraftPage = read('pages/spellcraft.tsx');
const sigilsPage = read('pages/sigils.tsx');
const spellbookPage = read('pages/spellbook.tsx');
const oracle = read('components/oracle/YesNoOracle.tsx');
const composer = read('components/spellcraft/SpellComposer.tsx');
const sigils = read('components/spellcraft/SigilStudio.tsx');
const spellbook = read('components/spellcraft/SpellbookPanel.tsx');
const local = read('lib/spellbook-local.ts');
const atlas = read('components/atlas/ConstellationAtlas.tsx');
const graph = read('lib/public-surface-graph.ts');

for (const [name, source] of [
  ['oracle', oraclePage],
  ['spellcraft', spellcraftPage],
  ['sigils', sigilsPage],
  ['spellbook', spellbookPage],
] as const) {
  assert.match(source, new RegExp(`rel="canonical" href="https:\\/\\/www\\.apocky\\.com\\/${name}"`));
  assert.match(source, /name="description"/);
}

assert.match(oraclePage, /WebApplication/);
assert.match(spellcraftPage, /WebApplication/);
assert.match(sigilsPage, /WebApplication/);
assert.match(spellbookPage, /WebApplication/);
assert.match(oracle, /Reveal yes \/ no/);
assert.match(oracle, /Nothing is uploaded/);
assert.match(oracle, /ORACLE_HIGH_STAKES_BLOCKED/);
assert.match(spellbook, /aria-label="Import verified Spellbook JSON"/);
assert.match(oracle, /Reproduction details/);
assert.match(oracle, /apocky-oracle-result/);
assert.match(composer, /Saved only when you choose/);
assert.match(composer, /SYMBOLIC_UNKNOWN_QUARANTINED/);
assert.match(composer, /executable: no/);
assert.match(sigils, /data:image\/svg\+xml/);
assert.doesNotMatch(sigils, /dangerouslySetInnerHTML/);
assert.match(sigils, /Download image/);
assert.match(sigils, /How it works/);
assert.match(spellbook, /Import a spellbook/);
assert.match(spellbook, /Export all/);
assert.match(spellbook, /Delete all/);
assert.match(local, /apocky\.symbolic-spellbook\.v1/);

const publicInteractiveSource = `${oracle}\n${composer}\n${sigils}\n${spellbook}`;
assert.doesNotMatch(publicInteractiveSource, /fetch\(|sendBeacon|XMLHttpRequest/);

for (const id of ['spellcraft', 'sigils', 'spellbook']) {
  assert.match(graph, new RegExp(`id: '${id}'`));
  assert.match(graph, new RegExp(`href: '\\/${id}'`));
}
const oracleDestination = getPublicSurfaceNode('oracle');
assert.equal(oracleDestination?.href, 'https://chaos-tarot.com/yes-no');
assert.equal(oracleDestination?.external, true);
assert.equal(oracleDestination?.availability, 'account_required');
assert.match(atlas, /id: 'matrix'/);
assert.match(atlas, /Public destinations by kind and access state/);
assert.match(atlas, /Find something useful/);
assert.match(atlas, /useState<AtlasView>\('index'\)/);

// eslint-disable-next-line no-console
console.log('symbolic-public-pages.test : OK');
