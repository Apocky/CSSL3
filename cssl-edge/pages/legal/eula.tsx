// apocky.com/legal/eula · Game-binary EULA
// Mirror of dist/LICENSE.md · web-readable form

import type { NextPage } from 'next';
import Head from 'next/head';

const EULA: NextPage = () => {
  return (
    <>
      <Head>
        <title>EULA · Labyrinth of Apocalypse · Apocky</title>
        <meta name="description" content="alpha-tester EULA · governs use of LoA.exe and proprietary substrate binaries" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <style>{`
          * { box-sizing: border-box; }
          html, body { margin: 0; padding: 0; }
          body {
            background: radial-gradient(ellipse at top, #15151f 0%, #0a0a0f 50%, #050507 100%);
            color: #e6e6f0;
            font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
            min-height: 100vh;
          }
          a { color: #7dd3fc; }
          h1 { font-size: clamp(1.75rem, 4vw, 2.25rem); margin: 0 0 1rem; font-weight: 700; }
          h2 { font-size: 1rem; text-transform: uppercase; letter-spacing: 0.15em; color: #a78bfa; margin: 2rem 0 0.6rem; }
          p, ul, ol { color: #cdd6e4; line-height: 1.65; font-size: 0.92rem; }
          ul, ol { padding-left: 1.4rem; }
          li { margin: 0.3rem 0; }
          code { background: rgba(20,20,30,0.7); padding: 0.1rem 0.3rem; border-radius: 3px; color: #fbbf24; font-size: 0.85em; }
        `}</style>
      </Head>
      <main style={{ maxWidth: 760, margin: '0 auto', padding: '4rem 1.5rem 6rem' }}>
        <a href="/" style={{ fontSize: '0.85rem', color: '#7a7a8c', textDecoration: 'none', display: 'inline-block', marginBottom: '2rem' }}>
          ← apocky.com
        </a>

        <h1
          style={{
            backgroundImage: 'linear-gradient(135deg, #ffffff 0%, #c084fc 60%, #7dd3fc 100%)',
            WebkitBackgroundClip: 'text',
            WebkitTextFillColor: 'transparent',
          }}
        >
          End-User License Agreement
        </h1>
        <p style={{ color: '#7a7a8c', fontSize: '0.85rem', marginTop: 0 }}>
          Labyrinth of Apocalypse · alpha-tester · v0.1.0-alpha · effective 2026-05-01
        </p>

        <p>
          This EULA governs your use of the <strong>Labyrinth of Apocalypse alpha binary</strong> (<code>LoA.exe</code>) and
          related proprietary substrate components distributed in this ZIP. By extracting, running, or otherwise
          using the binary, you accept these terms.
        </p>

        <h2>§ 1 · Grant of License</h2>
        <p>You are granted a non-exclusive, non-transferable, revocable license to :</p>
        <ul>
          <li>Install · run · benchmark · screenshot · record · stream · publish reviews of this alpha build</li>
          <li>Use the binary on any number of personal devices you own or control</li>
          <li>Provide feedback to Apocky</li>
        </ul>

        <h2>§ 2 · Restrictions</h2>
        <p>You may NOT :</p>
        <ul>
          <li>Redistribute the binary or any contents to third parties</li>
          <li>Reverse-engineer · decompile · or extract proprietary substrate code · KAN weights · render-pipeline shaders · 6-novelty-path implementations</li>
          <li>Use any extracted code or models in derivative works without separate written permission</li>
          <li>Resell · sublicense · or commercially exploit this alpha</li>
        </ul>

        <h2>§ 3 · Open-Source vs Proprietary</h2>
        <ul>
          <li><strong>Open-source (MIT/Apache-2.0)</strong> · CSSL language spec · csslc compiler · MIR · parser · lexer · cgen · spec/grand-vision/*.csl design docs · github.com/Apocky/CSSL3</li>
          <li><strong>Proprietary (this EULA)</strong> · LoA.exe game binary · KAN-substrate trained weights · render-pipeline shaders compiled-form · Coherence-Engine eval · Mycelial-Network impl · Σ-Chain consensus impl · 6-novelty-path implementations</li>
        </ul>

        <h2>§ 4 · Sovereignty · Privacy · Telemetry</h2>
        <p>This software is sovereign-respecting by design :</p>
        <ul>
          <li>No DRM · no rootkit · no kernel-driver · no anti-cheat-spyware</li>
          <li>All player state local-by-default (<code>%APPDATA%/LoA/</code> on Windows · OS-keychain for keypair)</li>
          <li>Cross-user sharing is opt-in per-event-grain · NEVER all-or-nothing</li>
          <li>Sensitive data (biometric · gaze · face · body) structurally banned at compile-time</li>
        </ul>

        <h2>§ 5 · Disclaimer of Warranty</h2>
        <p>
          THIS ALPHA IS PROVIDED "AS IS" WITHOUT WARRANTY OF ANY KIND. ALPHA SOFTWARE IS UNFINISHED AND MAY CONTAIN
          BUGS · INCLUDING DATA-LOSS BUGS. DO NOT USE THIS BINARY ON PRODUCTION SYSTEMS OR FOR ANY PURPOSE WHERE
          FAILURE WOULD CAUSE HARM.
        </p>

        <h2>§ 6 · Limitation of Liability</h2>
        <p>
          TO THE MAXIMUM EXTENT PERMITTED BY APPLICABLE LAW · APOCKY'S TOTAL LIABILITY UNDER THIS AGREEMENT SHALL NOT
          EXCEED THE GREATER OF (a) ONE HUNDRED USD ($100) OR (b) THE AMOUNT YOU PAID FOR THIS ALPHA (FREE = $0).
        </p>

        <h2>§ 7 · Feedback License</h2>
        <p>
          Feedback you provide grants Apocky a perpetual, royalty-free, worldwide, non-exclusive license to use it in
          development of LoA and future Apocky-projects, with or without attribution.
        </p>

        <h2>§ 8 · Refunds (when paid tiers launch)</h2>
        <ul>
          <li>14-day-no-questions-asked refund window via Stripe</li>
          <li>Jurisdictional rights respected (EU 14-day · UK 14-day · CN 7-day)</li>
          <li>In-game content prorata-refundable within window</li>
          <li>Earned-currency (Stabilized-Essence) NOT refundable</li>
        </ul>

        <h2>§ 9 · Termination</h2>
        <p>
          This license terminates automatically if you violate any terms. Upon termination, you must delete all
          copies. Sections 5 · 6 · 7 · 10 survive termination.
        </p>

        <h2>§ 10 · Governing Law</h2>
        <p>
          Governed by Arizona, USA law (without regard to conflict-of-laws). Disputes resolved in Maricopa County,
          Arizona courts.
        </p>

        <h2>§ 11 · Contact</h2>
        <p>
          Questions : <a href="mailto:apocky13@gmail.com?subject=%5BLoA-LICENSE%5D">apocky13@gmail.com</a> with{' '}
          <code>[LoA-LICENSE]</code> subject.
        </p>

        <footer style={{ marginTop: '4rem', paddingTop: '2rem', borderTop: '1px solid #1f1f2a', color: '#5a5a6a', fontSize: '0.78rem' }}>
          <p style={{ margin: 0 }}>§ ¬ harm in the making · sovereignty preserved · ¬ pay-for-power · t∞</p>
          <p style={{ margin: '0.4rem 0 0' }}>
            See also : <a href="/legal/privacy">Privacy Policy</a> · <a href="/legal/terms">Terms of Service</a>
          </p>
        </footer>
      </main>
    </>
  );
};

export default EULA;
