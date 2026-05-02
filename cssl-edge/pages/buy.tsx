// apocky.com/buy · product list + Stripe-Checkout initiator
// SSG-friendly · client-side fetch only on Buy-button click
// Stub-mode-aware : if STRIPE_SECRET_KEY missing, renders "alpha free / coming soon" pill.

import type { NextPage, GetStaticProps } from 'next';
import Head from 'next/head';
import { useState } from 'react';
import { PRODUCT_CATALOG, COSMETIC_LAUNCH_PAUSED, type ProductDescriptor } from '@/lib/stripe';
import { STRIPE_CHECKOUT_INIT } from '@/lib/cap';

interface BuyProps {
  products: ReadonlyArray<ProductDescriptor>;
  stripe_configured: boolean;
  cosmetic_launch_paused: boolean;
}

interface CheckoutResponseShape {
  ok?: boolean;
  url?: string;
  stub?: boolean;
  error?: string;
}

const Buy: NextPage<BuyProps> = ({ products, stripe_configured, cosmetic_launch_paused }) => {
  const [pendingId, setPendingId] = useState<string | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  async function onBuy(p: ProductDescriptor): Promise<void> {
    setErrorMsg(null);
    setPendingId(p.id);
    try {
      if (p.tier === 'alpha-free') {
        window.location.href = '/download';
        return;
      }
      const origin = typeof window !== 'undefined' ? window.location.origin : 'https://apocky.com';
      const res = await fetch('/api/payments/stripe/checkout', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          product_id: p.id,
          success_url: `${origin}/account?paid=${p.id}`,
          cancel_url: `${origin}/buy?cancelled=${p.id}`,
          cap: STRIPE_CHECKOUT_INIT,
        }),
      });
      const data = (await res.json()) as CheckoutResponseShape;
      if (data.stub === true) {
        setErrorMsg('Stripe is in stub-mode on this deploy. Check back soon · alpha is free at /download in the meantime.');
        return;
      }
      if (data.ok === true && typeof data.url === 'string' && data.url.length > 0) {
        window.location.href = data.url;
        return;
      }
      setErrorMsg(data.error ?? 'unknown error');
    } catch (err) {
      setErrorMsg(err instanceof Error ? err.message : 'network error');
    } finally {
      setPendingId(null);
    }
  }

  return (
    <>
      <Head>
        <title>Buy · Apocky</title>
        <meta name="description" content="Cosmetic-channel-only monetization · zero pay-for-power · 14-day no-questions refund · sovereignty respected." />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <meta name="theme-color" content="#0a0a0f" />
        <link rel="canonical" href="https://apocky.com/buy" />
        <style>{`
          * { box-sizing: border-box; }
          html, body { margin: 0; padding: 0; }
          body {
            background: radial-gradient(ellipse at top, #15151f 0%, #0a0a0f 50%, #050507 100%);
            color: #e6e6f0;
            font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
            min-height: 100vh;
            -webkit-font-smoothing: antialiased;
          }
          a { color: inherit; text-decoration: none; }
          a:hover { opacity: 0.85; }
          button:disabled { opacity: 0.5; cursor: not-allowed; }
        `}</style>
      </Head>
      <main
        style={{
          maxWidth: 880,
          margin: '0 auto',
          padding: '4rem 1.5rem 6rem',
          lineHeight: 1.65,
        }}
      >
        <a href="/" style={{ fontSize: '0.85rem', color: '#7a7a8c', display: 'inline-block', marginBottom: '2rem' }}>
          ← apocky.com
        </a>

        <h1
          style={{
            fontSize: 'clamp(1.75rem, 4vw, 2.5rem)',
            margin: 0,
            fontWeight: 700,
            letterSpacing: '-0.02em',
            backgroundImage: 'linear-gradient(135deg, #ffffff 0%, #c084fc 60%, #7dd3fc 100%)',
            WebkitBackgroundClip: 'text',
            WebkitTextFillColor: 'transparent',
          }}
        >
          Buy · Sustain · Receive
        </h1>
        <p style={{ color: '#a8a8b8', marginTop: '0.5rem', fontSize: '0.95rem' }}>
          § Cosmetic-channel-only monetization. Zero pay-for-power. 14-day no-questions refund. Sovereignty respected.
        </p>

        {cosmetic_launch_paused ? (
          <div
            style={{
              marginTop: '1.5rem',
              padding: '0.9rem 1.1rem',
              background: 'rgba(192, 132, 252, 0.08)',
              border: '1px solid rgba(192, 132, 252, 0.4)',
              borderRadius: 6,
              fontSize: '0.85rem',
              color: '#c084fc',
            }}
          >
            <strong>§ cosmetics phase-2 · main game first</strong> — the cosmetic store is held until LoA's main game ships.
            Alpha is{' '}
            <a href="/download" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>free at /download</a>{' '}
            and the support pathway today.
          </div>
        ) : null}

        {!stripe_configured ? (
          <div
            style={{
              marginTop: '1.5rem',
              padding: '0.9rem 1.1rem',
              background: 'rgba(251, 191, 36, 0.08)',
              border: '1px solid rgba(251, 191, 36, 0.4)',
              borderRadius: 6,
              fontSize: '0.85rem',
              color: '#fbbf24',
            }}
          >
            ⚠ Stripe is in <strong>stub-mode</strong> on this deploy — clicking Buy will not charge. The alpha download is{' '}
            <a href="/download" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>free</a> in the meantime.
          </div>
        ) : null}

        {errorMsg !== null ? (
          <div
            role="alert"
            style={{
              marginTop: '1.25rem',
              padding: '0.9rem 1.1rem',
              background: 'rgba(248, 113, 113, 0.1)',
              border: '1px solid rgba(248, 113, 113, 0.4)',
              borderRadius: 6,
              fontSize: '0.85rem',
              color: '#fca5a5',
            }}
          >
            {errorMsg}
          </div>
        ) : null}

        {/* ── PRODUCT GRID ── */}
        <section
          style={{
            marginTop: '2.5rem',
            display: 'grid',
            gridTemplateColumns: 'repeat(auto-fill, minmax(320px, 1fr))',
            gap: '1.25rem',
          }}
        >
          {products.map((p) => {
            const free = p.tier === 'alpha-free';
            const dollars = (p.price_cents / 100).toFixed(2);
            const subscript = free ? 'free during alpha' : p.tier === 'subscription' ? `$${dollars} / month` : `$${dollars} one-time`;
            return (
              <div
                key={p.id}
                style={{
                  padding: '1.5rem',
                  background: 'rgba(20, 20, 30, 0.5)',
                  border: '1px solid #1f1f2a',
                  borderRadius: 8,
                  display: 'flex',
                  flexDirection: 'column',
                }}
              >
                <div
                  style={{
                    fontSize: '0.65rem',
                    letterSpacing: '0.15em',
                    color: free ? '#34d399' : p.tier === 'subscription' ? '#a78bfa' : '#7dd3fc',
                    marginBottom: '0.5rem',
                    textTransform: 'uppercase',
                  }}
                >
                  {p.tier === 'alpha-free' ? 'ALPHA · FREE' : p.tier === 'subscription' ? 'SUBSCRIPTION' : 'COSMETIC'}
                </div>
                <h3 style={{ margin: 0, fontSize: '1.05rem', fontWeight: 600, color: '#e6e6f0' }}>
                  {p.display_name}
                </h3>
                <p style={{ fontSize: '0.85rem', color: '#a0a0b0', marginTop: '0.6rem', flexGrow: 1 }}>
                  {p.blurb}
                </p>
                <div style={{ marginTop: '1rem', display: 'flex', alignItems: 'baseline', justifyContent: 'space-between' }}>
                  <span style={{ fontSize: '0.95rem', color: '#cdd6e4', fontWeight: 600 }}>
                    {free ? 'free' : `$${dollars}`}
                  </span>
                  <span style={{ fontSize: '0.75rem', color: '#7a7a8c' }}>{subscript}</span>
                </div>
                <button
                  type="button"
                  onClick={() => { void onBuy(p); }}
                  disabled={pendingId !== null}
                  style={{
                    marginTop: '1.1rem',
                    padding: '0.7rem 1.1rem',
                    background: free
                      ? 'linear-gradient(135deg, #34d399 0%, #7dd3fc 100%)'
                      : 'linear-gradient(135deg, #c084fc 0%, #7dd3fc 100%)',
                    color: '#0a0a0f',
                    fontWeight: 700,
                    border: 'none',
                    borderRadius: 6,
                    fontSize: '0.9rem',
                    cursor: 'pointer',
                  }}
                >
                  {pendingId === p.id ? 'Redirecting …' : free ? 'Download alpha →' : 'Buy →'}
                </button>
              </div>
            );
          })}
        </section>

        {/* ── REFUND POLICY ── */}
        <section style={{ marginTop: '3rem' }}>
          <h2
            style={{
              fontSize: '0.7rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: '#7a7a8c',
              marginBottom: '0.8rem',
            }}
          >
            § Refund Policy
          </h2>
          <p style={{ fontSize: '0.9rem', color: '#cdd6e4' }}>
            14-day no-questions-asked refund · honors CA-Bus-Prof-§17602(b) auto-renewal-notice · cancel
            subscriptions any time at <a href="/account" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>/account</a> ·
            email <a href="mailto:apocky13@gmail.com?subject=%5Brefund%5D" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>apocky13@gmail.com</a> if Stripe is unreachable.
          </p>
        </section>

        {/* ── FOOTER ── */}
        <footer
          style={{
            marginTop: '4rem',
            paddingTop: '2.5rem',
            borderTop: '1px solid #1f1f2a',
            color: '#5a5a6a',
            fontSize: '0.78rem',
          }}
        >
          <p style={{ margin: 0 }}>§ ¬ harm in the making · sovereignty preserved · t∞</p>
          <p style={{ margin: '0.4rem 0 0' }}>
            © {new Date().getFullYear()} Apocky · cosmetic-channel-only · zero pay-for-power
          </p>
        </footer>
      </main>
    </>
  );
};

export const getStaticProps: GetStaticProps<BuyProps> = async () => {
  return {
    props: {
      products: PRODUCT_CATALOG.filter((p) => p.visible),
      stripe_configured: typeof process.env['STRIPE_SECRET_KEY'] === 'string' && process.env['STRIPE_SECRET_KEY'].length > 0,
      cosmetic_launch_paused: COSMETIC_LAUNCH_PAUSED,
    },
  };
};

export default Buy;
