// cssl-edge · /transparency/sovereign-cap
// Detail page for sovereign-cap audit. Shows the player's full cap-bypass
// audit log as a sortable table.
//
// Server-rendered (getServerSideProps) — calls /api/transparency/sovereign-cap
// on the same deployment. Sortable by timestamp ; default = most-recent first.

import type { GetServerSideProps, NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';

interface SovereignCapAuditRow {
  id: number;
  player_id: string;
  ts_iso: string;
  action_kind: string;
  cap_bypassed_kind: string;
  reason: string;
  caller_origin: string;
}

type SortDir = 'asc' | 'desc';

interface SovCapPageProps {
  player_id: string;
  rows: SovereignCapAuditRow[];
  total: number;
  sort_dir: SortDir;
  source: 'supabase' | 'stub' | 'unknown';
  fetch_failed: boolean;
}

// Render a single row of the audit table.
function AuditRow({ row }: { row: SovereignCapAuditRow }): JSX.Element {
  return (
    <tr style={{ borderBottom: '1px solid #1f1f29' }}>
      <td style={{ padding: '0.5rem 0.6rem', fontFamily: 'monospace', color: '#cdd6e4', whiteSpace: 'nowrap' }}>
        {row.ts_iso}
      </td>
      <td style={{ padding: '0.5rem 0.6rem', color: '#7dd3fc' }}>{row.action_kind}</td>
      <td style={{ padding: '0.5rem 0.6rem', color: '#fbbf24' }}>{row.cap_bypassed_kind}</td>
      <td style={{ padding: '0.5rem 0.6rem', color: '#cdd6e4', fontSize: '0.85rem' }}>{row.reason}</td>
      <td style={{ padding: '0.5rem 0.6rem', color: '#9aa0a6', fontSize: '0.85rem' }}>{row.caller_origin}</td>
    </tr>
  );
}

const SovCapPage: NextPage<SovCapPageProps> = ({
  player_id,
  rows,
  total,
  sort_dir,
  source,
  fetch_failed,
}) => {
  const empty = rows.length === 0;
  const otherDir: SortDir = sort_dir === 'desc' ? 'asc' : 'desc';
  const toggleHref = `/transparency/sovereign-cap?player_id=${encodeURIComponent(player_id)}&sort=${otherDir}`;
  return (
    <>
      <Head>
        <title>Sovereign-Cap Audit · cssl-edge</title>
        <meta name="description" content="Sovereign-cap audit detail · LoA-v13 transparency" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
      </Head>
      <main
        style={{
          fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
          maxWidth: 1200,
          margin: '0 auto',
          padding: '3rem 1.5rem',
          color: '#e6e6e6',
          background: '#0b0b10',
          minHeight: '100vh',
          lineHeight: 1.55,
        }}
      >
        <header style={{ marginBottom: '2rem' }}>
          <Link href="/transparency" style={{ color: '#7dd3fc', textDecoration: 'none' }}>
            ← transparency
          </Link>
          <h1 style={{ fontSize: '1.75rem', marginTop: '0.5rem', marginBottom: '0.25rem' }}>
            Sovereign-Cap Audit
          </h1>
          <p style={{ color: '#9aa0a6', marginTop: 0 }}>
            player_id : {player_id} · {total} event{total === 1 ? '' : 's'} · source :
            <span style={{ color: '#fbbf24' }}> {source}</span>
            {fetch_failed ? ' · upstream fetch failed' : ''}
          </p>
        </header>

        {empty ? (
          <section
            style={{
              padding: '3rem 1rem',
              textAlign: 'center',
              border: '1px dashed #1f1f29',
              borderRadius: 8,
              color: '#9aa0a6',
            }}
          >
            <p style={{ fontSize: '1rem', margin: 0 }}>No sovereign-cap events recorded.</p>
          </section>
        ) : (
          <section
            style={{
              background: '#13131a',
              border: '1px solid #1f1f29',
              borderRadius: 8,
              overflowX: 'auto',
            }}
          >
            <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: '0.9rem' }}>
              <thead>
                <tr style={{ background: '#0b0b10', color: '#9aa0a6', textAlign: 'left' }}>
                  <th style={{ padding: '0.6rem' }}>
                    <Link href={toggleHref} style={{ color: '#9aa0a6', textDecoration: 'none' }}>
                      timestamp {sort_dir === 'desc' ? '↓' : '↑'}
                    </Link>
                  </th>
                  <th style={{ padding: '0.6rem' }}>action_kind</th>
                  <th style={{ padding: '0.6rem' }}>cap_bypassed_kind</th>
                  <th style={{ padding: '0.6rem' }}>reason</th>
                  <th style={{ padding: '0.6rem' }}>caller_origin</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((row) => (
                  <AuditRow key={row.id} row={row} />
                ))}
              </tbody>
            </table>
          </section>
        )}

        <footer style={{ marginTop: '4rem', color: '#6b7280', fontSize: '0.85rem' }}>
          <p>
            Sort {sort_dir === 'desc' ? 'descending' : 'ascending'} by timestamp.
            Toggle by clicking the column header.
          </p>
        </footer>
      </main>
    </>
  );
};

