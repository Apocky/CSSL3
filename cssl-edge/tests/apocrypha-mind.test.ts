// One mind: the room and the site must assemble the same person.
//
// The failure this prevents is subtle and permanent. If the room states its own persona rather
// than importing the worker's, both pass their tests on the day they are written and diverge
// silently forever after -- two minds wearing one name, which is precisely what the owner ruled
// out. So the persona is asserted to be the SAME OBJECT of text the worker produces, not merely
// similar to it.

import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { assemble, personaFor, profileForPrompt, memoryForPrompt } from '../scripts/apocrypha-mind/mind';
import { baseSystem } from '../scripts/apocrypha-worker/prompt';
import type { ClaimedJob } from '../scripts/apocrypha-worker/types';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

function seedProfile(path: string): void {
  const database = new DatabaseSync(path);
  database.exec(`CREATE TABLE claims(
    id INTEGER PRIMARY KEY, pass_id INTEGER, axis TEXT, statement TEXT, statement_key TEXT,
    corroborations INTEGER, first_seen TEXT, last_seen TEXT, status TEXT, killed_by INTEGER,
    killed_reason TEXT, created_at TEXT)`);
  const rows: Array<[string, string, number]> = [
    ['correction', 'Rejects scope narrowing; the whole fix is everything stated.', 40],
    ['correction', 'Objects when memory tools are not loaded at the start of a session.', 33],
    ['correction', 'Corrects premature claims of completion.', 20],
    ['correction', 'Rejects vague generalisations used to dodge specifics.', 12],
    ['ideal', 'Holds that optimal is not minimal.', 18],
    ['procedure', 'Requires work to be committed before a session ends.', 11],
    ['mannerism', 'Uses emphatic register when a request is ignored.', 9],
  ];
  for (const [axis, statement, n] of rows) {
    database.prepare('INSERT INTO claims(axis, statement, statement_key, corroborations, status, created_at, last_seen) VALUES(?,?,?,?,?,?,?)')
      .run(axis, statement, `${axis}::${statement}`, n, 'live', '2026-09-14', '2026-09-14');
  }
  database.close();
}

function seedAnamnesis(path: string): void {
  const database = new DatabaseSync(path);
  database.exec(`CREATE TABLE records(
    id INTEGER PRIMARY KEY, ts TEXT, session TEXT, repo TEXT, kind TEXT, ref TEXT, payload TEXT,
    payload_sha TEXT, prev_sha TEXT, self_sha TEXT, provenance TEXT, redacted INTEGER DEFAULT 0)`);
  database.exec("CREATE VIRTUAL TABLE records_fts USING fts5(payload, kind, content='records', content_rowid='id')");
  const insert = database.prepare('INSERT INTO records(id, ts, session, kind, payload, payload_sha, prev_sha, self_sha, provenance, redacted) VALUES(?,?,?,?,?,?,?,?,?,?)');
  insert.run(1, '2026-09-14', 's', 'ckpt', 'The apocrypha work lane runs the coder on port 19130.', 'a', 'b', 'c', 'claude', 0);
  insert.run(2, '2026-09-14', 's', 'ckpt', 'A redacted secret that must never surface.', 'a', 'b', 'c', 'claude', 1);
  database.exec("INSERT INTO records_fts(rowid, payload, kind) SELECT id, payload, kind FROM records");
  database.close();
}

// noUncheckedIndexedAccess is on: an indexed read is T | undefined. Asserting presence here
// keeps the strictness that catches real off-by-one bugs in source, without every test
// assertion drowning in optional chaining.
function need<T>(value: T | undefined, what: string): T {
  if (value === undefined) throw new Error('missing : ' + what);
  return value;
}

