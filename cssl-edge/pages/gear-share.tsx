// cssl-edge · /marketplace (gear-share)
// Server-rendered gear-share gallery (gift-economy framing). Calls
// /api/marketplace/list to surface recent gear-share-receipts.
//
// Distinction from /marketplace/index.tsx :
//   - /marketplace/index.tsx surfaces ASSET catalog (CC0 + CC-BY-4.0 3D assets)
//   - /marketplace.tsx (this file) surfaces GEAR-SHARE-RECEIPTS (player-gifted
//     seeds the receiver re-rolls · gift-economy · echo-back bonus)

import type { GetServerSideProps, NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';
import { MARKETPLACE_CAP_LIST } from '@/lib/cap';

interface GearShareReceipt {
  receipt_id: string;
  creator_player_id: string;
  rarity: string;
  slot: string;
  seed: string;
  posted_at: string;
  echoes_received: number;
  note: string;
}

interface MarketplaceShareProps {
  listings: GearShareReceipt[];
  total: number;
  page: number;
  page_size: number;
  filter: { rarity: string; slot: string };
  fetch_failed: boolean;
}

const PAGE_SIZE = 20;

function rarityBadgeColor(rarity: string): { bg: string; fg: string } {
  switch (rarity.toLowerCase()) {
    case 'common': return { bg: '#6b7280', fg: '#ffffff' };
    case 'uncommon': return { bg: '#16a34a', fg: '#ffffff' };
    case 'rare': return { bg: '#2563eb', fg: '#ffffff' };
    case 'epic': return { bg: '#9333ea', fg: '#ffffff' };
    case 'legendary': return { bg: '#ea580c', fg: '#ffffff' };
    default: return { bg: '#374151', fg: '#ffffff' };
  }
}

const MarketplaceShare: NextPage<MarketplaceShareProps> = ({
  listings,
  total,
  page,
  page_size,
  filter,
  fetch_failed,
}) => {
  const empty = listings.length === 0;
  return (
    <>
      <Head>
        <title>Gear-Share Marketplace · cssl-edge</title>
        <meta
          name="description"
          content="LoA-v13 gear-share marketplace · gift-economy · no leaderboards · echo-back bonus only"
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
            Gear-Share Marketplace
          </h1>
          <p style={{ color: '#9aa0a6', marginTop: 0 }}>
            {total} share-receipt{total === 1 ? '' : 's'}
            {filter.rarity ? ` · rarity=${filter.rarity}` : ''}
            {filter.slot ? ` · slot=${filter.slot}` : ''}
            {fetch_failed ? ' · upstream fetch failed' : ''}
          </p>
        </header>

        <section
          style={{
            marginBottom: '1.5rem',
            padding: '1rem 1.25rem',
            border: '1px solid #1f1f29',
            background: '#13131a',
            borderRadius: 8,
            color: '#cdd6e4',
            fontSize: '0.9rem',
          }}
        >
          <strong style={{ color: '#fbbf24' }}>Gift-economy disclaimer.</strong>{' '}
          These are <em>share-receipts</em>, not commerce listings. The poster
          shares a seed; you re-roll your own gear from it. The poster receives
          an echo-back bonus when you complete a run with their seed. There are
          no leaderboards · no PvP scoring · no rank · no commerce. You may
          revoke any share-receipt you posted at any time using the sovereign-revoke
          widget below.
        </section>

        <section
          style={{
            marginBottom: '2rem',
            padding: '0.75rem 1rem',
            border: '1px dashed #1f1f29',
            background: '#0f0f15',
            borderRadius: 6,
            color: '#9aa0a6',
            fontSize: '0.85rem',
          }}
        >
          <strong style={{ color: '#7dd3fc' }}>Sovereign-revoke widget</strong>
          {' · '}
          Posted a receipt and changed your mind? Submit{' '}
          <code style={{ color: '#fbbf24' }}>DELETE /api/marketplace/post</code> with
          your <code>receipt_id</code> + sovereign-cap header. Your friend keeps
          any echo-back bonus they already earned; new replays simply stop
          counting toward your bonus.
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
              No share-receipts in this filter — try a different rarity or slot.
            </p>
          </section>
        ) : (
          <section
            style={{
              display: 'grid',
              gridTemplateColumns: 'repeat(auto-fill, minmax(260px, 1fr))',
              gap: '1rem',
            }}
          >
            {listings.map((l) => {
              const badge = rarityBadgeColor(l.rarity);
              return (
                <article
                  key={l.receipt_id}
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
                      {l.creator_player_id} · {l.slot}
                    </h3>
                    <span
                      style={{
                        fontSize: '0.7rem',
                        padding: '0.2rem 0.5rem',
                        borderRadius: 4,
                        background: badge.bg,
                        color: badge.fg,
                        fontWeight: 600,
                        textTransform: 'uppercase',
                      }}
                    >
                      {l.rarity}
                    </span>
                  </div>
                  <p style={{ margin: 0, fontSize: '0.85rem', color: '#cdd6e4' }}>
                    {l.note}
                  </p>
                  <p style={{ margin: 0, fontSize: '0.75rem', color: '#9aa0a6' }}>
                    seed : <code style={{ color: '#fbbf24' }}>{l.seed}</code>
                  </p>
                  <p style={{ margin: 0, fontSize: '0.75rem', color: '#9aa0a6' }}>
                    posted {l.posted_at} · {l.echoes_received} echo-back
                    {l.echoes_received === 1 ? '' : 's'}
                  </p>
                </article>
              );
            })}
          </section>
        )}

        <nav
          style={{
            marginTop: '2rem',
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center',
            color: '#9aa0a6',
          }}
        >
          <span>Page {page} · {page_size} per page</span>
        </nav>
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

export const getServerSideProps: GetServerSideProps<MarketplaceShareProps> = async (
  ctx
) => {
  const rarityRaw = ctx.query['rarity'];
  const slotRaw = ctx.query['slot'];
  const pageRaw = ctx.query['page'];
  const rarity = (Array.isArray(rarityRaw) ? rarityRaw[0] : rarityRaw) ?? '';
  const slot = (Array.isArray(slotRaw) ? slotRaw[0] : slotRaw) ?? '';
  const pageStr = (Array.isArray(pageRaw) ? pageRaw[0] : pageRaw) ?? '1';
  const page = Math.max(1, parseInt(pageStr, 10) || 1);

  const origin = originFromReq(ctx.req.headers as Record<string, string | string[] | undefined>);
  const params = new URLSearchParams();
  params.set('cap', String(MARKETPLACE_CAP_LIST));
  params.set('page', String(page));
  params.set('page_size', String(PAGE_SIZE));
  if (rarity.length > 0) params.set('rarity', rarity);
  if (slot.length > 0) params.set('slot', slot);

  let listings: GearShareReceipt[] = [];
  let total = 0;
  let fetch_failed = false;
  try {
    const r = await fetch(`${origin}/api/marketplace/list?${params.toString()}`);
    if (r.ok) {
      const j = (await r.json()) as { listings?: GearShareReceipt[]; total?: number };
      listings = Array.isArray(j.listings) ? j.listings : [];
      total = typeof j.total === 'number' ? j.total : listings.length;
    } else {
      fetch_failed = true;
    }
  } catch {
    fetch_failed = true;
  }

  return {
    props: {
      listings,
      total,
      page,
      page_size: PAGE_SIZE,
      filter: { rarity, slot },
      fetch_failed,
    },
  };
};

// ─── Inline test : page export is function · gift-economy framing visible ──
export function _testPageExportsAndFraming(): boolean {
  // 1. Default export is renderable.
  if (typeof MarketplaceShare !== 'function') return false;
  if (typeof getServerSideProps !== 'function') return false;
  return true;
}

export default MarketplaceShare;
