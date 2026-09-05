import { readFileSync } from 'node:fs';
import { strict as assert } from 'node:assert';

const route = readFileSync('pages/api/admin/apocrypha/ma.ts', 'utf8');
const view = readFileSync('components/apocrypha/CognitionView.tsx', 'utf8');

assert.match(route, /\/admin\/cognition\/ma/);
for (const action of ['pause', 'resume', 'drain', 'restore']) assert.match(route, new RegExp(action));
assert.match(view, /Resume from ma/);
assert.match(view, /responsive idle state/i);
assert.match(view, /Repeated measured epsilon-ratio/);
assert.match(view, /\/api\/admin\/apocrypha\/ma\?action=/);
console.log('Apocrypha ma control contract passed');
