// The living room: the one Apocrypha chat surface, rendered at / (the front door) and /room.
//
// The page polls one append-only river plus the room's in-flight turns and shows whatever
// arrived, whoever wrote it, prompted or not. Every message is a job in the queue (migration
// 0061): the local lane is answered by the PC worker, Apocrypha+ (Premium, the flagship) by the Vercel runner,
// and both answers land in the same river.

import Head from 'next/head';
import { useCallback, useEffect, useRef, useState } from 'react';

import DirectTools from './DirectTools';

import Composer from './Composer';
import PresenceStrip from './PresenceStrip';
import River from './River';
import type {
  EngineLane, FriendView, LiveTurnView, RoomSummary, PendingAttachment, PresenceView, RoomEventView, RoomName, RoomTool, ViewerView,
} from './types';
import { PeoplePanel, manage } from './People';
import styles from './Room.module.css';

const POLL_MS = 1_500;
const LANE_KEY = 'apocrypha.room.lane';
const PAGE = 200;
const MAX_UPLOAD_BYTES = 25 * 1024 * 1024;

interface EventsPayload {
  ok?: boolean;
  room?: { key: string; kind: string; role: string; quiet?: boolean };
  names?: Record<string, string>;
  events?: RoomEventView[];
  live?: LiveTurnView[];
  presence?: PresenceView | null;
  now?: string;
  viewer?: ViewerView;
  code?: string;
  error?: string;
}

function merge(current: readonly RoomEventView[], incoming: readonly RoomEventView[]): RoomEventView[] {
  if (incoming.length === 0) return [...current];
  const byId = new Map<number, RoomEventView>();
  for (const e of current) if (!e.pending) byId.set(e.id, e);
  for (const e of incoming) byId.set(e.id, e);
  const confirmedBodies = new Set(incoming.map((e) => e.body));
  const pending = current.filter((e) => e.pending && !confirmedBodies.has(e.body));
  const confirmed = [...byId.values()].sort((a, b) => a.id - b.id);
  return [...confirmed, ...pending];
}

function read(key: string): string | null {
  try { return window.localStorage.getItem(key); } catch { return null; }
}
function write(key: string, value: string): void {
  try { window.localStorage.setItem(key, value); } catch { /* storage may be unavailable */ }
}

async function toBase64(file: File): Promise<string> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  let binary = '';
  for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(binary);
}