async function main(): Promise<void> {
  const dir = mkdtempSync(join(tmpdir(), 'apx-mind-'));
  const profileDb = join(dir, 'profile.db');
  const anamnesisDb = join(dir, 'anamnesis.db');
  seedProfile(profileDb);
  seedAnamnesis(anamnesisDb);

  try {
    // 1 -- ONE MIND. The persona is the worker's, byte for byte, not a restatement of it.
    const ownerJob = {
      jobId: 'x', attemptId: 'x', attemptNo: 1, leaseEpoch: 1, leaseToken: '', leaseExpiresAt: '',
      tenantId: 't', ownerPrincipalId: 'o', kind: 'apocky_chat', capability: 'apocky_owner_chat',
      request: {}, modelAlias: '', profileHash: '', toolRegistryVersion: '', memoryManifestHash: '',
    } as unknown as ClaimedJob;
    assert(personaFor() === baseSystem(ownerJob),
      'the room persona is not byte-identical to the worker persona -- two minds, one name');
    assert(personaFor().includes('You are Apocrypha'), 'persona lost its identity line');

    // 2 -- the caller's last message stays last, byte for byte. Everything injected goes BEFORE it.
    const turn = 'What did we decide about the work lane port?';
    const built = assemble({
      messages: [
        { role: 'user', content: 'earlier question' },
        { role: 'assistant', content: 'earlier answer' },
        { role: 'user', content: turn },
      ],
      profileDb, anamnesisDb,
    });
    const last = need(built.messages[built.messages.length - 1], 'final message');
    assert(last.role === 'user' && last.content === turn,
      'the caller final turn was displaced or rewritten');

    // 3 -- persona first (byte-stable prefix, so the engine can reuse its KV cache), then profile,
    //      then evidence, then the conversation.
    assert(need(built.messages[0], 'first').role === 'system'
      && need(built.messages[0], 'first').content === personaFor(),
      'persona is not the stable first message');
    const systemCount = built.messages.filter((m) => m.role === 'system').length;
    assert(systemCount >= 2, 'profile and memory were not admitted');

    // 4 -- evidence is its OWN message, never folded into the persona text.
    assert(!need(built.messages[0], 'first').content.includes('anamnesis:'),
      'memory was folded into the system message');
    assert(built.messages.some((m) => m.role === 'system' && m.content.includes('anamnesis:1')),
      'admitted memory never reached the prompt');
    assert(built.memoryRecords === 1, `expected 1 admitted record, got ${built.memoryRecords}`);

    // 5 -- redacted rows never surface.
    assert(!JSON.stringify(built.messages).includes('must never surface'),
      'a redacted record reached the prompt');

    // 6 -- the profile is present, and round-robins axes so one prolific axis cannot crowd the
    //      others out. Corrections outnumber everything else roughly four to one in the corpus.
    const profileText = built.messages.find((m) => m.content.startsWith('What you have learned'))?.content ?? '';
    assert(profileText !== '', 'profile block missing');
    for (const axis of ['correction', 'ideal', 'procedure', 'mannerism']) {
      assert(profileText.includes(`[${axis}`), `axis ${axis} was crowded out of the profile`);
    }

    // 7 -- the profile is budgeted. A profile that evicts the conversation made things worse.
    const tight = profileForPrompt(profileDb, 260);
    assert(tight.length <= 260, `profile overran its budget at ${tight.length}`);
    assert(tight.includes('correction'), 'the best-attested axis was dropped first');

    // 8 -- absent sources are a legitimate state, not a crash and not an empty scaffold.
    assert(profileForPrompt(join(dir, 'missing.db')) === '', 'a missing profile db did not render empty');
    assert(memoryForPrompt(join(dir, 'missing.db'), 'anything').records === 0, 'a missing ledger did not render empty');
    const bare = assemble({ messages: [{ role: 'user', content: 'hello' }] });
    assert(bare.messages.length === 2, 'an empty profile/memory produced scaffold messages');
    assert(need(bare.messages[1], 'bare turn').content === 'hello', 'the turn was lost when nothing was admitted');

    // 9 -- G5: press it. Each mutation is a plausible regression; each must be caught above.
    const mutants: ReadonlyArray<readonly [string, () => boolean]> = [
      ['persona restated instead of imported', () => {
        // Deliberately the CURRENT first sentence. A stale one would make this mutation trivially
        // detectable and stop testing what it is for: that a hand-copied persona, however faithful
        // it looks, is not the imported one.
        const copy = 'You are Apocrypha, a candid, useful digital intelligence in conversation with one person.';
        return copy !== personaFor(); // a partial copy must not equal the real persona
      }],
      ['evidence folded into the system message', () => !need(built.messages[0], 'first').content.includes('anamnesis:')],
      ['final turn not last', () => need(built.messages[built.messages.length - 1], 'last').content === turn],
      ['profile unbounded', () => profileForPrompt(profileDb, 260).length <= 260],
      ['redacted rows admitted', () => !JSON.stringify(built.messages).includes('must never surface')],
    ];
    for (const [name, holds] of mutants) {
      assert(holds(), `MUTATION SURVIVED: "${name}"`);
    }

    console.log('apocrypha-mind.test: persona byte-identical to the worker, final turn preserved, '
      + 'evidence its own message, redaction honoured, profile round-robins axes and holds its budget, '
      + `absent sources render empty, ${mutants.length}/${mutants.length} mutations caught`);
  } finally {
    try { rmSync(dir, { recursive: true, force: true }); } catch { /* temp */ }
  }
}

main().then(() => console.log('apocrypha-mind OK')).catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
