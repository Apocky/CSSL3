// § Akashic-Webpage-Records · AkashicConsent.tsx
// First-visit default-deny choice + persistent, sovereign-revocable control.

import * as React from 'react';
import { useRouter } from 'next/router';
import {
  CONSENT_CHANGE_EVENT,
  isTelemetryBlackoutPath,
  storedConsentTier,
  withConsent,
} from '@/lib/akashic-telemetry';
import type { ConsentTier } from '@/lib/akashic-telemetry';

const NON_BLOCKING_APP_PATHS = ['/admin', '/clearing'];

interface TierOpt {
  tier: ConsentTier;
  title: string;
  short: string;
  detail: string;
}

const TIERS: TierOpt[] = [
  {
    tier: 'none',
    title: 'Off',
    short: 'Share nothing',
    detail:
      'The site stores only the fact that you chose Off in this browser. It does not start the optional reporting system or send diagnostic events. The site still works.',
  },
  {
    tier: 'spore',
    title: 'Basic',
    short: 'Page and speed problems',
    detail:
      'Shares the page address, the previous page address, browser-window size, page-speed measurements, missing files, network failures, and basic error details with apocky.com. Some records may be stored. The site obscures common patterns that look like email addresses, access tokens, or long numbers before sending, but that filter may miss sensitive text.',
  },
  {
    tier: 'mycelium',
    title: 'Error details',
    short: 'More information about failures',
    detail:
      'Includes Basic sharing plus extra application-error details, shortened error messages, and the list of code locations involved. These details may contain sensitive text even after filtering.',
  },
  {
    tier: 'akashic',
    title: 'Full diagnostics',
    short: 'Browser errors and named site steps',
    detail:
      'Includes Error details plus messages written to the browser error console and named steps in site flows. These messages are filtered for common sensitive patterns but can still contain sensitive text.',
  },
];

function tierTitle(tier: ConsentTier | null): string {
  return TIERS.find((option) => option.tier === tier)?.title ?? 'Off';
}

