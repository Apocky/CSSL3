import assert from 'node:assert/strict';

import {
  ALLOWED_SYMBOLIC_OPERATIONS,
  DEFAULT_SPELL_LIMITS,
  ENGINE_VERSION,
  HALOIC_VOCAB,
  VOCABULARY_HASH,
  VOCABULARY_ID,
  VOCAB_VERSION,
  analyzeSpell,
  compileSpell,
  createSigil,
  createSpellbookEntry,
  parseSpellbook,
  serializeSpellbook,
  sha256Hex,
  stableStringify,
  validateSpellProgram,
  verifySpellbookEntry,
  type SpellbookEntry,
  type ValidSpellAnalysis,
} from '../../lib/spellcraft';

assert.equal(
  sha256Hex('abc'),
  'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad',
  'portable SHA-256 must match its standard test vector',
);
assert.match(VOCABULARY_HASH, /^[a-f0-9]{64}$/);
assert.equal(new Set(HALOIC_VOCAB.map((entry) => entry.id)).size, HALOIC_VOCAB.length);
assert.ok(HALOIC_VOCAB.every((entry) => entry.id.startsWith(`${VOCABULARY_ID}:${entry.namespace}:`)));
assert.deepEqual(
  HALOIC_VOCAB.filter((entry) => entry.lexeme === 'alg').map((entry) => entry.namespace).sort(),
  ['root', 'verb'],
  'surface collisions remain explicit instead of silently overwriting one another',
);

const first = analyzeSpell('ka-sol-el');
const repeated = analyzeSpell('ka-sol-el');
assert.equal(first.status, 'valid');
assert.equal(repeated.status, 'valid');
assert.equal(stableStringify(first), stableStringify(repeated), 'analysis must be deterministic');
assert.equal(first.provenance.engineVersion, ENGINE_VERSION);
assert.equal(first.provenance.vocabularyVersion, VOCAB_VERSION);
assert.equal(first.provenance.vocabularyHash, VOCABULARY_HASH);
assert.equal(first.provenance.source, 'independently implemented, owner-authorized Haloic-derived symbolic model');
assert.equal(first.receipt.authority, 'none');
assert.equal(first.receipt.verdict, 'symbolic-valid');
assert.match(first.receipt.programHash, /^[a-f0-9]{64}$/);
assert.match(first.receipt.compiledHash, /^[a-f0-9]{64}$/);
assert.equal(first.compiled.executable, false);
assert.match(first.interpretation.disclaimer, /does not execute effects/i);
assert.ok(Object.isFrozen(first));
assert.ok(first.compiled.operations.length > 0);
for (const node of first.compiled.operations) {
  assert.ok(ALLOWED_SYMBOLIC_OPERATIONS.includes(node.operation));
  const nodeIndex = Number(node.id.slice('node-'.length));
  for (const input of node.inputs) {
    assert.ok(Number(input.slice('node-'.length)) < nodeIndex, 'compiled graph inputs must point backward and stay acyclic');
  }
}

const explicit = analyzeSpell('root:on');
assert.equal(explicit.status, 'valid');
if (explicit.status === 'valid') {
  assert.equal(explicit.program.form, 'word');
  const term = explicit.program.body.terms[0];
  assert.equal(term?.morphemes[0]?.id, `${VOCABULARY_ID}:root:on`);
  assert.equal(explicit.confidence, 1);
}

const conditional = analyzeSpell('ki shan verb:rad, ya sol um verb:alg.');
assert.equal(conditional.status, 'valid');
if (conditional.status === 'valid') {
  assert.equal(conditional.program.form, 'conditional');
  assert.equal(conditional.compiled.operations.at(-1)?.operation, 'symbolic.branch');
  assert.match(conditional.interpretation.text, /^If /);
  assert.match(conditional.interpretation.text, /reflect on/i);
}

for (const collision of ['alg', 'rad', 'on']) {
  const result = analyzeSpell(collision);
  assert.equal(result.status, 'quarantined', `${collision} must not compile through a namespace collision`);
  assert.ok(result.issues.some((issue) => issue.code === 'AMBIGUOUS_LEXEME'));
  assert.equal('compiled' in result, false);
  assert.equal('receipt' in result, false);
}

for (const unknown of ['unwritten', 'kasolel']) {
  const result = analyzeSpell(unknown);
  assert.equal(result.status, 'quarantined', `${unknown} must not receive speculative segmentation`);
  assert.ok(result.issues.some((issue) => issue.code === 'UNKNOWN_LEXEME'));
  assert.equal('compiled' in result, false);
}

