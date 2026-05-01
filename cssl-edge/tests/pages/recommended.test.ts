// cssl-edge · tests/pages/recommended.test.ts
// Smoke test for /marketplace/recommended. We exercise the page's default
// export + the getServerSideProps function with a mocked fetch — no Next.js
// runtime, no React renderer, no headless browser. This covers the
// "renders without throwing" contract for the data-fetch path.

import RecommendedPage, {
  getServerSideProps,
} from '@/pages/marketplace/recommended';
import type { GetServerSidePropsContext } from 'next';

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

// The page module must export a renderable component (function).
export function testPageExportIsFunction(): void {
  assert(
    typeof RecommendedPage === 'function',
    `default export must be a function/component, got ${typeof RecommendedPage}`
  );
}

// getServerSideProps must resolve to { props: ... } shape even when the
// upstream /api/asset/recommend fetch fails (defensive fallback path).
export async function testGetServerSidePropsFallback(): Promise<void> {
  // Patch global fetch to throw — exercises the catch branch.
  const originalFetch = globalThis.fetch;
  globalThis.fetch = (() => {
    throw new Error('mock fetch failure');
  }) as unknown as typeof fetch;

  try {
    const ctx = {
      query: { player_id: 'alice' },
      req: { headers: { host: 'localhost:3000' } },
      res: {},
      params: {},
      resolvedUrl: '/marketplace/recommended',
    } as unknown as GetServerSidePropsContext;

    const result = await getServerSideProps(ctx);
    assert('props' in result, 'getServerSideProps must return { props }');
    const props = (result as unknown as { props: Record<string, unknown> }).props;
    assert(
      Array.isArray(props['recommendations']),
      'recommendations must default to []'
    );
    assert(
      props['player_id'] === 'alice',
      `player_id must echo query, got ${String(props['player_id'])}`
    );
    assert(
      props['total'] === 0,
      'total must be 0 when fetch fails'
    );
  } finally {
    globalThis.fetch = originalFetch;
  }
}

// getServerSideProps with a successful (mocked) fetch must surface the rows.
export async function testGetServerSidePropsHappy(): Promise<void> {
  const originalFetch = globalThis.fetch;
  globalThis.fetch = (async () =>
    new Response(
      JSON.stringify({
        recommendations: [
          {
            asset_id: 'mock--row-1',
            source: 'mock',
            name: 'Mock Row 1',
            license: 'cc0',
            license_short: 'CC0',
            score: 0.5,
            why: 'mock why',
          },
        ],
        reason: 'mock-reason',
        total: 1,
      }),
      { status: 200, headers: { 'Content-Type': 'application/json' } }
    )) as unknown as typeof fetch;

  try {
    const ctx = {
      query: { player_id: 'bob' },
      req: { headers: { host: 'localhost:3000' } },
      res: {},
      params: {},
      resolvedUrl: '/marketplace/recommended',
    } as unknown as GetServerSidePropsContext;

    const result = await getServerSideProps(ctx);
    const props = (result as unknown as { props: Record<string, unknown> }).props;
    const recs = props['recommendations'] as Array<{ asset_id: string }>;
    assert(recs.length === 1, `expected 1 mocked row, got ${recs.length}`);
    const row = recs[0];
    if (row === undefined) throw new Error('row[0] missing');
    assert(row.asset_id === 'mock--row-1', 'expected mock asset_id surfaced');
    assert(props['reason'] === 'mock-reason', 'reason must surface');
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

async function runAll(): Promise<void> {
  testPageExportIsFunction();
  await testGetServerSidePropsFallback();
  await testGetServerSidePropsHappy();
  // eslint-disable-next-line no-console
  console.log('recommended.test : OK · 3 tests passed');
}

if (isMain) {
  runAll().catch((err) => {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  });
}
