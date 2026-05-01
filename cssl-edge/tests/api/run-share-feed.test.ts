// cssl-edge · tests/api/run-share-feed.test.ts

import {
  testCapsZeroDenies,
  testCapsSetReturnsFilteredFeed,
} from '@/pages/api/run-share/feed';

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

function runAll(): void {
  testCapsZeroDenies();
  testCapsSetReturnsFilteredFeed();
  // eslint-disable-next-line no-console
  console.log('run-share-feed.test : OK · 2 tests passed');
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
