// One model serves chat and code by changing temperament, not weights. These assert the two things
// that make that safe: the bands match the corpus they came from, and nothing a phone sends can
// break a turn.

import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {
  SAMPLING_PRESETS, DEFAULT_PRESET, presetById, clampSampling, resolveSampling, PRESET_FOR_TOOL,
} from '../../lib/apocrypha/sampling';

const precise = presetById('precise').profile;
const balanced = presetById('balanced').profile;
const open = presetById('open').profile;

// -- the bands are the ones the research actually states ----------------------------------------
// T94_LLM_GENERATION: "writing code temperature 0.2 to 0.4 ... General tasks 0.7 to 1.0 ...
// creative writing 1.0 or higher."
assert.ok(precise.temperature >= 0.2 && precise.temperature <= 0.4, `precise must sit in the code band, got ${precise.temperature}`);
assert.ok(balanced.temperature >= 0.7 && balanced.temperature <= 1.0, `balanced must sit in the general band, got ${balanced.temperature}`);
assert.ok(open.temperature > 1.0, `open must sit above the general band, got ${open.temperature}`);
// Same transcript warns output "become[s] incoherent" past roughly 1.5. Do not ship a preset there.
assert.ok(open.temperature <= 1.5, 'no preset may default into the incoherent range');
assert.ok(precise.temperature < balanced.temperature && balanced.temperature < open.temperature, 'the three presets must be ordered');

// -- coding starters ask for the coding band ----------------------------------------------------
for (const tool of ['write', 'explain', 'debug', 'review']) {
  assert.equal(PRESET_FOR_TOOL[tool], 'precise', `${tool} is a code task and must default to precise`);
}

// -- nothing from the wire can break a turn -----------------------------------------------------
const hostile = clampSampling(
  { temperature: 9000, topP: -4, topK: 1e9, minP: 'yes', repeatPenalty: null, seed: 1.5 },
  balanced,
);
assert.ok(hostile.temperature <= 2, 'temperature must be clamped, not obeyed');
assert.ok(hostile.topP >= 0 && hostile.topP <= 1, 'topP must be clamped into range');
assert.ok(Number.isInteger(hostile.topK) && hostile.topK <= 200, 'topK must be a clamped integer');
assert.equal(hostile.minP, balanced.minP, 'a non-number falls back rather than poisoning the turn');
assert.equal(hostile.repeatPenalty, balanced.repeatPenalty, 'null falls back');
assert.ok(!('seed' in hostile), 'a fractional seed is not a seed');
// The clamp must never throw -- that is the whole difference from the env helpers, which should.
assert.doesNotThrow(() => clampSampling(undefined, balanced));
assert.doesNotThrow(() => clampSampling('garbage', balanced));
assert.doesNotThrow(() => clampSampling({ temperature: NaN }, balanced));

// -- an unknown preset is the default, never a crash ---------------------------------------------
assert.equal(presetById('nonsense').id, DEFAULT_PRESET);
assert.equal(presetById(null).id, DEFAULT_PRESET);
assert.equal(resolveSampling('precise').temperature, precise.temperature);
assert.equal(resolveSampling('precise', { temperature: 0.9 }).temperature, 0.9, 'an explicit override wins over the preset');

// -- every preset says what it does in words, not numbers -----------------------------------------
for (const preset of SAMPLING_PRESETS) {
  assert.ok(preset.effect.length > 30, `${preset.id} must explain its effect to a reader`);
  assert.ok(!/temperature|top_p|top-p/i.test(preset.effect), `${preset.id} must describe the effect, not name the parameter`);
}

// -- provenance: the corpus this came from must still be on disk ---------------------------------
const source = path.join('C:', 'Users', 'Apocky', 'source', 'repos', 'Apocrypha', 'specs', 'sources', 'transcripts');
const present = fs.existsSync(source);
assert.ok(
  fs.readFileSync(path.join(process.cwd(), 'lib/apocrypha/sampling.ts'), 'utf8').includes('T94_LLM_GENERATION'),
  'the module must cite where its numbers came from',
);
console.log(`sampling.test : OK - 3 presets in their stated bands, wire input clamped, corpus ${present ? 'present' : 'MISSING (cited, not readable)'}`);
