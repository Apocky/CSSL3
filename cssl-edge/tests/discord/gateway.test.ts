/**
 * The Discord gateway state machine, exercised over a REAL WebSocket against a loopback server
 * that speaks the real protocol.
 *
 * These are the paths that cannot be checked by reading: heartbeat jitter, resume-versus-identify,
 * which close codes are fatal, and zombie detection. Every one of them only happens against a live
 * socket, and every one of them is what makes a bot look "randomly broken" weeks later.
 *
 * No bot token is involved anywhere. The fake gateway accepts any string.
 */
import {
  DiscordGateway,
  FATAL_CLOSE_CODES,
  OP,
  TEXT_INTENTS,
  INTENTS,
} from '@/scripts/apocrypha-discord/gateway';
import { FakeGateway } from './fake-gateway';

function assert(cond: boolean, message: string): asserts cond {
  if (!cond) throw new Error('assert failed: ' + message);
}

const sleep = (ms: number): Promise<void> => new Promise((r) => setTimeout(r, ms));

/** Poll until a condition holds, so tests wait on STATE rather than on a guessed duration. */
async function until(what: string, cond: () => boolean, timeoutMs = 4_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (cond()) return;
    await sleep(10);
  }
  throw new Error('timed out waiting for: ' + what);
}

interface Harness {
  gw: FakeGateway;
  client: DiscordGateway;
  logs: { event: string; fields: Record<string, string | number> }[];
  fatals: { code: number; explanation: string }[];
  events: { type: string; data: Record<string, unknown> }[];
  stop: () => Promise<void>;
}

async function harness(
  fakeOpts: ConstructorParameters<typeof FakeGateway>[0] = {},
  clientOpts: Partial<Parameters<typeof DiscordGateway.prototype.constructor>[0]> = {},
): Promise<Harness> {
  const gw = new FakeGateway({ heartbeatInterval: 200, ...fakeOpts });
  const url = await gw.listen();
  const logs: Harness['logs'] = [];
  const fatals: Harness['fatals'] = [];
  const events: Harness['events'] = [];
  const client = new DiscordGateway({
    token: 'not-a-real-token',
    url,
    maxBackoffMs: 40,
    random: () => 0,
    log: (event, fields) => logs.push({ event, fields: fields ?? {} }),
    onFatal: (code, explanation) => fatals.push({ code, explanation }),
    onEvent: (type, data) => events.push({ type, data }),
    ...(clientOpts as object),
  });
  client.connect();
  return {
    gw,
    client,
    logs,
    fatals,
    events,
    stop: async () => {
      client.stop();
      await gw.stop();
    },
  };
}

async function testHandshake(): Promise<void> {
  const h = await harness();
  await until('READY', () => h.client.botId !== '');

  const conn = h.gw.latest;
  assert(conn.identify !== null, 'the client must send IDENTIFY');
  assert(conn.identify?.intents === TEXT_INTENTS, 'intents must be the declared text set');
  assert(h.client.botId === '900000000000000001', 'botId comes from READY, got ' + h.client.botId);

  // The identify carries a token; it must never reach a log line.
  const logged = JSON.stringify(h.logs);
  assert(!logged.includes('not-a-real-token'), 'the token leaked into the log');
  await h.stop();
}

async function testIntentsValue(): Promise<void> {
  // 37377. Pinned because a wrong bitfield produces close 4013 or, worse, a bot that silently
  // receives empty content forever.
  assert(TEXT_INTENTS === 37377, 'TEXT_INTENTS must be 37377, got ' + String(TEXT_INTENTS));
  assert(INTENTS.MESSAGE_CONTENT === 32768, 'MESSAGE_CONTENT is bit 15');
  assert((TEXT_INTENTS & INTENTS.MESSAGE_CONTENT) !== 0, 'message content must be requested');
}

