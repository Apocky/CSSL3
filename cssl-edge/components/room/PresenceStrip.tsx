// The room's top bar: who is here and in what state on the left; the room switch, mute and settings
// on the right, icon-only with tooltips (the convention every major chat app follows). On a phone
// the state text truncates before any control does.

import Link from 'next/link';
import { useEffect, useRef, useState } from 'react';

import { GearIcon, MutedIcon, PeopleIcon, SpeakerIcon } from './Icons';
import { RoomPicker } from './People';
import Tip from './Tip';
import type { PresenceView, RoomName, RoomSummary } from './types';
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

export default function PresenceStrip({
  presence, disconnected, nowMs, rooms, room, onRoom, onCreated, onPeople, muted, onToggleMute, signedIn, consent, onConsent,
}: {
  readonly presence: PresenceView | null;
  readonly disconnected: boolean;
  readonly nowMs: number;
  readonly rooms: readonly RoomSummary[];
  readonly room: RoomName;
  readonly onRoom: (room: RoomName) => void;
  readonly onCreated: (room: RoomName) => void;
  readonly onPeople: () => void;
  readonly muted: boolean;
  readonly onToggleMute: () => void;
  readonly signedIn: boolean;
  readonly consent: boolean | null;
  readonly onConsent: (on: boolean) => void;
}): JSX.Element {
  const [settings, setSettings] = useState(false);
  const panel = useRef<HTMLDivElement | null>(null);
  const state = presence?.state ?? 'unknown';
  const degraded = state.startsWith('degraded');
  const dot = disconnected || degraded ? styles.dotBad : BUSY.has(state) ? styles.dotBusy : state === 'idle' ? styles.dotLive : '';
  const text = disconnected ? 'reconnecting…' : presence === null ? 'waking' : state;

  useEffect(() => {
    if (!settings) return undefined;
    const close = (event: MouseEvent | TouchEvent) => {
      if (panel.current && !panel.current.contains(event.target as Node)) setSettings(false);
    };
    const esc = (event: KeyboardEvent) => { if (event.key === 'Escape') setSettings(false); };
    document.addEventListener('mousedown', close);
    document.addEventListener('touchstart', close);
    document.addEventListener('keydown', esc);
    return () => {
      document.removeEventListener('mousedown', close);
      document.removeEventListener('touchstart', close);
      document.removeEventListener('keydown', esc);
    };
  }, [settings]);

  const muteLabel = 'Stop Apocrypha speaking unprompted in this browser (local, only you)';

  return <header className={styles.strip}>
    <div className={styles.stripLeft} role="status" aria-live="polite">
      <Link href="/" className={styles.brand}>Apocrypha</Link>
      <span className={styles.state}>
        <span className={`${styles.dot} ${dot}`} aria-hidden="true" />
        <span className={styles.stateText}>{text}</span>
        {presence && !disconnected ? <span className={styles.age}>{age(presence.at, nowMs)}</span> : null}
      </span>
    </div>
    <div className={styles.stripRight}>
      {signedIn ? <RoomPicker rooms={rooms} current={room} onPick={onRoom} onCreated={onCreated} /> : null}
      {signedIn ? <Tip label="People: invite friends, see who is here, set your name" align="end" side="bottom">
        <button type="button" className={styles.iconBtn} aria-label="People and invitations" onClick={onPeople}><PeopleIcon /></button>
      </Tip> : null}
      <Tip label={muted ? `Muted. ${muteLabel}` : muteLabel} align="end" side="bottom">
        <button type="button" className={`${styles.iconBtn} ${muted ? styles.iconOn : ''}`} aria-pressed={muted} aria-label={muted ? 'Unmute unprompted speech' : 'Mute unprompted speech'} onClick={onToggleMute}>
          {muted ? <MutedIcon /> : <SpeakerIcon />}
        </button>
      </Tip>
      <div className={styles.settingsWrap} ref={panel}>
        <Tip label="Settings" align="end" side="bottom">
          <button type="button" className={`${styles.iconBtn} ${settings ? styles.iconOn : ''}`} aria-label="Settings" aria-haspopup="dialog" aria-expanded={settings} onClick={() => setSettings((v) => !v)}>
            <GearIcon />
          </button>
        </Tip>
        {settings ? <div className={styles.settings} role="dialog" aria-label="Settings">
          <h2>Settings</h2>
          <label className={styles.setting}>
            <span>
              <strong>Help improve Apocrypha</strong>
              <small>{signedIn
                ? 'Share usage data (which features you use, how long answers take) to improve Apocrypha. Off by default; turning it off stops collection but does not delete your conversations.'
                : 'Sign in to choose whether your usage data is shared. Nothing is collected from you without that choice.'}</small>
            </span>
            <input type="checkbox" role="switch" checked={consent === true} disabled={!signedIn || consent === null} onChange={(e) => onConsent(e.target.checked)} />
          </label>
          <label className={styles.setting}>
            <span>
              <strong>Mute unprompted speech</strong>
              <small>Hide what Apocrypha says on its own initiative, in this browser only. Replies to messages still appear.</small>
            </span>
            <input type="checkbox" role="switch" checked={muted} onChange={onToggleMute} />
          </label>
          <div className={styles.settingLinks}>
            {signedIn ? <Link href="/account">Account</Link> : <Link href="/login?next=%2F">Sign in</Link>}
            <Link href="/legal/privacy">Privacy</Link>
            <Link href="/legal/terms">Terms</Link>
          </div>
        </div> : null}
      </div>
    </div>
  </header>;
}
