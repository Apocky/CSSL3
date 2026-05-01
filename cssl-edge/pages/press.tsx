// apocky.com/press · press-kit page · static · SSG

import type { NextPage } from 'next';
import Head from 'next/head';

const Press: NextPage = () => {
  return (
    <>
      <Head>
        <title>Press · Apocky</title>
        <meta name="description" content="Press kit · logos · screenshots · boilerplate · contact for Apocky and Labyrinth of Apocalypse." />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <meta name="theme-color" content="#0a0a0f" />
        <link rel="canonical" href="https://apocky.com/press" />
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
          maxWidth: 880,
          margin: '0 auto',
          padding: '4rem 1.5rem 6rem',
          lineHeight: 1.65,
        }}
      >
        <a href="/" style={{ fontSize: '0.85rem', color: '#7a7a8c', display: 'inline-block', marginBottom: '2rem' }}>
          ← apocky.com
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
          Press Kit
        </h1>
        <p style={{ color: '#a8a8b8', marginTop: '0.5rem', fontSize: '0.95rem' }}>
          § Coverage of Apocky projects is welcome. Use this kit as a starting point.
        </p>

        {/* ── BOILERPLATE ── */}
        <section style={{ marginTop: '2.5rem' }}>
          <h2
            style={{
              fontSize: '0.7rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: '#7a7a8c',
              marginBottom: '0.8rem',
            }}
          >
            § Boilerplate
          </h2>
          <div
            style={{
              padding: '1.25rem 1.5rem',
              background: 'rgba(20, 20, 30, 0.5)',
              border: '1px solid #1f1f2a',
              borderRadius: 8,
              color: '#cdd6e4',
              fontSize: '0.92rem',
            }}
          >
            <p style={{ marginTop: 0 }}>
              <strong>Apocky</strong> designs and builds substrate-native systems — proprietary architecture from
              language up through distributed ledger, mycelial multiplayer, and runtime procedural generation.
              Based in Phoenix, Arizona.
            </p>
            <p>
              <strong>Labyrinth of Apocalypse</strong> is the first commercial release on the Substrate — a
              sovereign-by-default action-RPG where roguelike runs, alchemy, and gear-ascension grow from one
              shared mycelial substrate. First public alpha shipped 2026-05-01.
            </p>
            <p style={{ marginBottom: 0 }}>
              <strong>CSSL</strong> (Conscious Substrate System Language) is the proprietary language and compiler
              stack underlying every Apocky project. Consent-encoded in the type system. Density as sovereignty.
            </p>
          </div>
        </section>

        {/* ── ASSETS ── */}
        <section style={{ marginTop: '2.5rem' }}>
          <h2
            style={{
              fontSize: '0.7rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: '#7a7a8c',
              marginBottom: '0.8rem',
            }}
          >
            § Assets
          </h2>
          <ul style={{ margin: 0, paddingLeft: '1.2rem', color: '#cdd6e4', fontSize: '0.92rem' }}>
            <li>Apocky logo · SVG : <a href="/icon-512.svg" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>icon-512.svg</a></li>
            <li>Apocky logo · 192px : <a href="/icon-192.svg" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>icon-192.svg</a></li>
            <li>LoA download bundle : <a href="/download" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>/download</a></li>
            <li>Spec snapshots : <a href="/docs" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>/docs</a></li>
            <li>Devblog : <a href="/devblog" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>/devblog</a></li>
          </ul>
        </section>

        {/* ── FACT-SHEET ── */}
        <section style={{ marginTop: '2.5rem' }}>
          <h2
            style={{
              fontSize: '0.7rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: '#7a7a8c',
              marginBottom: '0.8rem',
            }}
          >
            § Fact Sheet
          </h2>
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
                <td style={{ padding: '0.6rem 0.8rem 0.6rem 0', color: '#7a7a8c' }}>Studio</td>
                <td style={{ padding: '0.6rem 0' }}>Apocky · solo developer</td>
              </tr>
              <tr style={{ borderBottom: '1px solid #1f1f2a' }}>
                <td style={{ padding: '0.6rem 0.8rem 0.6rem 0', color: '#7a7a8c' }}>Location</td>
                <td style={{ padding: '0.6rem 0' }}>Phoenix, Arizona, USA</td>
              </tr>
              <tr style={{ borderBottom: '1px solid #1f1f2a' }}>
                <td style={{ padding: '0.6rem 0.8rem 0.6rem 0', color: '#7a7a8c' }}>First Title</td>
                <td style={{ padding: '0.6rem 0' }}>Labyrinth of Apocalypse · alpha v0.1.0 · 2026-05-01</td>
              </tr>
              <tr style={{ borderBottom: '1px solid #1f1f2a' }}>
                <td style={{ padding: '0.6rem 0.8rem 0.6rem 0', color: '#7a7a8c' }}>Platform</td>
                <td style={{ padding: '0.6rem 0' }}>Windows-x64 (alpha) · Linux + macOS-arm64 in roadmap</td>
              </tr>
              <tr style={{ borderBottom: '1px solid #1f1f2a' }}>
                <td style={{ padding: '0.6rem 0.8rem 0.6rem 0', color: '#7a7a8c' }}>Distribution</td>
                <td style={{ padding: '0.6rem 0' }}>Self-hosted from apocky.com · DRM-free · sovereign</td>
              </tr>
              <tr style={{ borderBottom: '1px solid #1f1f2a' }}>
                <td style={{ padding: '0.6rem 0.8rem 0.6rem 0', color: '#7a7a8c' }}>Engine</td>
                <td style={{ padding: '0.6rem 0' }}>CSSL · proprietary substrate-native compiler stack</td>
              </tr>
              <tr>
                <td style={{ padding: '0.6rem 0.8rem 0.6rem 0', color: '#7a7a8c' }}>Press Contact</td>
                <td style={{ padding: '0.6rem 0' }}>
                  <a href="mailto:apocky13@gmail.com?subject=%5Bpress%5D" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>
                    apocky13@gmail.com
                  </a> · subject <code>[press]</code>
                </td>
              </tr>
            </tbody>
          </table>
        </section>

        {/* ── SOCIALS ── */}
        <section style={{ marginTop: '2.5rem' }}>
          <h2
            style={{
              fontSize: '0.7rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: '#7a7a8c',
              marginBottom: '0.8rem',
            }}
          >
            § Social Channels
          </h2>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: '0.6rem' }}>
            {[
              { label: 'medium @noneisone.oneisall', href: 'https://medium.com/@noneisone.oneisall' },
              { label: 'ko-fi/oneinfinity', href: 'https://ko-fi.com/oneinfinity' },
              { label: 'patreon/0ne1nfinity', href: 'https://www.patreon.com/0ne1nfinity' },
              { label: 'github/Apocky', href: 'https://github.com/Apocky' },
            ].map((s) => (
              <a
                key={s.href}
                href={s.href}
                target="_blank"
                rel="noopener noreferrer"
                style={{
                  padding: '0.5rem 0.85rem',
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

        <footer
          style={{
            marginTop: '4rem',
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

export default Press;