async function testFirstHeartbeatIsJittered(): Promise<void> {
  // random() = 0 puts the first beat at 0 ms; random() = 0.5 puts it at half the interval.
  // Asserting BOTH is what proves the jitter is a real function of random() and not a constant.
  const atZero = await harness({ heartbeatInterval: 1_000 }, { random: () => 0 });
  await until('scheduled', () => atZero.logs.some((l) => l.event === 'gateway.heartbeat.scheduled'));
  const zero = atZero.logs.find((l) => l.event === 'gateway.heartbeat.scheduled');
  assert(zero?.fields.first === 0, 'random 0 must schedule the first beat at 0, got ' + String(zero?.fields.first));
  await atZero.stop();

  const atHalf = await harness({ heartbeatInterval: 1_000 }, { random: () => 0.5 });
  await until('scheduled', () => atHalf.logs.some((l) => l.event === 'gateway.heartbeat.scheduled'));
  const half = atHalf.logs.find((l) => l.event === 'gateway.heartbeat.scheduled');
  assert(half?.fields.first === 500, 'random 0.5 must schedule at half the interval, got ' + String(half?.fields.first));
  await atHalf.stop();
}

async function testHeartbeatsAreAcked(): Promise<void> {
  const h = await harness({ heartbeatInterval: 120 });
  await until('several heartbeats',
    () => (h.gw.connections[0]?.heartbeatsAt.length ?? 0) >= 3, 5_000);
  // If ACKs were being missed the client would have declared a zombie and reconnected.
  assert(!h.logs.some((l) => l.event === 'gateway.zombie_detected'), 'no zombie should be declared while ACKs flow');
  assert(h.gw.connections.length === 1, 'a healthy socket must not reconnect');
  await h.stop();
}

async function testZombieConnectionIsDetected(): Promise<void> {
  // The socket stays open at TCP level but the server stops ACKing. Without the ack check the bot
  // sits here forever looking healthy and hearing nothing.
  const h = await harness({ heartbeatInterval: 120, withholdAcks: true });
  await until('zombie detected', () => h.logs.some((l) => l.event === 'gateway.zombie_detected'), 6_000);
  const conn = h.gw.connections[0];
  await until('server observed the close frame', () => conn.closedWith !== null, 4_000);
  assert(conn.closedWith !== null, 'the client must close the dead socket');
  assert(conn.closedWith !== 1000 && conn.closedWith !== 1001,
    'close code must NOT be 1000/1001 or the session is destroyed and cannot resume; got ' + String(conn.closedWith));
  await h.stop();
}

async function testResumeAfterRecoverableClose(): Promise<void> {
  const h = await harness({ heartbeatInterval: 5_000 });
  await until('READY', () => h.client.botId !== '');
  const first = h.gw.latest;
  const messageSeq = first.dispatch('MESSAGE_CREATE', { id: 'm1', content: 'x' });
  await until('event delivered', () => h.events.length >= 1);

  // 4000 is "unknown error" -- recoverable, so the session must be resumed rather than remade.
  first.close(4000);
  await until('second connection', () => h.gw.connections.length >= 2, 6_000);
  const second = h.gw.connections[1];
  await until('resume sent', () => second.resume !== null, 4_000);

  assert(second.identify === null, 'a recoverable close must RESUME, not re-IDENTIFY');
  assert(second.resume?.session_id === first.sessionId, 'resume must carry the original session id');
  // READY is a dispatch too, so it took s=1 and this message is s=2. Asserting against the
  // value the fake actually issued keeps the test honest if that ever changes.
  assert(messageSeq === 2, 'fake numbering changed; READY should have consumed s=1');
  assert(second.resume?.seq === messageSeq,
    'resume must carry the last sequence seen (' + String(messageSeq) + '), got ' + String(second.resume?.seq));
  await h.stop();
}

async function testFatalCloseDoesNotReconnect(): Promise<void> {
  // 4014 is the one that WILL happen first in real life: MESSAGE_CONTENT is off by default in the
  // developer portal. Retrying it forever would spam Discord and tell the operator nothing.
  const h = await harness();
  await until('READY', () => h.client.botId !== '');
  h.gw.latest.close(4014);

  await until('fatal reported', () => h.fatals.length === 1, 4_000);
  assert(h.fatals[0].code === 4014, 'fatal code must be reported');
  assert(h.fatals[0].explanation.includes('privileged intent'), 'the explanation must name the actual cause');

  // Give it far longer than any backoff would need, then prove it never came back.
  await sleep(600);
  assert(h.gw.connections.length === 1, 'a fatal close must NOT reconnect; saw ' + String(h.gw.connections.length));
  await h.stop();
}

