// cssl-edge · tests/api/transparency-kan-canary.test.ts

import {
  testEnvMissingStubFallback,
  testSwapPointFilter,
} from '@/pages/api/transparency/kan-canary';

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

async function runAll(): Promise<void> {
  await testEnvMissingStubFallback();
  await testSwapPointFilter();
  // eslint-disable-next-line no-console
  console.log('transparency-kan-canary.test : OK · 2 tests passed');
}

if (isMain) {
  runAll().catch((err) => {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  });
}
