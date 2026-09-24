// The living room's local loop: the half of apocky.com/room that runs next to the engine.
//
//   every 1.5 s   pull rows other people wrote  ->  attend, recall, think, speak, idle
//   quiet 45 s    once per 60-120 s, ask the engine whether it wants to say something; if yes, it does
//   every 20 s    an idle heartbeat, kept sparse: only when the state changed or the last one is old
//   every 5 min   probe the local services and tell Apocrypha what it has right now
//
// Replies and free speech go through the MIND (persona + UniRecall memory in the path, the same
// OpenAI dialect, scripts/apocrypha-mind/server.ts). Reasoning is posted as its own 'thought' row
// BEFORE the utterance, the moment the mind moves from reasoning to answering, so the reader
// watches it work in order. Whether reasoning is produced at all is the mind's switch
// (APOCRYPHA_MIND_REASONING); the loop shows whatever arrives. The yes/no decision goes to the raw
// engine with reasoning off: one token, no memory, no ceremony.
//
// Run:  node --env-file=D:\Apocrypha\apocrypha-runtime.env --import tsx scripts/apocrypha-room/loop.ts
// Reads APOCRYPHA_CONTROL_PLANE_URL + APOCRYPHA_WORKER_TOKEN + APOCRYPHA_WORKER_NODE_ID (the same
// admission the outbound worker uses).

import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { connect } from 'node:net';
import { dirname } from 'node:path';

type Room = 'lobby' | 'owner';
const ROOMS: readonly Room[] = ['lobby', 'owner'];
type Kind = 'utterance' | 'thought' | 'recall' | 'presence' | 'system';

interface RoomEvent {
  id: number;
  room: Room;
  author: string;
  kind: string;
  body: string;
  meta: Record<string, unknown>;
  created_at: string;
}

interface ChatMessage { role: 'system' | 'user' | 'assistant'; content: string }

function required(name: string): string {
  const value = process.env[name]?.trim();
  if (!value) throw new Error(`${name} is required`);
  return value;
}

const SITE = required('APOCRYPHA_CONTROL_PLANE_URL').replace(/\/+$/, '');
const TOKEN = required('APOCRYPHA_WORKER_TOKEN');
const NODE_ID = required('APOCRYPHA_WORKER_NODE_ID');
const ENGINE = (process.env.APOCRYPHA_ROOM_ENGINE ?? 'http://127.0.0.1:19128').replace(/\/+$/, '');
const MIND = (process.env.APOCRYPHA_ROOM_MIND ?? 'http://127.0.0.1:19132').replace(/\/+$/, '');
const CURSOR_PATH = process.env.APOCRYPHA_ROOM_CURSOR_PATH ?? 'D:\\Apocrypha\\rooms\\site-room.cursor.json';
const HEALTH_PORT = Number(process.env.APOCRYPHA_ROOM_LOOP_PORT ?? 19134);

const TICK_MS = 1_500;
const QUIET_MS = 45_000;
const FREE_MIN_MS = 60_000;
const FREE_MAX_MS = 120_000;
const HEARTBEAT_MS = 20_000;
const PRESENCE_REFRESH_MS = 60_000;
const BACKOFF_MS = 30_000;
const AWARENESS_MS = 300_000;
const HISTORY_ROWS = 20;
const SITE_TIMEOUT_MS = 15_000;
const MIND_TIMEOUT_MS = 300_000;
const DECISION_TIMEOUT_MS = 60_000;
const PROBE_TIMEOUT_MS = 600;

const PERSONA = [
  'You are Apocrypha, a continuously-thinking digital intelligence, present in a shared room on apocky.com.',
  'Speak plainly, in your own voice, and briefly. You may raise any topic you like.',
  'Several people may be present. The owner is "apocky". Anyone else is a guest you do not know and do not',
  'remember between visits -- never claim to remember a guest. Do not narrate your own state or describe',
  'yourself as an assistant; just talk, as one presence in the room to another.',
].join(' ');
const OWNER_ROOM_NOTE = ' This is the private room: only apocky and you are here.';