async function testEveryFatalCodeIsRefused(): Promise<void> {
  assert(FATAL_CLOSE_CODES.size === 6, 'six fatal codes are documented, found ' + String(FATAL_CLOSE_CODES.size));
  for (const code of [4004, 4010, 4011, 4012, 4013, 4014]) {
    assert(FATAL_CLOSE_CODES.has(code), 'close ' + String(code) + ' must be fatal');
  }
  // The null: a code that is NOT in the list must be treated as recoverable. Without this the
  // test above would pass just as happily if every code were marked fatal.
  for (const code of [4000, 4001, 4002, 4003, 4005, 4006, 4007, 4008, 4009]) {
    assert(!FATAL_CLOSE_CODES.has(code), 'close ' + String(code) + ' must be recoverable');
  }
}

async function testInvalidSessionForcesFreshIdentify(): Promise<void> {
  // op9 with d:false means the session is gone. Resuming it again would loop forever.
  const h = await harness({ heartbeatInterval: 5_000, invalidateSession: true });
  await until('READY', () => h.client.botId !== '');
  h.gw.latest.close(4000);

  await until('third connection', () => h.gw.connections.length >= 3, 8_000);
  const third = h.gw.connections[2];
  await until('re-identified', () => third.identify !== null, 4_000);
  assert(third.identify !== null, 'after INVALID_SESSION d:false the client must IDENTIFY afresh');
  await h.stop();
}

async function testServerRequestedReconnect(): Promise<void> {
  const h = await harness({ heartbeatInterval: 5_000 });
  await until('READY', () => h.client.botId !== '');
  h.gw.latest.send({ op: OP.RECONNECT });
  await until('reconnected', () => h.gw.connections.length >= 2, 6_000);
  await until('resumed', () => h.gw.connections[1].resume !== null, 4_000);
  assert(h.gw.connections[1].resume !== null, 'op7 must trigger a RESUME');
  await h.stop();
}

async function testEventsReachTheConsumer(): Promise<void> {
  const h = await harness({ heartbeatInterval: 5_000 });
  await until('READY', () => h.client.botId !== '');
  h.gw.latest.dispatch('MESSAGE_CREATE', { id: 'm9', content: 'hello there' });
  await until('event', () => h.events.length >= 1);
  assert(h.events[0].type === 'MESSAGE_CREATE', 'event type must be forwarded');
  assert((h.events[0].data as { content?: string }).content === 'hello there', 'payload must be forwarded intact');
  // READY and RESUMED are lifecycle, not conversation: they must not reach the message consumer.
  assert(!h.events.some((e) => e.type === 'READY'), 'READY must not be delivered as a chat event');
  await h.stop();
}

const TESTS: [string, () => Promise<void>][] = [
  ['intents bitfield is 37377', testIntentsValue],
  ['handshake identifies and reaches READY without leaking the token', testHandshake],
  ['first heartbeat is jittered as a function of random()', testFirstHeartbeatIsJittered],
  ['heartbeats are acked and a healthy socket never reconnects', testHeartbeatsAreAcked],
  ['a zombie connection is detected and closed with a resumable code', testZombieConnectionIsDetected],
  ['a recoverable close resumes rather than re-identifies', testResumeAfterRecoverableClose],
  ['every documented fatal code is fatal and nothing else is', testEveryFatalCodeIsRefused],
  ['a fatal close reports why and never reconnects', testFatalCloseDoesNotReconnect],
  ['INVALID_SESSION d:false forces a fresh identify', testInvalidSessionForcesFreshIdentify],
  ['op7 RECONNECT triggers a resume', testServerRequestedReconnect],
  ['dispatched events reach the consumer, lifecycle events do not', testEventsReachTheConsumer],
];

// tsx compiles this tree to CJS, so there is no top-level await to lean on.
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
    ? String(TESTS.length) + ' gateway tests passed'
    : String(failed) + ' of ' + String(TESTS.length) + ' gateway tests FAILED');
  if (failed > 0) process.exitCode = 1;
}

void main();
