// apocky.com/legal/privacy · Privacy Policy
// v0.1.0-alpha · pre-Termly · sovereignty-respecting · GDPR/CCPA/COPPA-aware
// Replaces with Termly-generated when Apocky completes Termly setup

import type { NextPage } from 'next';
import Head from 'next/head';

const Privacy: NextPage = () => {
  return (
    <>
      <Head>
        <title>Privacy Policy · Apocky</title>
        <meta name="description" content="apocky.com Privacy Policy · sovereignty-respecting · GDPR/CCPA/COPPA-aware · ¬ surveillance · ¬ data-sale" />
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
          Privacy Policy
        </h1>
        <p style={{ color: '#7a7a8c', fontSize: '0.85rem', marginTop: 0 }}>
          v0.1.0-alpha · effective 2026-05-01 · this draft applies until Termly-generated successor lands
        </p>

        <p>
          Apocky ("<strong>Apocky</strong>", "<strong>we</strong>") operates apocky.com and its sub-products including the Labyrinth of
          Apocalypse alpha. This Privacy Policy describes what we collect, why, and how you control it. Sovereignty
          is a design axiom — when policy and convenience conflict, sovereignty wins.
        </p>

        <h2>§ Plain-language summary</h2>
        <ul>
          <li><strong>No surveillance.</strong> No third-party trackers · no advertising SDKs · no analytics that follow you cross-site.</li>
          <li><strong>No data sale.</strong> We do not sell your data. We do not rent your data. Ever. This is non-negotiable.</li>
          <li><strong>No password storage.</strong> Sign-in is magic-link or OAuth · we never see passwords.</li>
          <li><strong>Local-first.</strong> Game state lives on your machine. Cross-user features are opt-in per-event.</li>
          <li><strong>Sovereign-cap revoke.</strong> Any cap-grant is unilaterally revocable by you · always · immediately.</li>
          <li><strong>Sensitive data structurally banned.</strong> Biometric · gaze · face · body data is refused at compile-time. We physically cannot transmit it.</li>
        </ul>

        <h2>§ What we collect</h2>
        <p>The categories of data we collect when you use apocky.com or its products :</p>
        <ol>
          <li>
            <strong>Account identity</strong> · email address (for magic-link), OAuth-provider identifier (when you sign in with
            Google/Apple/GitHub/Discord), display name (if set), account creation timestamp.
          </li>
          <li>
            <strong>Purchase records</strong> (when paid tiers launch) · Stripe customer-ID, transaction IDs, refund history.
            Card numbers and CVV are <em>never</em> stored by us — Stripe handles all PCI-sensitive data.
          </li>
          <li>
            <strong>Opt-in event-stream</strong> · only events you explicitly opt-in to share via the Σ-Chain (Bazaar listings,
            Akashic-imprints, multiplayer-rumors, etc). All Ed25519-signed by your sovereign-keypair.
          </li>
          <li>
            <strong>Operational telemetry</strong> · server-side audit logs (timestamp · endpoint · response code · idempotency-key)
            scrubbed of personal content. Required for security · debugging · regulatory compliance (auto-renewal
            notices, etc.).
          </li>
        </ol>

        <h2>§ What we DO NOT collect</h2>
        <ul>
          <li>Your passwords (we never see them)</li>
          <li>Your IP address as a tracking identifier (Vercel server logs retain for short windows only)</li>
          <li>Cross-site browsing behavior</li>
          <li>Biometric · gaze · face · body data (structurally banned in code)</li>
          <li>Microphone audio without explicit opt-in (and even then, only transcript text egresses if you grant the cap)</li>
          <li>Webcam frames (never)</li>
          <li>Local game-state files (replays · screenshots · Home-dimension · etc) unless you explicitly upload</li>
          <li>Friend graphs or contact lists from other services</li>
        </ul>

        <h2>§ Why we collect what we collect</h2>
        <ul>
          <li><strong>Account identity</strong> — to let you sign in across multiple devices, deliver entitlements you've purchased, and remember your preferences.</li>
          <li><strong>Purchase records</strong> — for tax remittance (Stripe Tax · jurisdictional VAT/GST/sales-tax), refund processing within the 14-day window, and customer support.</li>
          <li><strong>Opt-in events</strong> — to power the Bazaar, Akashic-Records, multiplayer features <em>only when you opt-in</em>.</li>
          <li><strong>Operational telemetry</strong> — to detect abuse, debug bugs, and meet legal obligations (auto-renewal notices in California, GDPR Article 30 records-of-processing, etc.).</li>
        </ul>

        <h2>§ Your rights · sovereignty in practice</h2>
        <p>You have these rights at all times, with no friction or fee :</p>
        <ul>
          <li><strong>Access</strong> — view all data we hold about you via the <a href="/transparency">/transparency</a> dashboard.</li>
          <li><strong>Export</strong> — download a complete archive of your data in machine-readable JSON · GDPR Article 20 (right to data portability).</li>
          <li><strong>Correct</strong> — fix any inaccurate data via your <a href="/account">/account</a> page.</li>
          <li><strong>Delete</strong> — initiate account deletion with a 30-day grace period. After grace, account-private data is permanently deleted. Public Akashic-imprints (if you opted-in) are <em>anonymized</em>, not deleted, to preserve substrate continuity.</li>
          <li><strong>Restrict</strong> — revoke any opt-in cap unilaterally · effective immediately · no waiting period.</li>
          <li><strong>Object</strong> — opt out of any cross-user feature · opt-out persists · re-opt-in requires explicit action.</li>
          <li><strong>Portability</strong> — your sovereign-keypair stays on your device. No platform lock-in via identity.</li>
        </ul>

        <h2>§ Children · COPPA</h2>
        <p>
          apocky.com is not directed to children under 13. We do not knowingly collect personal data from children
          under 13. If you become aware that a child has provided us personal data without parental consent, contact us
          and we will delete it. Some paid features (gacha, age-restricted multiplayer modes) require 18+ and are
          age-gated at checkout.
        </p>

        <h2>§ Cookies · local storage</h2>
        <p>We use the minimum necessary :</p>
        <ul>
          <li><code>sb-access-token</code> · authentication session · HttpOnly · Secure · SameSite=Lax · expires on sign-out</li>
          <li><code>sb-refresh-token</code> · session refresh · same flags</li>
          <li><code>apocky-profile-links</code> · localStorage · your social-channel handles · client-side only · purgeable from /account</li>
        </ul>
        <p>
          We do not use third-party tracking cookies. We do not use Google Analytics, Facebook Pixel, or similar.
          We use Vercel's first-party server-side analytics for traffic counts (no cross-site identifiers).
        </p>

        <h2>§ Data residency · transfers</h2>
        <p>
          Server-side data lives in Vercel (region <code>iad1</code> — US East / North Virginia) and Supabase
          (Americas region per your existing project setup). EU residents : your data crosses the Atlantic when you
          interact with apocky.com. We rely on Standard Contractual Clauses (SCCs) for this transfer per Article 46
          GDPR. We are working toward EU-region Supabase replication for v1.0 launch.
        </p>

        <h2>§ Security</h2>
        <ul>
          <li>TLS 1.3 for all over-the-wire traffic</li>
          <li>BLAKE3 + Ed25519 for application-layer integrity (per Σ-Chain spec/14)</li>
          <li>Idempotency keys on all mutating requests · prevents replay attacks</li>
          <li>Schema validation server-side before any DB write</li>
          <li>RLS (Row-Level Security) on all Supabase tables · default-deny · per-player-scoped</li>
          <li>Stripe-Atlas C-Corp post-incorporation · SOC-2 Vercel + Supabase infrastructure</li>
          <li>No DRM · no rootkit · no kernel-driver · no anti-cheat-spyware on the game client</li>
        </ul>

        <h2>§ Subscriptions · auto-renewal</h2>
        <p>
          When paid tiers launch (v1.0), some products may auto-renew on a 90-day cycle (battle-pass, etc.). We comply
          with California Business and Professions Code § 17602(b) :
        </p>
        <ul>
          <li>If you accepted a free trial lasting more than 31 days · we send a notice 3-21 days before the trial converts to paid.</li>
          <li>If your subscription has an initial term of one year or longer · we send a notice 15-45 days before renewal.</li>
          <li>You can cancel any subscription at any time from <a href="/account">/account</a> · cancellation is effective at end of current billing cycle · no refund of prepaid period unless within 14-day window.</li>
        </ul>

        <h2>§ Changes to this policy</h2>
        <p>
          When we make material changes, we'll notify you via email (if you have an account) at least 30 days before
          the change takes effect. The current version is identified at the top of this page. Past versions are
          archived on github.com/Apocky/CSSL3 and viewable in version history.
        </p>

        <h2>§ Contact · complaints</h2>
        <p>
          Privacy questions, data-subject-requests, or concerns :{' '}
          <a href="mailto:apocky13@gmail.com?subject=%5Bprivacy%5D">apocky13@gmail.com</a> with subject{' '}
          <code>[privacy]</code>. We aim to respond within 30 days (GDPR mandate).
        </p>
        <p>
          If you're an EU resident and unsatisfied with our response, you have the right to complain to your local
          Data Protection Authority. UK : ICO. California residents : California AG's office under CCPA.
        </p>

        <h2>§ Versioning</h2>
        <p>
          This is the alpha-tier privacy policy. When Apocky completes Termly setup (post-Stripe-Atlas incorporation),
          a Termly-generated jurisdictional-auto-update successor will replace this document. The substantive
          principles above will not weaken — only the legal-language precision will sharpen.
        </p>

        <footer style={{ marginTop: '4rem', paddingTop: '2rem', borderTop: '1px solid #1f1f2a', color: '#5a5a6a', fontSize: '0.78rem' }}>
          <p style={{ margin: 0 }}>§ ¬ surveillance · ¬ data-sale · sovereignty-preserved · t∞</p>
          <p style={{ margin: '0.4rem 0 0' }}>
            See also : <a href="/legal/terms">Terms of Service</a> · <a href="/legal/eula">EULA (game binary)</a> · <a href="/transparency">/transparency dashboard</a>
          </p>
        </footer>
      </main>
    </>
  );
};

export default Privacy;
