// cssl-edge · tests/lib/sovereign.test.ts
// Lightweight self-test for lib/sovereign.ts. Framework-agnostic — runs via
// `npx tsx tests/lib/sovereign.test.ts`. Lives outside pages/ so Next.js does
// not register it as a route.

import {
  SOVEREIGN_CAP_HEX,
  SOVEREIGN_HEADER_NAME,
  isSovereignHeader,
  isSovereignFromIncoming,
} from '@/lib/sovereign';

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

export function testSovereignHexConstant(): void {
  assert(
    SOVEREIGN_CAP_HEX === '0xCAFEBABEDEADBEEF',
    `SOVEREIGN_CAP_HEX should be 0xCAFEBABEDEADBEEF, got ${SOVEREIGN_CAP_HEX}`
  );
}

export function testSovereignHeaderRejectsWithoutFlag(): void {
  const h = new Headers();
  h.set(SOVEREIGN_HEADER_NAME, SOVEREIGN_CAP_HEX);
  assert(
    isSovereignHeader(h, false) === false,
    'sovereign:false → must return false even with correct header'
  );
  assert(
    isSovereignHeader(h, undefined) === false,
    'sovereign:undefined → must return false'
  );
}

export function testSovereignHeaderAcceptsWithCorrectHeader(): void {
  const h = new Headers();
  h.set(SOVEREIGN_HEADER_NAME, SOVEREIGN_CAP_HEX);
  assert(
    isSovereignHeader(h, true) === true,
    'sovereign:true + correct header → must return true'
  );
}

export function testSovereignHeaderRejectsWrongHeader(): void {
  const h = new Headers();
  h.set(SOVEREIGN_HEADER_NAME, '0xDEADBEEF');
  assert(
    isSovereignHeader(h, true) === false,
    'sovereign:true + wrong header → must return false'
  );

  const h2 = new Headers();
  assert(
    isSovereignHeader(h2, true) === false,
    'sovereign:true + missing header → must return false'
  );
}

export function testSovereignFromIncomingShape(): void {
  // pages-router shape : IncomingHttpHeaders (record of string|string[]|undefined)
  const hdrs: Record<string, string | string[] | undefined> = {
    [SOVEREIGN_HEADER_NAME]: SOVEREIGN_CAP_HEX,
  };
  assert(
    isSovereignFromIncoming(hdrs, true) === true,
    'incoming-shape sovereign:true + header → true'
  );
  assert(
    isSovereignFromIncoming(hdrs, false) === false,
    'incoming-shape sovereign:false → false'
  );
  assert(
    isSovereignFromIncoming({}, true) === false,
    'incoming-shape sovereign:true + missing header → false'
  );
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  testSovereignHexConstant();
  testSovereignHeaderRejectsWithoutFlag();
  testSovereignHeaderAcceptsWithCorrectHeader();
  testSovereignHeaderRejectsWrongHeader();
  testSovereignFromIncomingShape();
  // eslint-disable-next-line no-console
  console.log('sovereign.test : OK · 5 tests passed');
}
