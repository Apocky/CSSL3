import { readFileSync } from 'node:fs';
import { strict as assert } from 'node:assert';

const page = readFileSync('pages/apocrypha.tsx', 'utf8');
const hook = readFileSync('lib/clearing/useClearing.ts', 'utf8');

assert.match(page, /The Clearing public room/);
assert.match(page, /Sign in to leave a trace/);
assert.match(page, /\/account#consent/);
assert.match(page, /Microphone is unavailable by site policy/);
assert.match(page, /Camera is unavailable by site policy/);
assert.match(hook, /clearing_room/);
assert.match(hook, /clearing_message/);
assert.match(hook, /clearing_reaction/);
assert.match(hook, /clearing_room_member/);
assert.match(hook, /clearing_send_message/);
assert.match(hook, /clearing_toggle_reaction/);
assert.match(hook, /postgres_changes/);
assert.match(hook, /presenceState/);
assert.match(hook, /removeChannel/);
console.log('Clearing source contract passed');
