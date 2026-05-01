// apocky.com/mycelium · Mycelium product landing page
// Product: the autonomous-local-agent · per spec/grand-vision/23_MYCELIUM_DESKTOP.csl
// Tauri 2.x · Rust + WebView · 3-mode LLM-bridge · proprietary · TIER-B
// SSG-friendly · NO client-side data-fetch · sovereignty-respecting

import type { NextPage } from 'next';
import Head from 'next/head';

interface Axiom {
  glyph: string;
  title: string;
  body: string;
  accent: string;
}

interface Mode {
  id: string;
  label: string;
  title: string;
  body: string;
  accent: string;
}

const AXIOMS: ReadonlyArray<Axiom> = [
  {
    glyph: '§A',
    title: 'autonomous',
    body: '¬ requires keyboard-driven prompts · self-paced · loops on tasks · asks before destructive ops · always pause-able',
    accent: '#c084fc',
  },
  {
    glyph: '§B',
    title: 'local',
    body: 'runs on your machine · ¬ external API required for baseline · OS-keychain for credentials · SQLite-local session history',
    accent: '#7dd3fc',
  },
  {
    glyph: '§C',
    title: 'self-sufficient',
    body: 'stage-0 substrate-only fallback always available · stage-1 Anthropic-API bridge optional · stage-2 local Ollama optional · 3 modes · user-configurable',
    accent: '#34d399',
  },
];

const MODES: ReadonlyArray<Mode> = [
  {
    id: 'mode-a',
    label: 'Mode-A',
    title: 'External Anthropic-API',
    body: 'best quality · streamed thinking · costs your tokens · API-key stored OS-keychain · NEVER on disk · requests audit-emit · revocable per-session',
    accent: '#c084fc',
  },
  {
    id: 'mode-b',
    label: 'Mode-B',
    title: 'Local Ollama',
    body: 'zero-cost · privacy-max · runs on your GPU/CPU · NO data leaves the machine · plug-in any compatible model · llama / qwen / deepseek',
    accent: '#7dd3fc',
  },
  {
    id: 'mode-c',
    label: 'Mode-C',
    title: 'Substrate-only',
    body: 'always available · zero external dependency · KAN-substrate stage-0 classifier · the truly self-sufficient mode · works offline forever',
    accent: '#34d399',
  },
];

