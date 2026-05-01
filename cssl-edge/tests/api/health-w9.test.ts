// cssl-edge · tests/api/health-w9.test.ts
// Drives W9-bump inline tests in pages/api/health.ts.

import {
  testHealthCarriesW9Keys,
  testHealthPaymentsReadyComposite,
} from '@/pages/api/health';

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

function runAll(): void {
  testHealthCarriesW9Keys();
  testHealthPaymentsReadyComposite();
  // eslint-disable-next-line no-console
  console.log('health-w9.test : OK · 2 tests passed');
}

if (isMain) {
  try { runAll(); }
  catch (err) { /* eslint-disable-next-line no-console */ console.error(err); process.exit(1); }
}
