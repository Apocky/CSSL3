import assert from 'node:assert/strict';
import { ACTIONS, INTENT_PRESETS, THEMES, buildIntent, selectedIntent } from '../../components/spellcraft/intent-presets';
import { analyzeSpell, createSigil, createSpellbookEntry, parseSpellbook, serializeSpellbook } from '../../lib/spellcraft';

let checked = 0;
for (const preset of INTENT_PRESETS) {
  const analysis = analyzeSpell(preset.source);
  assert.equal(analysis.status, 'valid', preset.label);
  if (analysis.status !== 'valid') throw new Error(preset.label);
  assert.equal(analysis.compiled.executable, false);
  assert.equal(analysis.receipt.authority, 'none');
  assert.equal(selectedIntent(` ${preset.source} `)?.label, preset.label);
  assert.ok(preset.prompt.endsWith('?'));
  const artwork = createSigil(analysis, { variant: 0 });
  assert.equal(artwork.svg, createSigil(analysis, { variant: 0 }).svg);
  assert.notEqual(artwork.svg, createSigil(analysis, { variant: 7 }).svg);
  const entry = createSpellbookEntry(analysis, { label: preset.label });
  const restored = parseSpellbook(serializeSpellbook([entry]));
  assert.equal(restored.valid, true);
  assert.equal(restored.payload?.entries[0]?.input, preset.source);
  checked++;
}
for (const theme of THEMES) for (const action of ACTIONS) {
  const source = buildIntent(action.value, theme.lexeme);
  const analysis = analyzeSpell(source);
  assert.equal(analysis.status, 'valid', `${action.label} ${theme.meaning}`);
  if (analysis.status !== 'valid') throw new Error(source);
  assert.equal(analysis.compiled.executable, false);
  assert.ok(analysis.interpretation.trace.some(item => item.gloss.includes(theme.meaning)), `${source} preserves selected meaning`);
  checked++;
}
assert.equal(checked, 111);
assert.throws(() => buildIntent('protect', 'unlisted'), /Choose a listed meaning/);
assert.throws(() => buildIntent('invented-action', 'ken'), /Choose a listed action/);
assert.equal(selectedIntent('arbitrary prose'), undefined);
assert.equal(analyzeSpell('xqzzy').status, 'quarantined');
assert.equal(analyzeSpell('root:alg').status, 'valid');
assert.equal(analyzeSpell('alg').status, 'quarantined');
console.log(`intent-presets.test : OK (${checked} validated choices; source/art/storage boundaries)`);