const malformedConditional = analyzeSpell('ki shan root:rad');
assert.equal(malformedConditional.status, 'rejected');
assert.ok(malformedConditional.issues.some((issue) => issue.code === 'INVALID_CONDITIONAL'));

const invalidMorphology = analyzeSpell('suffix:el-root:sol');
assert.equal(invalidMorphology.status, 'rejected');
assert.ok(invalidMorphology.issues.some((issue) => issue.code === 'INVALID_MORPHOLOGY'));

const punctuation = analyzeSpell('sol, um');
assert.equal(punctuation.status, 'rejected');
assert.ok(punctuation.issues.some((issue) => issue.code === 'INVALID_PUNCTUATION'));

const markup = analyzeSpell('<script>sol</script>');
assert.equal(markup.status, 'rejected');
assert.ok(markup.issues.some((issue) => issue.code === 'UNSUPPORTED_CHARACTER'));

const coercive = analyzeSpell('control another person without consent');
assert.equal(coercive.status, 'rejected');
assert.ok(coercive.issues.some((issue) => issue.code === 'DISALLOWED_INTENT'));

const overLimit = analyzeSpell('sol'.repeat(200), { limits: { maxInputChars: Number.MAX_SAFE_INTEGER } });
assert.equal(overLimit.status, 'rejected', 'callers cannot raise the hard input ceiling');
assert.ok(overLimit.issues.some((issue) => issue.code === 'INPUT_LIMIT'));
assert.equal(DEFAULT_SPELL_LIMITS.maxInputChars, 512);

const valid = first as ValidSpellAnalysis;
const sigil = createSigil(valid);
const sameSigil = createSigil(valid);
const alternateSigil = createSigil(valid, { variant: 1 });
assert.equal(sigil.svg, sameSigil.svg);
assert.equal(sigil.seedHash, sameSigil.seedHash);
assert.notEqual(sigil.seedHash, alternateSigil.seedHash);
assert.notEqual(sigil.svg, alternateSigil.svg);
assert.match(sigil.svg, /^<svg /);
assert.doesNotMatch(sigil.svg, /ka-sol-el/i, 'source text must not be embedded as hidden sigil data');
assert.doesNotMatch(sigil.svg, /<script|<metadata|data:/i);
assert.match(sigil.semantics.disclosure, /not a hidden message/i);
assert.throws(() => createSigil(valid, { variant: -1 }), RangeError);
assert.throws(() => createSigil(valid, { variant: 65_536 }), RangeError);

const entry = createSpellbookEntry(valid, {
  label: 'Light becoming',
  savedAt: '2026-09-03T00:00:00.000Z',
});
assert.match(entry.entryId, /^spell-[a-f0-9]{24}$/);
assert.equal(verifySpellbookEntry(entry).valid, true);
const serialized = serializeSpellbook([entry]);
const imported = parseSpellbook(serialized);
assert.equal(imported.valid, true);
assert.equal(imported.payload?.storage, 'caller-controlled-local-first');
assert.equal(imported.payload?.entries[0]?.contentHash, entry.contentHash);
assert.equal(serializeSpellbook(imported.payload?.entries ?? []), serialized, 'local export round-trip must be canonical');

const tampered = JSON.parse(serialized) as { entries: Array<Record<string, unknown>> };
if (tampered.entries[0]) tampered.entries[0].label = 'Changed after receipt';
const tamperResult = parseSpellbook(JSON.stringify(tampered));
assert.equal(tamperResult.valid, false);
assert.ok(tamperResult.issues.some((issue) => /content hash mismatch/i.test(issue)));

const forged = structuredClone(entry) as SpellbookEntry;
const forgedProgram = structuredClone(forged.program);
if (forgedProgram.form !== 'conditional') {
  const morpheme = forgedProgram.body.terms[0]?.morphemes[0];
  if (morpheme) (morpheme as { meaning: string }).meaning = 'forged meaning';
}
assert.equal(validateSpellProgram(forgedProgram).valid, false, 'forged vocabulary metadata must fail validation');
assert.throws(() => compileSpell(forgedProgram), /failed validation/);

assert.throws(
  () => createSpellbookEntry(valid, { savedAt: 'not-a-date' }),
  /ISO-8601/,
);
assert.equal(parseSpellbook('{broken').valid, false);
assert.equal(parseSpellbook('x'.repeat(512 * 1024 + 1)).valid, false);

console.log('spellcraft core is deterministic, fail-closed, non-executing, sealed, and local-first');
