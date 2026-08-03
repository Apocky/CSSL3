import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

const source = readFileSync(resolve(process.cwd(), 'pages/admin/apex.tsx'), 'utf8');

assert(source.includes('<AdminLayout'), 'page reuses AdminLayout');
assert(source.includes("authFetch('/api/admin/apocv4/health'"), 'health uses authenticated browser fetch');
assert(source.includes("authFetch('/api/admin/apocv4/objective'"), 'objective uses authenticated browser fetch');
assert(source.includes('✓ OBSERVED · TRANSPORT + TEST RECEIPTS'), 'observed evidence is visibly labeled');
assert(source.includes('◐ MODEL-REPORTED · NOT OBSERVED FACT'), 'model reports are visibly labeled');
assert(source.includes('not hidden chain-of-thought'), 'page disclaims hidden-reasoning theater');
assert(source.includes('Synchronous RunPod proxy bound: 95 seconds'), 'provider deadline is visible');
assert(source.includes('JSON.stringify({ objective: canonical })'), 'browser submits one exact objective field');
assert(!source.includes('privacy_partition'), 'browser cannot select a privacy partition');
assert(!source.includes('APOCV4_API_TOKEN'), 'runtime token is absent from client source');
assert(!source.includes('APOCV4_RUNTIME_URL'), 'runtime origin is absent from client source');

console.log('admin-apex.test : OK · owner UI preserves evidence-lane truth');
