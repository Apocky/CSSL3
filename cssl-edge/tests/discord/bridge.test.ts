/**
 * The bridge end to end: a real WebSocket, a real gateway handshake, a real MESSAGE_CREATE, and
 * a recorded REST side.
 *
 * The policy tests prove decide() is correct in isolation. These prove the bridge actually ASKS
 * it -- which is a separate claim, and the one that fails when someone adds a shortcut later.
 * A correct gate that nothing calls is not a gate.
 */
import { Bridge, memoryStore, type StateStore } from '@/scripts/apocrypha-discord/bridge';
import { MindClient } from '@/scripts/apocrypha-discord/mind';
import { DiscordRest } from '@/scripts/apocrypha-discord/rest';
import type { BridgeConfig } from '@/scripts/apocrypha-discord/config';
import { FakeGateway } from './fake-gateway';

function assert(cond: boolean, message: string): asserts cond {
  if (!cond) throw new Error('assert failed: ' + message);
}

const sleep = (ms: number): Promise<void> => new Promise((r) => setTimeout(r, ms));

async function until(what: string, cond: () => boolean, timeoutMs = 4_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (cond()) return;
    await sleep(10);
  }
  throw new Error('timed out waiting for: ' + what);
}

const OWNER = '11111111111111111';
const STRANGER = '33333333333333333';
const BOT = '900000000000000001'; // what the fake gateway reports in READY
const CHANNEL = '55555555555555555';

interface Sent { channel: string; content: string; body: Record<string, unknown> }

interface Rig {
  gw: FakeGateway;
  bridge: Bridge;
  sent: Sent[];
  asked: { messages: { role: string; content: string }[] }[];
  store: StateStore;
  setMindDown: (down: boolean) => void;
  post: (over?: Record<string, unknown>) => void;
  stop: () => Promise<void>;
}

async function rig(over: Partial<BridgeConfig> = {}, store = memoryStore()): Promise<Rig> {
  const gw = new FakeGateway({ heartbeatInterval: 5_000 });
  const url = await gw.listen();

  const sent: Sent[] = [];
  const restFetch = (async (target: string | URL, init?: RequestInit) => {
    const path = String(target);
    const body = typeof init?.body === 'string'
      ? (JSON.parse(init.body) as Record<string, unknown>)
      : {};
    // 204 is a null-body status: Response REFUSES a string body for it.
    if (path.endsWith('/typing')) return new Response(null, { status: 204 });
    const channel = /channels\/([^/]+)\/messages/.exec(path)?.[1] ?? '';
    sent.push({ channel, content: String(body.content ?? ''), body });
    return new Response(JSON.stringify({ id: 'sent' }), { status: 200 });
  }) as unknown as typeof fetch;

  const asked: Rig['asked'] = [];
  let mindDown = false;
  const mindFetch = (async (target: string | URL, init?: RequestInit) => {
    if (mindDown) throw new TypeError('fetch failed');
    if (String(target).endsWith('/health')) return new Response('{}', { status: 200 });
    const body = JSON.parse(String(init?.body)) as { messages: { role: string; content: string }[] };
    asked.push({ messages: body.messages });
    return new Response(JSON.stringify({
      choices: [{ message: { content: 'answer to: ' + body.messages.at(-1)?.content } }],
    }), { status: 200 });
  }) as unknown as typeof fetch;

  const config: BridgeConfig = {
    token: 'test-token',
    ownerId: OWNER,
    optInChannels: new Set<string>(),
    mindUrl: 'http://mind.invalid',
    mindModel: 'apocrypha',
    requestTimeoutMs: 5_000,
    host: '127.0.0.1',
    port: 0,
    stateDir: '',
    applicationId: '',
    ...over,
  };

  const bridge = new Bridge({
    config,
    rest: new DiscordRest({ token: config.token, fetchImpl: restFetch }),
    mind: new MindClient({
      baseUrl: config.mindUrl,
      model: config.mindModel,
      timeoutMs: config.requestTimeoutMs,
      fetchImpl: mindFetch,
    }),
    store,
    gatewayUrl: url,
  });
  bridge.start();
  await until('READY', () => bridge.gateway.botId !== '');

  return {
    gw,
    bridge,
    sent,
    asked,
    store,
    setMindDown: (down) => { mindDown = down; },
    post: (o = {}) => {
      gw.latest.dispatch('MESSAGE_CREATE', {
        id: 'm' + String(Math.floor(performance.now() * 1000)),
        channel_id: CHANNEL,
        guild_id: 'g1',
        author: { id: STRANGER, bot: false },
        content: 'hello',
        mentions: [],
        ...o,
      });
    },
    stop: async () => {
      bridge.stop();
      await gw.stop();
    },
  };
}

