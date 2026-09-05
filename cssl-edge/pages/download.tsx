// apocky.com/download · Labyrinth of Apocalypse · alpha download page
// SSG-friendly · static · no auth required for alpha (free tier)

import type { NextPage } from 'next';
import Head from 'next/head';

const VERSION = 'v0.1.0-alpha';
const PLATFORM = 'windows-x64';
const FILENAME = `LoA-${VERSION}-${PLATFORM}.zip`;
const FILE_URL = `/downloads/${FILENAME}`;
const SHA256_URL = `${FILE_URL}.sha256`;
const SIZE_MB = '3.35';
const SHA256_SHORT = '333d99b8…dc3a4f';
const RELEASE_DATE = '2026-05-03';
const PACKAGE_TEXT_UPDATED = '2026-07-27';

const Download: NextPage = () => {
  return (
    <>
      <Head>
        <title>Download · Labyrinth of Apocalypse · alpha</title>
        <meta name="description" content="Download the first public test build of Labyrinth of Apocalypse for 64-bit Windows. Read what works and what is unfinished before downloading." />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <meta name="theme-color" content="#0a0a0f" />
        <meta property="og:title" content="Download · Labyrinth of Apocalypse · alpha" />
        <meta property="og:description" content="First public test build for 64-bit Windows, with plain-language setup and current limitations." />
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
          }}
        >
          <strong style={{ color: '#fbbf24', fontSize: '0.85rem', letterSpacing: '0.1em' }}>
            EARLY TEST BUILD
          </strong>
          <p style={{ margin: '0.4rem 0 0', color: '#cdd6e4', fontSize: '0.92rem' }}>
            This is the first public version made for testing. It is unfinished,
            some features are missing, and you may find bugs. The sections below
            explain what works before you download it.
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
          {VERSION} · 64-bit Windows · program released {RELEASE_DATE} · package notes updated {PACKAGE_TEXT_UPDATED}
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
              {SIZE_MB} MB · direct download · no copy-protection software
            </span>
          </a>
        </section>

        <section style={{ marginBottom: '2.5rem' }}>
          <h2
            style={{
              fontSize: '0.7rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: '#fbbf24',
              marginBottom: '0.6rem',
            }}
          >
            Windows security notice
          </h2>
          <p style={{ color: '#cdd6e4', fontSize: '0.92rem', marginTop: 0 }}>
            <code style={{ color: '#fbbf24' }}>LoA.exe</code> is not digitally signed. Windows may warn that
            the publisher is unknown. A digital signature lets Windows verify who signed a program; the
            SHA-256 check below verifies the downloaded bytes but does not identify a publisher or prove that
            a program is safe. Do not bypass a warning unless you understand and accept that distinction.
          </p>
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
            Check the download
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
            <p style={{ margin: '0 0 0.65rem' }}>
              SHA-256 is a long fingerprint for a file. After downloading, you
              can compare the fingerprint to check that the file arrived
              unchanged.
            </p>
            <div>Shortened SHA-256 fingerprint: <code style={{ color: '#7dd3fc' }}>{SHA256_SHORT}</code></div>
            <div style={{ marginTop: '0.3rem' }}>
              Full fingerprint: <a href={SHA256_URL} style={{ color: '#7dd3fc', textDecoration: 'underline' }}>{SHA256_URL}</a>
            </div>
            <div style={{ marginTop: '0.5rem', fontSize: '0.78rem', color: '#7a7a8c' }}>
              Windows PowerShell command: <code>Get-FileHash {FILENAME} -Algorithm SHA256</code>
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
            How to run it
          </h2>
          <ol style={{ margin: 0, paddingLeft: '1.2rem', color: '#cdd6e4', fontSize: '0.92rem' }}>
            <li>Open the downloaded ZIP file and extract its contents to a folder you control, such as <code style={{ color: '#fbbf24' }}>C:\Games\LoA</code>.</li>
            <li>Double-click <code style={{ color: '#fbbf24' }}>LoA.exe</code>. This is the game program.</li>
            <li>Press <strong>/</strong>, type a message to the in-game guide, and press <strong>Enter</strong>.</li>
            <li>Open <code style={{ color: '#fbbf24' }}>CONTROLS.md</code> for the complete keyboard controls.</li>
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
            What the download contains
          </h2>
          <ul style={{ margin: 0, paddingLeft: '1.2rem', color: '#cdd6e4', fontSize: '0.92rem' }}>
            <li><code style={{ color: '#7dd3fc' }}>LoA.exe</code> — the 64-bit Windows game program.</li>
            <li><code style={{ color: '#7dd3fc' }}>README.md</code> — a short explanation of this early version.</li>
            <li><code style={{ color: '#7dd3fc' }}>LICENSE.md</code> — the End-User License Agreement, or EULA, which explains the terms for using this build.</li>
            <li><code style={{ color: '#7dd3fc' }}>CONTROLS.md</code> — the keyboard controls and optional developer features.</li>
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
            What works now
          </h2>
          <ul style={{ margin: 0, paddingLeft: '1.2rem', color: '#cdd6e4', fontSize: '0.92rem' }}>
            <li>Open the game window, move around, and take screenshots.</li>
            <li>Type messages to the in-game guide and read the replies on screen.</li>
            <li>Switch among four experimental display modes with the F1 through F4 keys.</li>
            <li>Use a local developer interface if you are testing integrations. The technical details are in <code style={{ color: '#fbbf24' }}>CONTROLS.md</code>.</li>
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
            Not finished yet
          </h2>
          <ul style={{ margin: 0, paddingLeft: '1.2rem', color: '#cdd6e4', fontSize: '0.92rem' }}>
            <li>Combat, crafting, brewing, and spell-casting.</li>
            <li>The marketplace and later progression screens.</li>
            <li>Online multiplayer and other features that connect different players.</li>
            <li>Accounts, online saved games, and purchase records.</li>
            <li>Linux and Apple-silicon Mac versions.</li>
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
            Privacy and control
          </h2>
          <p style={{ color: '#cdd6e4', fontSize: '0.92rem', marginTop: 0 }}>
            This test build is designed to run on your own computer. Its current
            core features do not require a remote language model. The online
            multiplayer, online saved-game, and cross-player features listed
            above are unfinished.
          </p>
          <p style={{ color: '#a8a8b8', fontSize: '0.85rem' }}>
            The download does not include digital rights management (DRM),
            rootkit software, a kernel driver, or anti-cheat monitoring
            software. DRM is software that restricts copying or use. A kernel
            driver runs with deep access to the operating system.
          </p>
          <p style={{ color: '#a8a8b8', fontSize: '0.85rem' }}>
            Running the program creates local diagnostic files in a <code>logs</code> folder. It may also create
            screenshots, cached files, or experimental state on your computer. The developer interface is
            intended to listen only on <code>127.0.0.1:3001</code>, which means this computer only. Do not
            expose that port to another device or the internet. Review the included <code>README.md</code> and
            <code> CONTROLS.md</code> before using developer features.
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
            Feedback and bug reports
          </h2>
          <ul style={{ margin: 0, paddingLeft: '1.2rem', color: '#cdd6e4', fontSize: '0.92rem' }}>
            <li>Email: <a href="mailto:apocky13@gmail.com?subject=%5BLoA-alpha%5D" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>apocky13@gmail.com</a>. Use the subject <code>[LoA-alpha]</code>.</li>
            <li>Optional support: <a href="https://ko-fi.com/oneinfinity" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>Ko-fi</a> or <a href="https://www.patreon.com/0ne1nfinity" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>Patreon</a>.</li>
            <li>Public code: <a href="https://github.com/Apocky/CSSL3" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>GitHub</a>.</li>
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
          <p style={{ margin: 0 }}>Built with care. Please report problems so they can be fixed.</p>
          <p style={{ margin: '0.4rem 0 0' }}>
            © {new Date().getFullYear()} Apocky. The test-build license is in <code>LICENSE.md</code>.
          </p>
        </footer>
      </main>
    </>
  );
};

export default Download;