export default function Room(): JSX.Element {
  const [room, setRoom] = useState<RoomName>('me');
  const [rooms, setRooms] = useState<RoomSummary[]>([]);
  const [friends, setFriends] = useState<FriendView[]>([]);
  const [names, setNames] = useState<Record<string, string>>({});
  const [peopleOpen, setPeopleOpen] = useState(false);
  const [roomNote, setRoomNote] = useState<string | null>(null);
  const [viewer, setViewer] = useState<ViewerView | null>(null);
  const [events, setEvents] = useState<RoomEventView[]>([]);
  const [live, setLive] = useState<LiveTurnView[]>([]);
  const [presence, setPresence] = useState<PresenceView | null>(null);
  const [disconnected, setDisconnected] = useState(false);
  const [muted, setMuted] = useState(false);
  const [lane, setLane] = useState<EngineLane>('local');
  const [tools, setTools] = useState<RoomTool[]>([]);
  const [attachments, setAttachments] = useState<PendingAttachment[]>([]);
  const [consent, setConsent] = useState<boolean | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [nowMs, setNowMs] = useState(() => Date.now());
  const [needsSignIn, setNeedsSignIn] = useState(false);
  const [inject, setInject] = useState<{ text: string; n: number } | undefined>(undefined);

  const lastId = useRef(0);
  const fails = useRef(0);
  const asked = useRef(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const generation = useRef(0);

  useEffect(() => {
    const tick = setInterval(() => setNowMs(Date.now()), 1_000);
    return () => clearInterval(tick);
  }, []);

  // Who is reading decides the default model: Premium when it is theirs and connected.
  useEffect(() => {
    if (!viewer) return;
    const saved = read(LANE_KEY);
    const premiumUsable = viewer.premium && viewer.premium_ready;
    setLane(saved === 'local' ? 'local' : premiumUsable ? 'flagship' : 'local');
    if (!viewer.signed_in) { setConsent(null); return; }
    void fetch('/api/apocrypha/member/consent', { credentials: 'same-origin', cache: 'no-store' })
      .then((r) => r.json() as Promise<{ ok?: boolean; consent?: { analytics?: boolean } }>)
      .then((p) => setConsent(p.ok === true ? p.consent?.analytics === true : false))
      .catch(() => setConsent(false));
  }, [viewer]);

  const reloadRooms = useCallback(async () => {
    try {
      const list = await manage<{ rooms: RoomSummary[]; friends: FriendView[] }>({ action: 'list' });
      setRooms(list.rooms); setFriends(list.friends);
    } catch { /* picker stays as it was */ }
  }, []);

  // Rooms, and an invitation carried in the link (?invite=...), once we know who is reading.
  useEffect(() => {
    if (!viewer?.signed_in) return;
    const params = new URLSearchParams(location.search);
    const token = params.get('invite');
    void (async () => {
      if (token) {
        try {
          const joined = await manage<{ key: string; title: string }>({ action: 'accept', token });
          setRoom(joined.key);
          setRoomNote(`You joined ${joined.title}.`);
        } catch (cause) { setRoomNote(cause instanceof Error ? cause.message : 'That invitation could not be used.'); }
        params.delete('invite');
        history.replaceState(null, '', `${location.pathname}${params.toString() ? `?${params}` : ''}`);
      }
      await reloadRooms();
    })();
  }, [viewer?.signed_in, reloadRooms]);

  const chooseLane = (next: EngineLane) => { setLane(next); write(LANE_KEY, next); };

  // Mute is the room's switch on the server: Apocrypha stops speaking unprompted there. Nothing
  // already said is hidden. Only the room's owner may flip it.
  const toggleMute = () => {
    const next = !muted;
    setMuted(next);
    void manage({ action: 'quiet', room, on: next }).catch((cause: unknown) => {
      setMuted(!next);
      setRoomNote(cause instanceof Error ? cause.message : 'Could not change that.');
    });
  };

  const changeConsent = async (on: boolean) => {
    setConsent(on);
    try {
      const r = await fetch('/api/apocrypha/member/consent', {
        method: 'POST', credentials: 'same-origin', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ analytics: on }),
      });
      const p = await r.json() as { ok?: boolean; consent?: { analytics?: boolean } };
      setConsent(p.ok === true ? p.consent?.analytics === true : !on);
    } catch {
      setConsent(!on);
    }
  };

  const poll = useCallback(async (gen: number) => {
    if (gen !== generation.current) return;
    const who = asked.current ? '' : '&who=1';
    try {
      const response = await fetch(`/api/room/events?room=${room}&after=${lastId.current}&limit=${PAGE}${who}`, { credentials: 'same-origin', cache: 'no-store' });
      const payload = await response.json() as EventsPayload;
      if (gen !== generation.current) return;
      if (response.status === 401 && payload.code === 'SIGN_IN_REQUIRED') { setNeedsSignIn(true); return; }
      if (!response.ok || payload.ok !== true) throw new Error(payload.error ?? payload.code ?? `HTTP ${response.status}`);
      setNeedsSignIn(false);
      asked.current = true;
      if (payload.viewer) setViewer(payload.viewer);
      if (payload.room) setMuted(payload.room.quiet === true);
      if (payload.names) setNames((n) => ({ ...n, ...payload.names }));
      if (payload.room && room === 'me') { generation.current += 1; setRoom(payload.room.key); return; }
      const incoming = payload.events ?? [];
      for (const e of incoming) if (e.id > lastId.current) lastId.current = e.id;
      if (incoming.length > 0) setEvents((current) => merge(current, incoming));
      setLive(payload.live ?? []);
      setPresence(payload.presence ?? null);
      fails.current = 0;
      setDisconnected(false);
    } catch (cause) {
      if (gen !== generation.current) return;
      fails.current += 1;
      if (fails.current >= 2) setDisconnected(true);
      if (cause instanceof Error && /not in that room/i.test(cause.message)) setRoom('me');
    } finally {
      if (gen === generation.current) {
        timer.current = setTimeout(() => {
          if (document.hidden) { timer.current = null; return; }
          void poll(gen);
        }, POLL_MS);
      }
    }
  }, [room]);

  useEffect(() => {
    generation.current += 1;
    const gen = generation.current;
    lastId.current = 0;
    fails.current = 0;
    setEvents([]);
    setLive([]);
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

  const attach = (files: File[]) => {
    for (const file of files) {
      const key = `${Date.now()}-${Math.random().toString(36).slice(2)}`;
      const mime = file.type || 'application/octet-stream';
      if (file.size > MAX_UPLOAD_BYTES) {
        setAttachments((a) => [...a, { key, name: file.name, mime, id: null, state: 'failed', error: 'Files are 25 MB at most.' }]);
        continue;
      }
      setAttachments((a) => [...a, { key, name: file.name, mime, id: null, state: 'uploading' }]);
      void (async () => {
        try {
          const response = await fetch('/api/apocrypha/member/attachments', {
            method: 'POST', credentials: 'same-origin', headers: { 'content-type': 'application/json' },
            body: JSON.stringify({ file_name: file.name, mime_type: mime, data_base64: await toBase64(file) }),
          });
          const payload = await response.json() as { ok?: boolean; attachment?: { id?: string }; error?: string };
          if (!response.ok || payload.ok !== true || !payload.attachment?.id) throw new Error(payload.error ?? `HTTP ${response.status}`);
          const id = payload.attachment.id;
          setAttachments((a) => a.map((x) => (x.key === key ? { ...x, id, state: 'ready' } : x)));
        } catch (cause) {
          setAttachments((a) => a.map((x) => (x.key === key ? { ...x, state: 'failed', error: cause instanceof Error ? cause.message : 'Upload failed.' } : x)));
        }
      })();
    }
  };

  const send = async (body: string): Promise<boolean> => {
    setError(null);
    const me = viewer?.author ?? (viewer?.owner ? 'apocky' : 'you');
    const optimistic: RoomEventView = {
      id: -Date.now(), room, author: me, kind: 'utterance', body, meta: {}, created_at: new Date().toISOString(), pending: true,
    };
    setEvents((current) => [...current, optimistic]);
    try {
      const response = await fetch('/api/room/say', {
        method: 'POST',
        credentials: 'same-origin',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          room, body, engine_lane: lane, time_zone: Intl.DateTimeFormat().resolvedOptions().timeZone,
          attachment_ids: attachments.filter((a) => a.state === 'ready' && a.id).map((a) => a.id),
          tools: lane === 'flagship' ? tools : [],
        }),
      });
      const payload = await response.json() as { ok?: boolean; event?: RoomEventView; job?: { lane?: string }; error?: string; code?: string };
      if (!response.ok || payload.ok !== true || !payload.event) throw new Error(payload.error ?? payload.code ?? `HTTP ${response.status}`);
      const event = payload.event;
      if (!asked.current || !viewer?.author) asked.current = false;
      setEvents((current) => merge(current.filter((e) => e !== optimistic), [event]));
      setAttachments([]);
      setTools([]);
      // Premium answers on the Vercel runner; kick it now rather than waiting for its minute sweep.
      if (payload.job?.lane === 'flagship') {
        void fetch('/api/apocrypha/runner/run', { method: 'POST', credentials: 'same-origin' }).catch(() => undefined);
      }
      return true;
    } catch (cause) {
      setEvents((current) => current.filter((e) => e !== optimistic));
      setError(cause instanceof Error ? cause.message : 'Could not send.');
      return false;
    }
  };

  const visible = events;
  const me = viewer?.author ?? null;

  return <>
    <Head>
      <title>Apocrypha</title>
      <meta name="description" content="A continuously-thinking digital intelligence. It may speak first." />
      <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover, interactive-widget=resizes-content" />
      <meta name="robots" content="index,follow" />
      <link rel="canonical" href="https://www.apocky.com/" />
      <meta name="referrer" content="no-referrer" />
      <meta name="theme-color" content="#05060b" />
    </Head>
    <main id="main-content" className={styles.page}>
      <PresenceStrip
        presence={presence} disconnected={disconnected} nowMs={nowMs}
        rooms={rooms} room={room} onRoom={setRoom}
        onCreated={(key) => { setRoom(key); void reloadRooms(); setPeopleOpen(true); }}
        onPeople={() => setPeopleOpen((v) => !v)}
        muted={muted} onToggleMute={toggleMute}
        signedIn={viewer?.signed_in === true} consent={consent} onConsent={(on) => void changeConsent(on)}
      />
      {roomNote ? <p className={styles.roomNote} role="status" onClick={() => setRoomNote(null)}>{roomNote}</p> : null}
      {peopleOpen && rooms.find((r) => r.key === room) ? <PeoplePanel
        room={rooms.find((r) => r.key === room)!}
        friends={friends}
        reloadFriends={() => void reloadRooms()}
        onClose={() => setPeopleOpen(false)}
        onLeft={() => { setPeopleOpen(false); setRoom('me'); void reloadRooms(); }}
      /> : null}
      {needsSignIn ? <div className={styles.gate}>
        <h1>Apocrypha</h1>
        <p>A continuously-thinking digital intelligence. The room is open to signed-in members.</p>
        <div className={styles.gateActions}>
          <a className={styles.gatePrimary} href="/login?next=%2F">Sign in</a>
          <a className={styles.gateSecondary} href="/register?next=%2F">Create an account</a>
        </div>
      </div> : <River events={visible} live={live} me={me} nowMs={nowMs} names={names} onRetry={(body) => void send(body)} />}
      {viewer?.direct ? <DirectTools events={visible} onText={(text) => setInject((i) => ({ text, n: (i?.n ?? 0) + 1 }))} /> : null}
      {needsSignIn ? null : <Composer
        room={room}
        signedIn={viewer?.signed_in === true}
        premium={viewer?.premium === true}
        premiumReady={viewer?.premium_ready === true}
        lane={lane} onLane={chooseLane}
        tools={tools} onTools={setTools}
        attachments={attachments} onAttach={attach}
        onRemoveAttachment={(key) => setAttachments((a) => a.filter((x) => x.key !== key))}
        onSend={send} error={error} busy={live.length > 0} inject={inject}
      />}
    </main>
  </>;
}
