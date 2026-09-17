// What Apocrypha is allowed to say about itself, and what it must never repeat back.
//
// Both gates come from one observed answer. Asked "How are you different from the base Qwen model?",
// Apocrypha replied:
//
//   "The mempalace:error, brainmonsoon:error ... entries in your last admitted-memory manifest
//    confirm that this turn has no live retrieval access ... I am not a fine-tuned Qwen model.
//    I do not rely on Qwen's training weights, architecture, or internal state."
//
// Two distinct failures in one paragraph:
//   1. it quoted the provenance envelope -- adapter names, failure states, the word manifest --
//      straight at the reader, and then REASONED FROM those failures to a claim about itself;
//   2. it denied its own substrate, flatly and incorrectly, while running on that very model.
//
// The second was not confabulation. The prompt forbids inventing what is not in admitted evidence,
// and nothing in the prompt admitted what it runs on, so denial was the only move left. The cure is
// to admit the true thing, not to loosen the rule.

import assert from 'node:assert/strict';
import { baseSystem } from '../scripts/apocrypha-worker/prompt';
import type { ClaimedJob } from '../scripts/apocrypha-worker/types';

function job(alias: string, capability = 'apocky_member_chat'): ClaimedJob {
  return {
    jobId: 'j', attemptId: 'a', attemptNo: 1, leaseEpoch: 1, leaseToken: '', leaseExpiresAt: '',
    tenantId: 't', ownerPrincipalId: 'o', kind: 'apocky_chat', capability,
    request: {}, modelAlias: alias, profileHash: '', toolRegistryVersion: '', memoryManifestHash: '',
  } as unknown as ClaimedJob;
}

const ALIAS = 'qwen3-coder-next-80b-a3b-q2kxl';
const prompt = baseSystem(job(ALIAS));

// -- 1. it knows what it runs on -------------------------------------------------------------------
// Grounded in the alias the JOB carries, not a literal in the prompt, so this cannot drift from
// whatever model is actually serving.
assert.ok(prompt.includes(ALIAS), 'the prompt must carry the real model alias from the job');
assert.ok(/never deny/i.test(prompt), 'the prompt must forbid denying the substrate outright');

// An empty alias must degrade to an honest generic, not render a hole.
const anonymous = baseSystem(job(''));
assert.ok(!/local {2,}model/.test(anonymous), 'an absent alias must not leave a gap in the sentence');
assert.ok(/local model/.test(anonymous), 'an absent alias must still admit that it runs on a local model');

// -- 2. knowing is not announcing ------------------------------------------------------------------
// The fix must not turn into a model that recites its own infrastructure unprompted. It answers when
// asked; it does not volunteer.
assert.ok(/do not volunteer/i.test(prompt), 'the prompt must still forbid volunteering infrastructure');
assert.ok(/answer honestly when you are asked/i.test(prompt), 'and must permit an honest answer when asked directly');

// -- 3. the provenance envelope is bookkeeping, never material --------------------------------------
// Each of these is a term from the leaked paragraph. Naming them is the point: a general "do not
// reveal infrastructure" line was already present and did not stop it.
for (const term of ['manifest', 'digest', 'availability', 'mempalace', 'brainmonsoon', 'tenant', 'principal']) {
  assert.ok(
    prompt.toLowerCase().includes(term),
    `the prompt must name "${term}" as something never to quote back -- the generic rule did not hold`,
  );
}
assert.ok(/never quote or paraphrase the provenance envelope/i.test(prompt),
  'the prohibition must be explicit about the envelope itself, not only about credentials');
assert.ok(/a failed adapter is not evidence/i.test(prompt),
  'the prompt must say a retrieval failure is not evidence -- it reasoned FROM the failures to a claim about itself');

console.log('apocrypha-self-description OK - substrate admitted from the job alias, provenance envelope barred by name');
