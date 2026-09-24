import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';

import ConversationMessageContent from '@/components/apocrypha/ConversationMessageContent';
import { authorLabel, type RoomEventView } from './types';
import styles from './Room.module.css';

const NEAR_BOTTOM_PX = 48;

function dayKey(iso: string): string {
  const d = new Date(iso);
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
}

function dayLabel(iso: string, nowMs: number): string {
  const d = new Date(iso);
  const today = new Date(nowMs);
  if (dayKey(iso) === dayKey(today.toISOString())) return 'Today';
  const yesterday = new Date(nowMs - 86_400_000);
  if (dayKey(iso) === dayKey(yesterday.toISOString())) return 'Yesterday';
  return d.toLocaleDateString(undefined, { weekday: 'short', month: 'short', day: 'numeric' });
}

function timeLabel(iso: string): string {
  return new Date(iso).toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
}

function Thought({ event }: { readonly event: RoomEventView }): JSX.Element {
  const [open, setOpen] = useState(false);
  const words = event.body.split(/\s+/).filter(Boolean).length;
  return <div className={styles.thought}>
    <button type="button" className={styles.thoughtHead} onClick={() => setOpen((v) => !v)} aria-expanded={open}>
      <span>{open ? '▾' : '▸'} {event.kind === 'recall' ? 'recalling' : 'thinking'}</span>
      <span className={styles.time}>{timeLabel(event.created_at)} · {words} words</span>
    </button>
    {open ? <div className={styles.thoughtBody}>{event.body}</div> : null}
  </div>;
}

function Row({ event }: { readonly event: RoomEventView }): JSX.Element {
  if (event.kind === 'system') return <div className={styles.system}>{event.body}</div>;
  if (event.kind === 'thought' || event.kind === 'recall') return <Thought event={event} />;
  const apocrypha = event.author === 'apocrypha';
  const authorClass = apocrypha ? styles.authorApocrypha : event.author === 'apocky' ? styles.authorOwner : '';
  return <div className={event.pending ? `${styles.row} ${styles.pending}` : styles.row}>
    <div className={styles.rowHead}>
      <span className={`${styles.author} ${authorClass}`}>{authorLabel(event.author)}</span>
      <span className={styles.time}>{event.pending ? 'sending' : timeLabel(event.created_at)}</span>
      {event.meta.unprompted === true ? <span className={styles.unprompted}>unprompted</span> : null}
    </div>
    <div className={styles.body}>
      <ConversationMessageContent content={event.body} assistant={apocrypha} />
    </div>
  </div>;
}

export default function River({ events, nowMs }: {
  readonly events: readonly RoomEventView[];
  readonly nowMs: number;
}): JSX.Element {
  const ref = useRef<HTMLDivElement | null>(null);
  const atBottom = useRef(true);
  const [showJump, setShowJump] = useState(false);

  const measure = useCallback(() => {
    const el = ref.current;
    if (!el) return;
    const near = el.scrollHeight - el.scrollTop - el.clientHeight < NEAR_BOTTOM_PX;
    atBottom.current = near;
    setShowJump(!near);
  }, []);

  useEffect(() => {
    const el = ref.current;
    if (!el) return undefined;
    el.addEventListener('scroll', measure, { passive: true });
    return () => el.removeEventListener('scroll', measure);
  }, [measure]);

  // Follow the river only while the reader is already at its mouth. Scrolled up means reading;
  // yanking them down for every new row would make the page unreadable the moment it got busy.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el || !atBottom.current) return;
    el.scrollTop = el.scrollHeight;
  }, [events]);

  const jump = () => {
    const el = ref.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
    atBottom.current = true;
    setShowJump(false);
  };

  let lastDay = '';
  return <div ref={ref} className={styles.river} aria-label="The room">
    {events.length === 0 ? <p className={styles.empty}>Nothing said yet. Apocrypha may speak first.</p> : null}
    {events.map((event) => {
      const day = dayKey(event.created_at);
      const separator = day !== lastDay;
      lastDay = day;
      return <div key={event.pending ? `p${event.id}` : event.id}>
        {separator ? <div className={styles.day}>{dayLabel(event.created_at, nowMs)}</div> : null}
        <Row event={event} />
      </div>;
    })}
    {showJump ? <button type="button" className={styles.jump} onClick={jump}>Jump to now</button> : null}
  </div>;
}
