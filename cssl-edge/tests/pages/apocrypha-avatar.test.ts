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
assert(source.includes('if (!displayAuthorized || !authorizationRef) return null'), 'avatar must fail hidden without committed authority');
assert(source.includes('data-display-authorized="true"'), 'authorized rendering must expose its machine-readable decision');
console.log('apocrypha-avatar.test : OK · state, organ, accessibility, and cycle contracts passed');