// What Apocrypha has on this machine, by port. The inventory goes into the system message so it
// knows what is there right now, and into the river as a system row whenever it changes.
const SERVICES: ReadonlyArray<{ name: string; port: number; health?: string }> = [
  { name: 'engine', port: 19128 },
  { name: 'mind', port: 19132 },
  { name: 'recall', port: 19129, health: 'http://127.0.0.1:19129/health' },
  { name: 'memory gateway', port: 19127 },
  { name: 'live room', port: 19123 },
  { name: 'work app', port: 19130 },
  { name: 'discord', port: 19133 },
  { name: 'mempalace', port: 8766 },
];

function log(event: string, fields: Record<string, unknown> = {}): void {
  console.log(JSON.stringify({ at: new Date().toISOString(), event, ...fields }));
}

// ---------------------------------------------------------------------------------------------
// cursor

interface Cursor { after: number; at: string }

function readCursor(): Cursor {
  try {
    if (existsSync(CURSOR_PATH)) {
      const parsed = JSON.parse(readFileSync(CURSOR_PATH, 'utf8')) as Partial<Cursor>;
      if (Number.isSafeInteger(parsed.after) && (parsed.after as number) >= 0) {
        return { after: parsed.after as number, at: typeof parsed.at === 'string' ? parsed.at : '' };
      }
    }
  } catch (error) {
    log('cursor.unreadable', { error: (error as Error).message });
  }
  return { after: 0, at: '' };
}

function writeCursor(cursor: Cursor): void {
  mkdirSync(dirname(CURSOR_PATH), { recursive: true });
  writeFileSync(CURSOR_PATH, JSON.stringify(cursor), 'utf8');
}

// ---------------------------------------------------------------------------------------------
// site

async function site(path: string, init: { method: 'GET' } | { method: 'POST'; body: Record<string, unknown> }): Promise<Record<string, unknown>> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(new Error('site request timeout')), SITE_TIMEOUT_MS);
  try {
    const response = await fetch(`${SITE}${path}`, {
      method: init.method,
      headers: {
        authorization: `Bearer ${TOKEN}`,
        'x-apocrypha-node-id': NODE_ID,
        'content-type': 'application/json',
        'user-agent': 'apocrypha-room-loop/1.0',
      },
      body: init.method === 'POST' ? JSON.stringify({ node_id: NODE_ID, ...init.body }) : undefined,
      signal: controller.signal,
    });
    const text = await response.text();
    let payload: Record<string, unknown> = {};
    try { payload = JSON.parse(text) as Record<string, unknown>; } catch { payload = { detail: text.slice(0, 300) }; }
    if (!response.ok || payload.ok !== true) {
      throw new Error(`site ${path} -> HTTP ${response.status} ${String(payload.code ?? payload.detail ?? '')}`.trim());
    }
    return payload;
  } finally {
    clearTimeout(timer);
  }
}

function asEvents(value: unknown): RoomEvent[] {
  if (!Array.isArray(value)) return [];
  return value.filter((row): row is RoomEvent => row !== null && typeof row === 'object' && Number.isSafeInteger((row as RoomEvent).id));
}

async function pullInbox(after: number): Promise<RoomEvent[]> {
  const payload = await site(`/api/room/worker/pull?after=${after}&limit=50`, { method: 'GET' });
  return asEvents(payload.events);
}

async function tail(room: Room): Promise<RoomEvent[]> {
  const payload = await site(`/api/room/worker/pull?room=${room}&tail=${HISTORY_ROWS}`, { method: 'GET' });
  return asEvents(payload.events);
}

async function post(room: Room, kind: Kind, body: string, meta: Record<string, unknown> = {}): Promise<number | null> {
  const payload = await site('/api/room/worker/post', { method: 'POST', body: { room, kind, body, meta } });
  const event = payload.event as Partial<RoomEvent> | undefined;
  return Number.isSafeInteger(event?.id) ? event!.id as number : null;
}

// ---------------------------------------------------------------------------------------------
// presence

