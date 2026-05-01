// cssl-edge · tests/api/mp-rendezvous-lobby.test.ts

import {
  testCapsZeroDenies,
  testCapsSetReturnsLobbies,
} from '@/pages/api/mp-rendezvous/lobby';

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

function runAll(): void {
  testCapsZeroDenies();
  testCapsSetReturnsLobbies();
  // eslint-disable-next-line no-console
  console.log('mp-rendezvous-lobby.test : OK · 2 tests passed');
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
