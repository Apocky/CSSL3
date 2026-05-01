// apocky.com/mycelium/docs · Mycelium getting-started docs
// per spec/grand-vision/23_MYCELIUM_DESKTOP.csl § UX § slash-commands
// SSG-friendly · static

import type { NextPage } from 'next';
import Head from 'next/head';

interface SlashCmd {
  cmd: string;
  body: string;
}

const SLASH_COMMANDS: ReadonlyArray<SlashCmd> = [
  { cmd: '/help', body: 'list all slash-commands · current mode · sovereign-cap status' },
  { cmd: '/mode <a|b|c>', body: 'switch LLM-bridge mode · A=Anthropic-API · B=Ollama · C=substrate-only' },
  { cmd: '/cap', body: 'show current sovereign-cap state · revoke per-action · global revoke' },
  { cmd: '/audit', body: 'dump recent decisions · Σ-Chain attestation on demand' },
  { cmd: '/wipe', body: 'erase local SQLite session-history · uninstall keeps OS-keychain pristine' },
  { cmd: '/spec <name>', body: 'load a spec from embedded knowledge · e.g. /spec 23 → MYCELIUM_DESKTOP' },
  { cmd: '/quit', body: 'exit cleanly · flushes unsaved-edits · revokes ephemeral caps' },
];