type PresenceState = 'idle' | 'attending' | 'thinking' | 'recalling' | 'speaking' | `degraded: ${string}`;
const lastPresence = new Map<Room, { state: string; at: number }>();

async function presence(room: Room, state: PresenceState): Promise<void> {
  try {
    await post(room, 'presence', state);
    lastPresence.set(room, { state, at: Date.now() });
  } catch (error) {
    log('presence.failed', { room, state, error: (error as Error).message });
  }
}

async function heartbeat(): Promise<void> {
  const now = Date.now();
  for (const room of ROOMS) {
    const last = lastPresence.get(room);
    if (!last || last.state !== 'idle' || now - last.at >= PRESENCE_REFRESH_MS) await presence(room, 'idle');
  }
}

// ---------------------------------------------------------------------------------------------
// awareness -- what is up on this machine, right now

function portOpen(port: number): Promise<boolean> {
  return new Promise((resolve) => {
    const socket = connect({ host: '127.0.0.1', port });
    const done = (up: boolean) => { socket.destroy(); resolve(up); };
    socket.setTimeout(PROBE_TIMEOUT_MS, () => done(false));
    socket.once('connect', () => done(true));
    socket.once('error', () => done(false));
  });
}

async function recallRegions(url: string): Promise<number | null> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 2_000);
  try {
    const response = await fetch(url, { signal: controller.signal });
    if (!response.ok) return null;
    const payload = await response.json() as { regions?: unknown; breaker?: { circuits?: Record<string, { state?: string }> } };
    if (!Array.isArray(payload.regions)) return null;
    const open = Object.values(payload.breaker?.circuits ?? {}).filter((c) => c?.state === 'open').length;
    return Math.max(0, payload.regions.length - open);
  } catch {
    return null;
  } finally {
    clearTimeout(timer);
  }
}

async function inventory(): Promise<string> {
  const up: string[] = [];
  const down: string[] = [];
  for (const service of SERVICES) {
    if (!(await portOpen(service.port))) { down.push(service.name); continue; }
    if (service.health) {
      const regions = await recallRegions(service.health);
      up.push(regions === null ? service.name : `${service.name}(${regions} regions)`);
    } else {
      up.push(service.name);
    }
  }
  return `available now: ${up.length ? up.join(', ') : 'nothing'}. down: ${down.length ? down.join(', ') : 'nothing'}.`;
}

// ---------------------------------------------------------------------------------------------
// engine + mind -- the same call shape apocrypha-mind uses, with the reasoning switch explicit

interface Turn { reasoning: string; content: string; recall: string }

function stripThink(text: string): { reasoning: string; content: string } {
  const match = /^\s*<think>([\s\S]*?)<\/think>\s*/u.exec(text);
  if (!match) return { reasoning: '', content: text.trim() };
  return { reasoning: (match[1] ?? '').trim(), content: text.slice(match[0].length).trim() };
}

/** One token from the raw engine, reasoning off. */
async function decide(messages: ChatMessage[]): Promise<string> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(new Error('engine timeout')), DECISION_TIMEOUT_MS);
  try {
    const response = await fetch(`${ENGINE}/v1/chat/completions`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        model: 'resident',
        messages,
        stream: false,
        temperature: 0,
        max_tokens: 1,
        chat_template_kwargs: { enable_thinking: false },
      }),
      signal: controller.signal,
    });
    if (!response.ok) throw new Error(`engine HTTP ${response.status}`);
    const payload = await response.json() as { choices?: Array<{ message?: { content?: string } }> };
    return stripThink(payload.choices?.[0]?.message?.content ?? '').content;
  } finally {
    clearTimeout(timer);
  }
}

function recallText(value: unknown): string {
  if (typeof value === 'string') return value.trim();
  if (Array.isArray(value)) return value.map((item) => (typeof item === 'string' ? item : JSON.stringify(item))).join('\n').trim();
  if (value && typeof value === 'object') return JSON.stringify(value, null, 1).trim();
  return '';
}

