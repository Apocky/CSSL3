// apocky.com/download · Labyrinth of Apocalypse · alpha download page
// SSG-friendly · static · no auth required for alpha (free tier)

import type { NextPage } from 'next';
import Head from 'next/head';

const VERSION = 'v0.1.0-alpha';
const PLATFORM = 'windows-x64';
const FILENAME = `LoA-${VERSION}-${PLATFORM}.zip`;
const FILE_URL = `/downloads/${FILENAME}`;
const SHA256_URL = `${FILE_URL}.sha256`;
const SIZE_MB = '3.41';
const SHA256_SHORT = '74299b2d…21666';
const RELEASE_DATE = '2026-05-03';

const Download: NextPage = () => {
  return (
    <>
      <Head>
        <title>Download · Labyrinth of Apocalypse · alpha</title>
        <meta name="description" content="Download Labyrinth of Apocalypse v0.1.0-alpha for Windows x64 · 3.41 MB · self-hosted · no DRM · sovereignty-respecting" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <meta name="theme-color" content="#0a0a0f" />
        <meta property="og:title" content="Download · Labyrinth of Apocalypse · alpha" />
        <meta property="og:description" content="First public alpha. Substrate-grown action-RPG. Self-hosted. No DRM." />
        <meta property="og:url" content="https://apocky.com/download" />
        <link rel="canonical" href="https://apocky.com/download" />
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
          @keyframes pulse-warn {
            0%, 100% { opacity: 0.8; }
            50% { opacity: 1; }
          }
        `}</style>
      </Head>
      <main
        style={{
          maxWidth: 760,
          margin: '0 auto',
          padding: '4rem 1.5rem 6rem',
          lineHeight: 1.65,
        }}
      >
        <a href="/" style={{ fontSize: '0.85rem', color: '#7a7a8c', display: 'inline-block', marginBottom: '2rem' }}>
          ← apocky.com
        </a>

        {/* ── ALPHA WARNING ── */}
        <div
          style={{
            padding: '1rem 1.25rem',
            background: 'rgba(251, 191, 36, 0.08)',
            border: '1px solid rgba(251, 191, 36, 0.4)',
            borderRadius: 6,
            marginBottom: '2.5rem',
            animation: 'pulse-warn 3s ease-in-out infinite',
          }}
        >
          <strong style={{ color: '#fbbf24', fontSize: '0.85rem', letterSpacing: '0.1em' }}>
            ⚠ ALPHA RELEASE
          </strong>
          <p style={{ margin: '0.4rem 0 0', color: '#cdd6e4', fontSize: '0.92rem' }}>
            This is a <strong>first public alpha</strong>. The substrate works. The game-loop is being woven on top in real-time. Expect bugs · expect missing features · feedback welcome.
          </p>
        </div>

        {/* ── HERO ── */}
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
          Labyrinth of Apocalypse
        </h1>
        <p style={{ color: '#a8a8b8', marginTop: '0.5rem', fontSize: '0.95rem' }}>
          {VERSION} · Windows x64 · released {RELEASE_DATE}
        </p>

        {/* ── DOWNLOAD CTA ── */}
        <section style={{ marginTop: '2rem', marginBottom: '2.5rem' }}>
          <a
            href={FILE_URL}
            download
            style={{
              display: 'inline-flex',
              flexDirection: 'column',
              alignItems: 'center',
              padding: '1.25rem 2.5rem',
              background: 'linear-gradient(135deg, #c084fc 0%, #7dd3fc 100%)',
              color: '#0a0a0f',
              fontWeight: 700,
              borderRadius: 8,
              fontSize: '1.05rem',
              boxShadow: '0 4px 24px rgba(124, 211, 252, 0.25)',
            }}
          >
            <span>↓ Download {FILENAME}</span>
            <span style={{ fontSize: '0.78rem', fontWeight: 400, opacity: 0.7, marginTop: '0.3rem' }}>
              {SIZE_MB} MB · self-hosted · no DRM
            </span>
          </a>
        </section>

        {/* ── INTEGRITY ── */}
        <section style={{ marginBottom: '2.5rem' }}>
          <h2
            style={{
              fontSize: '0.7rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: '#7a7a8c',
              marginBottom: '0.6rem',
            }}
          >
            § File Integrity
          </h2>
          <div
            style={{
              padding: '0.9rem 1.1rem',
              background: 'rgba(20, 20, 30, 0.5)',
              border: '1px solid #1f1f2a',
              borderRadius: 6,
              fontSize: '0.82rem',
              color: '#a8a8b8',
            }}
          >
            <div>SHA-256 : <code style={{ color: '#7dd3fc' }}>{SHA256_SHORT}</code></div>
            <div style={{ marginTop: '0.3rem' }}>
              full hash : <a href={SHA256_URL} style={{ color: '#7dd3fc', textDecoration: 'underline' }}>{SHA256_URL}</a>
            </div>
            <div style={{ marginTop: '0.5rem', fontSize: '0.78rem', color: '#7a7a8c' }}>
              verify on Windows : <code>Get-FileHash {FILENAME} -Algorithm SHA256</code>
            </div>
          </div>
        </section>

        {/* ── HOW TO RUN ── */}
        <section style={{ marginBottom: '2.5rem' }}>
          <h2
            style={{
              fontSize: '0.7rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: '#7a7a8c',
              marginBottom: '0.8rem',
            }}
          >
            § How to Run
          </h2>
          <ol style={{ margin: 0, paddingLeft: '1.2rem', color: '#cdd6e4', fontSize: '0.92rem' }}>
            <li>Extract the ZIP to a folder you own (e.g. <code style={{ color: '#fbbf24' }}>C:\Games\LoA</code>)</li>
            <li>Double-click <code style={{ color: '#fbbf24' }}>LoA.exe</code> · or run from PowerShell</li>
            <li>Press <strong>/</strong> to chat with the GM · type · press <strong>Enter</strong></li>
            <li>See <code style={{ color: '#fbbf24' }}>CONTROLS.md</code> in the ZIP for full keybindings</li>
          </ol>
        </section>

        {/* ── WHAT'S IN ── */}
        <section style={{ marginBottom: '2.5rem' }}>
          <h2
            style={{
              fontSize: '0.7rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: '#7a7a8c',
              marginBottom: '0.8rem',
            }}
          >
            § What's in the ZIP
          </h2>
          <ul style={{ margin: 0, paddingLeft: '1.2rem', color: '#cdd6e4', fontSize: '0.92rem' }}>
            <li><code style={{ color: '#7dd3fc' }}>LoA.exe</code> · 9.27 MB · self-contained Windows-x64 · no third-party DLLs</li>
            <li><code style={{ color: '#7dd3fc' }}>README.md</code> · alpha framing · what works · what doesn't</li>
            <li><code style={{ color: '#7dd3fc' }}>LICENSE.md</code> · alpha-tester EULA · refund policy · sovereignty</li>
            <li><code style={{ color: '#7dd3fc' }}>CONTROLS.md</code> · keybinding reference · MCP tools · sovereign-cap escape-hatch</li>
          </ul>
        </section>

        {/* ── WHAT WORKS ── */}
        <section style={{ marginBottom: '2.5rem' }}>
          <h2
            style={{
              fontSize: '0.7rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: '#34d399',
              marginBottom: '0.8rem',
            }}
          >
            ✓ What works in alpha
          </h2>
          <ul style={{ margin: 0, paddingLeft: '1.2rem', color: '#cdd6e4', fontSize: '0.92rem' }}>
            <li>Open a window · move around · take screenshots</li>
            <li>Press <code style={{ color: '#fbbf24' }}>/</code> to chat with the GM (text-input · GM responds in HUD chat-log)</li>
            <li>4 render modes : F1 mainstream · F2 spectral · F3 Stokes · F4 CFER</li>
            <li>118 MCP tools on <code style={{ color: '#fbbf24' }}>localhost:3001</code></li>
            <li>Σ-Chain · Mycelial · Akashic · Coder-runtime crates LIVE (planning-tier UI · stubbed in-game)</li>
          </ul>
        </section>

        {/* ── WHAT'S COMING ── */}
        <section style={{ marginBottom: '2.5rem' }}>
          <h2
            style={{
              fontSize: '0.7rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: '#fbbf24',
              marginBottom: '0.8rem',
            }}
          >
            ◐ Coming in upcoming alphas
          </h2>
          <ul style={{ margin: 0, paddingLeft: '1.2rem', color: '#cdd6e4', fontSize: '0.92rem' }}>
            <li>Combat · craft · brew · cast spells (FFI symbol-wire-up in-flight)</li>
            <li>Bazaar · Coherence-Engine ascension UI</li>
            <li>Multiplayer · cross-user mycelium · Akashic-Records browse</li>
            <li>Real-Supabase (auth · cloud-save · Stripe-entitlements)</li>
            <li>Linux-x64 · macOS-arm64 builds · trained-KAN-weights pack</li>
          </ul>
        </section>

        {/* ── PRIVACY ── */}
        <section style={{ marginBottom: '2.5rem' }}>
          <h2
            style={{
              fontSize: '0.7rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: '#7a7a8c',
              marginBottom: '0.8rem',
            }}
          >
            § Privacy · Sovereignty
          </h2>
          <p style={{ color: '#cdd6e4', fontSize: '0.92rem', marginTop: 0 }}>
            <strong>Fully self-hosted.</strong> No external API · no Claude · no Ollama · no remote-LLM. KAN-substrate stage-1 classifier runs LOCAL. All player state stays LOCAL by default. Cross-user features (Σ-Chain · Mycelium · Akashic-Records) are OPT-IN per-event-grain.
          </p>
          <p style={{ color: '#a8a8b8', fontSize: '0.85rem' }}>
            No DRM · no rootkit · no kernel-driver · no anti-cheat-spyware. Your machine is yours. Sovereign-cap unilaterally revocable.
          </p>
        </section>

        {/* ── FEEDBACK ── */}
        <section style={{ marginBottom: '2.5rem' }}>
          <h2
            style={{
              fontSize: '0.7rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: '#7a7a8c',
              marginBottom: '0.8rem',
            }}
          >
            § Feedback · Bug Reports
          </h2>
          <ul style={{ margin: 0, paddingLeft: '1.2rem', color: '#cdd6e4', fontSize: '0.92rem' }}>
            <li>email : <a href="mailto:apocky13@gmail.com?subject=%5BLoA-alpha%5D" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>apocky13@gmail.com</a> · subject <code>[LoA-alpha]</code></li>
            <li>support : <a href="https://ko-fi.com/oneinfinity" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>ko-fi.com/oneinfinity</a> · <a href="https://www.patreon.com/0ne1nfinity" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>patreon.com/0ne1nfinity</a></li>
            <li>code : <a href="https://github.com/Apocky/CSSL3" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>github.com/Apocky/CSSL3</a></li>
          </ul>
        </section>

        {/* ── FOOTER ── */}
        <footer
          style={{
            paddingTop: '2.5rem',
            borderTop: '1px solid #1f1f2a',
            color: '#5a5a6a',
            fontSize: '0.78rem',
          }}
        >
          <p style={{ margin: 0 }}>
            § ¬ harm in the making · sovereignty preserved · t∞
          </p>
          <p style={{ margin: '0.4rem 0 0' }}>
            © {new Date().getFullYear()} Apocky · alpha-tester EULA in <code>LICENSE.md</code>
          </p>
        </footer>
      </main>
    </>
  );
};

export default Download;
