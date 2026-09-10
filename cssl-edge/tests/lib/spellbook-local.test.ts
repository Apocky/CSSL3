import assert from 'node:assert/strict';

import { LOCAL_SPELLBOOK_KEY, readLocalSpellbook, writeLocalSpellbook } from '../../lib/spellbook-local';

const memory = new Map<string, string>();
const storage = {
  getItem(key: string): string | null { return memory.get(key) ?? null; },
  setItem(key: string, value: string): void { memory.set(key, value); },
};

assert.deepEqual(readLocalSpellbook(storage), { status: 'ready', entries: [] });
assert.equal(writeLocalSpellbook(storage, []).status, 'ready');
assert.ok(memory.get(LOCAL_SPELLBOOK_KEY)?.includes('apocky.spellbook.local-export/v1'));

memory.set(LOCAL_SPELLBOOK_KEY, '{"kind":"wrong"}');
const rejected = readLocalSpellbook(storage);
assert.equal(rejected.status, 'rejected');
if (rejected.status === 'rejected') assert.equal(rejected.code, 'SPELLBOOK_SCHEMA_REJECTED');

// eslint-disable-next-line no-console
console.log('spellbook-local.test : OK');