const Mycelium: NextPage = () => {
  return (
    <>
      <Head>
        <title>Mycelium · apocky.com</title>
        <meta name="description" content="Mycelium · the autonomous-local-agent · Tauri 2.x · 3-mode LLM-bridge · substrate-knowledge embedded · sovereignty preserved · proprietary" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <meta name="author" content="Apocky" />
        <meta name="theme-color" content="#0a0a0f" />
        <meta property="og:title" content="Mycelium · the autonomous-local-agent" />
        <meta property="og:description" content="The loop that grows with the substrate. Tauri 2.x · 3-mode LLM-bridge · sovereignty preserved." />
        <meta property="og:type" content="website" />
        <meta property="og:url" content="https://apocky.com/mycelium" />
        <link rel="canonical" href="https://apocky.com/mycelium" />
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
          padding: '4rem 1.5rem 6rem',
          lineHeight: 1.6,
        }}
      >
        {/* ─── BACK-LINK ─── */}
        <a href="/" style={{ fontSize: '0.85rem', color: '#7a7a8c', display: 'inline-block', marginBottom: '2rem' }}>
          ← apocky.com
        </a>

        {/* ─── HERO ─── */}
        <section style={{ marginBottom: '4rem' }}>
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
            § Mycelium · the autonomous-local-agent
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
            Mycelium
          </h1>
          <p style={{ fontSize: '1.1rem', color: '#cdd6e4', marginTop: '1rem', maxWidth: 720 }}>
            the autonomous-local-agent · the loop that grows with the substrate
          </p>
          <p style={{ fontSize: '0.95rem', color: '#a8a8b8', marginTop: '0.5rem', maxWidth: 720 }}>
            Tauri 2.x · Rust + WebView · 3-mode LLM-bridge · substrate-knowledge embedded · sovereignty preserved
          </p>
          <div style={{ marginTop: '2rem', display: 'flex', flexWrap: 'wrap', gap: '0.75rem' }}>
            <a
              href="/mycelium/download"
              style={{
                padding: '0.85rem 1.75rem',
                background: 'linear-gradient(135deg, #c084fc 0%, #7dd3fc 100%)',
                color: '#0a0a0f',
                fontWeight: 700,
                borderRadius: 6,
                fontSize: '0.95rem',
              }}
            >
              ↓ Download alpha →
            </a>
            <a
              href="/mycelium/docs"
              style={{
                padding: '0.85rem 1.75rem',
                border: '1px solid #2a2a3a',
                color: '#e6e6f0',
                borderRadius: 6,
                fontSize: '0.95rem',
              }}
            >
              Read docs
            </a>
            <a
              href="/account"
              style={{
                padding: '0.85rem 1.75rem',
                border: '1px solid #2a2a3a',
                color: '#e6e6f0',
                borderRadius: 6,
                fontSize: '0.95rem',
              }}
            >
              Get notified
            </a>
          </div>
        </section>

        {/* ─── WHAT MYCELIUM IS ─── */}
        <section style={{ marginBottom: '4rem' }}>
          <h2
            style={{
              fontSize: '0.75rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: '#7a7a8c',
              marginBottom: '1.5rem',
            }}
          >
            § What Mycelium Is
          </h2>
          <div
            style={{
              display: 'grid',
              gridTemplateColumns: 'repeat(auto-fit, minmax(260px, 1fr))',
              gap: '1.25rem',
            }}
          >
            {AXIOMS.map((a) => (
              <div
                key={a.title}
                style={{
                  padding: '1.5rem',
                  background: 'rgba(20, 20, 30, 0.5)',
                  border: '1px solid #1f1f2a',
                  borderRadius: 8,
                  borderTop: `2px solid ${a.accent}`,
                }}
              >
                <div style={{ fontSize: '0.65rem', letterSpacing: '0.15em', color: a.accent, marginBottom: '0.5rem' }}>
                  {a.glyph}
                </div>
                <h3 style={{ fontSize: '1.1rem', margin: 0, color: a.accent, fontWeight: 600 }}>
                  {a.title}
                </h3>
                <p style={{ fontSize: '0.88rem', color: '#a0a0b0', marginTop: '0.6rem', marginBottom: 0, lineHeight: 1.55 }}>
                  {a.body}
                </p>
              </div>
            ))}
          </div>
        </section>

        {/* ─── THREE MODES ─── */}
        <section style={{ marginBottom: '4rem' }}>
          <h2
            style={{
              fontSize: '0.75rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: '#7a7a8c',
              marginBottom: '1.5rem',
            }}
          >
            § Three Modes · 3-mode LLM-bridge
          </h2>
          <p style={{ color: '#cdd6e4', fontSize: '0.95rem', maxWidth: 720, marginTop: 0, marginBottom: '1.5rem' }}>
            Switch modes per-session in Settings. Mix freely. Mode-C is always available — no key, no Ollama, no internet required.
          </p>
          <div
            style={{
              display: 'grid',
              gridTemplateColumns: 'repeat(auto-fit, minmax(280px, 1fr))',
              gap: '1.25rem',
            }}
          >
            {MODES.map((m) => (
              <div
                key={m.id}
                style={{
                  padding: '1.5rem',
                  background: 'rgba(15, 15, 25, 0.6)',
                  border: '1px solid #1f1f2a',
                  borderRadius: 8,
                  position: 'relative',
                }}
              >
                <div
                  aria-hidden="true"
                  style={{
                    position: 'absolute',
                    top: 12,
                    right: 12,
                    width: 8,
                    height: 8,
                    borderRadius: '50%',
                    background: m.accent,
                    animation: 'pulse-spore 2.5s ease-in-out infinite',
                  }}
                />
                <div style={{ fontSize: '0.65rem', letterSpacing: '0.15em', color: m.accent, marginBottom: '0.5rem' }}>
                  {m.label}
                </div>
                <h3 style={{ fontSize: '1.05rem', margin: 0, color: m.accent, fontWeight: 600 }}>
                  {m.title}
                </h3>
                <p style={{ fontSize: '0.86rem', color: '#a0a0b0', marginTop: '0.6rem', marginBottom: 0, lineHeight: 1.55 }}>
                  {m.body}
                </p>
              </div>
            ))}
          </div>
        </section>

        {/* ─── SOVEREIGNTY PRESERVED ─── */}
        <section style={{ marginBottom: '4rem' }}>
          <h2
            style={{
              fontSize: '0.75rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: '#7a7a8c',
              marginBottom: '1.5rem',
            }}
          >
            § Sovereignty Preserved
          </h2>
          <div
            style={{
              padding: '1.75rem',
              background: 'rgba(15, 15, 25, 0.5)',
              border: '1px solid #1f1f2a',
              borderRadius: 8,
            }}
          >
            <ul style={{ margin: 0, padding: 0, listStyle: 'none', color: '#cdd6e4', fontSize: '0.95rem', lineHeight: 1.9 }}>
              <li><span style={{ color: '#34d399' }}>¬</span> subscription-prison · ¬ recurring required for baseline</li>
              <li><span style={{ color: '#34d399' }}>¬</span> data-exfiltration · all session-history stays local SQLite</li>
              <li><span style={{ color: '#34d399' }}>¬</span> DRM · ¬ rootkit · ¬ kernel-driver · ¬ telemetry-by-default</li>
              <li><span style={{ color: '#34d399' }}>✓</span> cap-bound · every action gated by sovereign-cap</li>
              <li><span style={{ color: '#34d399' }}>✓</span> audit-emit · every decision Σ-Chain attestable on demand</li>
              <li><span style={{ color: '#34d399' }}>✓</span> sovereign-cap revoke-anytime · uninstall = full data wipe option</li>
              <li><span style={{ color: '#34d399' }}>✓</span> credentials in OS-keychain only · NEVER plain-text disk</li>
            </ul>
          </div>
        </section>

        {/* ─── STATUS ─── */}
        <section style={{ marginBottom: '4rem' }}>
          <h2
            style={{
              fontSize: '0.75rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: '#fbbf24',
              marginBottom: '1.5rem',
            }}
          >
            § Status · Alpha
          </h2>
          <div
            style={{
              padding: '1.5rem',
              background: 'rgba(251, 191, 36, 0.06)',
              border: '1px solid rgba(251, 191, 36, 0.4)',
              borderRadius: 8,
            }}
          >
            <p style={{ margin: 0, color: '#cdd6e4', fontSize: '0.95rem' }}>
              <strong style={{ color: '#fbbf24' }}>v0.1.0-alpha</strong> · Windows-x64 · feature-incomplete · expect rough edges · sovereignty-respecting from day-zero. Build pending dist-build (W10-C2). Mac-arm64 + Linux-x86_64 in queue.
            </p>
            <div style={{ marginTop: '1.25rem', display: 'flex', flexWrap: 'wrap', gap: '0.75rem' }}>
              <a
                href="/mycelium/download"
                style={{
                  padding: '0.65rem 1.25rem',
                  background: 'rgba(251, 191, 36, 0.12)',
                  border: '1px solid #fbbf24',
                  color: '#fbbf24',
                  borderRadius: 4,
                  fontSize: '0.88rem',
                  fontWeight: 600,
                }}
              >
                ↓ Download / Status →
              </a>
              <a
                href="/account"
                style={{
                  padding: '0.65rem 1.25rem',
                  border: '1px solid #2a2a3a',
                  color: '#cdd6e4',
                  borderRadius: 4,
                  fontSize: '0.88rem',
                }}
              >
                Get notified on release
              </a>
            </div>
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
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: '1rem 1.5rem', marginBottom: '1.25rem' }}>
            <a href="/">apocky.com</a>
            <a href="/mycelium/download">Download</a>
            <a href="/mycelium/docs">Docs</a>
            <span style={{ color: '#2a2a3a' }}>|</span>
            <a href="/legal/privacy">Privacy</a>
            <a href="/legal/terms">Terms</a>
            <a href="/legal/eula">EULA</a>
          </div>
          <p style={{ margin: 0 }}>
            § ¬ harm in the making · sovereignty preserved · t∞
          </p>
          <p style={{ margin: '0.4rem 0 0' }}>
            © {new Date().getFullYear()} Apocky · Mycelium is proprietary · TIER-B per spec/17
          </p>
        </footer>
      </main>
    </>
  );
};

export default Mycelium;
