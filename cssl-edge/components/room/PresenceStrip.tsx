import Link from 'next/link';

import type { PresenceView } from './types';
import styles from './Room.module.css';

function age(iso: string, nowMs: number): string {
  const seconds = Math.max(0, Math.round((nowMs - Date.parse(iso)) / 1000));
  if (seconds < 5) return 'now';
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 48) return `${hours}h ago`;
  return `${Math.round(hours / 24)}d ago`;
}

const BUSY = new Set(['attending', 'thinking', 'recalling', 'speaking', 'remembering']);

export default function PresenceStrip({ presence, disconnected, nowMs, muted, onToggleMute }: {
  readonly presence: PresenceView | null;
  readonly disconnected: boolean;
  readonly nowMs: number;
  readonly muted: boolean;
  readonly onToggleMute: () => void;
}): JSX.Element {
  const state = presence?.state ?? 'unknown';
  const degraded = state.startsWith('degraded');
  const dot = disconnected || degraded
    ? styles.dotBad
    : BUSY.has(state) ? styles.dotBusy : state === 'idle' ? styles.dotLive : '';
  const text = disconnected ? 'DISCONNECTED' : presence === null ? 'no presence yet' : state;
  return <div className={styles.strip} role="status" aria-live="polite">
    <Link href="/" className={styles.brand}>Apocky</Link>
    <span className={styles.state}>
      <span className={`${styles.dot} ${dot}`} aria-hidden="true" />
      <span>Apocrypha: {text}</span>
      {presence && !disconnected ? <span className={styles.age}>{age(presence.at, nowMs)}</span> : null}
    </span>
    <button
      type="button"
      className={muted ? `${styles.toggle} ${styles.toggleOn}` : styles.toggle}
      onClick={onToggleMute}
      aria-pressed={muted}
      title="Hide what Apocrypha says unprompted"
    >
      {muted ? 'Muted' : 'Mute'}
    </button>
  </div>;
}
