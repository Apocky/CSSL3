import LazarusConsole, { _testExportsAreFunctions } from '@/pages/admin/lazarus';

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

export function testPageExportsAreFunctions(): void {
  assert(typeof LazarusConsole === 'function', 'default export must be a function/component');
  assert(_testExportsAreFunctions(), '_testExportsAreFunctions() must return true');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

if (isMain) {
  try {
    testPageExportsAreFunctions();
    // eslint-disable-next-line no-console
    console.log('lazarus-page.test : OK · 1 test passed');
  } catch (err) {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  }
}