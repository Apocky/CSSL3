// cssl-edge · tests/api/marketplace-list.test.ts
// Self-test for /api/marketplace/list. Drives the inline test fns.

import {
  testCapsZeroDenies,
  testCapsSetReturnsListings,
} from '@/pages/api/marketplace/list';

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

function runAll(): void {
  testCapsZeroDenies();
  testCapsSetReturnsListings();
  // eslint-disable-next-line no-console
  console.log('marketplace-list.test : OK · 2 tests passed');
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
