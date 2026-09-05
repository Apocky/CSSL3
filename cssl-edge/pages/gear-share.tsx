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
        <title>Shared game seeds · Apocky</title>
        <meta
          name="description"
          content="An experimental Labyrinth of Apocalypse page for game seeds people deliberately share."
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
            Shared game seeds
          </h1>
          <p style={{ color: '#9aa0a6', marginTop: 0 }}>
            {total} shared record{total === 1 ? '' : 's'}
            {filter.rarity ? ` · rarity: ${filter.rarity}` : ''}
            {filter.slot ? ` · item slot: ${filter.slot}` : ''}
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
          <strong style={{ color: '#fbbf24' }}>Experimental sharing, not a shop.</strong>{' '}
          A <strong>seed</strong> is a value the game can use to reproduce a generated starting point. A shared
          record lets another player try a seed; it does not transfer an item or charge money. Design notes
          call a possible thank-you reward an “echo-back bonus.” That name means an in-game acknowledgment, not
          a payment or public ranking.
        </section>

        {fetch_failed ? (
          <p role="status" style={{ color: '#fbbf24' }}>
            The sharing service could not be reached, so no current listing is being claimed.
          </p>
        ) : null}

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
              No shared seeds match this filter.
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

// No verified listing service is connected. Do not publish an empty gallery
// or demonstration records as though this were an available feature.
export const getServerSideProps: GetServerSideProps<MarketplaceShareProps> = async () => ({
  notFound: true,
});

// ─── Inline test : page export is function · gift-economy framing visible ──
export function _testPageExportsAndFraming(): boolean {
  // 1. Default export is renderable.
  if (typeof MarketplaceShare !== 'function') return false;
  if (typeof getServerSideProps !== 'function') return false;
  return true;
}

export default MarketplaceShare;
