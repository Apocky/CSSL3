// The profile store, and the one property automatic promotion rests on: revert actually reverts.
//
// Promotion is automatic by owner decision, so nothing stops a bad pass from reaching the live
// mind. What makes that safe is being able to take it back exactly -- not approximately. Two ways
// that quietly fails:
//
//   1. re-running a pass inflates corroborations without adding evidence, so the number the
//      retrieval layer ranks on becomes a count of how often the job ran;
//   2. revert removes a pass's claims but leaves corroborations it contributed to other claims,
//      so support that was withdrawn still counts.
//
// The invariant that catches both: corroborations == distinct evidence rows, before and after.

import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { ProfileStore, statementKey } from '../scripts/apocrypha-profile/store';
import type { Grounded } from '../scripts/apocrypha-profile/grounding';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

const REJECTED = {
  'axis-unknown': 0, 'statement-empty': 0, 'statement-too-long': 0, 'quote-too-short': 0,
  'quote-too-few-words': 0, 'chunk-not-in-batch': 0, 'quote-not-in-source': 0,
  'statement-unrelated-to-quote': 0,
} as const;

function grounded(axis: string, statement: string, chunkId: number, ts: string): Grounded {
  return {
    claim: { axis, statement, quote: 'a verbatim span long enough to pass the floor', chunkId },
    chunk: { id: chunkId, text: 'x', sha256: `sha${chunkId}`, sessionId: 's', eventTs: ts },
  };
}

async function main(): Promise<void> {
  const dir = mkdtempSync(join(tmpdir(), 'apx-profile-'));
  const path = join(dir, 'profile.db');
  const store = new ProfileStore(path);

  try {
    // Pass 1 -- two fresh claims.
    const p1 = store.beginPass('test-model', null);
    assert(store.record(p1, grounded('ideal', 'Optimal is not minimal.', 1, '2026-05-01T00:00:00Z')) === 'new', 'A should be new');
    assert(store.record(p1, grounded('procedure', 'Always commit before session end.', 2, '2026-05-02T00:00:00Z')) === 'new', 'B should be new');
    store.finishPass(p1, { chunksRead: 2, claimsOffered: 2, claimsGrounded: 2, claimsNew: 2, claimsCorroborated: 0, rejected: { ...REJECTED } });

    // Pass 2 -- corroborates A from a DIFFERENT chunk, and adds C.
    const p2 = store.beginPass('test-model', null);
    assert(store.record(p2, grounded('ideal', 'Optimal is not minimal.', 3, '2026-06-01T00:00:00Z')) === 'corroborated', 'A should corroborate');
    assert(store.record(p2, grounded('mannerism', 'Uses emphatic register when a request is ignored.', 4, '2026-06-02T00:00:00Z')) === 'new', 'C should be new');
    store.finishPass(p2, { chunksRead: 2, claimsOffered: 2, claimsGrounded: 2, claimsNew: 1, claimsCorroborated: 1, rejected: { ...REJECTED } });

    assert(store.audit().length === 0, 'invariant broken after two clean passes');
    assert(store.top('ideal', 5)[0].corroborations === 2, 'A should have 2 corroborations');
    assert(store.top('ideal', 5)[0].lastSeen === '2026-06-01T00:00:00Z', 'last_seen did not advance');

    // 1 -- re-recording the SAME chunk must not inflate. This is the failure that turns
    //      corroboration into a job-run counter.
    const p3 = store.beginPass('test-model', null);
    assert(store.record(p3, grounded('ideal', 'Optimal is not minimal.', 3, '2026-06-01T00:00:00Z')) === 'duplicate', 'same chunk must be a duplicate');
    assert(store.top('ideal', 5)[0].corroborations === 2, 'corroborations inflated on a repeat pass');
    assert(store.audit().length === 0, 'invariant broken by a duplicate');
    store.revertPass(p3);

    // 2 -- revert pass 2: C disappears, A drops back to its real support, B is untouched.
    const before = Object.fromEntries(['ideal', 'procedure', 'mannerism'].map((a) => [a, store.counts()[a] ?? 0]));
    assert(before.mannerism === 1, 'C missing before revert');
    const result = store.revertPass(p2);
    assert(result.deleted === 1, `revert deleted ${result.deleted} claims, expected 1`);
    assert(result.decremented === 1, `revert decremented ${result.decremented} claims, expected 1`);

    assert((store.counts().mannerism ?? 0) === 0, 'C survived the revert of its own pass');
    assert((store.counts().procedure ?? 0) === 1, 'B was collateral damage');
    const a = store.top('ideal', 5)[0];
    assert(a.corroborations === 1, `A kept withdrawn support: ${a.corroborations}`);
    assert(store.audit().length === 0, 'invariant broken after revert');

    // 3 -- reverting twice must not drive a claim below its real support.
    store.revertPass(p2);
    assert(store.top('ideal', 5)[0].corroborations === 1, 'double revert drove A below its evidence');
    assert(store.audit().length === 0, 'invariant broken after double revert');

    // 4 -- the dedupe key folds punctuation and case but NOT distinct axes: the same sentence
    //      observed as an ideal and as a procedure are different claims.
    assert(statementKey('ideal', 'Optimal is not minimal.') === statementKey('ideal', 'optimal  is not   minimal'),
      'dedupe key is sensitive to punctuation or spacing');
    assert(statementKey('ideal', 'X Y Z W') !== statementKey('procedure', 'X Y Z W'),
      'dedupe key collapses distinct axes');

    // 5 -- G5: press it. Corrupt the count directly and confirm audit() reports it. An invariant
    //      check never observed failing is not a check.
    const rogue = new ProfileStore(path);
    (rogue as unknown as { database: { exec(sql: string): void } }).database
      .exec('UPDATE claims SET corroborations = 99 WHERE axis = \'ideal\'');
    assert(rogue.audit().length === 1, 'audit() did not notice a corrupted corroboration count');
    rogue.close();

    store.close();
    console.log('apocrypha-profile-store.test: dedupe, corroboration=evidence invariant, '
      + 'duplicate-safe, revert exact, double-revert safe, audit proven to fail');
  } finally {
    try { rmSync(dir, { recursive: true, force: true }); } catch { /* temp dir */ }
  }
}

main().then(() => console.log('apocrypha-profile-store OK')).catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
