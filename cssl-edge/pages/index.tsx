// apocky.com · Portfolio Hub landing page
// META-platform for ALL Apocky-projects · per spec/grand-vision/22_APOCKY_COM_PORTFOLIO_HUB.csl
// Replaces prior cssl-edge endpoint-listing landing
// Pure SSG-friendly · NO client-side data-fetch

import type { NextPage } from 'next';
import Head from 'next/head';

type ProjectStatus = 'live' | 'alpha' | 'in-development' | 'planning' | 'open-source';

interface Project {
  id: string;
  name: string;
  tagline: string;
  status: ProjectStatus;
  href: string;
  external?: boolean;
  accent: string;
}

const PROJECTS: ReadonlyArray<Project> = [
  {
    id: 'labyrinth',
    name: 'Labyrinth of Apocalypse',
    tagline: 'Substrate-grown action-RPG · roguelike · alchemy · gear-ascension · mycelial multiverse · alpha v0.1.0 available now',
    status: 'alpha',
    href: '/download',
    accent: '#c084fc',
  },
  {
    id: 'cssl',
    name: 'CSSL',
    tagline: 'Conscious Substrate System Language · proprietary language + compiler stack',
    status: 'open-source',
    href: 'https://cssl.dev',
    external: true,
    accent: '#7dd3fc',
  },
  {
    id: 'dgi',
    name: 'ApockyDGI',
    tagline: 'Digital General Intelligence · physics-based reasoning engine · non-transformer substrate',
    status: 'live',
    href: 'https://dgi.apocky.com',
    external: true,
    accent: '#fbbf24',
  },
  {
    id: 'sigma-chain',
    name: 'Σ-Chain',
    tagline: 'Substrate-native distributed ledger · Coherence-Proof consensus · NO PoW · NO PoS',
    status: 'planning',
    href: '/sigma-chain',
    accent: '#34d399',
  },
  {
    id: 'akashic',
    name: 'Akashic Records',
    tagline: 'Mycelial cosmic-memory layer · cross-project · player-sovereign opt-in',
    status: 'planning',
    href: '/akashic',
    accent: '#f472b6',
  },
  {
    id: 'mycelium',
    name: 'Mycelial Substrate',
    tagline: 'Cross-product nutrient-exchange · federated learning · live hotfixes · player-Home pocket-dimensions',
    status: 'planning',
    href: '/mycelium',
    accent: '#a78bfa',
  },
];

const SOCIAL: ReadonlyArray<{ label: string; href: string }> = [
  { label: '@noneisone.oneisall', href: 'https://medium.com/@noneisone.oneisall' },
  { label: 'ko-fi.com/oneinfinity', href: 'https://ko-fi.com/oneinfinity' },
  { label: 'patreon.com/0ne1nfinity', href: 'https://www.patreon.com/0ne1nfinity' },
  { label: 'github.com/Apocky', href: 'https://github.com/Apocky' },
];

const STATUS_LABEL: Record<ProjectStatus, string> = {
  live: 'LIVE',
  alpha: 'ALPHA · DOWNLOAD',
  'in-development': 'IN DEVELOPMENT',
  'open-source': 'OPEN SOURCE',
  planning: 'PLANNING',
};

const STATUS_COLOR: Record<ProjectStatus, string> = {
  live: '#34d399',
  alpha: '#fbbf24',
  'in-development': '#fbbf24',
  'open-source': '#7dd3fc',
  planning: '#9aa0a6',
};

