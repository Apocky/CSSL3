// The river as a conversation: your messages on the right, Apocrypha on the left with its avatar,
// other people on the left under their name, room notices as quiet centered lines. Consecutive
// messages from one speaker group under one avatar/name. Apocrypha's reasoning and recall never
// sit inline: they fold into one "Thought" line under the reply they led to. Every Apocrypha body
// passes presentable(), so a leaked thought is withheld, never rendered.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';

import { ApocryphaAvatar } from '@/components/apocrypha/ApocryphaAvatar';
import ConversationMessageContent from '@/components/apocrypha/ConversationMessageContent';
import { presentable } from '@/lib/apocrypha/deliberation';
import { CheckIcon, CopyIcon, DownIcon } from './Icons';
import Tip from './Tip';
import { authorLabel, modelLabel, type LiveTurnView, type RoomEventView } from './types';
import styles from './Room.module.css';

const NEAR_BOTTOM_PX = 64;
const GROUP_GAP_MS = 5 * 60_000;

function dayKey(iso: string): string {
  const d = new Date(iso);
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
}

function dayLabel(iso: string, nowMs: number): string {
  if (dayKey(iso) === dayKey(new Date(nowMs).toISOString())) return 'Today';
  if (dayKey(iso) === dayKey(new Date(nowMs - 86_400_000).toISOString())) return 'Yesterday';
  return new Date(iso).toLocaleDateString(undefined, { weekday: 'short', month: 'short', day: 'numeric' });
}

function timeLabel(iso: string): string {
  return new Date(iso).toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
}

const words = (text: string) => text.split(/\s+/).filter(Boolean).length;

type Item =
  | { type: 'day'; key: string; label: string }
  | { type: 'notice'; key: string; event: RoomEventView }
  | { type: 'message'; key: string; event: RoomEventView; side: 'mine' | 'apocrypha' | 'other'; first: boolean; last: boolean; notes: RoomEventView[] }
  | { type: 'live'; key: string; turn: LiveTurnView };

function build(events: readonly RoomEventView[], live: readonly LiveTurnView[], me: string | null, nowMs: number): Item[] {
  const items: Item[] = [];
  let lastDay = '';
  let notes: RoomEventView[] = [];
  let prev: { author: string; at: number } | null = null;
  for (const event of events) {
    if (event.kind === 'presence') continue;
    const day = dayKey(event.created_at);
    if (day !== lastDay) {
      items.push({ type: 'day', key: `d${day}`, label: dayLabel(event.created_at, nowMs) });
      lastDay = day;
      prev = null;
    }
    if (event.kind === 'thought' || event.kind === 'recall') { notes.push(event); continue; }
    if (event.kind === 'system') {
      items.push({ type: 'notice', key: `n${event.id}`, event });
      prev = null;
      continue;
    }
    const side = event.author === 'apocrypha' ? 'apocrypha' : me !== null && event.author === me ? 'mine' : 'other';
    const at = Date.parse(event.created_at);
    const first = !(prev && prev.author === event.author && at - prev.at < GROUP_GAP_MS);
    const last = items[items.length - 1];
    if (!first && last?.type === 'message') last.last = false;
    items.push({ type: 'message', key: event.pending ? `p${event.id}` : `m${event.id}`, event, side, first, last: true, notes: side === 'apocrypha' ? notes : [] });
    if (side === 'apocrypha') notes = [];
    prev = { author: event.author, at };
  }
  for (const turn of live) items.push({ type: 'live', key: `l${turn.job_id}`, turn });
  return items;
}

function Thought({ notes, meta }: { readonly notes: readonly RoomEventView[]; readonly meta: Record<string, unknown> }): JSX.Element | null {
  const [open, setOpen] = useState(false);
  const thought = notes.filter((n) => n.kind === 'thought');
  const recall = notes.filter((n) => n.kind === 'recall');
  const thoughtWords = thought.reduce((sum, n) => sum + words(n.body), 0);
  const recallWords = recall.reduce((sum, n) => sum + words(n.body), 0);
  const recallRecords = typeof meta.recall_records === 'number' ? meta.recall_records : 0;
  if (thoughtWords === 0 && recallWords === 0 && recallRecords === 0) return null;
  const parts = [
    thoughtWords > 0 ? `thought ${thoughtWords} words` : null,
    recallWords > 0 ? `recalled ${recallWords} words` : recallRecords > 0 ? `recalled ${recallRecords} memories` : null,
  ].filter(Boolean).join(' · ');
  const expandable = thought.length + recall.length > 0;
  return <div className={styles.thought}>
    <button type="button" className={styles.thoughtHead} onClick={() => expandable && setOpen((v) => !v)} aria-expanded={expandable ? open : undefined} disabled={!expandable}>
      <span aria-hidden="true">{expandable ? (open ? '▾' : '▸') : '·'}</span> {parts}
    </button>
    {open ? <div className={styles.thoughtBody}>
      {thought.map((n) => <p key={n.id}><strong>Thinking</strong> {n.body}</p>)}
      {recall.map((n) => <p key={n.id}><strong>Recalling</strong> {n.body}</p>)}
    </div> : null}
  </div>;
}

function CopyButton({ text }: { readonly text: string }): JSX.Element {
  const [done, setDone] = useState(false);
  const copy = async () => {
    try { await navigator.clipboard.writeText(text); setDone(true); setTimeout(() => setDone(false), 1_500); } catch { /* clipboard refused */ }
  };
  return <Tip label={done ? 'Copied' : 'Copy this reply'} align="start">
    <button type="button" className={styles.msgAction} onClick={() => void copy()} aria-label="Copy this reply">
      {done ? <CheckIcon size={16} /> : <CopyIcon size={16} />}
    </button>
  </Tip>;
}

