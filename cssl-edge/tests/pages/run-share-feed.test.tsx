// cssl-edge · tests/pages/run-share-feed.test.tsx
// Smoke test for /run-share-feed. Verifies (a) page export is a function ·
// (b) getServerSideProps fetches + surfaces feed via mocked fetch.

import RunShareFeedPage, {
  getServerSideProps,
  _testPageExportsAndFraming,
} from '@/pages/run-share-feed';
import type { GetServerSidePropsContext } from 'next';

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

// 1. Page module exports + framing helper work · GSSP fetches feed.
export async function testPageExportsAndGSSP(): Promise<void> {
  assert(
    typeof RunShareFeedPage === 'function',
    `default export must be a function, got ${typeof RunShareFeedPage}`
  );
  assert(_testPageExportsAndFraming(), '_testPageExportsAndFraming must return true');

  const originalFetch = globalThis.fetch;
  globalThis.fetch = (async () =>
    new Response(
      JSON.stringify({
        feed: [
          {
            receipt_id: 'rs-001',
            player_id: 'alice',
            seed: 'seed-r1',
            scoring: { runtime_s: 423, depth: 7, completed: true },
            screenshot_handle: 'cdn:thumb-001',
            note: 'survived the glassroot',
            posted_at: '2026-04-30T10:00:00.000Z',
            echoes_received: 4,
          },
        ],
        total: 1,
        framing: 'gift-economy',
      }),
      { status: 200, headers: { 'Content-Type': 'application/json' } }
    )) as unknown as typeof fetch;

  try {
    const ctx = {
      query: { player_id: 'self', friend_list: 'alice,bob' },
      req: { headers: { host: 'localhost:3000' } },
      res: {},
      params: {},
      resolvedUrl: '/run-share-feed',
    } as unknown as GetServerSidePropsContext;

    const result = await getServerSideProps(ctx);
    const props = (result as unknown as { props: Record<string, unknown> }).props;
    assert(props['player_id'] === 'self', 'player_id must echo');
    assert(props['total'] === 1, 'total must surface');
    const feed = props['feed'] as Array<{ receipt_id: string }>;
    assert(feed.length === 1, `expected 1 feed item, got ${feed.length}`);
    assert(feed[0]?.receipt_id === 'rs-001', 'feed must echo receipt_id');
    assert(props['friend_list'] === 'alice,bob', 'friend_list must echo');
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
      console.log('run-share-feed.test : OK · 1 test passed');
    })
    .catch((err) => {
      // eslint-disable-next-line no-console
      console.error(err);
      process.exit(1);
    });
}