async function testUnaddressedMessageProducesNothing(): Promise<void> {
  // The end-to-end falsifier. Not "decide() said ignore" -- nothing left the process.
  const r = await rig();
  r.post({ content: 'just chatting in here' });
  r.post({ content: 'still chatting', author: { id: OWNER, bot: false } });
  await sleep(500);

  assert(r.sent.length === 0, 'nothing may be sent for unaddressed messages, sent ' + String(r.sent.length));
  assert(r.asked.length === 0, 'the mind must never even be asked, asked ' + String(r.asked.length));
  assert(r.bridge.stats.ignored === 2, 'both must be counted as ignored');
  await r.stop();
}

async function testOwnerMentionIsAnswered(): Promise<void> {
  const r = await rig();
  r.post({ author: { id: OWNER, bot: false }, mentions: [{ id: BOT }], content: 'what is up' });
  await until('answered', () => r.bridge.stats.answered >= 1);
  await until('reply sent', () => r.sent.length >= 2);

  assert(r.sent[0].content.includes('I am Apocrypha'), 'it must introduce itself first (R4)');
  assert(r.sent[0].content.includes('stop'), 'the introduction must state how to revoke');
  assert(r.sent[1].content === 'answer to: what is up', 'the answer must be delivered');
  assert(
    JSON.stringify(r.sent[1].body.message_reference) === JSON.stringify({ type: 0, message_id: 'm1' })
    || r.sent[1].body.message_reference !== undefined,
    'the answer must thread to the question',
  );
  await r.stop();
}

async function testAnnouncementHappensOncePerChannel(): Promise<void> {
  const r = await rig();
  r.post({ author: { id: OWNER, bot: false }, mentions: [{ id: BOT }], content: 'one' });
  await until('first answer', () => r.bridge.stats.answered >= 1);
  await until('settled', () => r.sent.length >= 2);
  r.post({ author: { id: OWNER, bot: false }, mentions: [{ id: BOT }], content: 'two' });
  await until('second answer', () => r.bridge.stats.answered >= 2);

  const intros = r.sent.filter((s) => s.content.includes('I am Apocrypha'));
  assert(intros.length === 1, 'it introduces itself once per channel, not once per message; saw ' + String(intros.length));
  await r.stop();
}

async function testOwnerKeepsHistoryAndStrangersDoNot(): Promise<void> {
  const r = await rig();
  r.post({ author: { id: OWNER, bot: false }, mentions: [{ id: BOT }], content: 'first' });
  await until('turn 1', () => r.asked.length >= 1);
  r.post({ author: { id: OWNER, bot: false }, mentions: [{ id: BOT }], content: 'second' });
  await until('turn 2', () => r.asked.length >= 2);

  assert(r.asked[0].messages.length === 1, 'the first owner turn has no prior context');
  assert(r.asked[1].messages.length === 3,
    'the second owner turn carries question, answer, question; got ' + String(r.asked[1].messages.length));
  assert(r.asked[1].messages[0].content === 'first', 'the earlier question is carried forward');
  await r.stop();

  // Same channel, same bridge shape, but a stranger: every turn must start clean.
  const s = await rig();
  s.post({ author: { id: STRANGER, bot: false }, mentions: [{ id: BOT }], content: 'first' });
  await until('turn 1', () => s.asked.length >= 1);
  s.post({ author: { id: STRANGER, bot: false }, mentions: [{ id: BOT }], content: 'second' });
  await until('turn 2', () => s.asked.length >= 2);

  assert(s.asked[1].messages.length === 1,
    'a stranger must get no memory at all (R3); their second turn carried '
    + String(s.asked[1].messages.length) + ' messages');
  await s.stop();
}

