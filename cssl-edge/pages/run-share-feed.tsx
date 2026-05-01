// cssl-edge · /run-share-feed
// Server-rendered friend-list run-share-feed (gift-economy framing).
// Calls /api/run-share/feed to surface friend run-replays.

import type { GetServerSideProps, NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';
import { RUN_SHARE_CAP_RECEIVE } from '@/lib/cap';

interface RunShareFeedItem {
  receipt_id: string;
  player_id: string;
  seed: string;
  scoring: { runtime_s: number; depth: number; completed: boolean };
  screenshot_handle: string;
  note: string;
  posted_at: string;
  echoes_received: number;
}

interface RunShareFeedProps {
  feed: RunShareFeedItem[];
  total: number;
  player_id: string;
  friend_list: string;
  fetch_failed: boolean;
}

function formatRuntime(secs: number): string {
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return `${m}m${s.toString().padStart(2, '0')}s`;
}

const RunShareFeed: NextPage<RunShareFeedProps> = ({
  feed,
  total,
  player_id,
  friend_list,
  fetch_failed,
}) => {
  const empty = feed.length === 0;
  return (
    <>
      <Head>
        <title>Run-Share Feed · cssl-edge</title>
        <meta
          name="description"
          content="LoA-v13 run-share feed · gift-economy · friend run-replays · no leaderboards"
        />
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
          <Link href="/" style={{ color: '#7dd3fc', textDecoration: 'none' }}>
            ← back
          </Link>
          <h1 style={{ fontSize: '1.75rem', marginTop: '0.5rem', marginBottom: '0.25rem' }}>
            Run-Share Feed
          </h1>
          <p style={{ color: '#9aa0a6', marginTop: 0 }}>
            player_id : {player_id} · {total} run{total === 1 ? '' : 's'}
            {friend_list ? ` · friends : ${friend_list}` : ''}
            {fetch_failed ? ' · upstream fetch failed' : ''}
          </p>
        </header>

        <section
          style={{
            marginBottom: '2rem',
            padding: '1rem 1.25rem',
            border: '1px solid #1f1f29',
            background: '#13131a',
            borderRadius: 8,
            color: '#cdd6e4',
            fontSize: '0.9rem',
          }}
        >
          <strong style={{ color: '#fbbf24' }}>Gift-economy framing.</strong>{' '}
          Friends share runs as gifts. You can attempt their seed; you are{' '}
          <em>not scored against</em> them. Completing a friend's seed sends them
          an echo-back bonus — the only multiplayer reward in the game. There are
          no leaderboards · no PvP · no rank.
        </section>

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
            <p style={{ fontSize: '1rem', margin: 0 }}>
              No friend runs in this feed yet. Add friends + their runs will
              appear here when they share.
            </p>
          </section>
        ) : (
          <section
            style={{
              display: 'grid',
              gridTemplateColumns: 'repeat(auto-fill, minmax(320px, 1fr))',
              gap: '1rem',
            }}
          >
            {feed.map((r) => (
              <article
                key={r.receipt_id}
                style={{
                  background: '#13131a',
                  border: '1px solid #1f1f29',
                  borderRadius: 8,
                  padding: '1rem',
                  display: 'flex',
                  flexDirection: 'column',
                  gap: '0.5rem',
                }}
              >
                <div
                  style={{
                    display: 'flex',
                    justifyContent: 'space-between',
                    alignItems: 'center',
                  }}
                >
                  <h3 style={{ fontSize: '0.95rem', margin: 0, color: '#e6e6e6' }}>
                    {r.player_id}
                  </h3>
                  <span
                    style={{
                      fontSize: '0.7rem',
                      padding: '0.2rem 0.5rem',
                      borderRadius: 4,
                      background: r.scoring.completed ? '#16a34a' : '#6b7280',
                      color: '#ffffff',
                      fontWeight: 600,
                    }}
                  >
                    {r.scoring.completed ? 'COMPLETED' : 'DIED'}
                  </span>
                </div>
                <p style={{ margin: 0, fontSize: '0.85rem', color: '#cdd6e4' }}>
                  {r.note}
                </p>
                <p style={{ margin: 0, fontSize: '0.75rem', color: '#9aa0a6' }}>
                  depth {r.scoring.depth} · {formatRuntime(r.scoring.runtime_s)}
                </p>
                <p style={{ margin: 0, fontSize: '0.75rem', color: '#9aa0a6' }}>
                  seed : <code style={{ color: '#fbbf24' }}>{r.seed}</code>
                </p>
                <p style={{ margin: 0, fontSize: '0.75rem', color: '#9aa0a6' }}>
                  posted {r.posted_at} · {r.echoes_received} echo-back
                  {r.echoes_received === 1 ? '' : 's'}
                </p>
              </article>
            ))}
          </section>
        )}
      </main>
    </>
  );
};

function originFromReq(reqHeaders: Record<string, string | string[] | undefined>): string {
  if (process.env['VERCEL_URL']) return `https://${process.env['VERCEL_URL']}`;
  const host = reqHeaders['host'];
  const h = Array.isArray(host) ? host[0] : host;
  return h ? `http://${h}` : 'http://localhost:3000';
}

export const getServerSideProps: GetServerSideProps<RunShareFeedProps> = async (
  ctx
) => {
  const playerRaw = ctx.query['player_id'];
  const friendsRaw = ctx.query['friend_list'];
  const player_id = (Array.isArray(playerRaw) ? playerRaw[0] : playerRaw) ?? 'anonymous';
  const friend_list = (Array.isArray(friendsRaw) ? friendsRaw[0] : friendsRaw) ?? '';

  const origin = originFromReq(ctx.req.headers as Record<string, string | string[] | undefined>);
  const params = new URLSearchParams();
  params.set('cap', String(RUN_SHARE_CAP_RECEIVE));
  params.set('player_id', player_id);
  if (friend_list.length > 0) params.set('friend_list', friend_list);
  params.set('limit', '40');

  let feed: RunShareFeedItem[] = [];
  let total = 0;
  let fetch_failed = false;
  try {
    const r = await fetch(`${origin}/api/run-share/feed?${params.toString()}`);
    if (r.ok) {
      const j = (await r.json()) as { feed?: RunShareFeedItem[]; total?: number };
      feed = Array.isArray(j.feed) ? j.feed : [];
      total = typeof j.total === 'number' ? j.total : feed.length;
    } else {
      fetch_failed = true;
    }
  } catch {
    fetch_failed = true;
  }

  return {
    props: {
      feed,
      total,
      player_id,
      friend_list,
      fetch_failed,
    },
  };
};

// ─── Inline test : page export is function ─────────────────────────────────
export function _testPageExportsAndFraming(): boolean {
  if (typeof RunShareFeed !== 'function') return false;
  if (typeof getServerSideProps !== 'function') return false;
  return true;
}

export default RunShareFeed;
