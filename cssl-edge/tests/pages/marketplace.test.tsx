// cssl-edge · tests/pages/marketplace.test.tsx
// Smoke test for /gear-share page (the gear-share marketplace; renamed from
// /marketplace.tsx to avoid Next.js route conflict with /marketplace/index.tsx).
// Verifies (a) page export is a function · (b) getServerSideProps fetches +
// surfaces listings via mocked fetch.

import GearSharePage, {
  getServerSideProps,
  _testPageExportsAndFraming,
} from '@/pages/gear-share';
import type { GetServerSidePropsContext } from 'next';

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

// 1. Page module exports + framing helper work as advertised.
export async function testPageExportsAndGSSP(): Promise<void> {
  assert(
    typeof GearSharePage === 'function',
    `default export must be a function, got ${typeof GearSharePage}`
  );
  assert(_testPageExportsAndFraming(), '_testPageExportsAndFraming must return true');

  // Drive getServerSideProps with a mocked fetch so we exercise the SSR path.
  const originalFetch = globalThis.fetch;
  globalThis.fetch = (async () =>
    new Response(
      JSON.stringify({
        listings: [
          {
            receipt_id: 'r-001',
            creator_player_id: 'alice',
            rarity: 'rare',
            slot: 'weapon',
            seed: 'seed-aaa-001',
            posted_at: '2026-04-30T10:00:00.000Z',
            echoes_received: 3,
            note: 'gift-note',
          },
        ],
        total: 1,
        framing: 'gift-economy',
      }),
      { status: 200, headers: { 'Content-Type': 'application/json' } }
    )) as unknown as typeof fetch;

  try {
    const ctx = {
      query: { rarity: 'rare', slot: 'weapon' },
      req: { headers: { host: 'localhost:3000' } },
      res: {},
      params: {},
      resolvedUrl: '/gear-share',
    } as unknown as GetServerSidePropsContext;

    const result = await getServerSideProps(ctx);
    const props = (result as unknown as { props: Record<string, unknown> }).props;
    assert(props['total'] === 1, 'total must surface from mocked fetch');
    const listings = props['listings'] as Array<{ receipt_id: string }>;
    assert(listings.length === 1, `expected 1 listing, got ${listings.length}`);
    assert(listings[0]?.receipt_id === 'r-001', 'listing must echo receipt_id');
    const filter = props['filter'] as { rarity: string; slot: string };
    assert(filter.rarity === 'rare', 'filter.rarity must echo');
    assert(filter.slot === 'weapon', 'filter.slot must echo');
  } finally {
    globalThis.fetch = originalFetch;
  }
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

if (isMain) {
  testPageExportsAndGSSP()
    .then(() => {
      // eslint-disable-next-line no-console
      console.log('marketplace.test : OK · 1 test passed');
    })
    .catch((err) => {
      // eslint-disable-next-line no-console
      console.error(err);
      process.exit(1);
    });
}
