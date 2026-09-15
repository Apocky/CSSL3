// Memory is loaded by default, and staying loaded is what makes that affordable.
//
// The behaviour this pins replaced a keyword allowlist: memory loaded only when the message
// contained "recall", "remember", "memory", "who am i". Every ordinary question -- what are we
// working on, how do I deploy this, what do I prefer -- matched nothing and was answered blind,
// while every adapter still reported state 'ok'. A blind worker was indistinguishable from a
// healthy one, so nothing ever surfaced it.
//
// Two properties, and they are load-bearing together. Default-on without residency is a federated
// probe on every turn; residency without a ceiling is a system confidently reciting last week.

import { isMemoryNeeded } from '../scripts/apocrypha-worker/retrieval';
import { WorkingMemory, mergeEpisodic } from '../scripts/apocrypha-worker/working-memory';
import type { ClaimedJob, RetrievalBundle, RetrievalRecord } from '../scripts/apocrypha-worker/types';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

function job(request: Record<string, unknown>): ClaimedJob {
  return {
    jobId: 'j', attemptId: 'a', attemptNo: 1, leaseEpoch: 1, leaseToken: 't',
    leaseExpiresAt: '', tenantId: 'tenant', ownerPrincipalId: 'owner', kind: 'chat' as never,
    capability: 'chat' as never, request, modelAlias: 'm', profileHash: 'p',
    toolRegistryVersion: 'v', memoryManifestHash: 'h',
  };
}

function record(source: string, provenanceId: string, text = 'x'): RetrievalRecord {
  return { source, provenanceId, text };
}

function bundle(records: RetrievalRecord[]): RetrievalBundle {
  return { query: 'q', results: [], records, digest: 'd', probedAt: null };
}

// The exact questions the old allowlist answered blind.
const ORDINARY = [
  'What are we working on?',
  'How should I deploy this?',
  'Which approach do I usually prefer here?',
  'Summarise where the build got to.',
  'hey',
  'Fix the streaming bug.',
];

