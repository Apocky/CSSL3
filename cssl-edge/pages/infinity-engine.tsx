// apocky.com/infinity-engine · canonical overview of The Infinity Engine
// § The engine has-a-name · this is the public face
// I> phone-first responsive · matches index.tsx palette
// I> ¬ engagement-bait · ¬ scarcity-pressure · sovereignty-respecting throughout
// W14-H · sibling W14-M provides live status @ /engine

import type { NextPage } from 'next';
import Head from 'next/head';

interface PoweredProject {
  id: string;
  name: string;
  status: string;
  href: string;
  external?: boolean;
  blurb: string;
}

const POWERED: ReadonlyArray<PoweredProject> = [
  {
    id: 'loa',
    name: 'Labyrinth of Apocalypse',
    status: 'alpha',
    href: '/download',
    blurb: 'first commercial title · roguelike action-RPG · runtime-procgen · mycelial multiverse',
  },
  {
    id: 'cssl',
    name: 'CSSL',
    status: 'open-source',
    href: 'https://cssl.dev',
    external: true,
    blurb: 'the language + compiler stack underneath everything · proprietary-everything thesis',
  },
  {
    id: 'sigma-chain',
    name: 'Σ-Chain',
    status: 'planning',
    href: '/sigma-chain',
    blurb: 'substrate-native distributed ledger · Coherence-Proof · NO PoW · NO PoS · NO gas',
  },
  {
    id: 'mycelium',
    name: 'Mycelium',
    status: 'alpha',
    href: '/mycelium',
    blurb: 'autonomous-local-agent · 3-mode LLM-bridge · cross-project knowledge-substrate',
  },
  {
    id: 'akashic',
    name: 'Akashic Records',
    status: 'planning',
    href: '/akashic',
    blurb: 'cross-project cosmic-memory layer · player-sovereign opt-in only',
  },
];

