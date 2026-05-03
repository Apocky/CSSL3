// apocky.com/store · 4-tier monetization storefront
// per spec/57_MONETIZATION_PIVOT · ship-revenue-NOW · 2026-05-03
// SSG · static · sovereignty-respecting · Stripe-button placeholders (wire post-launch)

import type { NextPage } from 'next';
import Head from 'next/head';

interface Tier {
  id: string;
  badge: string;
  title: string;
  pitch: string;
  price: string;
  cadence: string;
  features: string[];
  cta: string;
  ctaHref: string;
  accent: string;
  highlight?: boolean;
}

const TIERS: ReadonlyArray<Tier> = [
  {
    id: 'harness',
    badge: 'TIER-1 · DEVELOPER TOOLING',
    title: 'Sovereign MCP Harness',
    pitch:
      'Stop renting your AI. Run your own MCP workspace. Point Grok / Claude / ChatGPT / Cursor at your codebase with NO cloud-lock-in. 16 built-in tools. 5-minute setup.',
    price: '$49',
    cadence: '/mo · Starter',
    features: [
      '1-user · 5 core tools · email-support',
      'PRO $99/mo · all 16 tools · priority support',
      'STUDIO $199/mo · 5-user team · whitelabel · onboarding',
      'LIFETIME $999 · everything-forever · pre-launch limited',
      'Cap-witnessed · audit-logged · sovereign-revoke-anytime',
      '¬ DRM · ¬ rootkit · ¬ telemetry-by-default',
    ],
    cta: 'Get the Harness',
    ctaHref: '/products/harness',
    accent: '#c084fc',
    highlight: true,
  },
  {
    id: 'early-access',
    badge: 'TIER-2 · APOCKY.COM SUBSCRIPTION',
    title: 'Early-Access Membership',
    pitch:
      'Watch the Infinity Engine + CSSL stack come together in real-time. Private builds. Private Discord. Spec-retro feed. 1:1 sessions on the Studio tier.',
    price: '$19',
    cadence: '/mo · Early',
    features: [
      'EARLY $19/mo · alpha-builds + harness-updates + private Discord',
      'STUDIO $99/mo · 1hr/mo private 1:1 + custom-tool dev',
      'LIFETIME $999 · Early+Studio forever · pre-launch 50 seats',
      'Substrate evolution updates · spec-retro-feed',
      'Closed-alpha LoA keys (when alpha lands)',
      'Cancel-anytime · sovereignty-preserved',
    ],
    cta: 'Subscribe',
    ctaHref: '/products/early-access',
    accent: '#7dd3fc',
  },
  {
    id: 'loa-alpha',
    badge: 'TIER-3 · GAME ALPHA-PASS',
    title: 'Labyrinth of Apocalypse · Alpha',
    pitch:
      'First public CSSL-native game. Engine runs. 1440p window. Closed alpha. Sovereign + substrate-engine. The novelty + the principle. Limited keys.',
    price: '$29',
    cadence: 'one-time · pre-launch',
    features: [
      'ALPHA-PASS $29-49 one-time (pre-launch $19 first 100)',
      'Alpha-build keys · Win-x64 · CSSL-native engine binary',
      'Discord channel for alpha-testers',
      'Bug-reports directly to Apocky',
      'Names-in-credits at v1.0',
      '¬ refund · alpha-EULA covers',
    ],
    cta: 'Buy Alpha-Pass',
    ctaHref: '/products/loa-alpha',
    accent: '#fbbf24',
  },
  {
    id: 'consulting',
    badge: 'TIER-4 · CUSTOM ENGAGEMENTS',
    title: 'Sovereign Engineering Consulting',
    pitch:
      'Want a CSSL-like stack for your codebase? Bespoke MCP harness for your engine? Architectural assessment of your sovereign-engine vision? 1-4 week engagements.',
    price: '$5,000',
    cadence: '+ per engagement',
    features: [
      'CSSL-STACK-ASSESSMENT $5,000 · 1-week · written report',
      'CUSTOM-MCP-HARNESS $10,000 · 2-weeks · tailored harness',
      'SOVEREIGN-ENGINE-CONSULTING $15,000 · 4-weeks · architecture + initial impl',
      'NDA on request',
      'Apocky personally · ¬ subcontracted',
      'Inbound-only · email-form below',
    ],
    cta: 'Inquire',
    ctaHref: 'mailto:apocky13@gmail.com?subject=%5Bconsulting-inquiry%5D',
    accent: '#34d399',
  },
];

