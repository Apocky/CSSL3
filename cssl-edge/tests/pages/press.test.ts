// cssl-edge · tests/pages/press.test.ts
// Smoke: /press page module loads + default-export is a function.

import Press from '@/pages/press';

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

export function testPressDefaultExport(): void {
  assert(typeof Press === 'function', 'press default export must be a component (function)');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  try {
    testPressDefaultExport();
    // eslint-disable-next-line no-console
    console.log('press.test : OK · 1 test passed');
  } catch (err) {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  }
}
