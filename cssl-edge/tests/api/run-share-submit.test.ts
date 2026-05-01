// cssl-edge · tests/api/run-share-submit.test.ts

import {
  testCapsZeroDenies,
  testCapsSetAcceptsReceipt,
} from '@/pages/api/run-share/submit';

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

function runAll(): void {
  testCapsZeroDenies();
  testCapsSetAcceptsReceipt();
  // eslint-disable-next-line no-console
  console.log('run-share-submit.test : OK · 2 tests passed');
}

if (isMain) {
  try {
    runAll();
  } catch (err) {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  }
}