const MyceliumDocs: NextPage = () => {
  return (
    <>
      <Head>
        <title>Docs · Mycelium · apocky.com</title>
        <meta name="description" content="Mycelium getting-started · installation · 3-mode setup · API-key management · slash-commands · spec reference" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <meta name="theme-color" content="#0a0a0f" />
        <link rel="canonical" href="https://apocky.com/mycelium/docs" />
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
          code { color: #7dd3fc; }
        `}</style>
      </Head>
      <main
        style={{
          maxWidth: 800,
          margin: '0 auto',
          padding: '4rem 1.5rem 6rem',
          lineHeight: 1.65,
        }}
      >
        <a href="/mycelium" style={{ fontSize: '0.85rem', color: '#7a7a8c', display: 'inline-block', marginBottom: '2rem' }}>
          ← Mycelium
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
          Mycelium · Docs
        </h1>
        <p style={{ color: '#a8a8b8', marginTop: '0.5rem', fontSize: '0.95rem' }}>
          getting-started · 3-mode setup · slash-commands · spec reference
        </p>

        {/* ── INSTALL ── */}
        <section style={{ marginTop: '2.5rem' }}>
          <h2 style={{ fontSize: '1rem', color: '#c084fc', marginBottom: '0.6rem' }}>§ Installation</h2>
          <ol style={{ margin: 0, paddingLeft: '1.2rem', color: '#cdd6e4', fontSize: '0.92rem' }}>
            <li>Download <code>Mycelium-v0.1.0-alpha-windows-x64.exe</code> from <a href="/mycelium/download" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>/mycelium/download</a></li>
            <li>Verify SHA-256 (PowerShell : <code>Get-FileHash Mycelium-*.exe -Algorithm SHA256</code>)</li>
            <li>Run the installer · accept Defender SmartScreen warning (alpha · code-signing pending)</li>
            <li>WebView2 auto-installs on first-run if missing (Win10 only · Win11 ships it)</li>
          </ol>
        </section>

        {/* ── FIRST-RUN ── */}
        <section style={{ marginTop: '2.5rem' }}>
          <h2 style={{ fontSize: '1rem', color: '#c084fc', marginBottom: '0.6rem' }}>§ First-Run · Mode Selection</h2>
          <p style={{ color: '#cdd6e4', fontSize: '0.92rem', marginTop: 0 }}>
            On first launch Mycelium asks which LLM-bridge mode you want. You can switch any time via <code>/mode</code>.
          </p>
          <ul style={{ margin: 0, paddingLeft: '1.2rem', color: '#cdd6e4', fontSize: '0.92rem' }}>
            <li><strong style={{ color: '#c084fc' }}>Mode-A</strong> — paste your Anthropic API key in Settings → keys are stored in Windows Credential Manager · NEVER on disk · streaming SSE</li>
            <li><strong style={{ color: '#7dd3fc' }}>Mode-B</strong> — point at a local Ollama endpoint (default <code>http://localhost:11434</code>) · zero-cost · privacy-max</li>
            <li><strong style={{ color: '#34d399' }}>Mode-C</strong> — substrate-only · always available · zero external dep · works fully offline</li>
          </ul>
        </section>

        {/* ── API-KEY ── */}
        <section style={{ marginTop: '2.5rem' }}>
          <h2 style={{ fontSize: '1rem', color: '#c084fc', marginBottom: '0.6rem' }}>§ API-Key Management</h2>
          <p style={{ color: '#cdd6e4', fontSize: '0.92rem', marginTop: 0 }}>
            Keys are stored in the OS-keychain only:
          </p>
          <ul style={{ margin: 0, paddingLeft: '1.2rem', color: '#cdd6e4', fontSize: '0.92rem' }}>
            <li><strong>Windows</strong> · Credential Manager (Generic credential under <code>Mycelium/anthropic</code>)</li>
            <li><strong>macOS</strong> · Keychain (future · v0.2.x)</li>
            <li><strong>Linux</strong> · libsecret (future · v0.2.x)</li>
          </ul>
          <p style={{ color: '#a8a8b8', fontSize: '0.88rem', marginTop: '0.6rem' }}>
            Mycelium NEVER writes plain-text keys to disk · NEVER logs them · audit-emit redacts them in trace output. <code>/wipe</code> erases SQLite history but NOT keychain entries (use OS tools to remove those).
          </p>
        </section>

        {/* ── OLLAMA ── */}
        <section style={{ marginTop: '2.5rem' }}>
          <h2 style={{ fontSize: '1rem', color: '#c084fc', marginBottom: '0.6rem' }}>§ Ollama Setup (Mode-B)</h2>
          <ol style={{ margin: 0, paddingLeft: '1.2rem', color: '#cdd6e4', fontSize: '0.92rem' }}>
            <li>Install Ollama from <a href="https://ollama.ai" target="_blank" rel="noopener noreferrer" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>ollama.ai ↗</a></li>
            <li>Pull a model · e.g. <code>ollama pull qwen2.5-coder:14b</code> (recommended for Mycelium)</li>
            <li>Confirm <code>http://localhost:11434</code> responds</li>
            <li>In Mycelium Settings → set Mode-B endpoint + model</li>
            <li>Run <code>/mode b</code> to switch · everything stays local</li>
          </ol>
        </section>

        {/* ── MODE-C ── */}
        <section style={{ marginTop: '2.5rem' }}>
          <h2 style={{ fontSize: '1rem', color: '#c084fc', marginBottom: '0.6rem' }}>§ Mode-C · Substrate-Only Fallback</h2>
          <p style={{ color: '#cdd6e4', fontSize: '0.92rem', marginTop: 0 }}>
            Mode-C uses Apocky's KAN-substrate stage-0 classifier baked into the binary. It works fully offline, requires no key, no Ollama, no internet. Quality is lower than Mode-A or B but covers core capabilities: file edit · git ops · MCP tool dispatch · spec lookup. Use it when you want a guaranteed-private session or when other modes are unavailable.
          </p>
        </section>

        {/* ── SLASH COMMANDS ── */}
        <section style={{ marginTop: '2.5rem' }}>
          <h2 style={{ fontSize: '1rem', color: '#c084fc', marginBottom: '0.6rem' }}>§ Slash-Commands Reference</h2>
          <div
            style={{
              padding: '1rem 1.25rem',
              background: 'rgba(20, 20, 30, 0.5)',
              border: '1px solid #1f1f2a',
              borderRadius: 6,
              fontSize: '0.86rem',
            }}
          >
            <table style={{ width: '100%', borderCollapse: 'collapse' }}>
              <tbody>
                {SLASH_COMMANDS.map((s) => (
                  <tr key={s.cmd} style={{ borderTop: '1px solid #1f1f2a' }}>
                    <td style={{ padding: '0.5rem 0.6rem 0.5rem 0', color: '#fbbf24', whiteSpace: 'nowrap', verticalAlign: 'top' }}>
                      <code>{s.cmd}</code>
                    </td>
                    <td style={{ padding: '0.5rem 0', color: '#cdd6e4' }}>{s.body}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>

        {/* ── SPEC ── */}
        <section style={{ marginTop: '2.5rem' }}>
          <h2 style={{ fontSize: '1rem', color: '#c084fc', marginBottom: '0.6rem' }}>§ Full Spec</h2>
          <p style={{ color: '#cdd6e4', fontSize: '0.92rem', marginTop: 0 }}>
            The full architecture spec lives at <code>specs/grand-vision/23_MYCELIUM_DESKTOP.csl</code> on GitHub : <a href="https://github.com/Apocky/CSSL3/blob/main/specs/grand-vision/23_MYCELIUM_DESKTOP.csl" target="_blank" rel="noopener noreferrer" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>github.com/Apocky/CSSL3 ↗</a>. CSL3 glyph-native · dense by design · density = sovereignty.
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
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: '1rem 1.5rem', marginBottom: '1.25rem' }}>
            <a href="/mycelium">Mycelium</a>
            <a href="/mycelium/download">Download</a>
            <span style={{ color: '#2a2a3a' }}>|</span>
            <a href="/legal/privacy">Privacy</a>
            <a href="/legal/eula">EULA</a>
          </div>
          <p style={{ margin: 0 }}>
            § ¬ harm in the making · sovereignty preserved · t∞
          </p>
        </footer>
      </main>
    </>
  );
};

export default MyceliumDocs;
