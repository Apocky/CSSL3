// apocky.com/legal/terms · Terms of Service
// v0.1.0-alpha · pre-Termly · replaces with Termly-generated when ready

import type { NextPage } from 'next';
import Head from 'next/head';

const Terms: NextPage = () => {
  return (
    <>
      <Head>
        <title>Terms of Service · Apocky</title>
        <meta name="description" content="apocky.com Terms of Service · alpha-tier · sovereignty-respecting" />
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
          Terms of Service
        </h1>
        <p style={{ color: '#7a7a8c', fontSize: '0.85rem', marginTop: 0 }}>
          v0.1.0-alpha · effective 2026-05-01 · this draft applies until Termly-generated successor lands
        </p>

        <h2>§ Acceptance</h2>
        <p>
          By creating an account on apocky.com or using any apocky-project (LoA · CSSL · DGI · etc.), you accept these
          Terms of Service. If you do not accept, do not create an account and do not use the products.
        </p>

        <h2>§ Eligibility</h2>
        <ul>
          <li>You must be at least 13 years old (COPPA)</li>
          <li>You must be at least 18 to purchase paid tiers, gacha pulls, or age-restricted features</li>
          <li>You are responsible for compliance with your local laws</li>
        </ul>

        <h2>§ Account · authentication</h2>
        <ul>
          <li>One human · one account · accounts are not transferable</li>
          <li>You may link multiple OAuth providers · unlinking always leaves at least one (sovereignty)</li>
          <li>You are responsible for maintaining access to your magic-link email</li>
          <li>We never see your passwords (magic-link or OAuth only)</li>
        </ul>

        <h2>§ Acceptable use</h2>
        <p>You agree NOT to :</p>
        <ul>
          <li>Reverse-engineer, decompile, or extract proprietary substrate code from any binary product</li>
          <li>Use automation to abuse rate limits or skew federated-learning aggregates</li>
          <li>Submit content that is illegal, harassing, or violates others' rights</li>
          <li>Attempt to bypass cap-gating, anti-cheat heuristics, or entitlement-validation</li>
          <li>Use the products to harm other users, the substrate, or third parties</li>
        </ul>
        <p>
          You agree to :
        </p>
        <ul>
          <li>Treat other players with respect (gift-economy axiom · per spec/13)</li>
          <li>Report bugs and security issues responsibly to <a href="mailto:apocky13@gmail.com?subject=%5Bsecurity%5D">apocky13@gmail.com</a></li>
        </ul>

        <h2>§ Subscriptions · auto-renewal · refunds</h2>
        <p>
          Some products (battle-pass, premium-tier) may auto-renew on a 90-day cycle. We comply with California
          Business and Professions Code § 17602(b) :
        </p>
        <ul>
          <li>Pre-renewal notice 15-45 days before annual-or-longer terms renew</li>
          <li>Pre-trial-conversion notice 3-21 days before free-trial converts (when trial &gt; 31 days)</li>
          <li>Cancel anytime from <a href="/account">/account</a> · effective end-of-current-cycle</li>
          <li>14-day no-questions-asked refund · automated via Stripe</li>
          <li>EU 14-day statutory right of withdrawal · UK 14-day · CN 7-day · all auto-honored</li>
          <li>Earned-currency (Stabilized-Essence) is not refundable (it is earned, not purchased)</li>
          <li>In-game cosmetic content consumed within refund window is prorata-refundable</li>
        </ul>

        <h2>§ Content · UGC · Akashic-Records</h2>
        <p>
          When you opt-in to share content (Bazaar listings · Akashic-imprints · Memorial-Wall ascriptions) :
        </p>
        <ul>
          <li>You retain ownership · grant Apocky a non-exclusive license to display the content</li>
          <li>You may delete your content · public-attributed imprints become anonymized after delete (substrate continuity preserved)</li>
          <li>You are responsible for the content you submit · do not submit content you don't have rights to</li>
          <li>Apocky may remove content that violates Acceptable Use without prior notice</li>
        </ul>

        <h2>§ Sovereign-cap · revocability</h2>
        <p>
          Every cap-grant you make is unilaterally revocable. Effective immediately. No waiting period. No exit fee.
          Revoking a cap may degrade some features (you cannot participate in cross-user mycelium without
          MP_CAP_RELAY_DATA, etc.) but the substrate gracefully degrades to stage-0-fallback.
        </p>

        <h2>§ Intellectual property</h2>
        <ul>
          <li>CSSL language · csslc compiler · spec/grand-vision/* · MIT/Apache-2.0 dual-license at github.com/Apocky/CSSL3</li>
          <li>LoA binary · KAN-weights · render-pipeline-internals · Coherence-Engine impl · proprietary · all rights reserved</li>
          <li>Trademarks (filing pending) · "Apocky" · "Labyrinth of Apocalypse" · "CSSL" · "Ouroboroid" · "Σ-Chain" · "Mycelial Substrate"</li>
          <li>Your content remains yours · we license-it-to-display · we don't claim ownership</li>
          <li>Feedback you provide to us grants us a perpetual royalty-free license to use it (per the alpha-tester EULA)</li>
        </ul>

        <h2>§ Disclaimers · liability</h2>
        <p>
          ALPHA SOFTWARE PROVIDED "AS IS". We disclaim implied warranties of merchantability, fitness for a particular
          purpose, and non-infringement. Total liability under these Terms shall not exceed the greater of $100 USD
          or the amount you paid in the prior 12 months.
        </p>

        <h2>§ Termination</h2>
        <p>
          You may terminate your account at any time from <a href="/account">/account</a>. We may terminate or suspend
          your account for material breach of these Terms after notice (except for security-critical actions which
          may be immediate). Sections 6 (Liability) · 7 (Feedback) · 9 (IP) · 10 (Governing law) survive termination.
        </p>

        <h2>§ Governing law · dispute resolution</h2>
        <p>
          These Terms are governed by Arizona, USA law (without regard to conflict-of-laws). Disputes shall be
          resolved in Maricopa County, Arizona courts. EU residents retain mandatory consumer protections under
          local law.
        </p>

        <h2>§ Changes</h2>
        <p>
          Material changes notified to you via email at least 30 days in advance. Continued use after the effective
          date constitutes acceptance. If you reject, you may close your account before the effective date with no
          adverse consequence.
        </p>

        <h2>§ Contact</h2>
        <p>
          Terms questions : <a href="mailto:apocky13@gmail.com?subject=%5Bterms%5D">apocky13@gmail.com</a> with subject{' '}
          <code>[terms]</code>.
        </p>

        <h2>§ Versioning</h2>
        <p>
          This is the alpha-tier Terms. When Apocky completes Termly setup (post-Stripe-Atlas incorporation), a
          Termly-generated jurisdictional-auto-update successor will replace this document.
        </p>

        <footer style={{ marginTop: '4rem', paddingTop: '2rem', borderTop: '1px solid #1f1f2a', color: '#5a5a6a', fontSize: '0.78rem' }}>
          <p style={{ margin: 0 }}>§ sovereignty preserved · gift-economy · t∞</p>
          <p style={{ margin: '0.4rem 0 0' }}>
            See also : <a href="/legal/privacy">Privacy Policy</a> · <a href="/legal/eula">EULA</a>
          </p>
        </footer>
      </main>
    </>
  );
};

export default Terms;