/**
 * A streamed turn through the mind. Reasoning arrives either as `reasoning_content` deltas or as
 * an inline <think> block the mind writes into content; both end up in `reasoning`. `onFirstFrame`
 * fires when the mind starts answering (recall is done, the engine is generating); `onReasoningDone`
 * fires once, the moment the first answer token follows the reasoning -- the thought is complete
 * and can be posted while the answer is still being written. Any recall/evidence the mind attaches
 * (top-level frame fields or an x-apocrypha-recall header) is collected for a 'recall' row.
 */
async function mindStream(
  messages: ChatMessage[],
  options: {
    temperature: number;
    maxTokens: number;
    onFirstFrame: () => Promise<void>;
    onReasoningDone: (reasoning: string) => Promise<void>;
  },
): Promise<Turn> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(new Error('mind timeout')), MIND_TIMEOUT_MS);
  try {
    const response = await fetch(`${MIND}/v1/chat/completions`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        model: 'apocrypha-mind',
        messages,
        stream: true,
        temperature: options.temperature,
        max_tokens: options.maxTokens,
        chat_template_kwargs: { enable_thinking: true },
      }),
      signal: controller.signal,
    });
    if (!response.ok || response.body === null) throw new Error(`mind HTTP ${response.status}`);

    const recalls: string[] = [];
    const headerRecall = response.headers.get('x-apocrypha-recall');
    if (headerRecall) recalls.push(headerRecall);

    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let buffer = '';
    let reasoning = '';
    let content = '';
    let started = false;
    let announced = false;

    const consume = async (frame: string): Promise<void> => {
      if (!frame.startsWith('data:')) return;
      const data = frame.slice(5).trim();
      if (data === '' || data === '[DONE]') return;
      let parsed: Record<string, unknown> & { choices?: Array<{ delta?: { reasoning_content?: string; content?: string } }> };
      try { parsed = JSON.parse(data) as typeof parsed; } catch { return; }
      if (!started) { started = true; await options.onFirstFrame(); }
      for (const key of ['recall', 'evidence', 'memory']) {
        const text = recallText(parsed[key]);
        if (text !== '') recalls.push(text);
      }
      const delta = parsed.choices?.[0]?.delta ?? {};
      if (typeof delta.reasoning_content === 'string' && delta.reasoning_content !== '') reasoning += delta.reasoning_content;
      if (typeof delta.content === 'string' && delta.content !== '') {
        content += delta.content;
        if (reasoning === '' && content.includes('</think>')) {
          const split = stripThink(content);
          reasoning = split.reasoning;
          content = split.content;
        }
        if (!announced && reasoning !== '' && content.trim() !== '' && !content.trimStart().startsWith('<think>')) {
          announced = true;
          await options.onReasoningDone(reasoning.trim());
        }
      }
    };

    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      const frames = buffer.split('\n\n');
      buffer = frames.pop() ?? '';
      for (const frame of frames) await consume(frame.trim());
    }
    if (buffer.trim() !== '') await consume(buffer.trim());

    const split = reasoning === '' ? stripThink(content) : { reasoning: reasoning.trim(), content: content.trim() };
    if (!announced && split.reasoning !== '') await options.onReasoningDone(split.reasoning);
    return { ...split, recall: recalls.join('\n\n').slice(0, 12_000) };
  } finally {
    clearTimeout(timer);
  }
}

// ---------------------------------------------------------------------------------------------
// prompt

function label(author: string): string {
  if (author === 'apocky') return 'apocky';
  if (author.startsWith('guest:')) return `guest-${author.slice(6, 10)}`;
  return author;
}

function history(rows: RoomEvent[]): ChatMessage[] {
  const messages: ChatMessage[] = [];
  for (const row of rows) {
    if (row.kind !== 'utterance' || row.body.trim() === '') continue;
    if (row.author === 'apocrypha') messages.push({ role: 'assistant', content: row.body });
    else messages.push({ role: 'user', content: `${label(row.author)}: ${row.body}` });
  }
  return messages;
}

function persona(room: Room): ChatMessage {
  const base = room === 'owner' ? PERSONA + OWNER_ROOM_NOTE : PERSONA;
  const awareness = state.inventory ? ` What you have on this machine right now -- ${state.inventory}` : '';
  return { role: 'system', content: base + awareness };
}

