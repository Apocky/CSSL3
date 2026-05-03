// apocky.com/products/harness · Tier-1 Sovereign MCP Harness sales page
// per spec/57_MONETIZATION_PIVOT + spec/59_GROK_VELOCITY_RESPONSE
// Stub-mode-aware · stripe checkout via /api/payments/stripe/checkout

import type { GetServerSideProps, NextPage } from 'next';
import Head from 'next/head';
import { useState } from 'react';
import { findProduct, type ProductDescriptor } from '@/lib/stripe';
import { STRIPE_CHECKOUT_INIT } from '@/lib/cap';

interface HarnessProps {
  products: ProductDescriptor[];
  stripe_configured: boolean;
}

const TIER_IDS = ['harness-starter', 'harness-pro', 'harness-studio', 'harness-lifetime'];

const HarnessPage: NextPage<HarnessProps> = ({ products, stripe_configured }) => {
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
          success_url: 'https://apocky.com/products/harness?success=1',
          cancel_url: 'https://apocky.com/products/harness?cancelled=1',
          cap: STRIPE_CHECKOUT_INIT,
        }),
      });
      const data = await res.json();
      if (data?.checkout_url) {
        window.location.href = data.checkout_url;
        return;
      }
      if (data?.stub) {
        setErr(`stub-mode: ${data.message ?? 'wire STRIPE_PRICE_HARNESS_* env-vars to enable real checkout'}`);
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
        <title>Sovereign MCP Harness · apocky.com</title>
        <meta name="description" content="Run your own MCP workspace. Point Grok / Claude / ChatGPT / Cursor at your codebase with no cloud lock-in. 16 built-in tools. Sovereignty-respecting. From $49/mo · Lifetime $999." />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <meta name="theme-color" content="#0a0a0f" />
        <meta property="og:title" content="Sovereign MCP Harness · apocky.com" />
        <meta property="og:description" content="Stop renting your AI. Run your own MCP workspace. 16 built-in tools. From $49/mo." />
        <meta property="og:url" content="https://apocky.com/products/harness" />
        <link rel="canonical" href="https://apocky.com/products/harness" />
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
          <div style={{ display: 'inline-block', padding: '0.25rem 0.75rem', border: '1px solid #2a2a3a', borderRadius: 4, fontSize: '0.7rem', letterSpacing: '0.15em', color: '#a78bfa', marginBottom: '1.5rem', textTransform: 'uppercase' }}>
            § Tier-1 · Developer Tooling · TIER-B PROPRIETARY
          </div>
          <h1 style={{ fontSize: 'clamp(2rem, 5vw, 3.5rem)', lineHeight: 1.1, margin: 0, fontWeight: 700, letterSpacing: '-0.02em', backgroundImage: 'linear-gradient(135deg, #ffffff 0%, #c084fc 60%, #7dd3fc 100%)', WebkitBackgroundClip: 'text', WebkitTextFillColor: 'transparent' }}>
            Sovereign MCP Harness
          </h1>
          <p style={{ fontSize: '1.1rem', color: '#cdd6e4', marginTop: '1rem', maxWidth: 720 }}>
            Stop renting your AI. Run your own MCP workspace. Point Grok · Claude · ChatGPT · Cursor at your codebase with NO cloud lock-in.
          </p>
          <p style={{ fontSize: '0.95rem', color: '#a8a8b8', marginTop: '0.6rem', maxWidth: 720 }}>
            16 built-in tools (csl_parse · cssl_compile · fs_read/write · infinity_engine_sync · ...). 5-minute setup with Cloudflare Tunnel. Cap-witnessed · audit-logged · sovereign-revoke-anytime. ¬ DRM · ¬ rootkit · ¬ telemetry-by-default.
          </p>
        </section>

        {!stripe_configured && (
          <div style={{ padding: '1rem 1.25rem', background: 'rgba(251, 191, 36, 0.08)', border: '1px solid rgba(251, 191, 36, 0.4)', borderRadius: 6, marginBottom: '2rem', color: '#fbbf24', fontSize: '0.88rem' }}>
            ◐ Stripe not yet configured · checkout in stub-mode. Email <a href="mailto:apocky13@gmail.com?subject=%5Bharness-pre-order%5D" style={{ color: '#fbbf24', textDecoration: 'underline' }}>apocky13@gmail.com</a> for pre-order until Stripe price-IDs land.
          </div>
        )}

        {err && (
          <div style={{ padding: '0.8rem 1rem', background: 'rgba(248, 113, 113, 0.08)', border: '1px solid rgba(248, 113, 113, 0.4)', borderRadius: 6, marginBottom: '1.5rem', color: '#fca5a5', fontSize: '0.85rem' }}>
            ‼ {err}
          </div>
        )}

        <section style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(240px, 1fr))', gap: '1.25rem', marginBottom: '4rem' }}>
          {products.map((p, idx) => {
            const isLifetime = p.tier === 'lifetime';
            const accent = idx === 1 ? '#c084fc' : isLifetime ? '#fbbf24' : '#7dd3fc';
            const cadence = isLifetime ? 'one-time · 50 seats' : 'per month';
            const dollars = (p.price_cents / 100).toFixed(0);
            return (
              <article key={p.id} style={{
                padding: '1.5rem 1.25rem',
                background: idx === 1 ? 'rgba(192, 132, 252, 0.06)' : 'rgba(20, 20, 30, 0.5)',
                border: idx === 1 ? `1px solid ${accent}` : '1px solid #1f1f2a',
                borderRadius: 10,
                borderTop: `3px solid ${accent}`,
              }}>
                {idx === 1 && (
                  <div style={{ fontSize: '0.6rem', letterSpacing: '0.15em', color: accent, marginBottom: '0.4rem' }}>
                    MOST POPULAR
                  </div>
                )}
                <h2 style={{ fontSize: '1.1rem', margin: 0, color: '#ffffff', fontWeight: 700 }}>
                  {p.display_name.replace('Sovereign MCP Harness · ', '')}
                </h2>
                <div style={{ marginTop: '0.6rem' }}>
                  <span style={{ fontSize: '1.8rem', fontWeight: 700, color: accent }}>${dollars}</span>
                  <span style={{ fontSize: '0.78rem', color: '#7a7a8c', marginLeft: '0.4rem' }}>{cadence}</span>
                </div>
                <p style={{ fontSize: '0.82rem', color: '#a0a0b0', marginTop: '0.6rem', minHeight: '5rem' }}>
                  {p.blurb}
                </p>
                <button
                  onClick={() => checkout(p.id)}
                  disabled={busy !== null}
                  style={{
                    width: '100%',
                    marginTop: '1rem',
                    padding: '0.7rem 1.25rem',
                    background: idx === 1 ? `linear-gradient(135deg, ${accent} 0%, #7dd3fc 100%)` : 'transparent',
                    border: idx === 1 ? 'none' : `1px solid ${accent}`,
                    color: idx === 1 ? '#0a0a0f' : accent,
                    fontWeight: 700,
                    borderRadius: 6,
                    fontSize: '0.85rem',
                    fontFamily: 'inherit',
                    cursor: busy !== null ? 'wait' : 'pointer',
                    opacity: busy === p.id ? 0.6 : 1,
                  }}
                >
                  {busy === p.id ? 'opening checkout...' : 'Buy →'}
                </button>
              </article>
            );
          })}
        </section>

        <section style={{ marginBottom: '3rem', padding: '1.75rem', background: 'rgba(15, 15, 25, 0.5)', border: '1px solid #1f1f2a', borderRadius: 8 }}>
          <h2 style={{ fontSize: '0.7rem', textTransform: 'uppercase', letterSpacing: '0.18em', color: '#7a7a8c', marginBottom: '1rem' }}>
            § What you get
          </h2>
          <ul style={{ margin: 0, padding: 0, listStyle: 'none', color: '#cdd6e4', fontSize: '0.92rem', lineHeight: 1.85 }}>
            <li><span style={{ color: '#7dd3fc' }}>·</span> Production-ready FastMCP server (harness.py · ~250 LOC) + project-tools (project_tools.py)</li>
            <li><span style={{ color: '#7dd3fc' }}>·</span> Terminal MCP client (mcp_client.py · httpx + JSON-RPC 2.0)</li>
            <li><span style={{ color: '#7dd3fc' }}>·</span> Docker + docker-compose · one-command containerized deploy</li>
            <li><span style={{ color: '#7dd3fc' }}>·</span> Supabase schema for tool-call logging + audit + RLS</li>
            <li><span style={{ color: '#7dd3fc' }}>·</span> 5-minute Cloudflare-tunnel setup guide</li>
            <li><span style={{ color: '#7dd3fc' }}>·</span> 16 built-in tools (5 wired today · 11 stubbed for-customer-customization)</li>
            <li><span style={{ color: '#7dd3fc' }}>·</span> Custom-tool template · write your own MCP tool in &lt;100 lines</li>
            <li><span style={{ color: '#7dd3fc' }}>·</span> Discord access for-customers · direct support via Apocky</li>
          </ul>
        </section>

        <section style={{ marginBottom: '3rem' }}>
          <h2 style={{ fontSize: '0.7rem', textTransform: 'uppercase', letterSpacing: '0.18em', color: '#7a7a8c', marginBottom: '1rem' }}>
            § Why this matters
          </h2>
          <p style={{ color: '#cdd6e4', fontSize: '0.92rem' }}>
            The current AI-coding-assistant market has you renting cycles from someone else's cloud, with their tool-set, their privacy posture, their lock-in. The Sovereign MCP Harness flips that: <strong>you</strong> run the server. <strong>You</strong> control the tools. <strong>You</strong> hold the bearer token. Your code never leaves your machine. Your assistant works against your real codebase via your tunnel · period.
          </p>
          <p style={{ color: '#a8a8b8', fontSize: '0.88rem', marginTop: '0.8rem' }}>
            Built for solo devs and small studios who hate cloud lock-in and want their own stack. The same harness Apocky uses to drive the Infinity Engine + CSSL toolchain. Battle-tested in real production code · 8,510 LOC of pure-CSSL stdlib + engine code currently shipping through this exact harness.
          </p>
        </section>

        <footer style={{ paddingTop: '2.5rem', borderTop: '1px solid #1f1f2a', color: '#5a5a6a', fontSize: '0.78rem' }}>
          <p style={{ margin: 0 }}>§ ¬ harm · sovereignty preserved · t∞</p>
          <p style={{ margin: '0.4rem 0 0' }}>© {new Date().getFullYear()} Apocky · TIER-B proprietary · ¬ DRM · ¬ rootkit · 14-day refund</p>
        </footer>
      </main>
    </>
  );
};

export const getServerSideProps: GetServerSideProps<HarnessProps> = async () => {
  const products = TIER_IDS.map((id) => findProduct(id)).filter((p): p is ProductDescriptor => p !== null);
  return {
    props: {
      products,
      stripe_configured: typeof process.env['STRIPE_SECRET_KEY'] === 'string' && process.env['STRIPE_SECRET_KEY'].length > 0,
    },
  };
};

export default HarnessPage;
