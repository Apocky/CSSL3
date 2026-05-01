// cssl-edge · tests/api/transparency-cocreative-bias.test.ts
// Lightweight self-test for /api/transparency/cocreative-bias. Exercises the
// inline test functions defined in the route module via the standard
// `npm test` chain.

import {
  testEnvMissingStubFallback,
  testReturnsArrayWithLimit,
} from '@/pages/api/transparency/cocreative-bias';

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

async function runAll(): Promise<void> {
  await testEnvMissingStubFallback();
  await testReturnsArrayWithLimit();
  // eslint-disable-next-line no-console
  console.log('transparency-cocreative-bias.test : OK · 2 tests passed');
}

if (isMain) {
  runAll().catch((err) => {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  });
}
