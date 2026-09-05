import Buy, { SUPPORT_LINKS } from '@/pages/buy';

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

export function testBuyDefaultExport(): void {
  assert(typeof Buy === 'function', 'buy default export must be a component');
}

export function testSupportLinks(): void {
  assert(SUPPORT_LINKS.length === 2, 'Ko-fi and Patreon are the two support destinations');
  const names = new Set(SUPPORT_LINKS.map((link) => link.name));
  assert(names.has('Ko-fi'), 'Ko-fi link is present');
  assert(names.has('Patreon'), 'Patreon link is present');
  for (const link of SUPPORT_LINKS) {
    assert(link.href.startsWith('https://'), `${link.name} uses HTTPS`);
    assert(link.description.length > 0, `${link.name} has a plain-language description`);
  }
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  try {
    testBuyDefaultExport();
    testSupportLinks();
    // eslint-disable-next-line no-console
    console.log('buy.test : OK · 2 tests passed');
  } catch (err) {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  }
}