async function main(): Promise<void> {
  // 1 -- default ON. This is the whole point; every one of these used to load nothing.
  for (const prompt of ORDINARY) {
    assert(isMemoryNeeded(job({ prompt })), `memory was NOT loaded for: "${prompt}"`);
  }
  assert(isMemoryNeeded(job({})), 'an empty request must still load memory');

  // 2 -- an explicit opt-out is still honoured, and only an explicit one.
  assert(!isMemoryNeeded(job({ prompt: 'x', memory_requested: false })), 'explicit opt-out ignored');
  assert(!isMemoryNeeded(job({ prompt: 'x', needs_memory: false })), 'explicit opt-out ignored');
  assert(isMemoryNeeded(job({ prompt: 'x', memory_requested: 'false' })), 'a string must not opt out');
  assert(isMemoryNeeded(job({ prompt: 'x', memory_requested: 0 })), 'a falsy non-false must not opt out');

  // 3 -- residency. A fake clock, so the windows are asserted rather than slept through.
  let clock = 1_000_000;
  let loads = 0;
  const memory = new WorkingMemory({
    freshMs: 100, staleMs: 200, maxAgeMs: 500, maxRecords: 4, maxConversations: 2,
    now: () => clock,
  });
  const load = async (): Promise<RetrievalBundle> => {
    loads += 1;
    return bundle([record('anamnesis', `r${loads}`)]);
  };

  const cold = await memory.ensure('k', load);
  assert(cold.origin === 'cold' && loads === 1, 'a cold conversation must actually read');

  clock += 50;
  const fresh = await memory.ensure('k', load);
  assert(fresh.origin === 'fresh' && loads === 1, 'a fresh turn must not re-read');
  assert(fresh.bundle.records.length === 1, 'the warm turn carried no records');

  // Past fresh, inside stale: served warm, no refresh yet.
  clock += 100;
  const warm = await memory.ensure('k', load);
  assert(warm.origin === 'revalidating' && loads === 1, 'refreshed too eagerly');

  // Past stale: still served immediately, refresh runs BEHIND the turn.
  clock += 150;
  const revalidating = await memory.ensure('k', load);
  assert(revalidating.origin === 'revalidating', 'stale turn should still serve warm');
  await new Promise((resolve) => setImmediate(resolve));
  assert(loads === 2, 'the background revalidation did not run');

  // 4 -- the ceiling. "Warm" must never mean "indefinitely old".
  clock += 10_000;
  const expired = await memory.ensure('k', load);
  assert(expired.origin === 'expired' && loads === 3, 'an entry past maxAgeMs was served anyway');

  // 5 -- episodic: what was recalled on an earlier turn is still there later, without re-recall.
  const early = revalidating.bundle.records.some((r) => r.provenanceId === 'r1');
  assert(early || expired.bundle.records.some((r) => r.provenanceId === 'r1'),
    'the episodic set dropped an earlier turn instead of accumulating');
  assert(mergeEpisodic([record('a', '1')], [record('a', '2')], 10).length === 2, 'merge lost a record');
  assert(mergeEpisodic([record('a', '1')], [record('a', '1')], 10).length === 1, 'merge failed to dedupe');
  assert(mergeEpisodic([record('a', '1')], [record('a', '2')], 10)[0].provenanceId === '2',
    'the newest record must lead');
  assert(mergeEpisodic(
    [record('a', '1'), record('a', '2')], [record('a', '3')], 2,
  ).length === 2, 'the cap was not enforced');

  // 6 -- a failing read on an expired entry is DEGRADED, not healthy. The caller has to be able
  //      to tell "old" from "broken".
  clock += 10_000;
  const failing = await memory.ensure('k', async () => { throw new Error('gateway down'); });
  assert(failing.origin === 'failed', `a failed refresh reported ${failing.origin}`);
  assert(failing.bundle.records.length > 0, 'a failed refresh should still hold the last good set');

  // 7 -- a cold conversation with a failing read must THROW, not silently return nothing. Empty
  //      memory presented as success is the original bug in a new costume.
  let threw = false;
  try {
    await memory.ensure('never-seen', async () => { throw new Error('gateway down'); });
  } catch { threw = true; }
  assert(threw, 'a cold failed read returned empty memory instead of failing');

  // 8 -- bounded. An unbounded per-conversation cache is a memory leak in a long-lived worker.
  clock += 1;
  await memory.ensure('a', load);
  clock += 1;
  await memory.ensure('b', load);
  clock += 1;
  await memory.ensure('c', load);
  assert(memory.stats().conversations <= 2, `eviction failed: ${memory.stats().conversations} held`);

  // 9 -- G5: press it. Each mutation is a plausible regression; each must be caught above.
  const mutants: ReadonlyArray<readonly [string, () => boolean]> = [
    ['memory off by default (the old allowlist)', () => {
      const old = (j: ClaimedJob) => /recall|remember|memory|who am i/i.test(String(j.request.prompt ?? ''));
      // Caught when the old gate disagrees with default-on for at least one ordinary prompt.
      return ORDINARY.some((prompt) => old(job({ prompt })) !== true);
    }],
    ['loose equality lets a falsy value opt out', () => {
      // `0 == false` is true, so a sloppy check silently disables memory for any falsy field.
      const loose = (j: ClaimedJob) => !(j.request.memory_requested == false);
      return loose(job({ prompt: 'x', memory_requested: 0 })) !== true;
    }],
    ['cache never expires', () => {
      // If maxAgeMs were ignored, the expired turn would have served warm and not re-read.
      return expired.origin === 'expired' && loads >= 3;
    }],
    ['episodic replaces instead of merging', () => mergeEpisodic([record('a', '1')], [record('a', '2')], 10).length === 2],
    ['failed read reported as fresh', () => failing.origin === 'failed'],
  ];
  for (const [name, holds] of mutants) {
    assert(holds(), `MUTATION SURVIVED: "${name}" -- this suite does not detect it`);
  }

  console.log(`apocrypha-working-memory.test: ${ORDINARY.length} ordinary prompts load memory by default, `
    + 'opt-out is explicit-only, fresh/stale/expired windows, background revalidation, episodic '
    + `accumulation, degraded-not-healthy on failure, bounded, ${mutants.length}/${mutants.length} mutations caught`);
}

main().then(() => console.log('apocrypha-working-memory OK')).catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
