// apocky.com/products/early-access · Tier-2 apocky.com Early-Access sales page
// per spec/57_MONETIZATION_PIVOT + spec/59_GROK_VELOCITY_RESPONSE

import type { GetServerSideProps, NextPage } from 'next';
import Head from 'next/head';
import { useState } from 'react';
import { findProduct, type ProductDescriptor } from '@/lib/stripe';
import { STRIPE_CHECKOUT_INIT } from '@/lib/cap';

interface EAProps {
  products: ProductDescriptor[];
  stripe_configured: boolean;
}

const TIER_IDS = ['apocky-early-access', 'apocky-studio', 'apocky-lifetime'];

const EarlyAccessPage: NextPage<EAProps> = ({ products, stripe_configured }) => {
  const [busy, setBusy] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);

  async function checkout(productId: string) {
    setBusy(productId);
    setErr(null);
    try {
      const res = await fetch('/api/payments/stripe/checkout', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          product_id: productId,
          success_url: 'https://apocky.com/products/early-access?success=1',
          cancel_url: 'https://apocky.com/products/early-access?cancelled=1',
          cap: STRIPE_CHECKOUT_INIT,
        }),
      });
      const data = await res.json();
      if (data?.checkout_url) {
        window.location.href = data.checkout_url;
        return;
      }
      if (data?.stub) {
        setErr(`stub-mode: ${data.message ?? 'wire STRIPE_PRICE_EARLY_ACCESS env-vars to enable real checkout'}`);
      } else if (data?.error) {
        setErr(data.error);
      } else {
        setErr('checkout returned unexpected shape');
      }
    } catch (e) {
      setErr(e instanceof Error ? e.message : 'network error');
    } finally {
      setBusy(null);
    }
  }

  return (
    <>
      <Head>
        <title>Early-Access · apocky.com</title>
        <meta name="description" content="Watch the Infinity Engine + CSSL stack come together in real-time. Private alpha builds. Private Discord. Spec-retro feed. From $19/mo · Lifetime $999." />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <meta name="theme-color" content="#0a0a0f" />
        <meta property="og:title" content="apocky.com · Early-Access" />
        <meta property="og:description" content="Watch the Infinity Engine come together in real-time. Private builds. Private Discord. From $19/mo." />
        <meta property="og:url" content="https://apocky.com/products/early-access" />
        <link rel="canonical" href="https://apocky.com/products/early-access" />
        <style>{`
          * { box-sizing: border-box; }
          html, body { margin: 0; padding: 0; }
          body {
            background: radial-gradient(ellipse at top, #15151f 0%, #0a0a0f 50%, #050507 100%);
            color: #e6e6f0;
            font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
            min-height: 100vh;
          }
          a { color: inherit; text-decoration: none; }
          a:hover { opacity: 0.85; }
        `}</style>
      </Head>
      <main style={{ maxWidth: 1080, margin: '0 auto', padding: '4rem 1.5rem 6rem', lineHeight: 1.6 }}>
        <a href="/store" style={{ fontSize: '0.85rem', color: '#7a7a8c', display: 'inline-block', marginBottom: '2rem' }}>
          ← /store
        </a>

        <section style={{ marginBottom: '3rem' }}>
          <div style={{ display: 'inline-block', padding: '0.25rem 0.75rem', border: '1px solid #2a2a3a', borderRadius: 4, fontSize: '0.7rem', letterSpacing: '0.15em', color: '#7dd3fc', marginBottom: '1.5rem', textTransform: 'uppercase' }}>
            § Tier-2 · apocky.com Subscription
          </div>
          <h1 style={{ fontSize: 'clamp(2rem, 5vw, 3.5rem)', lineHeight: 1.1, margin: 0, fontWeight: 700, letterSpacing: '-0.02em', backgroundImage: 'linear-gradient(135deg, #ffffff 0%, #7dd3fc 60%, #34d399 100%)', WebkitBackgroundClip: 'text', WebkitTextFillColor: 'transparent' }}>
            Early-Access
          </h1>
          <p style={{ fontSize: '1.1rem', color: '#cdd6e4', marginTop: '1rem', maxWidth: 720 }}>
            Watch the Infinity Engine + CSSL stack come together in real-time.
          </p>
          <p style={{ fontSize: '0.95rem', color: '#a8a8b8', marginTop: '0.6rem', maxWidth: 720 }}>
            Private alpha-builds. Private Discord. Spec-retro feed (the same docs Apocky writes for himself · 22 csslc fixes / 8,510 LOC of pure-CSSL stdlib+engine in single sessions). Studio tier unlocks 1:1 sessions and quarterly custom-tool dev. Cancel anytime.
          </p>
        </section>

        {!stripe_configured && (
          <div style={{ padding: '1rem 1.25rem', background: 'rgba(251, 191, 36, 0.08)', border: '1px solid rgba(251, 191, 36, 0.4)', borderRadius: 6, marginBottom: '2rem', color: '#fbbf24', fontSize: '0.88rem' }}>
            ◐ Stripe not yet configured · checkout in stub-mode. Email <a href="mailto:apocky13@gmail.com?subject=%5Bearly-access-pre-order%5D" style={{ color: '#fbbf24', textDecoration: 'underline' }}>apocky13@gmail.com</a> for pre-order until Stripe price-IDs land.
          </div>
        )}

        {err && (
          <div style={{ padding: '0.8rem 1rem', background: 'rgba(248, 113, 113, 0.08)', border: '1px solid rgba(248, 113, 113, 0.4)', borderRadius: 6, marginBottom: '1.5rem', color: '#fca5a5', fontSize: '0.85rem' }}>
            ‼ {err}
          </div>
        )}

        <section style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(280px, 1fr))', gap: '1.5rem', marginBottom: '4rem' }}>
          {products.map((p, idx) => {
            const isLifetime = p.tier === 'lifetime';
            const accent = idx === 0 ? '#7dd3fc' : isLifetime ? '#fbbf24' : '#34d399';
            const cadence = isLifetime ? 'one-time · 50 seats' : 'per month';
            const dollars = (p.price_cents / 100).toFixed(0);
            return (
              <article key={p.id} style={{
                padding: '1.5rem 1.25rem',
                background: idx === 1 ? 'rgba(52, 211, 153, 0.06)' : 'rgba(20, 20, 30, 0.5)',
                border: idx === 1 ? `1px solid ${accent}` : '1px solid #1f1f2a',
                borderRadius: 10,
                borderTop: `3px solid ${accent}`,
              }}>
                <h2 style={{ fontSize: '1.15rem', margin: 0, color: '#ffffff', fontWeight: 700 }}>
                  {p.display_name.replace('apocky.com · ', '')}
                </h2>
                <div style={{ marginTop: '0.6rem' }}>
                  <span style={{ fontSize: '2rem', fontWeight: 700, color: accent }}>${dollars}</span>
                  <span style={{ fontSize: '0.85rem', color: '#7a7a8c', marginLeft: '0.4rem' }}>{cadence}</span>
                </div>
                <p style={{ fontSize: '0.85rem', color: '#a0a0b0', marginTop: '0.6rem', minHeight: '5rem' }}>
                  {p.blurb}
                </p>
                <button
                  onClick={() => checkout(p.id)}
                  disabled={busy !== null}
                  style={{
                    width: '100%',
                    marginTop: '1rem',
                    padding: '0.75rem 1.25rem',
                    background: idx === 1 ? `linear-gradient(135deg, ${accent} 0%, #7dd3fc 100%)` : 'transparent',
                    border: idx === 1 ? 'none' : `1px solid ${accent}`,
                    color: idx === 1 ? '#0a0a0f' : accent,
                    fontWeight: 700,
                    borderRadius: 6,
                    fontSize: '0.88rem',
                    fontFamily: 'inherit',
                    cursor: busy !== null ? 'wait' : 'pointer',
                    opacity: busy === p.id ? 0.6 : 1,
                  }}
                >
                  {busy === p.id ? 'opening checkout...' : 'Subscribe →'}
                </button>
              </article>
            );
          })}
        </section>

        <section style={{ marginBottom: '3rem', padding: '1.75rem', background: 'rgba(15, 15, 25, 0.5)', border: '1px solid #1f1f2a', borderRadius: 8 }}>
          <h2 style={{ fontSize: '0.7rem', textTransform: 'uppercase', letterSpacing: '0.18em', color: '#7a7a8c', marginBottom: '1rem' }}>
            § What every tier gets
          </h2>
          <ul style={{ margin: 0, padding: 0, listStyle: 'none', color: '#cdd6e4', fontSize: '0.92rem', lineHeight: 1.85 }}>
            <li><span style={{ color: '#7dd3fc' }}>·</span> Private alpha-builds of the Infinity Engine (Win-x64 first · Linux/Mac following revenue)</li>
            <li><span style={{ color: '#7dd3fc' }}>·</span> Sovereign MCP Harness updates + new-tool releases</li>
            <li><span style={{ color: '#7dd3fc' }}>·</span> Private Discord with Apocky · ¬ moderated-clone · direct creator access</li>
            <li><span style={{ color: '#7dd3fc' }}>·</span> Spec-retro feed (the actual docs Apocky writes session-by-session · CSL3-glyph-dense)</li>
            <li><span style={{ color: '#7dd3fc' }}>·</span> Closed-alpha LoA keys when alpha lands · sovereignty-engine-game</li>
          </ul>
          <h2 style={{ fontSize: '0.7rem', textTransform: 'uppercase', letterSpacing: '0.18em', color: '#7a7a8c', margin: '1.5rem 0 1rem' }}>
            § Studio tier adds
          </h2>
          <ul style={{ margin: 0, padding: 0, listStyle: 'none', color: '#cdd6e4', fontSize: '0.92rem', lineHeight: 1.85 }}>
            <li><span style={{ color: '#34d399' }}>·</span> 1 hour / month private 1:1 with Apocky (architecture review · CSSL spec-author · custom-tool spec)</li>
            <li><span style={{ color: '#34d399' }}>·</span> Custom-tool development · 1 small tool / quarter</li>
            <li><span style={{ color: '#34d399' }}>·</span> Studio-only Discord channel · roadmap-input quarterly</li>
          </ul>
          <h2 style={{ fontSize: '0.7rem', textTransform: 'uppercase', letterSpacing: '0.18em', color: '#7a7a8c', margin: '1.5rem 0 1rem' }}>
            § Lifetime
          </h2>
          <p style={{ color: '#cdd6e4', fontSize: '0.92rem', margin: 0 }}>
            Early+Studio benefits forever · 50 founder seats only · pre-launch limited. The patron tier · names in attestation · t∞.
          </p>
        </section>

        <footer style={{ paddingTop: '2.5rem', borderTop: '1px solid #1f1f2a', color: '#5a5a6a', fontSize: '0.78rem' }}>
          <p style={{ margin: 0 }}>§ ¬ harm · sovereignty preserved · t∞</p>
          <p style={{ margin: '0.4rem 0 0' }}>© {new Date().getFullYear()} Apocky · cancel-anytime · per-Stripe-policy refunds</p>
        </footer>
      </main>
    </>
  );
};

export const getServerSideProps: GetServerSideProps<EAProps> = async () => {
  const products = TIER_IDS.map((id) => findProduct(id)).filter((p): p is ProductDescriptor => p !== null);
  return {
    props: {
      products,
      stripe_configured: typeof process.env['STRIPE_SECRET_KEY'] === 'string' && process.env['STRIPE_SECRET_KEY'].length > 0,
    },
  };
};

export default EarlyAccessPage;
