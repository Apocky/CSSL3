// cssl-edge · /transparency
// Top-level transparency dashboard. Two summary cards :
//   1. Sovereign-Cap Audit  — count of cap-bypass events + last-event ts
//   2. Cocreative Bias Trend — 6-axis bias vector + recent-feedback count
//
// Server-rendered (getServerSideProps). Each card carries a "View Detail"
// link to a per-card detail page. Stub-friendly : when Supabase env-vars are
// absent, the underlying APIs return deterministic stubs and this page
// renders identically.

import type { GetServerSideProps, NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';

interface SovCapSummary {
  count: number;
  last_ts: string | null;
  last_action: string | null;
  source: 'supabase' | 'stub';
}

interface BiasSummary {
  axes_count: number;
  feedback_count: number;
  last_feedback_ts: string | null;
  updated_at: string;
  source: 'supabase' | 'stub';
}

interface TransparencyProps {
  player_id: string;
  sov_cap: SovCapSummary;
  bias: BiasSummary;
  fetch_failed: boolean;
}

const Transparency: NextPage<TransparencyProps> = ({
  player_id,
  sov_cap,
  bias,
  fetch_failed,
}) => {
  return (
    <>
      <Head>
        <title>Transparency · cssl-edge</title>
        <meta name="description" content="LoA-v13 transparency dashboard · sovereign-cap audit + cocreative bias" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
      </Head>
      <main
        style={{
          fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
          maxWidth: 1100,
          margin: '0 auto',
          padding: '3rem 1.5rem',
          color: '#e6e6e6',
          background: '#0b0b10',
          minHeight: '100vh',
          lineHeight: 1.55,
        }}
      >
        <header style={{ marginBottom: '2rem' }}>
          <Link href="/" style={{ color: '#7dd3fc', textDecoration: 'none' }}>
            ← back
          </Link>
          <h1 style={{ fontSize: '1.75rem', marginTop: '0.5rem', marginBottom: '0.25rem' }}>
            Transparency
          </h1>
          <p style={{ color: '#9aa0a6', marginTop: 0 }}>
            player_id : {player_id}
            {fetch_failed ? ' · upstream fetch failed · cards show defaults' : ''}
          </p>
        </header>

        <section
          style={{
            display: 'grid',
            gridTemplateColumns: 'repeat(auto-fit, minmax(360px, 1fr))',
            gap: '1.25rem',
          }}
        >
          {/* Sovereign-Cap Audit card */}
          <article
            style={{
              background: '#13131a',
              border: '1px solid #1f1f29',
              borderRadius: 8,
              padding: '1.25rem',
              display: 'flex',
              flexDirection: 'column',
              gap: '0.5rem',
            }}
          >
            <h2 style={{ fontSize: '1rem', textTransform: 'uppercase', letterSpacing: '0.08em', color: '#9aa0a6', margin: 0 }}>
              Sovereign-Cap Audit
            </h2>
            <p style={{ margin: 0, fontSize: '0.9rem', color: '#cdd6e4' }}>
              Cap-bypass events authorized by sovereign-cap header.
            </p>
            <dl style={{ display: 'grid', gridTemplateColumns: 'auto 1fr', gap: '0.25rem 1rem', margin: '0.75rem 0', fontSize: '0.85rem' }}>
              <dt style={{ color: '#9aa0a6' }}>events</dt>
              <dd style={{ margin: 0, color: '#7dd3fc' }}>{sov_cap.count}</dd>
              <dt style={{ color: '#9aa0a6' }}>last ts</dt>
              <dd style={{ margin: 0, color: '#cdd6e4' }}>{sov_cap.last_ts ?? '—'}</dd>
              <dt style={{ color: '#9aa0a6' }}>last action</dt>
              <dd style={{ margin: 0, color: '#cdd6e4' }}>{sov_cap.last_action ?? '—'}</dd>
              <dt style={{ color: '#9aa0a6' }}>source</dt>
              <dd style={{ margin: 0, color: '#fbbf24' }}>{sov_cap.source}</dd>
            </dl>
            <Link
              href={`/transparency/sovereign-cap?player_id=${encodeURIComponent(player_id)}`}
              style={{
                marginTop: '0.5rem',
                padding: '0.5rem 0.75rem',
                background: '#1f1f29',
                color: '#7dd3fc',
                textAlign: 'center',
                borderRadius: 4,
                textDecoration: 'none',
                fontSize: '0.85rem',
              }}
            >
              View Detail →
            </Link>
          </article>

          {/* Cocreative Bias Trend card */}
          <article
            style={{
              background: '#13131a',
              border: '1px solid #1f1f29',
              borderRadius: 8,
              padding: '1.25rem',
              display: 'flex',
              flexDirection: 'column',
              gap: '0.5rem',
            }}
          >
            <h2 style={{ fontSize: '1rem', textTransform: 'uppercase', letterSpacing: '0.08em', color: '#9aa0a6', margin: 0 }}>
              Cocreative Bias Trend
            </h2>
            <p style={{ margin: 0, fontSize: '0.9rem', color: '#cdd6e4' }}>
              Six-axis bias vector + recent feedback signals applied to it.
            </p>
            <dl style={{ display: 'grid', gridTemplateColumns: 'auto 1fr', gap: '0.25rem 1rem', margin: '0.75rem 0', fontSize: '0.85rem' }}>
              <dt style={{ color: '#9aa0a6' }}>axes</dt>
              <dd style={{ margin: 0, color: '#7dd3fc' }}>{bias.axes_count}</dd>
              <dt style={{ color: '#9aa0a6' }}>feedback rows</dt>
              <dd style={{ margin: 0, color: '#7dd3fc' }}>{bias.feedback_count}</dd>
              <dt style={{ color: '#9aa0a6' }}>last feedback</dt>
              <dd style={{ margin: 0, color: '#cdd6e4' }}>{bias.last_feedback_ts ?? '—'}</dd>
              <dt style={{ color: '#9aa0a6' }}>updated</dt>
              <dd style={{ margin: 0, color: '#cdd6e4' }}>{bias.updated_at}</dd>
              <dt style={{ color: '#9aa0a6' }}>source</dt>
              <dd style={{ margin: 0, color: '#fbbf24' }}>{bias.source}</dd>
            </dl>
            <span
              style={{
                marginTop: '0.5rem',
                padding: '0.5rem 0.75rem',
                background: '#1f1f29',
                color: '#6b7280',
                textAlign: 'center',
                borderRadius: 4,
                fontSize: '0.85rem',
                border: '1px dashed #1f1f29',
              }}
            >
              Detail page coming next session
            </span>
          </article>
        </section>

        <footer style={{ marginTop: '4rem', color: '#6b7280', fontSize: '0.85rem' }}>
          <p>
            Source : CSSLv3 · branch <code>cssl/session-15/W-W6-edge-dashboard-retry</code>
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

const BIAS_AXES_COUNT = 6;

export const getServerSideProps: GetServerSideProps<TransparencyProps> = async (
  ctx
) => {
  const playerRaw = ctx.query['player_id'];
  const player_id = (Array.isArray(playerRaw) ? playerRaw[0] : playerRaw) ?? 'anonymous';
  const origin = originFromReq(ctx.req.headers as Record<string, string | string[] | undefined>);

  let fetch_failed = false;

  let sov_cap: SovCapSummary = { count: 0, last_ts: null, last_action: null, source: 'stub' };
  try {
    const r = await fetch(
      `${origin}/api/transparency/sovereign-cap?player_id=${encodeURIComponent(player_id)}&limit=10`
    );
    if (r.ok) {
      const j = (await r.json()) as {
        rows?: Array<{ ts_iso?: string; action_kind?: string }>;
        total?: number;
        source?: 'supabase' | 'stub';
      };
      const rows = Array.isArray(j.rows) ? j.rows : [];
      const first = rows[0];
      sov_cap = {
        count: typeof j.total === 'number' ? j.total : rows.length,
        last_ts: first !== undefined && typeof first.ts_iso === 'string' ? first.ts_iso : null,
        last_action: first !== undefined && typeof first.action_kind === 'string' ? first.action_kind : null,
        source: j.source === 'supabase' ? 'supabase' : 'stub',
      };
    } else {
      fetch_failed = true;
    }
  } catch {
    fetch_failed = true;
  }

  let bias: BiasSummary = {
    axes_count: BIAS_AXES_COUNT,
    feedback_count: 0,
    last_feedback_ts: null,
    updated_at: '—',
    source: 'stub',
  };
  try {
    const r = await fetch(
      `${origin}/api/transparency/cocreative-bias?player_id=${encodeURIComponent(player_id)}&limit=10`
    );
    if (r.ok) {
      const j = (await r.json()) as {
        bias_vector?: { updated_at?: string };
        feedback?: Array<{ ts_iso?: string }>;
        total?: number;
        source?: 'supabase' | 'stub';
      };
      const fb = Array.isArray(j.feedback) ? j.feedback : [];
      const first = fb[0];
      bias = {
        axes_count: BIAS_AXES_COUNT,
        feedback_count: typeof j.total === 'number' ? j.total : fb.length,
        last_feedback_ts: first !== undefined && typeof first.ts_iso === 'string' ? first.ts_iso : null,
        updated_at:
          j.bias_vector !== undefined && typeof j.bias_vector.updated_at === 'string'
            ? j.bias_vector.updated_at
            : '—',
        source: j.source === 'supabase' ? 'supabase' : 'stub',
      };
    } else {
      fetch_failed = true;
    }
  } catch {
    fetch_failed = true;
  }

  return {
    props: { player_id, sov_cap, bias, fetch_failed },
  };
};

// ─── Inline test : page export is renderable ───────────────────────────────
// Pages-router does NOT auto-execute named exports on import, so it's safe to
// expose this for the test harness.
export function _testExportsAreFunctions(): boolean {
  if (typeof Transparency !== 'function') return false;
  if (typeof getServerSideProps !== 'function') return false;
  return true;
}

export default Transparency;