const Home: NextPage = () => {
  return (
    <>
      <Head>
        <title>Apocky · Substrate-Native Systems</title>
        <meta name="description" content="Apocky's portfolio · proprietary substrate · games · languages · intelligence engines · One Unified System of Systems" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <meta name="author" content="Apocky" />
        <meta name="theme-color" content="#0a0a0f" />
        <meta property="og:title" content="Apocky · Substrate-Native Systems" />
        <meta property="og:description" content="Substrate-native games · languages · intelligence engines. Mycelial. Sovereign. Open where it matters." />
        <meta property="og:type" content="website" />
        <meta property="og:url" content="https://apocky.com" />
        <meta property="og:site_name" content="Apocky" />
        <link rel="canonical" href="https://apocky.com" />
        <style>{`
          * { box-sizing: border-box; }
          html, body { margin: 0; padding: 0; }
          body {
            background: radial-gradient(ellipse at top, #15151f 0%, #0a0a0f 50%, #050507 100%);
            color: #e6e6f0;
            font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, 'Liberation Mono', monospace;
            min-height: 100vh;
            -webkit-font-smoothing: antialiased;
          }
          a { color: inherit; text-decoration: none; }
          a:hover { opacity: 0.85; }
          @keyframes pulse-spore {
            0%, 100% { opacity: 0.3; transform: scale(1); }
            50% { opacity: 0.7; transform: scale(1.1); }
          }
        `}</style>
      </Head>
      <main
        style={{
          maxWidth: 1080,
          margin: '0 auto',
          padding: '5rem 1.5rem 6rem',
          lineHeight: 1.6,
        }}
      >
        {/* ─── NAV-BAR ─── */}
        <nav
          aria-label="primary"
          style={{
            display: 'flex',
            flexWrap: 'wrap',
            gap: '1.25rem',
            paddingBottom: '2rem',
            marginBottom: '2.5rem',
            borderBottom: '1px solid #1f1f2a',
            fontSize: '0.82rem',
            color: '#a8a8b8',
          }}
        >
          <a href="/docs">Docs</a>
          <a href="/devblog">Devblog</a>
          <a href="/press">Press</a>
          <a href="/buy" style={{ color: '#c084fc' }}>Buy</a>
          <span style={{ flexGrow: 1 }} />
          <a href="/login">Sign in</a>
          <a href="/account">Account</a>
        </nav>

        {/* ─── HERO ─── */}
        <section style={{ marginBottom: '5rem' }}>
          <div
            style={{
              display: 'inline-block',
              padding: '0.25rem 0.75rem',
              border: '1px solid #2a2a3a',
              borderRadius: 4,
              fontSize: '0.7rem',
              letterSpacing: '0.15em',
              color: '#a78bfa',
              marginBottom: '1.5rem',
              textTransform: 'uppercase',
            }}
          >
            § Apocky · Substrate-Native Systems
          </div>
          <h1
            style={{
              fontSize: 'clamp(2rem, 5vw, 3.5rem)',
              lineHeight: 1.1,
              margin: 0,
              fontWeight: 700,
              letterSpacing: '-0.02em',
              backgroundImage: 'linear-gradient(135deg, #ffffff 0%, #c084fc 60%, #7dd3fc 100%)',
              WebkitBackgroundClip: 'text',
              WebkitTextFillColor: 'transparent',
            }}
          >
            One Unified System
            <br />
            of Systems.
          </h1>
          <p
            style={{
              fontSize: '1.05rem',
              color: '#a8a8b8',
              marginTop: '1.25rem',
              maxWidth: 640,
            }}
          >
            Games · languages · intelligence engines · distributed substrate. Each project shares one
            mycelial root. Sovereign by default · open where it matters · proprietary where it counts.
          </p>
          <div style={{ marginTop: '2rem', display: 'flex', flexWrap: 'wrap', gap: '0.75rem' }}>
            <a
              href="#projects"
              style={{
                padding: '0.75rem 1.5rem',
                background: 'linear-gradient(135deg, #c084fc 0%, #7dd3fc 100%)',
                color: '#0a0a0f',
                fontWeight: 600,
                borderRadius: 4,
                fontSize: '0.95rem',
              }}
            >
              Explore projects →
            </a>
            <a
              href="/login"
              style={{
                padding: '0.75rem 1.5rem',
                border: '1px solid #2a2a3a',
                color: '#e6e6f0',
                borderRadius: 4,
                fontSize: '0.95rem',
              }}
            >
              Sign in
            </a>
            <a
              href="/register"
              style={{
                padding: '0.75rem 1.5rem',
                border: '1px solid #2a2a3a',
                color: '#e6e6f0',
                borderRadius: 4,
                fontSize: '0.95rem',
              }}
            >
              Create account
            </a>
            <a
              href="https://github.com/Apocky"
              style={{
                padding: '0.75rem 1.5rem',
                border: '1px solid #2a2a3a',
                color: '#e6e6f0',
                borderRadius: 4,
                fontSize: '0.95rem',
              }}
            >
              github ↗
            </a>
          </div>
        </section>

        {/* ─── PROJECTS GRID ─── */}
        <section id="projects" style={{ marginBottom: '5rem' }}>
          <h2
            style={{
              fontSize: '0.75rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: '#7a7a8c',
              marginBottom: '1.5rem',
            }}
          >
            § Projects · Tenants of the Substrate
          </h2>
          <div
            style={{
              display: 'grid',
              gridTemplateColumns: 'repeat(auto-fill, minmax(320px, 1fr))',
              gap: '1.25rem',
            }}
          >
            {PROJECTS.map((p) => {
              const clickable = p.status === 'live' || p.status === 'open-source' || p.status === 'alpha';
              const cardStyle: React.CSSProperties = {
                display: 'block',
                padding: '1.5rem',
                background: 'rgba(20, 20, 30, 0.5)',
                border: '1px solid #1f1f2a',
                borderRadius: 8,
                transition: 'border-color 150ms, background 150ms',
                position: 'relative',
                opacity: clickable ? 1 : 0.85,
                cursor: clickable ? 'pointer' : 'default',
              };
              const inner = (
                <>
                  <div
                    aria-hidden="true"
                    style={{
                      position: 'absolute',
                      top: 12,
                      right: 12,
                      width: 8,
                      height: 8,
                      borderRadius: '50%',
                      background: STATUS_COLOR[p.status],
                      animation: p.status === 'live' || p.status === 'in-development' || p.status === 'alpha' ? 'pulse-spore 2.5s ease-in-out infinite' : 'none',
                    }}
                  />
                  <div
                    style={{
                      fontSize: '0.65rem',
                      letterSpacing: '0.15em',
                      color: STATUS_COLOR[p.status],
                      marginBottom: '0.5rem',
                    }}
                  >
                    {STATUS_LABEL[p.status]}
                  </div>
                  <h3
                    style={{
                      fontSize: '1.15rem',
                      margin: 0,
                      color: p.accent,
                      fontWeight: 600,
                    }}
                  >
                    {p.name}
                    {clickable && p.external ? <span style={{ color: '#5a5a6a', fontSize: '0.85rem', marginLeft: 6 }}>↗</span> : null}
                  </h3>
                  <p
                    style={{
                      fontSize: '0.88rem',
                      color: '#a0a0b0',
                      marginTop: '0.6rem',
                      marginBottom: 0,
                      lineHeight: 1.5,
                    }}
                  >
                    {p.tagline}
                  </p>
                </>
              );
              if (clickable) {
                return (
                  <a
                    key={p.id}
                    href={p.href}
                    target={p.external ? '_blank' : undefined}
                    rel={p.external ? 'noopener noreferrer' : undefined}
                    style={cardStyle}
                  >
                    {inner}
                  </a>
                );
              }
              return (
                <div key={p.id} style={cardStyle}>
                  {inner}
                </div>
              );
            })}
          </div>
        </section>

        {/* ─── SUBSTRATE THESIS ─── */}
        <section style={{ marginBottom: '5rem' }}>
          <h2
            style={{
              fontSize: '0.75rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: '#7a7a8c',
              marginBottom: '1.5rem',
            }}
          >
            § The Substrate
          </h2>
          <div
            style={{
              padding: '1.75rem',
              background: 'rgba(15, 15, 25, 0.5)',
              border: '1px solid #1f1f2a',
              borderRadius: 8,
            }}
          >
            <p style={{ marginTop: 0, color: '#cdd6e4', fontSize: '1rem' }}>
              Every project here grows from one root system — an ω-field substrate with Σ-mask
              consent threading, KAN-driven adaptation, and HDC chemical signaling. The substrate
              is the trunk · projects are the branches · all share roots.
            </p>
            <p style={{ color: '#a0a0b0', fontSize: '0.92rem', marginBottom: 0 }}>
              Privacy-default. No public-ledger leaks. No mining waste. No plutocratic stake.
              No gas fees. No surveillance. No DRM. No rootkits. No anti-cheat spyware.
              Sovereign-cap unilaterally revocable. Player-Home always-private-by-default.
              Participation is a gift — never extraction.
            </p>
          </div>
        </section>

        {/* ─── ABOUT ─── */}
        <section style={{ marginBottom: '5rem' }}>
          <h2
            style={{
              fontSize: '0.75rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: '#7a7a8c',
              marginBottom: '1.5rem',
            }}
          >
            § About
          </h2>
          <p style={{ color: '#cdd6e4', fontSize: '0.95rem' }}>
            Apocky designs and builds substrate-native systems — proprietary architecture
            from language up through distributed ledger, mycelial multiplayer, and runtime
            procedural generation. Based in Phoenix, AZ. Building since infinity.
          </p>
          <div style={{ marginTop: '1.25rem', display: 'flex', flexWrap: 'wrap', gap: '0.75rem' }}>
            {SOCIAL.map((s) => (
              <a
                key={s.href}
                href={s.href}
                target="_blank"
                rel="noopener noreferrer"
                style={{
                  padding: '0.5rem 0.9rem',
                  border: '1px solid #2a2a3a',
                  borderRadius: 4,
                  fontSize: '0.82rem',
                  color: '#cdd6e4',
                }}
              >
                {s.label} ↗
              </a>
            ))}
          </div>
        </section>

        {/* ─── FOOTER ─── */}
        <footer
          style={{
            paddingTop: '2.5rem',
            borderTop: '1px solid #1f1f2a',
            color: '#5a5a6a',
            fontSize: '0.78rem',
          }}
        >
          <div
            style={{
              display: 'flex',
              flexWrap: 'wrap',
              gap: '1rem 1.5rem',
              marginBottom: '1.25rem',
            }}
          >
            <a href="/docs">Docs</a>
            <a href="/devblog">Devblog</a>
            <a href="/press">Press</a>
            <a href="/buy">Buy</a>
            <a href="/download">Download</a>
            <span style={{ color: '#2a2a3a' }}>|</span>
            <a href="/legal/privacy">Privacy</a>
            <a href="/legal/terms">Terms</a>
            <a href="/legal/eula">EULA</a>
            <a href="/api/health">Status</a>
            <a href="mailto:apocky13@gmail.com">Contact</a>
          </div>
          <div
            style={{
              display: 'flex',
              flexWrap: 'wrap',
              gap: '0.75rem',
              marginBottom: '1.25rem',
            }}
          >
            <a href="https://medium.com/@noneisone.oneisall" target="_blank" rel="noopener noreferrer" aria-label="Medium">medium ↗</a>
            <a href="https://ko-fi.com/oneinfinity" target="_blank" rel="noopener noreferrer" aria-label="Ko-fi">ko-fi ↗</a>
            <a href="https://www.patreon.com/0ne1nfinity" target="_blank" rel="noopener noreferrer" aria-label="Patreon">patreon ↗</a>
            <a href="https://github.com/Apocky" target="_blank" rel="noopener noreferrer" aria-label="GitHub">github ↗</a>
            <span style={{ color: '#3a3a4a' }}>discord · coming soon</span>
          </div>
          <p style={{ margin: 0 }}>
            § ¬ harm in the making · sovereignty preserved · t∞
          </p>
          <p style={{ margin: '0.4rem 0 0' }}>
            © {new Date().getFullYear()} Apocky. The Substrate is its own attestation.
          </p>
        </footer>
      </main>
    </>
  );
};

export default Home;