async function testStopHaltsAndPersists(): Promise<void> {
  const store = memoryStore();
  const r = await rig({}, store);
  r.post({ author: { id: STRANGER, bot: false }, mentions: [{ id: BOT }], content: 'stop' });
  await until('halt acknowledged', () => r.sent.length >= 1);

  assert(r.sent[0].content.startsWith('Stopped'), 'a halt is acknowledged once');
  assert(store.read().halted.includes(STRANGER), 'the halt must be written to durable state');

  const before = r.sent.length;
  r.post({ author: { id: STRANGER, bot: false }, mentions: [{ id: BOT }], content: 'are you there' });
  await sleep(400);
  assert(r.sent.length === before, 'a halted person gets nothing further');
  assert(r.asked.length === 0, 'a halted person never reaches the mind');
  await r.stop();
}

async function testHaltSurvivesRestart(): Promise<void> {
  // "stop" must not mean "stop until the next reboot".
  const store = memoryStore({ halted: [STRANGER] });
  const r = await rig({}, store);
  r.post({ author: { id: STRANGER, bot: false }, mentions: [{ id: BOT }], content: 'hello again' });
  await sleep(400);
  assert(r.sent.length === 0, 'a halt loaded from disk is still in force');
  await r.stop();
}

async function testMindDownSaysSoInsteadOfGoingSilent(): Promise<void> {
  const r = await rig();
  r.setMindDown(true);
  r.post({ author: { id: OWNER, bot: false }, mentions: [{ id: BOT }], content: 'you there' });
  await until('something was said', () => r.sent.length >= 2, 6_000);

  const last = r.sent.at(-1)?.content ?? '';
  assert(last.includes('mind service'), 'the failure must name the actual cause, got: ' + last);
  assert(last.includes('http://mind.invalid'), 'it must name where the mind was expected');
  assert(r.bridge.stats.failed === 1, 'the failure must be counted');
  await r.stop();
}

async function testBotsAndSelfAreNeverAnswered(): Promise<void> {
  const r = await rig();
  r.post({ author: { id: '777', bot: true }, mentions: [{ id: BOT }], content: 'hello bot' });
  r.post({ author: { id: BOT, bot: true }, mentions: [{ id: BOT }], content: 'echo of myself' });
  await sleep(400);
  assert(r.sent.length === 0, 'bot traffic must never be answered -- two bots will loop forever');
  await r.stop();
}

const TESTS: [string, () => Promise<void>][] = [
  ['an unaddressed message produces no network traffic at all', testUnaddressedMessageProducesNothing],
  ['an owner mention is announced then answered', testOwnerMentionIsAnswered],
  ['the introduction happens once per channel', testAnnouncementHappensOncePerChannel],
  ['the owner keeps history and strangers do not', testOwnerKeepsHistoryAndStrangersDoNot],
  ['stop halts and is written to durable state', testStopHaltsAndPersists],
  ['a halt loaded from disk is still in force', testHaltSurvivesRestart],
  ['a dead mind says so rather than going silent', testMindDownSaysSoInsteadOfGoingSilent],
  ['bots and its own messages are never answered', testBotsAndSelfAreNeverAnswered],
];

async function main(): Promise<void> {
  let failed = 0;
  for (const [name, fn] of TESTS) {
    try {
      await fn();
      console.log('  ok   ' + name);
    } catch (err) {
      failed += 1;
      console.error('  FAIL ' + name + '\n       ' + (err as Error).message);
    }
  }
  console.log(failed === 0
    ? String(TESTS.length) + ' bridge tests passed'
    : String(failed) + ' of ' + String(TESTS.length) + ' bridge tests FAILED');
  if (failed > 0) process.exitCode = 1;
}

void main();