// ─── server-side data fetch ────────────────────────────────────────────────

function originFromReq(reqHeaders: Record<string, string | string[] | undefined>): string {
  if (process.env['VERCEL_URL']) return `https://${process.env['VERCEL_URL']}`;
  const host = reqHeaders['host'];
  const h = Array.isArray(host) ? host[0] : host;
  return h ? `http://${h}` : 'http://localhost:3000';
}

// Sort rows by ts_iso. Used by both the page and a unit test below.
export function sortAuditRows(
  rows: SovereignCapAuditRow[],
  dir: SortDir
): SovereignCapAuditRow[] {
  const copy = rows.slice();
  copy.sort((a, b) => {
    if (a.ts_iso === b.ts_iso) return 0;
    if (dir === 'desc') return a.ts_iso > b.ts_iso ? -1 : 1;
    return a.ts_iso < b.ts_iso ? -1 : 1;
  });
  return copy;
}

export const getServerSideProps: GetServerSideProps<SovCapPageProps> = async (
  ctx
) => {
  const playerRaw = ctx.query['player_id'];
  const sortRaw = ctx.query['sort'];

  const player_id = (Array.isArray(playerRaw) ? playerRaw[0] : playerRaw) ?? 'anonymous';
  const sortStr = (Array.isArray(sortRaw) ? sortRaw[0] : sortRaw) ?? 'desc';
  const sort_dir: SortDir = sortStr === 'asc' ? 'asc' : 'desc';

  const origin = originFromReq(ctx.req.headers as Record<string, string | string[] | undefined>);

  let rows: SovereignCapAuditRow[] = [];
  let total = 0;
  let source: 'supabase' | 'stub' | 'unknown' = 'unknown';
  let fetch_failed = false;
  try {
    const r = await fetch(
      `${origin}/api/transparency/sovereign-cap?player_id=${encodeURIComponent(player_id)}&limit=200`
    );
    if (r.ok) {
      const j = (await r.json()) as {
        rows?: SovereignCapAuditRow[];
        total?: number;
        source?: 'supabase' | 'stub';
      };
      rows = Array.isArray(j.rows) ? j.rows : [];
      total = typeof j.total === 'number' ? j.total : rows.length;
      source = j.source === 'supabase' ? 'supabase' : 'stub';
    } else {
      fetch_failed = true;
    }
  } catch {
    fetch_failed = true;
  }

  const sorted = sortAuditRows(rows, sort_dir);

  return {
    props: {
      player_id,
      rows: sorted,
      total,
      sort_dir,
      source,
      fetch_failed,
    },
  };
};

// ─── Inline test : sortAuditRows works · page export is function ───────────
export function _testSortAndExports(): boolean {
  // 1. Default export is renderable.
  if (typeof SovCapPage !== 'function') return false;
  if (typeof getServerSideProps !== 'function') return false;
  // 2. sortAuditRows produces correct ordering.
  const sample: SovereignCapAuditRow[] = [
    { id: 1, player_id: 'p', ts_iso: '2026-01-01T10:00:00.000Z', action_kind: 'a', cap_bypassed_kind: 'A', reason: '', caller_origin: '' },
    { id: 2, player_id: 'p', ts_iso: '2026-01-03T10:00:00.000Z', action_kind: 'b', cap_bypassed_kind: 'B', reason: '', caller_origin: '' },
    { id: 3, player_id: 'p', ts_iso: '2026-01-02T10:00:00.000Z', action_kind: 'c', cap_bypassed_kind: 'C', reason: '', caller_origin: '' },
  ];
  const desc = sortAuditRows(sample, 'desc');
  if (desc[0]?.id !== 2) return false;
  if (desc[1]?.id !== 3) return false;
  if (desc[2]?.id !== 1) return false;
  const asc = sortAuditRows(sample, 'asc');
  if (asc[0]?.id !== 1) return false;
  if (asc[2]?.id !== 2) return false;
  return true;
}

export default SovCapPage;
