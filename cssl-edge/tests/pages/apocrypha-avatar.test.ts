import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const source = readFileSync(resolve(process.cwd(), 'components/apocrypha/ApocryphaAvatar.tsx'), 'utf8');
const assert = (condition: boolean, message: string) => {
  if (!condition) throw new Error(`assert failed : ${message}`);
};

for (const state of ['private', 'ready', 'thinking', 'queued', 'dreaming', 'degraded', 'offline']) {
  assert(source.includes(`${state}:`), `missing state palette: ${state}`);
}
for (const organ of ['memory', 'language', 'reason', 'agency', 'perception', 'dream']) {
  assert(source.includes(`'${organ}'`), `missing organ node: ${organ}`);
}
assert(source.includes('prefers-reduced-motion'), 'reduced-motion fallback missing');
assert(source.includes('aria-labelledby'), 'SVG accessible name missing');
assert(source.includes('cycleProgress'), 'dream-cycle phase ring missing');
assert(source.includes('data-apocrypha-state'), 'machine-readable state missing');
assert(source.includes('data-presence-provenance'), 'presence provenance marker missing');
assert(source.includes('laboratory-preview'), 'laboratory preview mode missing');
assert(source.includes("not Apocrypha's chosen avatar"), 'laboratory preview authorship disclaimer missing');
console.log('apocrypha-avatar.test : OK · state, organ, accessibility, and cycle contracts passed');