const InfinityEngine: NextPage = () => {
  return (
    <>
      <Head>
        <title>The Infinity Engine · Apocky</title>
        <meta
          name="description"
          content="The Infinity Engine · proprietary substrate-native runtime that runs everything Apocky builds. Always running, always learning, sovereign by default."
        />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta name="theme-color" content="#0a0a0f" />
        <meta name="apple-mobile-web-app-capable" content="yes" />
        <meta name="apple-mobile-web-app-status-bar-style" content="black-translucent" />
        <meta property="og:title" content="The Infinity Engine · proprietary substrate-native runtime" />
        <meta
          property="og:description"
          content="One engine that powers Labyrinth of Apocalypse, CSSL, Σ-Chain, Mycelium, and every future Apocky project. Substrate that runs everything · learns while you sleep · sovereign by default."
        />
        <meta property="og:type" content="website" />
        <meta property="og:url" content="https://apocky.com/infinity-engine" />
        <meta property="og:site_name" content="Apocky" />
        <meta name="twitter:card" content="summary_large_image" />
        <meta name="twitter:title" content="The Infinity Engine · Apocky" />
        <meta
          name="twitter:description"
          content="The substrate that runs everything · learns while you sleep · sovereign by default."
        />
        <link rel="canonical" href="https://apocky.com/infinity-engine" />
        <style>{`
          * { box-sizing: border-box; }
          html, body { margin: 0; padding: 0; }
          body {
            background: radial-gradient(ellipse at top, #15151f 0%, #0a0a0f 50%, #050507 100%);
            color: #e6e6f0;
            font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
            min-height: 100vh;
            min-height: 100dvh;
            -webkit-font-smoothing: antialiased;
            -webkit-text-size-adjust: 100%;
          }
          a { color: inherit; text-decoration: none; }
          a:hover { opacity: 0.85; }
          @keyframes pulse-spore {
            0%, 100% { opacity: 0.3; transform: scale(1); }
            50% { opacity: 0.7; transform: scale(1.1); }
          }
          .arch-row {
            display: flex;
            flex-direction: column;
            gap: 0.6rem;
          }
          @media (min-width: 720px) {
            .arch-row { flex-direction: row; align-items: stretch; }
            .arch-arrow { display: flex; align-items: center; padding: 0 0.6rem; color: #5a5a6a; }
          }
        `}</style>
      </Head>
      <main
        style={{
          maxWidth: 880,
          margin: '0 auto',
          padding: '4rem 1.25rem 6rem',
          lineHeight: 1.65,
        }}
      >
        <a
          href="/"
          style={{
            fontSize: '0.85rem',
            color: '#7a7a8c',
            display: 'inline-block',
            marginBottom: '2rem',
          }}
        >
          ← apocky.com
        </a>

        {/* ─── HERO ─── */}
        <section style={{ marginBottom: '3.5rem' }}>
          <div
            style={{
              display: 'inline-block',
              padding: '0.25rem 0.75rem',
              border: '1px solid #2a2a3a',
              borderRadius: 4,
              fontSize: '0.7rem',
              letterSpacing: '0.15em',
              color: '#a78bfa',
              marginBottom: '1.25rem',
              textTransform: 'uppercase',
            }}
          >
            § The Infinity Engine · alpha
          </div>
          <h1
            style={{
              fontSize: 'clamp(1.85rem, 5vw, 3rem)',
              lineHeight: 1.1,
              margin: 0,
              fontWeight: 700,
              letterSpacing: '-0.02em',
              backgroundImage: 'linear-gradient(135deg, #ffffff 0%, #c084fc 60%, #7dd3fc 100%)',
              WebkitBackgroundClip: 'text',
              WebkitTextFillColor: 'transparent',
            }}
          >
            The Infinity Engine
          </h1>
          <p
            style={{
              fontSize: '1.05rem',
              color: '#a8a8b8',
              marginTop: '1rem',
              maxWidth: 640,
            }}
          >
            substrate that runs everything · learns while you sleep · sovereign by default.
          </p>
        </section>

        {/* ─── WHAT IS IT ─── */}
        <section style={{ marginBottom: '3rem' }}>
          <h2 style={sectionHeader}>§ What is it</h2>
          <p style={paragraph}>
            The Infinity Engine is the proprietary runtime underlying every project on apocky.com.
            Where most studios glue together off-the-shelf engines, languages, ledgers, and AI
            stacks, this one substrate handles all of those concerns from a single root —
            consent-encoded in the type system, mycelial across projects, sovereign by default.
          </p>
          <ul style={{ ...paragraph, paddingLeft: '1.2rem', margin: '1rem 0 0' }}>
            <li><strong>One root</strong> · ω-field substrate · every project shares the same trunk.</li>
            <li><strong>Always running</strong> · persistent process · self-authoring during idle.</li>
            <li><strong>Always learning</strong> · KAN-driven adaptation across players + projects.</li>
            <li><strong>Sovereign-cap</strong> · every cell Σ-masked · player-revocable, unilaterally.</li>
          </ul>
        </section>

        {/* ─── WHAT DOES IT POWER ─── */}
        <section style={{ marginBottom: '3rem' }}>
          <h2 style={sectionHeader}>§ What does it power</h2>
          <p style={{ ...paragraph, marginBottom: '1rem' }}>
            Every Apocky project today, and every one to come.
          </p>
          <div
            style={{
              display: 'grid',
              gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))',
              gap: '0.85rem',
            }}
          >
            {POWERED.map((p) => (
              <a
                key={p.id}
                href={p.href}
                target={p.external ? '_blank' : undefined}
                rel={p.external ? 'noopener noreferrer' : undefined}
                style={{
                  display: 'block',
                  padding: '1.1rem 1.2rem',
                  background: 'rgba(20, 20, 30, 0.5)',
                  border: '1px solid #1f1f2a',
                  borderRadius: 8,
                  transition: 'border-color 150ms',
                }}
              >
                <div
                  style={{
                    fontSize: '0.62rem',
                    letterSpacing: '0.15em',
                    color: '#7a7a8c',
                    textTransform: 'uppercase',
                    marginBottom: '0.4rem',
                  }}
                >
                  {p.status}
                </div>
                <div style={{ fontSize: '1rem', color: '#c084fc', fontWeight: 600 }}>
                  {p.name}
                  {p.external ? <span style={{ color: '#5a5a6a', fontSize: '0.8rem', marginLeft: 5 }}>↗</span> : null}
                </div>
                <p style={{ margin: '0.4rem 0 0', fontSize: '0.85rem', color: '#a0a0b0', lineHeight: 1.5 }}>
                  {p.blurb}
                </p>
              </a>
            ))}
          </div>
        </section>

        {/* ─── ARCHITECTURE ─── */}
        <section style={{ marginBottom: '3rem' }}>
          <h2 style={sectionHeader}>§ Architecture · substrate → engine → games</h2>
          <p style={paragraph}>
            Layered, but inseparable. The substrate is the trunk. The engine is the persistent
            process that hosts it. Games and projects are branches sharing one root system.
          </p>
          <div
            style={{
              padding: '1.25rem',
              background: 'rgba(15, 15, 25, 0.5)',
              border: '1px solid #1f1f2a',
              borderRadius: 8,
              marginTop: '1rem',
              fontSize: '0.85rem',
              color: '#cdd6e4',
            }}
          >
            <div className="arch-row">
              <ArchBlock title="SUBSTRATE" subtitle="ω-field · Σ-mask · KAN · HDC" accent="#7dd3fc" />
              <div className="arch-arrow">→</div>
              <ArchBlock title="THE INFINITY ENGINE" subtitle="persistent process · runtime mutate" accent="#c084fc" />
              <div className="arch-arrow">→</div>
              <ArchBlock title="PROJECTS" subtitle="LoA · CSSL · Σ-Chain · Mycelium" accent="#34d399" />
            </div>
          </div>
        </section>

        {/* ─── CSSL-FIRST AUTHORING ─── */}
        <section style={{ marginBottom: '3rem' }}>
          <h2 style={sectionHeader}>§ Authored in CSSL · proprietary language, top-to-bottom</h2>
          <p style={paragraph}>
            The Infinity Engine is authored in CSSL — Apocky's proprietary substrate-system language
            — not in Rust, not in C++, not in any off-the-shelf scripting layer. Every new scene,
            every per-frame tick, every intent kind, every gear/loot classifier is authored as a
            <code style={{ color: '#a78bfa', padding: '0 0.2rem' }}>.cssl</code> source file
            and compiled by csslc. Rust is the bootstrap host for the stage-0 compiler, and the
            host-glue staticlibs that resolve the FFI symbols CSSL declares — never the canonical
            authoring surface for game-logic.
          </p>
          <p style={{ ...paragraph, marginTop: '0.85rem' }}>
            Twelve lines of CSSL is enough to drive the entire LoA engine — the auto-default-link
            mechanism in csslc resolves <code style={{ color: '#7dd3fc', padding: '0 0.2rem' }}>__cssl_engine_run</code>{' '}
            against the loa-host staticlib at compile time:
          </p>
          <pre
            style={{
              background: 'rgba(15, 15, 25, 0.7)',
              border: '1px solid #1f1f2a',
              borderRadius: 6,
              padding: '0.95rem 1.1rem',
              fontSize: '0.78rem',
              lineHeight: 1.55,
              overflowX: 'auto',
              marginTop: '0.85rem',
              fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
              color: '#cdd6e4',
            }}
          >
{`module com.apocky.loa.main

// § FFI declaration · engine entry-point
extern "C" fn __cssl_engine_run() -> i32 ;

// § main · the pure-CSSL entry-point
fn main() -> i32 {
    let exit_code: i32 = __cssl_engine_run() ;
    exit_code
}`}
          </pre>
          <p style={{ ...paragraph, marginTop: '0.85rem', fontSize: '0.88rem', color: '#a0a0b0' }}>
            Read the full CSSL surface at <a href="/docs/cssl-language" style={linkStyle}>/docs/cssl-language</a>{' '}
            and the FFI conventions at <a href="/docs/cssl-ffi" style={linkStyle}>/docs/cssl-ffi</a>.
          </p>
        </section>

        {/* ─── PERSISTENT PROCESS ─── */}
        <section style={{ marginBottom: '3rem' }}>
          <h2 style={sectionHeader}>§ Persistent process · always running, always learning</h2>
          <p style={paragraph}>
            The engine is not summoned per session — it is on. While you sleep, while you work,
            while LoA is closed, the engine plays its own playtests, distills KAN updates,
            self-authors content drafts, and anchors what mattered to Σ-Chain. Idle time is the
            engine's most-productive shift.
          </p>
          <p style={{ ...paragraph, marginTop: '0.75rem' }}>
            Live status of the engine is public: see <a href="/engine" style={linkStyle}>/engine</a>.
            Heartbeats, cycle counts, and the recent-event feed are visible to anyone — that is the
            transparency mandate.
          </p>
        </section>

        {/* ─── OPEN-SOURCE VS PROPRIETARY ─── */}
        <section style={{ marginBottom: '3rem' }}>
          <h2 style={sectionHeader}>§ Open-source vs proprietary · 5-tier distribution</h2>
          <p style={paragraph}>
            Different layers ship under different licenses, by design. Some pieces are public goods.
            Some are the engine itself, and stay closed.
          </p>
          <ul style={{ ...paragraph, paddingLeft: '1.2rem', marginTop: '0.75rem', listStyle: 'none' }}>
            <li><Pill color="#7dd3fc" label="A" /> <strong>Open</strong> · CSSL language + spec snapshots, freely usable.</li>
            <li><Pill color="#c084fc" label="B" /> <strong>Proprietary</strong> · the engine itself, closed-source.</li>
            <li><Pill color="#fbbf24" label="C" /> <strong>Server-only</strong> · mycelium, anchoring, marketplace orchestration.</li>
            <li><Pill color="#a78bfa" label="D" /> <strong>Private</strong> · per-player sovereign-cap state, never shared.</li>
            <li><Pill color="#34d399" label="E" /> <strong>Protocol</strong> · cross-project federation interfaces, openly specified.</li>
          </ul>
        </section>

        {/* ─── SOVEREIGNTY ─── */}
        <section style={{ marginBottom: '3rem' }}>
          <h2 style={sectionHeader}>§ Sovereignty · every cell Σ-masked, every step revocable</h2>
          <div
            style={{
              padding: '1.25rem',
              background: 'rgba(15, 15, 25, 0.5)',
              border: '1px solid #1f1f2a',
              borderRadius: 8,
            }}
          >
            <p style={{ ...paragraph, marginTop: 0 }}>
              The engine never assumes consent. Every substrate cell carries a Σ-mask gate.
              Every player-Home is private-by-default. Every adaptive cycle the engine runs on
              your behalf is opt-in, transparent, and unilaterally revocable.
            </p>
            <p style={{ ...paragraph, marginBottom: 0, color: '#a0a0b0', fontSize: '0.92rem' }}>
              No mining waste. No plutocratic stake. No public-ledger leaks of who-did-what. No
              gas fees. No surveillance. No DRM. No rootkits. No anti-cheat spyware. Participation
              is a gift — never extraction.
            </p>
          </div>
        </section>

        {/* ─── STATUS ─── */}
        <section style={{ marginBottom: '3rem' }}>
          <h2 style={sectionHeader}>§ Status · open roadmap</h2>
          <table
            style={{
              width: '100%',
              borderCollapse: 'collapse',
              fontSize: '0.88rem',
              color: '#cdd6e4',
            }}
          >
            <tbody>
              <tr style={{ borderBottom: '1px solid #1f1f2a' }}>
                <td style={statusLabelCell}>Engine</td>
                <td style={statusValueCell}>alpha · persistent-process running locally + cloud-mirror</td>
              </tr>
              <tr style={{ borderBottom: '1px solid #1f1f2a' }}>
                <td style={statusLabelCell}>LoA</td>
                <td style={statusValueCell}>alpha v0.1.0 · first commercial release · 2026-05-01</td>
              </tr>
              <tr style={{ borderBottom: '1px solid #1f1f2a' }}>
                <td style={statusLabelCell}>CSSL</td>
                <td style={statusValueCell}>open-source · spec stable · compiler shipping per-wave</td>
              </tr>
              <tr style={{ borderBottom: '1px solid #1f1f2a' }}>
                <td style={statusLabelCell}>Σ-Chain</td>
                <td style={statusValueCell}>planning · Coherence-Proof spec frozen · genesis pending</td>
              </tr>
              <tr style={{ borderBottom: '1px solid #1f1f2a' }}>
                <td style={statusLabelCell}>Mycelium</td>
                <td style={statusValueCell}>alpha · 3-mode local-agent · cross-project knowledge active</td>
              </tr>
              <tr>
                <td style={statusLabelCell}>Roadmap</td>
                <td style={statusValueCell}>
                  open · see <a href="/devblog" style={linkStyle}>devblog</a> + <a href="/docs" style={linkStyle}>specs</a>
                </td>
              </tr>
            </tbody>
          </table>
        </section>

        {/* ─── LINKS ─── */}
        <section style={{ marginBottom: '3rem' }}>
          <h2 style={sectionHeader}>§ Links</h2>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: '0.6rem' }}>
            <LinkChip href="/engine" label="live engine status →" />
            <LinkChip href="/docs" label="spec snapshots" />
            <LinkChip href="/content" label="published content" />
            <LinkChip href="/devblog" label="devblog" />
            <LinkChip href="/" label="apocky.com" />
          </div>
        </section>

        <footer
          style={{
            marginTop: '3.5rem',
            paddingTop: '2.5rem',
            borderTop: '1px solid #1f1f2a',
            color: '#5a5a6a',
            fontSize: '0.78rem',
          }}
        >
          <p style={{ margin: 0 }}>§ ¬ harm in the making · sovereignty preserved · t∞</p>
        </footer>
      </main>
    </>
  );
};

