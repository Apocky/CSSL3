// The living room: the one Apocrypha chat surface, rendered at / (the front door) and /room.
//
// Not a chatbot page. There is no request/response shape here at all: the page polls one
// append-only river and shows whatever arrived, whoever wrote it, prompted or not. Apocrypha's
// state comes from the same river (presence rows), so "is it there" and "what did it say" are one
// question with one answer. The only thing the page owns is the reader's scroll position.

import Head from 'next/head';
import { useCallback, useEffect, useRef, useState } from 'react';

import Composer from './Composer';
import PresenceStrip from './PresenceStrip';
import River from './River';
import type { PresenceView, RoomEventView, RoomName } from './types';
import styles from './Room.module.css';

const POLL_MS = 1_500;
const MUTE_KEY = 'apocrypha.room.mute';
const PAGE = 200;

interface EventsPayload {
  ok?: boolean;
  events?: RoomEventView[];
  presence?: PresenceView | null;
  now?: string;
  viewer?: { owner?: boolean };
  code?: string;
  error?: string;
}

function merge(current: readonly RoomEventView[], incoming: readonly RoomEventView[]): RoomEventView[] {
  if (incoming.length === 0) return [...current];
  const byId = new Map<number, RoomEventView>();
  for (const e of current) if (!e.pending) byId.set(e.id, e);
  for (const e of incoming) byId.set(e.id, e);
  // An optimistic row is superseded by the confirmed row with the same author-visible body.
  const confirmedBodies = new Set(incoming.map((e) => e.body));
  const pending = current.filter((e) => e.pending && !confirmedBodies.has(e.body));
  const confirmed = [...byId.values()].sort((a, b) => a.id - b.id);
  return [...confirmed, ...pending];
}

export default function Room(): JSX.Element {
  const [room, setRoom] = useState<RoomName>('lobby');
  const [owner, setOwner] = useState(false);
  const [events, setEvents] = useState<RoomEventView[]>([]);
  const [presence, setPresence] = useState<PresenceView | null>(null);
  const [disconnected, setDisconnected] = useState(false);
  const [muted, setMuted] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [nowMs, setNowMs] = useState(() => Date.now());

  const lastId = useRef(0);
  const fails = useRef(0);
  const asked = useRef(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const generation = useRef(0);

  useEffect(() => {
    try { setMuted(window.localStorage.getItem(MUTE_KEY) === '1'); } catch { /* storage may be unavailable */ }
    const tick = setInterval(() => setNowMs(Date.now()), 5_000);
    return () => clearInterval(tick);
  }, []);

  const toggleMute = () => {
    setMuted((m) => {
      try { window.localStorage.setItem(MUTE_KEY, m ? '0' : '1'); } catch { /* ignore */ }
      return !m;
    });
  };

  const poll = useCallback(async (gen: number) => {
    if (gen !== generation.current) return;
    const who = asked.current ? '' : '&who=1';
    try {
      const response = await fetch(
        `/api/room/events?room=${room}&after=${lastId.current}&limit=${PAGE}${who}`,
        { credentials: 'same-origin', cache: 'no-store' },
      );
      const payload = await response.json() as EventsPayload;
      if (gen !== generation.current) return;
      if (!response.ok || payload.ok !== true) throw new Error(payload.error ?? payload.code ?? `HTTP ${response.status}`);
      asked.current = true;
      if (payload.viewer) setOwner(payload.viewer.owner === true);
      const incoming = payload.events ?? [];
      for (const e of incoming) if (e.id > lastId.current) lastId.current = e.id;
      if (incoming.length > 0) setEvents((current) => merge(current, incoming));
      setPresence(payload.presence ?? null);
      if (payload.now) setNowMs(Date.parse(payload.now) || Date.now());
      fails.current = 0;
      setDisconnected(false);
    } catch (cause) {
      if (gen !== generation.current) return;
      fails.current += 1;
      if (fails.current >= 2) setDisconnected(true);
      if (cause instanceof Error && cause.message === 'That room is private.') {
        setRoom('lobby');
      }
    } finally {
      if (gen === generation.current) {
        timer.current = setTimeout(() => {
          if (document.hidden) { timer.current = null; return; }
          void poll(gen);
        }, POLL_MS);
      }
    }
  }, [room]);

  // One chain per room. Switching rooms bumps the generation so a late response from the old
  // room cannot land in the new one, and the river starts over at the present.
  useEffect(() => {
    generation.current += 1;
    const gen = generation.current;
    lastId.current = 0;
    fails.current = 0;
    setEvents([]);
    setPresence(null);
    setError(null);
    if (timer.current) { clearTimeout(timer.current); timer.current = null; }
    void poll(gen);
    const onVisible = () => {
      if (!document.hidden && timer.current === null && gen === generation.current) void poll(gen);
    };
    document.addEventListener('visibilitychange', onVisible);
    return () => {
      document.removeEventListener('visibilitychange', onVisible);
      if (timer.current) { clearTimeout(timer.current); timer.current = null; }
    };
  }, [poll]);

  const send = async (body: string): Promise<boolean> => {
    setError(null);
    const optimistic: RoomEventView = {
      id: -Date.now(), room, author: owner ? 'apocky' : 'you', kind: 'utterance', body, meta: {},
      created_at: new Date().toISOString(), pending: true,
    };
    setEvents((current) => [...current, optimistic]);
    try {
      const response = await fetch('/api/room/say', {
        method: 'POST',
        credentials: 'same-origin',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ room, body }),
      });
      const payload = await response.json() as { ok?: boolean; event?: RoomEventView; error?: string; code?: string };
      if (!response.ok || payload.ok !== true || !payload.event) {
        throw new Error(payload.error ?? payload.code ?? `HTTP ${response.status}`);
      }
      const event = payload.event;
      setEvents((current) => merge(current.filter((e) => e !== optimistic), [event]));
      return true;
    } catch (cause) {
      setEvents((current) => current.filter((e) => e !== optimistic));
      setError(cause instanceof Error ? cause.message : 'Could not send.');
      return false;
    }
  };

  const visible = muted
    ? events.filter((e) => !(e.author === 'apocrypha' && e.meta.unprompted === true))
    : events;
  const river = visible.filter((e) => e.kind !== 'presence');

  return <>
    <Head>
      <title>Apocrypha</title>
      <meta name="description" content="A continuously-thinking digital intelligence. It may speak first." />
      <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
      <meta name="robots" content="index,follow" />
      <link rel="canonical" href="https://www.apocky.com/" />
      <meta name="referrer" content="no-referrer" />
      <meta name="theme-color" content="#05060b" />
    </Head>
    <main id="main-content" className={styles.page}>
      <PresenceStrip presence={presence} disconnected={disconnected} nowMs={nowMs} />
      <p className={styles.tagline}>A continuously-thinking digital intelligence. It may speak first.</p>
      <River events={river} nowMs={nowMs} />
      <Composer room={room} owner={owner} onRoom={setRoom} onSend={send} error={error} muted={muted} onToggleMute={toggleMute} />
    </main>
  </>;
}
