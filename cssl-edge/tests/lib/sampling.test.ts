// One model serves chat and code by changing temperament, not weights. These assert the two things
// that make that safe: the bands match the corpus they came from, and nothing a phone sends can
// break a turn.

import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {
  SAMPLING_PRESETS, DEFAULT_PRESET, presetById, clampSampling, resolveSampling, PRESET_FOR_TOOL,
  toEngineParams,
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
console.log(`sampling.test : OK - presets in their stated bands, wire input clamped, corpus ${present ? 'present' : 'MISSING (cited, not readable)'}`);

// -- the cutover hazards, pinned ------------------------------------------------------------------
// Found by pre-testing rather than by breaking chat: the coder GGUF declares no `enable_thinking`
// (grep: 1 hit in the Qwen3.5 chat model, 0 in Qwen3-Coder-Next). The work launcher passes --jinja.
// Handing a Jinja template a variable it never declares can refuse the request, so one engine
// serving both lanes would have 400'd every chat turn.
const worker = fs.readFileSync(path.join(process.cwd(), 'scripts/apocrypha-worker/qwen.ts'), 'utf8');
assert.ok(
  !/^\s*chat_template_kwargs:/m.test(worker),
  'enable_thinking must never be sent unconditionally -- it is model-specific',
);
assert.ok(worker.includes('THINKING_KWARG_SUPPORTED'), 'the thinking kwarg must be gated behind an explicit opt-in');
assert.ok(
  worker.includes("=== 'on'"),
  'the gate must default OFF: a template that wants the kwarg and misses it still renders; one that gets an undeclared kwarg can refuse',
);

// The worker must take its sampling from the shared table, not from literals.
assert.ok(worker.includes('BALANCED.temperature'), 'worker sampling defaults come from the preset table');
assert.ok(!/temperature:\s*0\.65/.test(worker), 'the old hardcoded 0.65 must not return');
console.log('sampling.test : OK - cutover hazards pinned (thinking kwarg gated, no literal sampling)');


// -- the full dial surface ------------------------------------------------------------------------
// Measured against llama.cpp b9743 on 2026-09-16: every field below was POSTed to the running
// engine and accepted, and the load-bearing ones were separately shown to CHANGE THE OUTPUT.
// These gates exist because the previous module drove 5 of ~14 dials and left the rest to whatever
// the GGUF metadata happened to carry.

// PRODUCTION LOCK. `balanced` is what the public chat room runs on. Changing any of these five
// changes what apocky.com says to people, so they are pinned by value, not by band.
const PUBLIC_CHAT = { temperature: 0.8, topP: 0.9, topK: 40, minP: 0.05, repeatPenalty: 1.08 };
for (const [key, value] of Object.entries(PUBLIC_CHAT)) {
  assert.equal(
    (balanced as unknown as Record<string, unknown>)[key], value,
    `balanced.${key} is the live public-chat value; changing it changes production output`,
  );
}
// ...and it must stay MINIMAL: an optional dial leaking into balanced would silently alter chat.
for (const extra of ['dryMultiplier', 'xtcProbability', 'topNSigma', 'greedy', 'typicalP']) {
  assert.equal(
    (balanced as unknown as Record<string, unknown>)[extra], undefined,
    `balanced must not carry ${extra}: public chat keeps the engine's own default`,
  );
}

// Exact means reproducible, and reproducible means greedy -- not "temperature nearly zero".
const exact = presetById('exact').profile;
assert.equal(exact.greedy, true, 'the exact preset must take the greedy path');
const exactWire = toEngineParams(exact);
assert.equal(exactWire.top_k, 1, 'greedy must collapse the candidate set to the argmax');
assert.equal(exactWire.temperature, 0, 'greedy must not leave a temperature for the sampler to act on');

// Code work uses DRY, which penalises repeated SEQUENCES, instead of repeat_penalty, which cannot
// tell a stuck loop from an indent.
const preciseProfile = presetById('precise').profile;
assert.equal(preciseProfile.repeatPenalty, 1, 'precise must not punish ordinary code repetition');
assert.ok((preciseProfile.dryMultiplier ?? 0) > 0, 'precise must carry DRY as its anti-repetition dial');
assert.ok(
  (preciseProfile.drySequenceBreakers ?? []).includes('\n'),
  'DRY needs a newline breaker or it fights the shape of the language',
);

// -- the wire format ------------------------------------------------------------------------------
// snake_case, and ABSENT dials omitted entirely. Sending null where the engine wants a number is
// how a working turn starts 400ing.
const wire = toEngineParams(preciseProfile);
assert.equal(wire.dry_multiplier, preciseProfile.dryMultiplier, 'camelCase must be rendered as the engine spells it');
assert.ok('repeat_last_n' in wire, 'a dial the profile sets must reach the engine');
for (const key of Object.keys(wire)) {
  assert.notEqual(wire[key], undefined, `${key} must never be sent as undefined`);
  assert.notEqual(wire[key], null, `${key} must never be sent as null`);
}
const bare = toEngineParams(balanced);
for (const absent of ['dry_multiplier', 'xtc_probability', 'top_n_sigma', 'typical_p', 'seed']) {
  assert.ok(!(absent in bare), `balanced sets no ${absent}, so it must not appear on the wire at all`);
}

// -- clamps cover the new dials too ---------------------------------------------------------------
const hostileDials = clampSampling(
  { dryMultiplier: 99, dryBase: -5, xtcProbability: 7, topNSigma: -3, repeatLastN: 9_999_999,
    drySequenceBreakers: new Array(50).fill('x') },
  preciseProfile,
);
assert.ok((hostileDials.dryMultiplier ?? 0) <= 5, 'dryMultiplier must be bounded');
assert.ok((hostileDials.dryBase ?? 0) >= 1, 'dryBase must be bounded below');
assert.ok((hostileDials.xtcProbability ?? 0) <= 1, 'xtcProbability is a probability');
assert.ok((hostileDials.topNSigma ?? 0) >= 0, 'topNSigma must not go negative');
assert.ok((hostileDials.repeatLastN ?? 0) <= 4096, 'repeatLastN must be bounded');
assert.ok((hostileDials.drySequenceBreakers ?? []).length <= 16, 'the breaker list must be capped');
assert.equal(clampSampling({ temperature: 'hot' }, preciseProfile).temperature, preciseProfile.temperature,
  'a non-number falls back to the preset rather than breaking the turn');

console.log('sampling.test : OK - full dial surface, public-chat values locked, wire format omits absent dials');
