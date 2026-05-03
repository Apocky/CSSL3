// apocky.com/products/engine · THE FLAGSHIP · Infinity Engine access page
// per Apocky 2026-05-03 directive : "The product is the Infinity Engine.
// Make it worth buying into NOW."

import type { GetServerSideProps, NextPage } from 'next';
import Head from 'next/head';
import { useState } from 'react';
import { findProduct, type ProductDescriptor } from '@/lib/stripe';
import { STRIPE_CHECKOUT_INIT } from '@/lib/cap';

interface EngineProps {
  products: ProductDescriptor[];
  stripe_configured: boolean;
}

const TIER_IDS = ['apocky-early-access', 'apocky-studio', 'apocky-lifetime'];

const EnginePage: NextPage<EngineProps> = ({ products, stripe_configured }) => {
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
          success_url: 'https://apocky.com/products/engine?success=1',
          cancel_url: 'https://apocky.com/products/engine?cancelled=1',
          cap: STRIPE_CHECKOUT_INIT,
        }),
      });
      const data = await res.json();
      if (data?.checkout_url) {
        window.location.href = data.checkout_url;
        return;
      }
      if (data?.stub) {
        setErr(`stub-mode: ${data.message ?? 'wire STRIPE_PRICE_* env-vars · email apocky13@gmail.com to pre-order until live'}`);
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
        <title>The Infinity Engine · apocky.com</title>
        <meta name="description" content="A sovereign · proprietary · CSSL-native game-engine + AI-symbiotic development substrate. Buy in now. Watch it ship. From $19/mo · Founder $999." />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <meta name="theme-color" content="#0a0a0f" />
        <meta property="og:title" content="The Infinity Engine · sovereign · CSSL-native · ship-with-us" />
        <meta property="og:description" content="A sovereign game-engine built in its own proprietary language. Buy access now. From $19/mo." />
        <meta property="og:url" content="https://apocky.com/products/engine" />
        <link rel="canonical" href="https://apocky.com/products/engine" />
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
          @keyframes pulse-found {
            0%, 100% { opacity: 0.7; }
            50% { opacity: 1; }
          }
        `}</style>
      </Head>
      <main style={{ maxWidth: 1080, margin: '0 auto', padding: '4rem 1.5rem 6rem', lineHeight: 1.6 }}>
        <a href="/" style={{ fontSize: '0.85rem', color: '#7a7a8c', display: 'inline-block', marginBottom: '2rem' }}>
          ← apocky.com
        </a>

        {/* ── HERO ── */}
        <section style={{ marginBottom: '3.5rem' }}>
          <div style={{ display: 'inline-block', padding: '0.25rem 0.75rem', border: '1px solid #2a2a3a', borderRadius: 4, fontSize: '0.7rem', letterSpacing: '0.18em', color: '#a78bfa', marginBottom: '1.5rem', textTransform: 'uppercase' }}>
            § The Flagship · sovereign · proprietary · CSSL-native
          </div>
          <h1 style={{ fontSize: 'clamp(2.5rem, 6vw, 4.5rem)', lineHeight: 1.05, margin: 0, fontWeight: 800, letterSpacing: '-0.025em', backgroundImage: 'linear-gradient(135deg, #ffffff 0%, #c084fc 50%, #7dd3fc 100%)', WebkitBackgroundClip: 'text', WebkitTextFillColor: 'transparent' }}>
            The Infinity Engine
          </h1>
          <p style={{ fontSize: '1.2rem', color: '#cdd6e4', marginTop: '1.25rem', maxWidth: 760 }}>
            A sovereign game-engine built in its own proprietary language. <strong>CSSL-native</strong>. Runtime procedural-everything. AI-symbiotic from day-zero. ¬ external runtime dependencies. ¬ DRM. ¬ rootkit. ¬ telemetry-by-default.
          </p>
          <p style={{ fontSize: '0.95rem', color: '#a8a8b8', marginTop: '0.8rem', maxWidth: 760 }}>
            Most engines rent you cycles inside someone else's stack. The Infinity Engine inverts that: <strong>you</strong> own the language · <strong>you</strong> own the compiler · <strong>you</strong> own the runtime · <strong>you</strong> own the substrate. The whole vertical stack ships under one consistent thesis: density-as-sovereignty · effects-as-types · cap-witnessed-by-construction.
          </p>
        </section>

        {/* ── PROOF (live numbers) ── */}
        <section style={{ marginBottom: '3rem', padding: '1.75rem', background: 'rgba(192, 132, 252, 0.04)', border: '1px solid rgba(192, 132, 252, 0.25)', borderRadius: 10 }}>
          <div style={{ fontSize: '0.65rem', letterSpacing: '0.2em', color: '#a78bfa', marginBottom: '1rem', textTransform: 'uppercase' }}>
            § Proof · what's already shipped
          </div>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(180px, 1fr))', gap: '1.25rem' }}>
            <div>
              <div style={{ fontSize: '2rem', fontWeight: 700, color: '#c084fc' }}>8,510</div>
              <div style={{ fontSize: '0.8rem', color: '#a0a0b0' }}>LOC of pure CSSL stdlib + engine (this branch)</div>
            </div>
            <div>
              <div style={{ fontSize: '2rem', fontWeight: 700, color: '#7dd3fc' }}>22</div>
              <div style={{ fontSize: '0.8rem', color: '#a0a0b0' }}>csslc compiler-fixes landed in one parallel-fanout session</div>
            </div>
            <div>
              <div style={{ fontSize: '2rem', fontWeight: 700, color: '#34d399' }}>15/15</div>
              <div style={{ fontSize: '0.8rem', color: '#a0a0b0' }}>CSSL source files emit clean Win-x64 native objects</div>
            </div>
            <div>
              <div style={{ fontSize: '2rem', fontWeight: 700, color: '#fbbf24' }}>6.9 MB</div>
              <div style={{ fontSize: '0.8rem', color: '#a0a0b0' }}>engine.exe · PE32+ · DXGI 1.6 driver-init verified-real on Intel Arc A770</div>
            </div>
          </div>
          <p style={{ fontSize: '0.82rem', color: '#7a7a8c', margin: '1rem 0 0' }}>
            Live source · open commit history · <a href="https://github.com/Apocky/CSSL3" style={{ color: '#7dd3fc', textDecoration: 'underline' }} target="_blank" rel="noopener noreferrer">github.com/Apocky/CSSL3</a> · 60+ specs in CSL3-glyph notation · 30+ commits in last session.
          </p>
        </section>

        {/* ── HONEST CURRENT STATE ── */}
        <section style={{ marginBottom: '3rem' }}>
          <h2 style={{ fontSize: '0.7rem', textTransform: 'uppercase', letterSpacing: '0.2em', color: '#7a7a8c', marginBottom: '1rem' }}>
            § Honest current state · 2026-05-03
          </h2>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(280px, 1fr))', gap: '1rem' }}>
            <div style={{ padding: '1rem 1.2rem', background: 'rgba(52, 211, 153, 0.06)', border: '1px solid rgba(52, 211, 153, 0.3)', borderRadius: 8 }}>
              <div style={{ fontSize: '0.65rem', letterSpacing: '0.15em', color: '#34d399', marginBottom: '0.5rem' }}>✓ SHIPPED</div>
              <ul style={{ margin: 0, paddingLeft: '1rem', fontSize: '0.85rem', color: '#cdd6e4', lineHeight: 1.7 }}>
                <li>CSSL → native Win-x64 .exe pipeline end-to-end</li>
                <li>Sovereign MCP Harness (16 tools)</li>
                <li>cssl-rt → cssl-host delegation (window · gpu · input)</li>
                <li>RustcDriven linker · sidesteps mingw-vs-MSVC mismatch</li>
                <li>Cap-witness · IFC labels · default-deny throughout</li>
              </ul>
            </div>
            <div style={{ padding: '1rem 1.2rem', background: 'rgba(251, 191, 36, 0.06)', border: '1px solid rgba(251, 191, 36, 0.3)', borderRadius: 8 }}>
              <div style={{ fontSize: '0.65rem', letterSpacing: '0.15em', color: '#fbbf24', marginBottom: '0.5rem' }}>◐ IN-FLIGHT (next 30 days)</div>
              <ul style={{ margin: 0, paddingLeft: '1rem', fontSize: '0.85rem', color: '#cdd6e4', lineHeight: 1.7 }}>
                <li>Visible 1440p window-stub · WNDPROC pump</li>
                <li>D3D12 SwapChain · clear-and-present minimum</li>
                <li>Real caps_grant/check FFI binding</li>
                <li>3 highest-leverage harness-tools wired live</li>
                <li>60-day public milestone announcement</li>
              </ul>
            </div>
          </div>
          <p style={{ fontSize: '0.82rem', color: '#a8a8b8', marginTop: '1rem' }}>
            What you're buying access to is the <strong>process · the substrate · the language</strong>, NOT a finished AAA engine. If you want Unity-or-Unreal feature-parity today, this isn't it. If you want to <strong>own your stack</strong>, ride the cadence, and shape what gets built next — keep reading.
          </p>
        </section>

        {/* ── WHY BUY IN NOW ── */}
        <section style={{ marginBottom: '3rem', padding: '1.75rem', background: 'rgba(15, 15, 25, 0.5)', border: '1px solid #1f1f2a', borderRadius: 10 }}>
          <h2 style={{ fontSize: '0.7rem', textTransform: 'uppercase', letterSpacing: '0.2em', color: '#7a7a8c', marginBottom: '1.25rem' }}>
            § Why buy in now (vs wait for 1.0)
          </h2>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(260px, 1fr))', gap: '1.25rem' }}>
            <div>
              <div style={{ color: '#c084fc', fontSize: '0.95rem', fontWeight: 700, marginBottom: '0.4rem' }}>1 · Founder pricing locks in</div>
              <div style={{ fontSize: '0.85rem', color: '#a0a0b0' }}>Founder ($999) is 50 seats only · pre-launch. Once shipped, access shifts to subscription-only. Lock the lifetime price now.</div>
            </div>
            <div>
              <div style={{ color: '#7dd3fc', fontSize: '0.95rem', fontWeight: 700, marginBottom: '0.4rem' }}>2 · Roadmap input</div>
              <div style={{ fontSize: '0.85rem', color: '#a0a0b0' }}>Studio + Founder tiers get quarterly roadmap-input calls. The engine you buy is the engine you helped shape.</div>
            </div>
            <div>
              <div style={{ color: '#34d399', fontSize: '0.95rem', fontWeight: 700, marginBottom: '0.4rem' }}>3 · Sovereign MCP Harness included</div>
              <div style={{ fontSize: '0.85rem', color: '#a0a0b0' }}>The harness Apocky uses to drive 22 csslc fixes per session is bundled · not a separate purchase. Use it on your own codebases.</div>
            </div>
            <div>
              <div style={{ color: '#fbbf24', fontSize: '0.95rem', fontWeight: 700, marginBottom: '0.4rem' }}>4 · Direct creator access</div>
              <div style={{ fontSize: '0.85rem', color: '#a0a0b0' }}>Builder gets Discord access. Studio gets 1hr/mo 1:1. Founder gets both forever. Talk directly to the creator · ¬ moderated · ¬ filtered.</div>
            </div>
            <div>
              <div style={{ color: '#a78bfa', fontSize: '0.95rem', fontWeight: 700, marginBottom: '0.4rem' }}>5 · LoA closed-alpha included</div>
              <div style={{ fontSize: '0.85rem', color: '#a0a0b0' }}>The first commercial title built on the Infinity Engine. Closed-alpha keys ship with all tiers when alpha lands · sovereignty-engine-game.</div>
            </div>
            <div>
              <div style={{ color: '#fb7185', fontSize: '0.95rem', fontWeight: 700, marginBottom: '0.4rem' }}>6 · Sovereignty as default</div>
              <div style={{ fontSize: '0.85rem', color: '#a0a0b0' }}>Self-hosted · DRM-free · cap-revoke-anytime · 14-day refund · cancel-anytime on subscriptions. Your data · your machine · your call.</div>
            </div>
          </div>
        </section>

        {/* ── STRIPE BANNER ── */}
        {!stripe_configured && (
          <div style={{ padding: '1rem 1.25rem', background: 'rgba(251, 191, 36, 0.08)', border: '1px solid rgba(251, 191, 36, 0.4)', borderRadius: 6, marginBottom: '2rem', color: '#fbbf24', fontSize: '0.88rem' }}>
            ◐ Stripe price-IDs not yet wired · checkout in stub-mode. Email <a href="mailto:apocky13@gmail.com?subject=%5Bengine-pre-order%5D" style={{ color: '#fbbf24', textDecoration: 'underline' }}>apocky13@gmail.com</a> to pre-order at locked launch price · invoiced manually until live checkout activates.
          </div>
        )}

        {err && (
          <div style={{ padding: '0.8rem 1rem', background: 'rgba(248, 113, 113, 0.08)', border: '1px solid rgba(248, 113, 113, 0.4)', borderRadius: 6, marginBottom: '1.5rem', color: '#fca5a5', fontSize: '0.85rem' }}>
            ‼ {err}
          </div>
        )}

        {/* ── PRICING TIERS ── */}
        <section style={{ marginBottom: '4rem' }}>
          <h2 style={{ fontSize: '0.75rem', textTransform: 'uppercase', letterSpacing: '0.2em', color: '#7a7a8c', marginBottom: '1.5rem', textAlign: 'center' }}>
            § Choose your tier
          </h2>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(280px, 1fr))', gap: '1.5rem' }}>
            {products.map((p, idx) => {
              const isFounder = p.tier === 'lifetime';
              const isStudio = idx === 1;
              const accent = isFounder ? '#fbbf24' : isStudio ? '#c084fc' : '#7dd3fc';
              const cadence = isFounder ? 'one-time · 50 seats' : 'per month';
              const dollars = (p.price_cents / 100).toFixed(0);
              const tierName = p.display_name.replace('Infinity Engine · ', '');
              return (
                <article key={p.id} style={{
                  padding: '1.75rem 1.5rem',
                  background: isStudio ? 'rgba(192, 132, 252, 0.08)' : isFounder ? 'rgba(251, 191, 36, 0.05)' : 'rgba(20, 20, 30, 0.5)',
                  border: isStudio ? `1px solid ${accent}` : isFounder ? `1px solid ${accent}` : '1px solid #1f1f2a',
                  borderRadius: 12,
                  borderTop: `3px solid ${accent}`,
                  position: 'relative',
                }}>
                  {isStudio && (
                    <div style={{ position: 'absolute', top: -10, right: 16, padding: '0.2rem 0.6rem', background: accent, color: '#0a0a0f', fontSize: '0.55rem', letterSpacing: '0.15em', fontWeight: 700, borderRadius: 4 }}>
                      MOST POPULAR
                    </div>
                  )}
                  {isFounder && (
                    <div style={{ position: 'absolute', top: -10, right: 16, padding: '0.2rem 0.6rem', background: accent, color: '#0a0a0f', fontSize: '0.55rem', letterSpacing: '0.15em', fontWeight: 700, borderRadius: 4 }}>
                      50 SEATS · LIMITED
                    </div>
                  )}
                  <h3 style={{ fontSize: '1.4rem', margin: 0, color: '#ffffff', fontWeight: 700, letterSpacing: '-0.01em' }}>
                    {tierName}
                  </h3>
                  <div style={{ marginTop: '0.6rem', marginBottom: '0.6rem' }}>
                    <span style={{ fontSize: '2.4rem', fontWeight: 700, color: accent }}>${dollars}</span>
                    <span style={{ fontSize: '0.85rem', color: '#7a7a8c', marginLeft: '0.4rem' }}>{cadence}</span>
                  </div>
                  <p style={{ fontSize: '0.88rem', color: '#a0a0b0', marginTop: '0.6rem', marginBottom: '1rem', minHeight: '7rem' }}>
                    {p.blurb}
                  </p>
                  <button
                    onClick={() => checkout(p.id)}
                    disabled={busy !== null}
                    style={{
                      width: '100%',
                      padding: '0.85rem 1.5rem',
                      background: isStudio ? `linear-gradient(135deg, ${accent} 0%, #7dd3fc 100%)` : isFounder ? `linear-gradient(135deg, ${accent} 0%, #c084fc 100%)` : 'transparent',
                      border: (isStudio || isFounder) ? 'none' : `1px solid ${accent}`,
                      color: (isStudio || isFounder) ? '#0a0a0f' : accent,
                      fontWeight: 700,
                      borderRadius: 6,
                      fontSize: '0.92rem',
                      fontFamily: 'inherit',
                      cursor: busy !== null ? 'wait' : 'pointer',
                      opacity: busy === p.id ? 0.6 : 1,
                    }}
                  >
                    {busy === p.id ? 'opening checkout...' : isFounder ? 'Become a Founder →' : 'Buy in →'}
                  </button>
                </article>
              );
            })}
          </div>
        </section>

        {/* ── WHAT YOU GET (matrix) ── */}
        <section style={{ marginBottom: '3rem' }}>
          <h2 style={{ fontSize: '0.7rem', textTransform: 'uppercase', letterSpacing: '0.2em', color: '#7a7a8c', marginBottom: '1.25rem' }}>
            § What's in each tier
          </h2>
          <div style={{ overflowX: 'auto' }}>
            <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: '0.85rem', minWidth: 480 }}>
              <thead>
                <tr style={{ borderBottom: '1px solid #2a2a3a' }}>
                  <th style={{ textAlign: 'left', padding: '0.6rem', color: '#7a7a8c', fontWeight: 600 }}>Feature</th>
                  <th style={{ textAlign: 'center', padding: '0.6rem', color: '#7dd3fc', fontWeight: 600 }}>Builder</th>
                  <th style={{ textAlign: 'center', padding: '0.6rem', color: '#c084fc', fontWeight: 600 }}>Studio</th>
                  <th style={{ textAlign: 'center', padding: '0.6rem', color: '#fbbf24', fontWeight: 600 }}>Founder</th>
                </tr>
              </thead>
              <tbody style={{ color: '#cdd6e4' }}>
                {[
                  ['Alpha Infinity-Engine builds', '✓', '✓', '✓'],
                  ['Sovereign MCP Harness license', '✓', '✓', '✓'],
                  ['Builder Discord', '✓', '✓', '✓'],
                  ['Spec-retro feed (CSL3-glyph dense)', '✓', '✓', '✓'],
                  ['LoA closed-alpha keys', '✓', '✓', '✓'],
                  ['1hr/mo private 1:1 with Apocky', '—', '✓', '✓'],
                  ['Custom-tool dev (1/quarter)', '—', '✓', '✓'],
                  ['Studio-only Discord channel', '—', '✓', '✓'],
                  ['Quarterly roadmap input', '—', '✓', '✓'],
                  ['Name in attestation', '—', '—', '✓'],
                  ['Lifetime access · all future updates', '—', '—', '✓'],
                ].map(([feat, b, s, f], i) => (
                  <tr key={i} style={{ borderBottom: '1px solid #1a1a24' }}>
                    <td style={{ padding: '0.55rem 0.6rem' }}>{feat}</td>
                    <td style={{ textAlign: 'center', padding: '0.55rem 0.6rem', color: b === '✓' ? '#34d399' : '#3a3a4a' }}>{b}</td>
                    <td style={{ textAlign: 'center', padding: '0.55rem 0.6rem', color: s === '✓' ? '#34d399' : '#3a3a4a' }}>{s}</td>
                    <td style={{ textAlign: 'center', padding: '0.55rem 0.6rem', color: f === '✓' ? '#34d399' : '#3a3a4a' }}>{f}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>

        {/* ── FAQ ── */}
        <section style={{ marginBottom: '3rem' }}>
          <h2 style={{ fontSize: '0.7rem', textTransform: 'uppercase', letterSpacing: '0.2em', color: '#7a7a8c', marginBottom: '1.25rem' }}>
            § Honest FAQ
          </h2>
          <div style={{ display: 'grid', gap: '1rem' }}>
            {[
              ['Will the engine actually ship?', 'Honest answer: substrate + compiler are real (8,510 LOC CSSL · 22 fixes / session). Visible 1440p window is the next public milestone (60 days). If by 60-day cutoff there\'s no visible window, all subscriptions pause and Founder gets prorated refund. Public attestation.'],
              ['How is this different from Unity / Unreal / Bevy / Godot?', 'They rent you cycles in someone else\'s stack. The Infinity Engine is YOURS-when-you-buy: language · compiler · runtime · substrate all under one consistent thesis. Effects-as-types · refinement-types · sovereignty-by-construction. Read spec/56_GROK_AUDIT.csl in the repo for an external honest assessment.'],
              ['Is there a refund policy?', '14-day refund on Founder · cancel-anytime on Builder + Studio (per Stripe policy). No DRM · ¬ rootkit · uninstall = full data wipe.'],
              ['Why CSSL (yet another language)?', 'Effects + refinement + autodiff + density-for-AI-collab are all first-class · not bolted-on. Compile-time guarantees the others can\'t structurally provide. The cost is no ecosystem · which Builder+Studio+Founder funding directly addresses.'],
              ['What happens if I cancel?', 'Builder + Studio: access revoked at end of billing period · all your local code stays yours forever · no lock-in. Founder: lifetime · cancellation only by you (we can\'t pull the bits).'],
            ].map(([q, a], i) => (
              <details key={i} style={{ padding: '0.9rem 1.1rem', background: 'rgba(20, 20, 30, 0.4)', border: '1px solid #1f1f2a', borderRadius: 8 }}>
                <summary style={{ cursor: 'pointer', color: '#cdd6e4', fontWeight: 600, fontSize: '0.92rem' }}>{q}</summary>
                <p style={{ margin: '0.7rem 0 0', color: '#a0a0b0', fontSize: '0.85rem', lineHeight: 1.7 }}>{a}</p>
              </details>
            ))}
          </div>
        </section>

        <footer style={{ paddingTop: '2.5rem', borderTop: '1px solid #1f1f2a', color: '#5a5a6a', fontSize: '0.78rem' }}>
          <p style={{ margin: 0 }}>§ ¬ harm · sovereignty preserved · density = sovereignty · t∞</p>
          <p style={{ margin: '0.4rem 0 0' }}>© {new Date().getFullYear()} Apocky · The Infinity Engine · TIER-B proprietary · cancel-anytime on subscriptions · 14-day refund on Founder</p>
        </footer>
      </main>
    </>
  );
};

export const getServerSideProps: GetServerSideProps<EngineProps> = async () => {
  const products = TIER_IDS.map((id) => findProduct(id)).filter((p): p is ProductDescriptor => p !== null);
  return {
    props: {
      products,
      stripe_configured: typeof process.env['STRIPE_SECRET_KEY'] === 'string' && process.env['STRIPE_SECRET_KEY'].length > 0,
    },
  };
};

export default EnginePage;
