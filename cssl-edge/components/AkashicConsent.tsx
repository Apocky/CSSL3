// § Akashic-Webpage-Records · AkashicConsent.tsx
// First-visit overlay · sovereign-revocable cap-grant. Phone-first responsive.
// Three tiers : Spore / Mycelium / Akashic + None toggle. Sovereignty-respecting
// copy ; NO dark-pattern. User picks · user changes · user revokes.
//
// Renders only when localStorage has no prior choice. After grant, sets the
// stored tier and dismisses. /admin/telemetry surfaces the same controls
// post-first-visit.

import * as React from 'react';
import { withConsent, currentTier } from '@/lib/akashic-telemetry';
import type { ConsentTier } from '@/lib/akashic-telemetry';

const STORAGE_KEY = 'akashic.consent.shown.v1';

interface TierOpt {
  tier: ConsentTier;
  glyph: string;
  title: string;
  short: string;
  detail: string;
}

const TIERS: TierOpt[] = [
  {
    tier: 'none',
    glyph: '',
    title: 'silence',
    short: 'no Records · no telemetry',
    detail:
      'Nothing leaves your browser. The site keeps working ; we just lose the signal that helps us see when something breaks.',
  },
  {
    tier: 'spore',
    glyph: 'spore',
    title: 'Spore',
    short: 'aggregate counts only · k-anon ≥ 10',
    detail:
      'Page-views + Web Vitals + error-counts. NO stack traces. NO content. Server requires k≥10 users before any pattern is retained. Ephemeral session-id ; never your identity.',
  },
  {
    tier: 'mycelium',
    glyph: 'mycelium',
    title: 'Mycelium',
    short: '+ stack traces · k-anon ≥ 5',
    detail:
      'Everything in Spore + redacted stack-traces when errors happen. Cluster-signatures help us heal recurring bugs faster. k≥5 still required.',
  },
  {
    tier: 'akashic',
    glyph: 'akashic',
    title: 'Akashic',
    short: 'full-fidelity · always-purgeable',
    detail:
      'Everything in Mycelium + console.error/.warn + per-page navigation breadcrumbs. Highest signal · longest reach. You can purge all your events any time from /admin/telemetry.',
  },
];

export function AkashicConsent(): React.ReactElement | null {
  const [open, setOpen] = React.useState(false);
  const [chosen, setChosen] = React.useState<ConsentTier>('spore');

  React.useEffect(() => {
    try {
      if (typeof localStorage === 'undefined') return;
      const shown = localStorage.getItem(STORAGE_KEY);
      if (shown !== '1') {
        setOpen(true);
        setChosen(currentTier());
      }
    } catch {
      // storage unavailable · skip overlay (privacy-mode)
    }
  }, []);

  const handleGrant = React.useCallback(
    (tier: ConsentTier): void => {
      withConsent(tier);
      try {
        if (typeof localStorage !== 'undefined') {
          localStorage.setItem(STORAGE_KEY, '1');
        }
      } catch {
        // ignore
      }
      setOpen(false);
    },
    []
  );

  if (!open) return null;

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="akashic-consent-title"
      style={{
        position: 'fixed',
        inset: 0,
        backgroundColor: 'rgba(10,10,15,0.92)',
        zIndex: 999_999,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        padding: '1rem',
        fontFamily: 'system-ui, sans-serif',
        color: '#e6e6f0',
      }}
    >
      <div
        style={{
          backgroundColor: '#15151f',
          border: '1px solid #303040',
          borderRadius: '0.75rem',
          padding: '1.5rem',
          maxWidth: '40rem',
          width: '100%',
          maxHeight: '90vh',
          overflowY: 'auto',
        }}
      >
        <h2
          id="akashic-consent-title"
          style={{
            marginTop: 0,
            fontSize: '1.4rem',
            fontWeight: 600,
            letterSpacing: '0.01em',
          }}
        >
          The Akashic Records remember your session
        </h2>
        <p style={{ opacity: 0.85, lineHeight: 1.5, fontSize: '0.95rem' }}>
          apocky.com runs a substrate-native diagnostic layer · every page-event
          becomes a cell in the ω-field. Cells are Σ-mask-gated for sovereignty
          · k-anonymous before any pattern is retained · purgeable at any time
          from <code style={{ background: '#0a0a0f', padding: '0.1em 0.3em' }}>/admin/telemetry</code>.
          Pick what you want to share. You can change or revoke any time.
        </p>

        <div style={{ display: 'flex', flexDirection: 'column', gap: '0.5rem', marginTop: '1rem' }}>
          {TIERS.map((opt) => (
            <button
              key={opt.tier}
              onClick={() => setChosen(opt.tier)}
              aria-pressed={chosen === opt.tier}
              style={{
                textAlign: 'left',
                padding: '0.85rem 1rem',
                backgroundColor: chosen === opt.tier ? '#1f1f33' : '#0e0e18',
                border: chosen === opt.tier ? '1px solid #5a4cff' : '1px solid #2a2a3a',
                borderRadius: '0.5rem',
                color: '#e6e6f0',
                cursor: 'pointer',
                display: 'flex',
                gap: '0.75rem',
                alignItems: 'flex-start',
                fontFamily: 'inherit',
              }}
            >
              <span style={{ fontSize: '1rem', marginTop: '0.1rem', color: '#8a7dff', fontWeight: 600 }}>
                {opt.glyph !== '' ? opt.glyph : '·'}
              </span>
              <span style={{ display: 'flex', flexDirection: 'column', gap: '0.2rem' }}>
                <span style={{ fontWeight: 600, fontSize: '1rem' }}>
                  {opt.title}
                </span>
                <span style={{ fontSize: '0.85rem', opacity: 0.85 }}>
                  {opt.short}
                </span>
                <span style={{ fontSize: '0.78rem', opacity: 0.7, marginTop: '0.15rem' }}>
                  {opt.detail}
                </span>
              </span>
            </button>
          ))}
        </div>

        <div
          style={{
            display: 'flex',
            gap: '0.5rem',
            marginTop: '1.25rem',
            flexWrap: 'wrap',
          }}
        >
          <button
            onClick={() => handleGrant(chosen)}
            style={{
              padding: '0.65rem 1.25rem',
              backgroundColor: '#5a4cff',
              color: 'white',
              border: 'none',
              borderRadius: '0.4rem',
              cursor: 'pointer',
              fontWeight: 600,
              fontSize: '0.95rem',
              flex: '1 1 auto',
              minWidth: '10rem',
            }}
          >
            grant {chosen}
          </button>
          <button
            onClick={() => handleGrant('none')}
            style={{
              padding: '0.65rem 1.25rem',
              backgroundColor: 'transparent',
              color: '#e6e6f0',
              border: '1px solid #404050',
              borderRadius: '0.4rem',
              cursor: 'pointer',
              fontSize: '0.95rem',
              flex: '0 0 auto',
            }}
          >
            silence
          </button>
        </div>

        <p
          style={{
            fontSize: '0.75rem',
            opacity: 0.55,
            marginTop: '1rem',
            marginBottom: 0,
            lineHeight: 1.5,
          }}
        >
          No third-party tracking. No advertising IDs. No cookies for telemetry.
          Session-id is ephemeral random · resets on tab-close. Server only
          retains aggregated patterns when k ≥ tier-threshold ; single events
          purgeable on demand.
        </p>
      </div>
    </div>
  );
}

export default AkashicConsent;