// ─────────────────────────────────────────────
// § Shared inline styles
// ─────────────────────────────────────────────

const sectionHeader: React.CSSProperties = {
  fontSize: '0.75rem',
  textTransform: 'uppercase',
  letterSpacing: '0.18em',
  color: '#7a7a8c',
  marginBottom: '1rem',
};

const paragraph: React.CSSProperties = {
  color: '#cdd6e4',
  fontSize: '0.95rem',
  margin: 0,
};

const linkStyle: React.CSSProperties = {
  color: '#7dd3fc',
  textDecoration: 'underline',
};

const statusLabelCell: React.CSSProperties = {
  padding: '0.6rem 0.8rem 0.6rem 0',
  color: '#7a7a8c',
  fontSize: '0.85rem',
  whiteSpace: 'nowrap',
};

const statusValueCell: React.CSSProperties = {
  padding: '0.6rem 0',
  color: '#cdd6e4',
};

// ─────────────────────────────────────────────
// § Sub-components
// ─────────────────────────────────────────────

interface ArchBlockProps {
  title: string;
  subtitle: string;
  accent: string;
}

function ArchBlock(props: ArchBlockProps): JSX.Element {
  return (
    <div
      style={{
        flex: 1,
        padding: '0.85rem 1rem',
        background: 'rgba(20, 20, 30, 0.6)',
        border: '1px solid #2a2a3a',
        borderRadius: 6,
      }}
    >
      <div
        style={{
          fontSize: '0.68rem',
          letterSpacing: '0.15em',
          color: props.accent,
          textTransform: 'uppercase',
          fontWeight: 700,
        }}
      >
        {props.title}
      </div>
      <div style={{ marginTop: '0.35rem', fontSize: '0.82rem', color: '#a0a0b0' }}>
        {props.subtitle}
      </div>
    </div>
  );
}

interface PillProps {
  color: string;
  label: string;
}

function Pill(props: PillProps): JSX.Element {
  return (
    <span
      style={{
        display: 'inline-block',
        minWidth: 22,
        padding: '0.05rem 0.4rem',
        marginRight: '0.45rem',
        borderRadius: 3,
        background: props.color,
        color: '#0a0a0f',
        fontSize: '0.7rem',
        fontWeight: 700,
        textAlign: 'center',
      }}
    >
      {props.label}
    </span>
  );
}

interface LinkChipProps {
  href: string;
  label: string;
}

function LinkChip(props: LinkChipProps): JSX.Element {
  return (
    <a
      href={props.href}
      style={{
        padding: '0.55rem 0.9rem',
        border: '1px solid #2a2a3a',
        borderRadius: 4,
        fontSize: '0.85rem',
        color: '#cdd6e4',
        background: 'rgba(20, 20, 30, 0.5)',
      }}
    >
      {props.label}
    </a>
  );
}

export default InfinityEngine;
