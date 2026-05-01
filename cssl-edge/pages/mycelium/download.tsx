// apocky.com/mycelium/download · Mycelium alpha download page
// per spec/grand-vision/23_MYCELIUM_DESKTOP.csl § DISTRIBUTION
// SSG-friendly · static · placeholder-aware until W10-C2 dist-build replaces .exe

import type { NextPage } from 'next';
import Head from 'next/head';

const VERSION = 'v0.1.0-alpha';
const PLATFORM = 'windows-x64';
const FILENAME = `Mycelium-${VERSION}-${PLATFORM}.exe`;
const FILE_URL = `/downloads/${FILENAME}`;
const SHA256_PLACEHOLDER = 'pending dist-build (W10-C2)';
const BLAKE3_PLACEHOLDER = 'pending dist-build (W10-C2)';
const BUILD_PENDING = true;

const MyceliumDownload: NextPage = () => {
  return (
    <>
      <Head>
        <title>Download · Mycelium · alpha</title>
        <meta name="description" content="Download Mycelium v0.1.0-alpha for Windows x64 · Tauri 2.x · 3-mode LLM-bridge · self-hosted · no DRM · sovereignty-respecting" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <meta name="theme-color" content="#0a0a0f" />
        <meta property="og:title" content="Download · Mycelium · alpha" />
        <meta property="og:description" content="Mycelium alpha — autonomous-local-agent. Self-hosted. No DRM. Sovereignty-respecting." />
        <meta property="og:url" content="https://apocky.com/mycelium/download" />
        <link rel="canonical" href="https://apocky.com/mycelium/download" />
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
        <a href="/mycelium" style={{ fontSize: '0.85rem', color: '#7a7a8c', display: 'inline-block', marginBottom: '2rem' }}>
          ← Mycelium
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
            ⚠ ALPHA — feature-incomplete · expect rough edges · sovereignty-respecting
          </strong>
          <p style={{ margin: '0.4rem 0 0', color: '#cdd6e4', fontSize: '0.92rem' }}>
            Mycelium is in active development. The substrate-only mode works today. The Anthropic-API and Ollama bridges are wired and being hardened. Feedback welcome.
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
          Mycelium {VERSION} · Windows-x64
        </h1>
        <p style={{ color: '#a8a8b8', marginTop: '0.5rem', fontSize: '0.95rem' }}>
          the autonomous-local-agent · Tauri 2.x · 3-mode LLM-bridge
        </p>

        {/* ── DOWNLOAD CTA ── */}
        <section style={{ marginTop: '2rem', marginBottom: '2.5rem' }}>
          {BUILD_PENDING ? (
            <div
              style={{
                display: 'inline-flex',
                flexDirection: 'column',
                alignItems: 'flex-start',
                padding: '1.25rem 2rem',
                background: 'rgba(251, 191, 36, 0.06)',
                border: '1px dashed rgba(251, 191, 36, 0.5)',
                color: '#fbbf24',
                fontWeight: 700,
                borderRadius: 8,
                fontSize: '1rem',
              }}
            >
              <span>◐ Build pending · check back at v0.1.0 release</span>
              <span style={{ fontSize: '0.78rem', fontWeight: 400, opacity: 0.85, marginTop: '0.4rem', color: '#cdd6e4' }}>
                Filename will be <code style={{ color: '#fbbf24' }}>{FILENAME}</code> · NSIS-bundled Tauri build · ≤ 50 MB
              </span>
            </div>
          ) : (
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
              }}
            >
              <span>↓ Download {FILENAME}</span>
              <span style={{ fontSize: '0.78rem', fontWeight: 400, opacity: 0.7, marginTop: '0.3rem' }}>
                self-hosted · no DRM · sovereignty-respecting
              </span>
            </a>
          )}
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
            <div>SHA-256 : <code style={{ color: '#7dd3fc' }}>{SHA256_PLACEHOLDER}</code></div>
            <div style={{ marginTop: '0.3rem' }}>
              BLAKE3 : <code style={{ color: '#7dd3fc' }}>{BLAKE3_PLACEHOLDER}</code>
            </div>
            <div style={{ marginTop: '0.5rem', fontSize: '0.78rem', color: '#7a7a8c' }}>
              both hashes will be published with the dist-build · verify on Windows : <code>Get-FileHash {FILENAME} -Algorithm SHA256</code>
            </div>
          </div>
        </section>

        {/* ── WEBVIEW2 ── */}
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
            § WebView2 Runtime
          </h2>
          <p style={{ color: '#cdd6e4', fontSize: '0.92rem', marginTop: 0 }}>
            <strong>Windows 11</strong> ships WebView2 by default — nothing to install. <strong>Windows 10</strong> users may need the runtime: <a href="https://developer.microsoft.com/en-us/microsoft-edge/webview2/" target="_blank" rel="noopener noreferrer" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>Microsoft WebView2 Evergreen Installer ↗</a>. The Tauri bundle will offer to install it on first-run if missing.
          </p>
        </section>

        {/* ── EULA ── */}
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
            § Alpha EULA
          </h2>
          <p style={{ color: '#cdd6e4', fontSize: '0.92rem', marginTop: 0 }}>
            Mycelium ships under the same alpha-tester EULA as the rest of Apocky's portfolio: <a href="/legal/eula" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>/legal/eula</a>. Sovereignty-respecting · no rootkit · no DRM · 14-day refund · sovereign-cap unilaterally revocable.
          </p>
        </section>

        {/* ── INCLUDED ── */}
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
            ✓ What's included
          </h2>
          <ul style={{ margin: 0, paddingLeft: '1.2rem', color: '#cdd6e4', fontSize: '0.92rem' }}>
            <li><code style={{ color: '#7dd3fc' }}>Mycelium.exe</code> · Win-x64 · self-contained · NSIS-bundled</li>
            <li><code style={{ color: '#7dd3fc' }}>frontend/</code> · React + Vite · bundled into the .exe (¬ external resources)</li>
            <li>Tauri 2.x runtime · WebView2 host (uses system WebView2)</li>
            <li>Windows Credential Manager integration · API-keys never on disk</li>
            <li>Embedded substrate-knowledge · all <code>specs/grand-vision/*.csl</code> + <code>memory/*.md</code> baked at build-time</li>
            <li>SQLite local-session-history · ~10 MB cap · auto-rotates</li>
            <li>3-mode LLM-bridge · Mode-A external · Mode-B Ollama · Mode-C substrate-only</li>
          </ul>
        </section>

        {/* ── NOT INCLUDED ── */}
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
            ◐ What's NOT included yet
          </h2>
          <ul style={{ margin: 0, paddingLeft: '1.2rem', color: '#cdd6e4', fontSize: '0.92rem' }}>
            <li>Code-signing certificate · Apocky-action pending · Defender SmartScreen warning expected on first-run</li>
            <li>Anthropic API key · paste in Settings · stored in OS-keychain · NEVER on disk</li>
            <li>Ollama runtime · download separately if you want Mode-B · <a href="https://ollama.ai" target="_blank" rel="noopener noreferrer" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>ollama.ai ↗</a></li>
            <li>Mac-arm64 · Linux-x86_64 builds · queued for v0.2.x</li>
            <li>Trained-KAN-weights pack · stage-2 quality · ships in v0.3.x</li>
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
            <strong>Local-by-default.</strong> Mode-C (substrate-only) is fully offline · no network calls. Mode-A and Mode-B make outbound calls only to the endpoints you configure. ¬ telemetry · ¬ analytics · ¬ DRM · ¬ rootkit · ¬ kernel-driver. Sovereign-cap unilaterally revocable. Uninstall = full data wipe option.
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
            <li>email : <a href="mailto:apocky13@gmail.com?subject=%5BMycelium-alpha%5D" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>apocky13@gmail.com</a> · subject <code>[Mycelium-alpha]</code></li>
            <li>support : <a href="https://ko-fi.com/oneinfinity" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>ko-fi.com/oneinfinity</a> · <a href="https://www.patreon.com/0ne1nfinity" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>patreon.com/0ne1nfinity</a></li>
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
            © {new Date().getFullYear()} Apocky · Mycelium TIER-B proprietary · alpha-tester EULA in <a href="/legal/eula" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>/legal/eula</a>
          </p>
        </footer>
      </main>
    </>
  );
};

export default MyceliumDownload;