const Store: NextPage = () => {
  return (
    <>
      <Head>
        <title>Store · apocky.com</title>
        <meta name="description" content="Sovereign AI tooling · Infinity Engine early-access · LoA alpha-pass · Sovereign-engineering consulting · Apocky monetizable products." />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <meta name="theme-color" content="#0a0a0f" />
        <meta property="og:title" content="apocky.com · Store" />
        <meta property="og:description" content="Sovereign AI tooling. Infinity Engine early-access. LoA alpha-pass. Custom sovereign-engineering. No DRM. No telemetry. Yours." />
        <meta property="og:url" content="https://apocky.com/store" />
        <link rel="canonical" href="https://apocky.com/store" />
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
        `}</style>
      </Head>
      <main style={{ maxWidth: 1200, margin: '0 auto', padding: '4rem 1.5rem 6rem', lineHeight: 1.6 }}>
        <a href="/" style={{ fontSize: '0.85rem', color: '#7a7a8c', display: 'inline-block', marginBottom: '2rem' }}>
          ← apocky.com
        </a>

        <section style={{ marginBottom: '3rem' }}>
          <div style={{ display: 'inline-block', padding: '0.25rem 0.75rem', border: '1px solid #2a2a3a', borderRadius: 4, fontSize: '0.7rem', letterSpacing: '0.15em', color: '#a78bfa', marginBottom: '1.5rem', textTransform: 'uppercase' }}>
            § Store · sovereignty-respecting · ¬ DRM · ¬ rootkit
          </div>
          <h1 style={{ fontSize: 'clamp(2rem, 5vw, 3.5rem)', lineHeight: 1.1, margin: 0, fontWeight: 700, letterSpacing: '-0.02em', backgroundImage: 'linear-gradient(135deg, #ffffff 0%, #c084fc 60%, #7dd3fc 100%)', WebkitBackgroundClip: 'text', WebkitTextFillColor: 'transparent' }}>
            Sovereign Tooling, Real Code, Real Results
          </h1>
          <p style={{ fontSize: '1.05rem', color: '#cdd6e4', marginTop: '1rem', maxWidth: 760 }}>
            Four ways to support + accelerate the Apocky stack. Run your own MCP workspace, watch the Infinity Engine come together in real-time, play the LoA closed-alpha, or have a sovereign-engineering consult.
          </p>
          <p style={{ fontSize: '0.92rem', color: '#a8a8b8', marginTop: '0.6rem', maxWidth: 760 }}>
            <strong>22 csslc compiler-fixes in one parallel-fanout session.</strong> 8,510 LOC of pure-CSSL stdlib + engine. 15/15 source files emitting clean Win-x64 native objects. The pace is real because the architecture rewards parallel-AI-augmented dev.
          </p>
        </section>

        <section style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(280px, 1fr))', gap: '1.5rem', marginBottom: '4rem' }}>
          {TIERS.map((t) => (
            <article
              key={t.id}
              style={{
                padding: '1.75rem 1.5rem',
                background: t.highlight ? 'rgba(192, 132, 252, 0.06)' : 'rgba(20, 20, 30, 0.5)',
                border: t.highlight ? `1px solid ${t.accent}` : '1px solid #1f1f2a',
                borderRadius: 10,
                borderTop: `3px solid ${t.accent}`,
                position: 'relative',
              }}
            >
              <div style={{ fontSize: '0.65rem', letterSpacing: '0.18em', color: t.accent, marginBottom: '0.6rem' }}>
                {t.badge}
              </div>
              <h2 style={{ fontSize: '1.4rem', margin: 0, color: '#ffffff', fontWeight: 700, letterSpacing: '-0.01em' }}>
                {t.title}
              </h2>
              <p style={{ fontSize: '0.92rem', color: '#cdd6e4', marginTop: '0.6rem', minHeight: '5rem' }}>
                {t.pitch}
              </p>
              <div style={{ marginTop: '0.8rem', marginBottom: '0.6rem' }}>
                <span style={{ fontSize: '2rem', fontWeight: 700, color: t.accent }}>{t.price}</span>
                <span style={{ fontSize: '0.85rem', color: '#7a7a8c', marginLeft: '0.4rem' }}>{t.cadence}</span>
              </div>
              <ul style={{ margin: 0, padding: 0, listStyle: 'none', color: '#a0a0b0', fontSize: '0.85rem', lineHeight: 1.65 }}>
                {t.features.map((f, i) => (
                  <li key={i} style={{ paddingLeft: '1rem', position: 'relative', marginBottom: '0.2rem' }}>
                    <span style={{ position: 'absolute', left: 0, color: t.accent }}>·</span>
                    {f}
                  </li>
                ))}
              </ul>
              <a
                href={t.ctaHref}
                style={{
                  display: 'inline-block',
                  marginTop: '1.25rem',
                  padding: '0.75rem 1.5rem',
                  background: t.highlight ? `linear-gradient(135deg, ${t.accent} 0%, #7dd3fc 100%)` : 'transparent',
                  border: t.highlight ? 'none' : `1px solid ${t.accent}`,
                  color: t.highlight ? '#0a0a0f' : t.accent,
                  fontWeight: 700,
                  borderRadius: 6,
                  fontSize: '0.9rem',
                }}
              >
                {t.cta} →
              </a>
            </article>
          ))}
        </section>

        <section style={{ marginBottom: '3rem', padding: '2rem', background: 'rgba(15, 15, 25, 0.5)', border: '1px solid #1f1f2a', borderRadius: 8 }}>
          <h2 style={{ fontSize: '0.7rem', textTransform: 'uppercase', letterSpacing: '0.2em', color: '#7a7a8c', marginBottom: '1rem' }}>
            § Why this isn't another marketing page
          </h2>
          <ul style={{ color: '#cdd6e4', fontSize: '0.92rem', lineHeight: 1.85, paddingLeft: '1.2rem', margin: 0 }}>
            <li><strong>Real codebase</strong> · github.com/Apocky/CSSL3 · 8,510 LOC pure CSSL stdlib + engine · 4,500 LOC compiler-advance</li>
            <li><strong>Real velocity</strong> · 22 compiler fixes / one parallel-fanout session · 15/15 .cssl files emitting native objects today</li>
            <li><strong>Real engine</strong> · engine/main.cssl → 6.9 MB Win-x64 PE32+ exe · runs · Intel Arc A770 driver-init verified</li>
            <li><strong>Real sovereignty</strong> · cap-witness default-deny · IFC-labels (Sensitive&lt;Behavioral|Voice&gt;) · ¬ DRM · ¬ rootkit</li>
            <li><strong>Real honesty</strong> · the engine doesn't yet open a visible window · the trace-eprintlns blocker is documented in spec/55 · we're shipping anyway because the substrate is real and the velocity is real</li>
          </ul>
        </section>

        <footer style={{ paddingTop: '2.5rem', borderTop: '1px solid #1f1f2a', color: '#5a5a6a', fontSize: '0.78rem' }}>
          <p style={{ margin: 0 }}>§ ¬ harm in the making · sovereignty preserved · t∞</p>
          <p style={{ margin: '0.4rem 0 0' }}>© {new Date().getFullYear()} Apocky · cosmetic-channel-only · ¬ pay-for-power · ¬ DRM</p>
        </footer>
      </main>
    </>
  );
};

export default Store;