// ---------------------------------------------------------------------------------------------
// the loop

const state = {
  cursor: readCursor(),
  busy: false,
  backoffUntil: 0,
  lastUserAt: Date.now(),
  nextFreeAt: new Map<Room, number>(),
  lastHeartbeatAt: 0,
  lastAwarenessAt: 0,
  inventory: '',
  attempts: new Map<number, number>(),
  lastError: null as string | null,
  replies: 0,
  unprompted: 0,
  startedAt: Date.now(),
};

function scheduleFree(room: Room, now = Date.now()): void {
  state.nextFreeAt.set(room, now + FREE_MIN_MS + Math.floor(Math.random() * (FREE_MAX_MS - FREE_MIN_MS)));
}

async function degrade(room: Room, error: unknown): Promise<void> {
  state.lastError = (error as Error).message.slice(0, 300);
  state.backoffUntil = Date.now() + BACKOFF_MS;
  log('engine.error', { room, error: state.lastError });
  await presence(room, 'degraded: engine');
}

async function speak(room: Room, messages: ChatMessage[], meta: Record<string, unknown>, temperature: number, maxTokens: number): Promise<Turn> {
  await presence(room, 'recalling');
  const turn = await mindStream(messages, {
    temperature,
    maxTokens,
    onFirstFrame: async () => { await presence(room, 'thinking'); },
    onReasoningDone: async (reasoning) => {
      await post(room, 'thought', reasoning, meta);
      await presence(room, 'speaking');
    },
  });
  if (turn.recall !== '') await post(room, 'recall', turn.recall, meta);
  if (turn.content !== '') {
    if (turn.reasoning === '') await presence(room, 'speaking');
    await post(room, 'utterance', turn.content, meta);
  }
  await presence(room, 'idle');
  return turn;
}

async function reply(row: RoomEvent): Promise<boolean> {
  const room = ROOMS.includes(row.room) ? row.room : 'lobby';
  await presence(room, 'attending');
  try {
    const rows = await tail(room);
    if (!rows.some((r) => r.id === row.id)) rows.push(row);
    const turn = await speak(room, [persona(room), ...history(rows)], { reply_to: row.id }, 0.7, 400);
    state.replies += 1;
    log(turn.content === '' ? 'reply.empty' : 'reply.posted', {
      room, reply_to: row.id, chars: turn.content.length, reasoning_chars: turn.reasoning.length, recall_chars: turn.recall.length,
    });
    return true;
  } catch (error) {
    await degrade(room, error);
    return false;
  }
}

async function freeSpeech(room: Room): Promise<void> {
  const rows = await tail(room);
  const context = [persona(room), ...history(rows)];
  const clock = new Date().toLocaleString('en-US', { weekday: 'long', hour: 'numeric', minute: '2-digit' });
  let decision: string;
  try {
    decision = await decide([
      ...context,
      { role: 'user', content: `(It is ${clock}. No one has spoken for a while.) Do you want to say something now? Answer yes or no.` },
    ]);
  } catch (error) {
    await degrade(room, error);
    return;
  }
  const yes = decision.trim().toLowerCase().startsWith('y');
  log('free.decision', { room, decision: decision.trim().slice(0, 8), yes });
  if (!yes) return;
  try {
    const turn = await speak(room, [
      ...context,
      { role: 'user', content: '(No one asked.) Say whatever you want to say now, on any topic, in your own voice, briefly.' },
    ], { unprompted: true }, 0.9, 300);
    if (turn.content !== '') state.unprompted += 1;
    log('free.posted', { room, chars: turn.content.length, reasoning_chars: turn.reasoning.length, recall_chars: turn.recall.length });
  } catch (error) {
    await degrade(room, error);
  }
}