function Avatar({ thinking = false }: { readonly thinking?: boolean }): JSX.Element {
  return <span className={styles.avatar} aria-hidden="true">
    <ApocryphaAvatar state={thinking ? 'thinking' : 'ready'} size={30} detail="compact" />
  </span>;
}

function Message({ item, names }: { readonly item: Extract<Item, { type: 'message' }>; readonly names: Record<string, string> }): JSX.Element {
  const { event, side, first, last, notes } = item;
  const apocrypha = side === 'apocrypha';
  const body = apocrypha ? presentable(event.body).text : event.body;
  const model = apocrypha ? modelLabel(event.meta) : null;
  const elapsed = typeof event.meta.elapsed_s === 'number' ? `${event.meta.elapsed_s}s` : null;
  const detail = [timeLabel(event.created_at), model, elapsed, event.meta.unprompted === true ? 'unprompted' : null].filter(Boolean).join(' · ');
  return <div className={`${styles.msgRow} ${styles[`side_${side}`]} ${first ? styles.groupStart : ''}`}>
    {side !== 'mine' ? (apocrypha && first ? <Avatar /> : <span className={styles.avatarSpacer} aria-hidden="true" />) : null}
    <div className={styles.msgCol}>
      {first && side === 'other' ? <div className={styles.msgName}>{names[event.author] ?? authorLabel(event.author)}</div> : null}
      {first && apocrypha ? <div className={styles.msgName}>Apocrypha</div> : null}
      {apocrypha && notes.length + (typeof event.meta.recall_records === 'number' ? 1 : 0) > 0 ? <Thought notes={notes} meta={event.meta} /> : null}
      <div className={`${styles.bubble} ${event.pending ? styles.pending : ''}`} title={detail}>
        <ConversationMessageContent content={body} assistant={apocrypha} />
      </div>
      {last || apocrypha ? <div className={styles.msgMeta}>
        <span>{event.pending ? 'sending…' : detail}</span>
        {apocrypha && !event.pending ? <CopyButton text={body} /> : null}
      </div> : null}
    </div>
  </div>;
}

function Live({ turn, nowMs }: { readonly turn: LiveTurnView; readonly nowMs: number }): JSX.Element {
  const [startedAt] = useState(() => Date.now());
  const seconds = Math.max(0, Math.round((nowMs - startedAt) / 1000));
  const waiting = turn.status === 'queued';
  const label = waiting
    ? (turn.lane === 'flagship' ? 'Waiting for Apocrypha+' : 'Waiting for Apocrypha')
    : `Thinking${seconds > 0 ? ` · ${seconds}s` : '…'}`;
  return <div className={`${styles.msgRow} ${styles.side_apocrypha} ${styles.groupStart}`} aria-live="polite">
    <Avatar thinking />
    <div className={styles.msgCol}>
      <div className={styles.msgName}>Apocrypha <span className={styles.laneTag}>{turn.lane === 'flagship' ? 'Apocrypha+' : 'Local'}</span></div>
      {turn.text === '' ? <div className={`${styles.bubble} ${styles.typing}`}>
        <span className={styles.dots} aria-hidden="true"><i /><i /><i /></span>
        <span className={styles.typingLabel}>{label}</span>
      </div> : <div className={styles.bubble}><ConversationMessageContent content={turn.text} assistant /></div>}
    </div>
  </div>;
}

export default function River({ events, live, me, nowMs, names }: {
  readonly names: Record<string, string>;
  readonly events: readonly RoomEventView[];
  readonly live: readonly LiveTurnView[];
  readonly me: string | null;
  readonly nowMs: number;
}): JSX.Element {
  const ref = useRef<HTMLDivElement | null>(null);
  const atBottom = useRef(true);
  const [showJump, setShowJump] = useState(false);
  const items = useMemo(() => build(events, live, me, nowMs), [events, live, me, nowMs]);

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

  // Follow the river only while the reader is already at its mouth; scrolled up means reading.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el || !atBottom.current) return;
    el.scrollTop = el.scrollHeight;
  }, [items]);

  const jump = () => {
    const el = ref.current;
    if (!el) return;
    el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' });
    atBottom.current = true;
    setShowJump(false);
  };

  return <div className={styles.riverWrap}>
    <div ref={ref} className={styles.river} role="log" aria-label="The room" aria-live="polite">
      <div className={styles.riverInner}>
        {items.length === 0 ? <div className={styles.empty}>
          <ApocryphaAvatar state="ready" size={72} detail="compact" />
          <p>Nothing said yet. Apocrypha may speak first.</p>
        </div> : null}
        {items.map((item) => {
          if (item.type === 'day') return <div key={item.key} className={styles.day}><span>{item.label}</span></div>;
          if (item.type === 'notice') return <div key={item.key} className={styles.notice} title={timeLabel(item.event.created_at)}>{item.event.body}</div>;
          if (item.type === 'live') return <Live key={item.key} turn={item.turn} nowMs={nowMs} />;
          return <Message key={item.key} item={item} names={names} />;
        })}
      </div>
    </div>
    {showJump ? <Tip label="Jump to the latest message" side="top">
      <button type="button" className={styles.jump} onClick={jump} aria-label="Jump to the latest message"><DownIcon size={18} /></button>
    </Tip> : null}
  </div>;
}
