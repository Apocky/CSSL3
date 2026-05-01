// cssl-edge · tests/api/transparency-sovereign-cap.test.ts
// Lightweight self-test for /api/transparency/sovereign-cap. Exercises the
// inline test functions defined in the route module so the harness can
// drive them through the standard `npm test` chain.

import {
  testEnvMissingStubFallback,
  testReturnsArray,
} from '@/pages/api/transparency/sovereign-cap';

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

async function runAll(): Promise<void> {
  await testEnvMissingStubFallback();
  await testReturnsArray();
  // eslint-disable-next-line no-console
  console.log('transparency-sovereign-cap.test : OK · 2 tests passed');
}

if (isMain) {
  runAll().catch((err) => {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  });
}