export function AkashicConsent(): React.ReactElement | null {
  const router = useRouter();
  const [open, setOpen] = React.useState(false);
  const [chosen, setChosen] = React.useState<ConsentTier>('none');
  const [saved, setSaved] = React.useState<ConsentTier | null>(null);
  const [saveError, setSaveError] = React.useState('');
  const [tierDetailOpen, setTierDetailOpen] = React.useState(false);
  const openerRef = React.useRef<HTMLButtonElement>(null);
  const dialogRef = React.useRef<HTMLDivElement>(null);
  const blackout = isTelemetryBlackoutPath(router.pathname);
  const compactSurface = NON_BLOCKING_APP_PATHS.some((prefix) =>
    router.pathname === prefix || router.pathname.startsWith(`${prefix}/`),
  );
  const selectedTier = TIERS.find((option) => option.tier === chosen) ?? TIERS[0];

  React.useEffect(() => {
    const sync = (): void => {
      const stored = storedConsentTier();
      setSaved(stored);
      setChosen(stored ?? 'none');
      setOpen((wasOpen) => {
        if (blackout || compactSurface) return false;
        // Optional reporting is default-off. Keep its explicit opener visible,
        // but never cover the public experience before a visitor asks to see it.
        return wasOpen;
      });
    };
    sync();
    window.addEventListener(CONSENT_CHANGE_EVENT, sync);
    return () => window.removeEventListener(CONSENT_CHANGE_EVENT, sync);
  }, [blackout, compactSurface]);

  React.useEffect(() => {
    if (open) dialogRef.current?.focus();
  }, [open]);

  React.useEffect(() => {
    setTierDetailOpen(chosen !== 'none');
  }, [chosen]);

  const closePanel = React.useCallback((): void => {
    setOpen(false);
    window.setTimeout(() => openerRef.current?.focus(), 0);
  }, []);

  const handleGrant = React.useCallback(
    (tier: ConsentTier): void => {
      const persisted = withConsent(tier);
      const actual = storedConsentTier();
      setSaved(actual);
      setChosen(actual ?? 'none');
      if (!persisted || actual !== tier) {
        setSaveError(
          tier === 'none'
            ? 'Optional reporting is off in this tab, but the browser could not remember that choice. Check this control again after reloading.'
            : 'The browser could not save that choice. Your previous setting remains in effect.',
        );
        setOpen(true);
        return;
      }
      setSaveError('');
      closePanel();
    },
    [closePanel]
  );

  // Immersive rooms and telemetry-blackout surfaces must not receive a fixed
  // diagnostics opener over their primary controls. Keep this return after every
  // hook so client-side navigation cannot change hook order.
  if (blackout || compactSurface) return null;

  if (!open) {
    const label = blackout
      ? saved === null
        ? 'Optional data sharing is off on this page'
        : `Optional data sharing is off here. Saved choice: ${tierTitle(saved)}`
      : saved === null
        ? 'Optional data sharing: off'
        : `Optional data sharing: ${tierTitle(saved)}`;
    return (
      <button
        className="apx-diagnostics-opener"
        ref={openerRef}
        type="button"
        onClick={() => setOpen(true)}
        aria-haspopup="dialog"
        aria-label={`${label}. Open optional data-sharing choices.`}
        style={{
          position: 'fixed',
          right: '0.75rem',
          bottom: '0.75rem',
          zIndex: 2_147_483_646,
          minHeight: '44px',
          padding: '0.5rem 0.8rem',
          borderRadius: '999px',
          border: '1px solid rgba(120, 231, 255, 0.3)',
          backgroundColor: 'rgba(4, 6, 20, 0.96)',
          color: '#e7e9ff',
          cursor: 'pointer',
          font: '600 0.72rem system-ui, sans-serif',
        }}
      >
        <span className="apx-diagnostics-opener-label">{label}</span>
      </button>
    );
  }

  return (
    <div
      ref={dialogRef}
      role="dialog"
      aria-modal="false"
      aria-labelledby="akashic-consent-title"
      aria-describedby="akashic-consent-description"
      tabIndex={-1}
      onKeyDown={(event) => {
        if (event.key === 'Escape') closePanel();
      }}
      style={{
        position: 'fixed',
        right: '0.5rem',
        bottom: '3.5rem',
        zIndex: 2_147_483_646,
        width: 'min(28rem, calc(100vw - 1rem))',
        maxWidth: 'calc(100vw - 1rem)',
        fontFamily: 'system-ui, sans-serif',
        color: '#e7e9ff',
      }}
    >
      <div
        style={{
          backgroundColor: '#050719',
          border: '1px solid #293660',
          borderRadius: '0.65rem',
          width: '100%',
          maxHeight: 'min(32rem, calc(100vh - 4rem), calc(100dvh - 4rem))',
          overflow: 'hidden',
          display: 'flex',
          flexDirection: 'column',
          boxSizing: 'border-box',
          boxShadow: '0 18px 48px rgba(0,0,0,0.45)',
        }}
      >
        <div
          style={{
            minHeight: 0,
            overflowY: 'auto',
            overscrollBehavior: 'contain',
            padding: '0.85rem 0.85rem 0.7rem',
          }}
        >
          <h2
            id="akashic-consent-title"
            style={{
              margin: 0,
              fontSize: '1rem',
              fontWeight: 650,
              letterSpacing: '0.01em',
            }}
          >
            Optional site data
          </h2>
          <p
            id="akashic-consent-description"
            style={{ opacity: 0.82, lineHeight: 1.4, fontSize: '0.76rem', margin: '0.4rem 0 0' }}
          >
            The site works with this off. Nothing from this optional reporting
            system is sent until you choose a level and save it.
          </p>
          {blackout && (
            <p role="status" style={{ lineHeight: 1.4, fontSize: '0.72rem', color: '#78e7ff', margin: '0.5rem 0 0' }}>
              This page never uses optional reporting. A saved choice applies
              only after you leave this page.
            </p>
          )}
          {saveError !== '' && (
            <p role="alert" style={{ lineHeight: 1.4, fontSize: '0.72rem', color: '#e6aaa0', margin: '0.5rem 0 0' }}>
              {saveError}
            </p>
          )}

          <div
            role="radiogroup"
            aria-label="How much optional site data to share"
            style={{
              display: 'grid',
              gridTemplateColumns: 'repeat(2, minmax(0, 1fr))',
              gap: '0.45rem',
              marginTop: '0.7rem',
            }}
          >
            {TIERS.map((opt) => (
              <button
                type="button"
                role="radio"
                aria-checked={chosen === opt.tier}
                key={opt.tier}
                onClick={() => setChosen(opt.tier)}
                style={{
                  minHeight: '52px',
                  textAlign: 'left',
                  padding: '0.5rem 0.65rem',
                  backgroundColor: chosen === opt.tier ? '#11183c' : '#060817',
                  border: chosen === opt.tier ? '2px solid #78e7ff' : '1px solid #293660',
                  borderRadius: '0.45rem',
                  color: '#e7e9ff',
                  cursor: 'pointer',
                  display: 'flex',
                  flexDirection: 'column',
                  justifyContent: 'center',
                  gap: '0.1rem',
                  fontFamily: 'inherit',
                }}
              >
                <span style={{ fontWeight: 650, fontSize: '0.8rem' }}>
                  {opt.title}
                </span>
                <span style={{ fontSize: '0.68rem', opacity: 0.76 }}>
                  {opt.short}
                </span>
              </button>
            ))}
          </div>

          <details
            key={selectedTier?.tier}
            open={tierDetailOpen}
            onToggle={(event) => setTierDetailOpen(event.currentTarget.open)}
            style={{ marginTop: '0.55rem', borderTop: '1px solid #293660' }}
          >
            <summary
              style={{
                minHeight: '44px',
                display: 'flex',
                alignItems: 'center',
                cursor: 'pointer',
                fontSize: '0.73rem',
                fontWeight: 600,
              }}
            >
              What is shared
            </summary>
            <p style={{ fontSize: '0.7rem', lineHeight: 1.45, opacity: 0.78, margin: '0 0 0.65rem' }}>
              {selectedTier?.detail}
            </p>
          </details>

          <details style={{ borderTop: '1px solid #293660' }}>
            <summary
              style={{
                minHeight: '44px',
                display: 'flex',
                alignItems: 'center',
                cursor: 'pointer',
                fontSize: '0.73rem',
                fontWeight: 600,
              }}
            >
              How this browser remembers your choice
            </summary>
            <p style={{ fontSize: '0.68rem', lineHeight: 1.45, opacity: 0.72, margin: '0 0 0.35rem' }}>
              The choice is saved in this browser. Any choice other than Off
              creates a random identifier for this browser tab and sends the
              selected reports to apocky.com. Individual reports may be stored.
              A text filter hides some common patterns that resemble secrets,
              but it cannot guarantee that every piece of sensitive text is
              removed.
            </p>
          </details>
        </div>

        <div
          style={{
            flex: '0 0 auto',
            display: 'flex',
            gap: '0.5rem',
            flexWrap: 'wrap',
            padding: '0.7rem 0.85rem',
            borderTop: '1px solid #293660',
            backgroundColor: '#050719',
          }}
        >
          <button
            type="button"
            onClick={() => handleGrant(chosen)}
            style={{
              minHeight: '44px',
              padding: '0.55rem 0.85rem',
              background: 'linear-gradient(135deg, #78e7ff, #a78bfa)',
              color: '#03040d',
              border: 'none',
              borderRadius: '0.4rem',
              cursor: 'pointer',
              fontWeight: 600,
              fontSize: '0.82rem',
              flex: '1 1 10rem',
              minWidth: '10rem',
            }}
          >
            Save choice: {selectedTier?.title}
          </button>
          <button
            type="button"
            onClick={() => saved !== null && saved !== 'none' ? handleGrant('none') : closePanel()}
            style={{
              minHeight: '44px',
              padding: '0.55rem 0.85rem',
              backgroundColor: 'transparent',
              color: '#e7e9ff',
              border: '1px solid #5265a7',
              borderRadius: '0.4rem',
              cursor: 'pointer',
              fontSize: '0.82rem',
              flex: '1 1 auto',
            }}
          >
            {saved !== null && saved !== 'none' ? 'Turn off now' : saved === null ? 'Not now' : 'Close'}
          </button>
        </div>
      </div>
    </div>
  );
}

export default AkashicConsent;
