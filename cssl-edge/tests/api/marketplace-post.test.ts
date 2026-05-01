// cssl-edge · tests/api/marketplace-post.test.ts

import {
  testCapsZeroDenies,
  testCapsSetCreatesReceipt,
} from '@/pages/api/marketplace/post';

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

function runAll(): void {
  testCapsZeroDenies();
  testCapsSetCreatesReceipt();
  // eslint-disable-next-line no-console
  console.log('marketplace-post.test : OK · 2 tests passed');
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
