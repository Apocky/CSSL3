// cssl-edge · tests/pages/transparency-index.test.ts
// Smoke test for /transparency. We exercise the page's default export +
// getServerSideProps with a mocked fetch — no Next.js runtime, no React
// renderer. Covers the "renders without throwing" contract for the
// data-fetch path.

import TransparencyPage, {
  getServerSideProps,
  _testExportsAreFunctions,
} from '@/pages/transparency/index';
import type { GetServerSidePropsContext } from 'next';

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

// 1. Page module exposes a renderable component + getServerSideProps fn.
export function testPageExportsAreFunctions(): void {
  assert(
    typeof TransparencyPage === 'function',
    `default export must be a function/component, got ${typeof TransparencyPage}`
  );
  assert(_testExportsAreFunctions(), '_testExportsAreFunctions() must return true');
}

// 2. getServerSideProps degrades gracefully when both upstream APIs throw.
export async function testGetServerSidePropsFetchFailure(): Promise<void> {
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
      resolvedUrl: '/transparency',
    } as unknown as GetServerSidePropsContext;

    const result = await getServerSideProps(ctx);
    assert('props' in result, 'must return { props }');
    const props = (result as unknown as { props: Record<string, unknown> }).props;
    assert(props['player_id'] === 'alice', 'player_id must echo');
    assert(props['fetch_failed'] === true, 'fetch_failed must be true on failure');
    const sov = props['sov_cap'] as { count: number };
    assert(sov.count === 0, 'sov_cap.count must be 0 when fetch fails');
  } finally {
    globalThis.fetch = originalFetch;
  }
}

// 3. getServerSideProps surfaces values from a successful (mocked) fetch.
export async function testGetServerSidePropsHappy(): Promise<void> {
  const originalFetch = globalThis.fetch;
  let callCount = 0;
  globalThis.fetch = (async (url: string) => {
    callCount += 1;
    if (url.includes('/api/transparency/sovereign-cap')) {
      return new Response(
        JSON.stringify({
          rows: [
            {
              id: 1,
              player_id: 'bob',
              ts_iso: '2026-04-30T11:00:00.000Z',
              action_kind: 'companion.relay',
              cap_bypassed_kind: 'COMPANION_REMOTE_RELAY',
              reason: 'sovereign-bypass present',
              caller_origin: 'cssl-host',
            },
          ],
          total: 1,
          source: 'stub',
        }),
        { status: 200, headers: { 'Content-Type': 'application/json' } }
      );
    }
    return new Response(
      JSON.stringify({
        bias_vector: { updated_at: '2026-04-30T11:30:00.000Z' },
        feedback: [{ ts_iso: '2026-04-30T11:25:00.000Z' }],
        total: 1,
        source: 'stub',
      }),
      { status: 200, headers: { 'Content-Type': 'application/json' } }
    );
  }) as unknown as typeof fetch;

  try {
    const ctx = {
      query: { player_id: 'bob' },
      req: { headers: { host: 'localhost:3000' } },
      res: {},
      params: {},
      resolvedUrl: '/transparency',
    } as unknown as GetServerSidePropsContext;

    const result = await getServerSideProps(ctx);
    const props = (result as unknown as { props: Record<string, unknown> }).props;
    const sov = props['sov_cap'] as { count: number; last_action: string | null };
    const bias = props['bias'] as { feedback_count: number };
    assert(sov.count === 1, `expected 1 cap event, got ${sov.count}`);
    assert(sov.last_action === 'companion.relay', 'last_action must surface');
    assert(bias.feedback_count === 1, 'feedback_count must surface');
    assert(callCount === 2, `expected 2 fetch calls, got ${callCount}`);
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
  testPageExportsAreFunctions();
  await testGetServerSidePropsFetchFailure();
  await testGetServerSidePropsHappy();
  // eslint-disable-next-line no-console
  console.log('transparency-index.test : OK · 3 tests passed');
}

if (isMain) {
  runAll().catch((err) => {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  });
}