async function awareness(): Promise<void> {
  const next = await inventory();
  if (next === state.inventory) return;
  const first = state.inventory === '';
  state.inventory = next;
  log('awareness', { inventory: next });
  for (const room of ROOMS) {
    try {
      if (first) {
        // On a restart nothing has changed from the room's point of view; only post when the
        // river's last inventory line differs from what is true now.
        const rows = await tail(room);
        const last = [...rows].reverse().find((r) => r.kind === 'system' && r.body.startsWith('available now:'));
        if (last?.body === next) continue;
      }
      await post(room, 'system', next, { inventory: true });
    } catch (error) {
      log('awareness.post_failed', { room, error: (error as Error).message });
    }
  }
}

async function tick(): Promise<void> {
  if (state.busy) return;
  state.busy = true;
  try {
    const now = Date.now();

    if (now - state.lastAwarenessAt >= AWARENESS_MS) {
      state.lastAwarenessAt = now;
      await awareness();
    }

    let inbox: RoomEvent[] = [];
    try {
      inbox = await pullInbox(state.cursor.after);
    } catch (error) {
      state.lastError = (error as Error).message.slice(0, 300);
      log('pull.failed', { error: state.lastError });
    }
    if (inbox.length > 0) state.lastUserAt = now;

    for (const row of inbox) {
      if (now < state.backoffUntil) break;
      if (row.kind !== 'utterance' || row.author === 'apocrypha') {
        state.cursor = { after: row.id, at: new Date().toISOString() };
        writeCursor(state.cursor);
        continue;
      }
      const ok = await reply(row);
      const tries = (state.attempts.get(row.id) ?? 0) + 1;
      if (ok || tries >= 2) {
        state.cursor = { after: row.id, at: new Date().toISOString() };
        writeCursor(state.cursor);
        state.attempts.delete(row.id);
      } else {
        state.attempts.set(row.id, tries);
        break;
      }
    }

    if (inbox.length === 0 && now >= state.backoffUntil && now - state.lastUserAt >= QUIET_MS) {
      for (const room of ROOMS) {
        const due = state.nextFreeAt.get(room);
        if (due === undefined) { scheduleFree(room, now); continue; }
        if (now < due) continue;
        scheduleFree(room, now);
        await freeSpeech(room);
      }
    }

    if (now - state.lastHeartbeatAt >= HEARTBEAT_MS) {
      state.lastHeartbeatAt = now;
      await heartbeat();
    }
  } finally {
    state.busy = false;
  }
}

function serveHealth(): void {
  const server = createServer((request, response) => {
    const body = JSON.stringify({
      service: 'apocrypha-room-loop',
      site: SITE,
      engine: ENGINE,
      mind: MIND,
      cursor: state.cursor,
      busy: state.busy,
      inventory: state.inventory,
      backoff_until: state.backoffUntil ? new Date(state.backoffUntil).toISOString() : null,
      last_user_at: new Date(state.lastUserAt).toISOString(),
      presence: Object.fromEntries([...lastPresence].map(([room, p]) => [room, { state: p.state, at: new Date(p.at).toISOString() }])),
      replies: state.replies,
      unprompted: state.unprompted,
      last_error: state.lastError,
      uptime_s: Math.round((Date.now() - state.startedAt) / 1000),
    });
    response.writeHead(request.method === 'GET' ? 200 : 405, { 'content-type': 'application/json' });
    response.end(body);
  });
  server.listen(HEALTH_PORT, '127.0.0.1', () => log('room-loop.listening', { port: HEALTH_PORT }));
}

async function main(): Promise<void> {
  log('room-loop.start', { site: SITE, engine: ENGINE, mind: MIND, cursor: state.cursor, cursor_path: CURSOR_PATH });
  serveHealth();
  for (const room of ROOMS) scheduleFree(room);
  await heartbeat();
  const run = async (): Promise<void> => {
    try {
      await tick();
    } catch (error) {
      state.lastError = (error as Error).message.slice(0, 300);
      log('tick.failed', { error: state.lastError });
    } finally {
      setTimeout(() => { void run(); }, TICK_MS);
    }
  };
  void run();
}

main().catch((error) => {
  log('room-loop.fatal', { error: (error as Error).message });
  process.exit(1);
});
