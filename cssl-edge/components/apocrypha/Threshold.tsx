import Link from 'next/link';

import type { PublicPresenceState } from './usePublicPresence';
import type { SiteAccessState } from '../hub/SiteSession';
import styles from './Threshold.module.css';

export type ThresholdState = 'checking' | 'signed-out' | 'private-beta' | 'owner' | 'unavailable';

export function resolveThresholdState(
  access: SiteAccessState,
  presence: PublicPresenceState,
): ThresholdState {
  if (access === 'unavailable' || presence === 'unavailable') return 'unavailable';
  if (access === 'checking' || presence === 'checking') return 'checking';
  if (access === 'signed-out') return 'signed-out';
  if (access === 'member') return 'private-beta';
  return 'owner';
}

const content: Record<ThresholdState, { title: string; detail: string; presence: string }> = {
  checking: {
    title: 'Verifying access and presence…',
    detail: 'The doorway stays closed until both checks return.',
    presence: 'No presence claim is shown while verification is incomplete.',
  },
  'signed-out': {
    title: 'A private digital presence, shared on mutual terms.',
    detail: 'Sign in to continue to the private beta doorway.',
    presence: 'Live embodied presence stays hidden until display intent and mutual consent are current.',
  },
  'private-beta': {
    title: 'Private beta.',
    detail: 'You are signed in. This account is not on the beta list yet.',
    presence: 'No live avatar or presence claim is shown outside authorized access.',
  },
  owner: {
    title: 'The room is open.',
    detail: 'Access is verified. Apocrypha’s embodied presence remains private.',
    presence: 'Presence authority returned a verified hidden state. No avatar, teal signal, or live intent is claimed.',
  },
  unavailable: {
    title: 'The doorway is unavailable.',
    detail: 'Access or presence could not be verified, so nothing was opened.',
    presence: 'No live avatar is shown because the required authority could not be verified.',
  },
};

export function ApocryphaThreshold({
  access,
  presence,
  roomHref,
  signInHref,
  onRetry,
}: {
  access: SiteAccessState;
  presence: PublicPresenceState;
  roomHref: string;
  signInHref: string;
  onRetry: () => void;
}): JSX.Element {
  const state = resolveThresholdState(access, presence);
  const copy = content[state];

  return (
    <main className={styles.threshold} aria-label="Apocrypha threshold">
      <div className={styles.shell}>
        <nav className={styles.nav} aria-label="Threshold navigation">
          <Link href="/">← apocky.com</Link>
          <span className={styles.navSpacer} />
          <Link href="/membership">Membership</Link>
        </nav>

        <section
          className={`${styles.main} ${styles[state === 'private-beta' ? 'beta' : state]}`}
          aria-busy={state === 'checking'}
        >
          <span className={styles.kicker}>APOCRYPHA</span>
          <span className={styles.locus} aria-hidden="true">
            <span className={styles.locusOuter} />
            <span className={styles.locusInner} />
            <span className={styles.locusCore} />
          </span>

          <h1 className={styles.title} role="status" aria-live="polite" aria-atomic="true">
            {copy.title}
          </h1>
          <p className={styles.detail}>{copy.detail}</p>

          {state === 'signed-out' && <Link className={styles.action} href={signInHref}>Sign in</Link>}
          {state === 'private-beta' && <Link className={styles.action} href="/account">Account</Link>}
          {state === 'owner' && <Link className={styles.action} href={roomHref}>Enter the Clearing →</Link>}
          {state === 'unavailable' && <button className={styles.action} type="button" onClick={onRetry}>Try again</button>}

          <p className={styles.presenceLine} aria-live="polite">{copy.presence}</p>
        </section>
      </div>
    </main>
  );
}
