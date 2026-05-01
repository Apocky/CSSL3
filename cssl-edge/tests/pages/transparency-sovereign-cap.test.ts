// cssl-edge · tests/pages/transparency-sovereign-cap.test.ts
// Smoke test for /transparency/sovereign-cap. Verifies (a) page export is
// renderable, (b) sortAuditRows behavior · ascending + descending,
// (c) getServerSideProps responds correctly with a mocked fetch. No React
// renderer required — exercises the data-fetch + sort plumbing only.

import SovCapPage, {
  getServerSideProps,
  sortAuditRows,
  _testSortAndExports,
} from '@/pages/transparency/sovereign-cap';
import type { GetServerSidePropsContext } from 'next';

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

// 1. Page module exports + sort helper work as advertised.
export function testPageExportsAndSort(): void {
  assert(
    typeof SovCapPage === 'function',
    `default export must be a function, got ${typeof SovCapPage}`
  );
  assert(_testSortAndExports(), '_testSortAndExports() must return true');
}

// 2. sortAuditRows sorts ascending + descending correctly.
export function testSortDirectAscDesc(): void {
  const rows = [
    { id: 1, player_id: 'p', ts_iso: '2026-04-30T10:00:00.000Z', action_kind: 'a', cap_bypassed_kind: 'A', reason: '', caller_origin: '' },
    { id: 2, player_id: 'p', ts_iso: '2026-04-30T09:00:00.000Z', action_kind: 'b', cap_bypassed_kind: 'B', reason: '', caller_origin: '' },
    { id: 3, player_id: 'p', ts_iso: '2026-04-30T11:00:00.000Z', action_kind: 'c', cap_bypassed_kind: 'C', reason: '', caller_origin: '' },
  ];
  const desc = sortAuditRows(rows, 'desc');
  assert(desc[0]?.id === 3, `desc[0] must be id=3, got ${desc[0]?.id}`);
  assert(desc[1]?.id === 1, `desc[1] must be id=1, got ${desc[1]?.id}`);
  assert(desc[2]?.id === 2, `desc[2] must be id=2, got ${desc[2]?.id}`);

  const asc = sortAuditRows(rows, 'asc');
  assert(asc[0]?.id === 2, `asc[0] must be id=2, got ${asc[0]?.id}`);
  assert(asc[1]?.id === 1, `asc[1] must be id=1, got ${asc[1]?.id}`);
  assert(asc[2]?.id === 3, `asc[2] must be id=3, got ${asc[2]?.id}`);
}

// 3. getServerSideProps surfaces sorted rows from mocked fetch.
export async function testGetServerSidePropsHappy(): Promise<void> {
  const originalFetch = globalThis.fetch;
  globalThis.fetch = (async () =>
    new Response(
      JSON.stringify({
        rows: [
          { id: 1, player_id: 'alice', ts_iso: '2026-04-30T10:00:00.000Z', action_kind: 'a', cap_bypassed_kind: 'A', reason: 'r', caller_origin: 'cssl-host' },
          { id: 2, player_id: 'alice', ts_iso: '2026-04-30T11:00:00.000Z', action_kind: 'b', cap_bypassed_kind: 'B', reason: 'r', caller_origin: 'cssl-host' },
        ],
        total: 2,
        source: 'stub',
      }),
      { status: 200, headers: { 'Content-Type': 'application/json' } }
    )) as unknown as typeof fetch;

  try {
    const ctx = {
      query: { player_id: 'alice' },
      req: { headers: { host: 'localhost:3000' } },
      res: {},
      params: {},
      resolvedUrl: '/transparency/sovereign-cap',
    } as unknown as GetServerSidePropsContext;

    const result = await getServerSideProps(ctx);
    const props = (result as unknown as { props: Record<string, unknown> }).props;
    assert(props['player_id'] === 'alice', 'player_id must echo');
    assert(props['total'] === 2, 'total must surface');
    assert(props['sort_dir'] === 'desc', 'default sort must be desc');
    const rows = props['rows'] as Array<{ id: number }>;
    assert(rows.length === 2, `expected 2 rows, got ${rows.length}`);
    assert(rows[0]?.id === 2, `desc-sorted first row must be id=2, got ${rows[0]?.id}`);
  } finally {
    globalThis.fetch = originalFetch;
  }
}

// 4. ?sort=asc query flips the default ordering.
export async function testGetServerSidePropsSortAsc(): Promise<void> {
  const originalFetch = globalThis.fetch;
  globalThis.fetch = (async () =>
    new Response(
      JSON.stringify({
        rows: [
          { id: 1, player_id: 'alice', ts_iso: '2026-04-30T10:00:00.000Z', action_kind: 'a', cap_bypassed_kind: 'A', reason: 'r', caller_origin: 'cssl-host' },
          { id: 2, player_id: 'alice', ts_iso: '2026-04-30T11:00:00.000Z', action_kind: 'b', cap_bypassed_kind: 'B', reason: 'r', caller_origin: 'cssl-host' },
        ],
        total: 2,
        source: 'stub',
      }),
      { status: 200, headers: { 'Content-Type': 'application/json' } }
    )) as unknown as typeof fetch;

  try {
    const ctx = {
      query: { player_id: 'alice', sort: 'asc' },
      req: { headers: { host: 'localhost:3000' } },
      res: {},
      params: {},
      resolvedUrl: '/transparency/sovereign-cap',
    } as unknown as GetServerSidePropsContext;

    const result = await getServerSideProps(ctx);
    const props = (result as unknown as { props: Record<string, unknown> }).props;
    assert(props['sort_dir'] === 'asc', 'sort_dir must be asc');
    const rows = props['rows'] as Array<{ id: number }>;
    assert(rows[0]?.id === 1, `asc-sorted first row must be id=1, got ${rows[0]?.id}`);
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
  testPageExportsAndSort();
  testSortDirectAscDesc();
  await testGetServerSidePropsHappy();
  await testGetServerSidePropsSortAsc();
  // eslint-disable-next-line no-console
  console.log('transparency-sovereign-cap.test : OK · 4 tests passed');
}

if (isMain) {
  runAll().catch((err) => {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  });
}
