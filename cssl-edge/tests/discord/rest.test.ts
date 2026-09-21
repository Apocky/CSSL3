/**
 * The REST half: splitting, rate limits, and not leaking the token.
 *
 * The chunking tests matter more than they look. Apocrypha writes long answers containing fenced
 * code, Discord rejects anything over 2000 characters outright, and a naive split lands in the
 * middle of a code block -- which renders the remainder as prose and makes a correct answer look
 * like a broken one.
 */
import {
  DiscordRest,
  MESSAGE_LIMIT,
  chunkMessage,
  inviteUrl,
  INVITE_PERMISSIONS,
} from '@/scripts/apocrypha-discord/rest';

function assert(cond: boolean, message: string): asserts cond {
  if (!cond) throw new Error('assert failed: ' + message);
}

interface Captured {
  url: string;
  method: string;
  headers: Record<string, string>;
  body: unknown;
}

function recorder(responses: Response[] = []): { calls: Captured[]; impl: typeof fetch } {
  const calls: Captured[] = [];
  let i = 0;
  const impl = (async (url: string | URL | Request, init?: RequestInit) => {
    calls.push({
      url: String(url),
      method: init?.method ?? 'GET',
      headers: (init?.headers ?? {}) as Record<string, string>,
      body: typeof init?.body === 'string' ? JSON.parse(init.body) : undefined,
    });
    const res = responses[i] ?? new Response('{}', { status: 200 });
    i += 1;
    return res;
  }) as unknown as typeof fetch;
  return { calls, impl };
}

function testShortMessagePassesThrough(): void {
  assert(chunkMessage('hello').length === 1, 'a short message is one chunk');
  assert(chunkMessage('  hello  ')[0] === 'hello', 'chunks are trimmed');
  assert(chunkMessage('').length === 0, 'an empty answer produces nothing to send');
  assert(chunkMessage('   \n  ').length === 0, 'whitespace only produces nothing to send');
}

function testLongMessageIsSplitUnderTheLimit(): void {
  const para = 'This is a sentence that carries a reasonable amount of text in it. ';
  const long = Array.from({ length: 120 }, () => para).join('\n\n');
  assert(long.length > MESSAGE_LIMIT * 3, 'the fixture must actually be long, got ' + String(long.length));

  const chunks = chunkMessage(long);
  assert(chunks.length >= 4, 'a very long answer needs several chunks, got ' + String(chunks.length));
  for (const [i, c] of chunks.entries()) {
    assert(c.length <= MESSAGE_LIMIT,
      'chunk ' + String(i) + ' is ' + String(c.length) + ' chars, over the ' + String(MESSAGE_LIMIT) + ' limit');
    assert(c.trim().length > 0, 'no chunk may be blank');
  }
}

function testNothingIsLost(): void {
  const long = Array.from({ length: 200 }, (_, i) => 'line ' + String(i)).join('\n');
  const rejoined = chunkMessage(long).join('\n');
  for (const probe of ['line 0', 'line 57', 'line 123', 'line 199']) {
    assert(rejoined.includes(probe), 'splitting dropped ' + probe);
  }
}

function testCodeFencesSurviveTheSplit(): void {
  // A single fenced block long enough to force a split.
  const code = Array.from({ length: 200 }, (_, i) => '  const value' + String(i) + ' = ' + String(i) + ';').join('\n');
  const text = 'Here is the change:\n\n```ts\n' + code + '\n```';
  assert(text.length > MESSAGE_LIMIT, 'the fixture must force a split');

  const chunks = chunkMessage(text);
  assert(chunks.length >= 2, 'the fixture must actually split');
  for (const [i, c] of chunks.entries()) {
    const fences = (c.match(/^```/gm) ?? []).length;
    assert(fences % 2 === 0,
      'chunk ' + String(i) + ' ends inside an open code fence (' + String(fences) + ' fence markers)');
    assert(c.length <= MESSAGE_LIMIT, 'chunk ' + String(i) + ' exceeds the limit');
  }
  assert(chunks[0].includes('```ts'), 'the first chunk keeps the language tag');
  assert(chunks[1].startsWith('```ts'), 'a continued chunk REOPENS the fence with its language');
}

function testUnsplittableRunStillFits(): void {
  // One 6000-character word: no paragraph, no line, nowhere nice to cut.
  const blob = 'A'.repeat(6_000);
  const chunks = chunkMessage(blob);
  for (const c of chunks) {
    assert(c.length <= MESSAGE_LIMIT, 'a hard cut must still respect the limit, got ' + String(c.length));
  }
  assert(chunks.join('').length === blob.length, 'a hard cut must not lose characters');
}

async function testTokenOnlyAppearsInTheAuthorizationHeader(): Promise<void> {
  const logs: string[] = [];
  const rec = recorder();
  const rest = new DiscordRest({
    token: 'super-secret-token-value',
    fetchImpl: rec.impl,
    log: (event, fields) => logs.push(event + ' ' + JSON.stringify(fields ?? {})),
  });
  await rest.sendMessage('123', 'hello');

  assert(rec.calls[0].headers.Authorization === 'Bot super-secret-token-value',
    'the token must be sent as a bot authorization header');
  assert(!JSON.stringify(logs).includes('super-secret-token-value'), 'the token leaked into a log');
  assert(!JSON.stringify(rec.calls[0].body).includes('super-secret-token-value'),
    'the token leaked into a request body');
}

async function testUserAgentIsSet(): Promise<void> {
  // Discord's Cloudflare layer rejects requests without one, and the failure looks like a
  // network problem rather than a missing header.
  const rec = recorder();
  const rest = new DiscordRest({ token: 't', fetchImpl: rec.impl });
  await rest.sendMessage('123', 'hi');
  const ua = rec.calls[0].headers['User-Agent'];
  assert(typeof ua === 'string' && ua.startsWith('DiscordBot ('),
    'a DiscordBot user agent is mandatory, got ' + String(ua));
}

async function testRepliesNeverPing(): Promise<void> {
  const rec = recorder();
  const rest = new DiscordRest({ token: 't', fetchImpl: rec.impl });
  await rest.sendMessage('c1', 'answer', { replyTo: 'm1' });
  const body = rec.calls[0].body as Record<string, unknown>;
  assert(JSON.stringify(body.message_reference) === JSON.stringify({ type: 0, message_id: 'm1' }),
    'a reply must reference the original message');
  assert(JSON.stringify(body.allowed_mentions) === JSON.stringify({ parse: [] }),
    'a reply must not ping anyone; the person is already reading the thread');
}

async function testOnlyTheFirstChunkThreads(): Promise<void> {
  const rec = recorder();
  const rest = new DiscordRest({ token: 't', fetchImpl: rec.impl });
  const long = Array.from({ length: 150 }, () => 'A sentence of some length here. ').join('\n\n');
  const sent = await rest.sendMessage('c1', long, { replyTo: 'm1' });

  assert(sent >= 2, 'the fixture must split, sent ' + String(sent));
  assert(rec.calls[0].body !== undefined, 'first call has a body');
  assert('message_reference' in (rec.calls[0].body as object), 'the first chunk threads');
  for (let i = 1; i < rec.calls.length; i += 1) {
    assert(!('message_reference' in (rec.calls[i].body as object)),
      'chunk ' + String(i) + ' must not repeat the reply chip');
  }
}

async function testRateLimitRetryUsesSeconds(): Promise<void> {
  // retry_after is SECONDS, and a float. Reading it as milliseconds makes the bot hammer
  // straight through its own backoff and earn a longer ban.
  const slept: number[] = [];
  const rec = recorder([
    new Response(JSON.stringify({ retry_after: 1.5, global: false }), {
      status: 429,
      headers: { 'retry-after': '1.5' },
    }),
    new Response('{}', { status: 200 }),
  ]);
  const rest = new DiscordRest({
    token: 't',
    fetchImpl: rec.impl,
    sleepImpl: async (ms) => { slept.push(ms); },
  });
  const sent = await rest.sendMessage('c1', 'hi');

  assert(slept.length === 1, 'the bridge must wait once, waited ' + String(slept.length) + ' times');
  assert(slept[0] === 1_500, 'retry_after 1.5 s must become 1500 ms, got ' + String(slept[0]));
  assert(sent === 1, 'the message must go out after the wait');
  assert(rec.calls.length === 2, 'the request must be retried exactly once');
}

async function testRateLimitGivesUpEventually(): Promise<void> {
  const always429 = Array.from({ length: 10 }, () => new Response(JSON.stringify({ retry_after: 0.1 }), {
    status: 429,
    headers: { 'retry-after': '0.1' },
  }));
  const rec = recorder(always429);
  const rest = new DiscordRest({
    token: 't',
    fetchImpl: rec.impl,
    sleepImpl: async () => {},
    maxRetries: 2,
  });
  const sent = await rest.sendMessage('c1', 'hi');
  assert(sent === 0, 'a permanently limited send reports zero sent');
  assert(rec.calls.length === 3, 'maxRetries 2 means 3 attempts total, got ' + String(rec.calls.length));
}

function testInviteUrlIsLeastPrivilege(): void {
  const url = inviteUrl('12345');
  assert(url.includes('client_id=12345'), 'the invite must carry the application id');
  assert(url.includes('permissions=' + INVITE_PERMISSIONS), 'the invite must carry the permissions');

  const perms = BigInt(INVITE_PERMISSIONS);
  const ADMINISTRATOR = 1n << 3n;
  const MENTION_EVERYONE = 1n << 17n;
  const MANAGE_MESSAGES = 1n << 13n;
  assert((perms & ADMINISTRATOR) === 0n, 'never request Administrator');
  assert((perms & MENTION_EVERYONE) === 0n, 'never request mention-everyone');
  assert((perms & MANAGE_MESSAGES) === 0n, 'never request manage-messages');
  // The null: it must still request what it actually needs, or the check above is vacuous.
  assert((perms & (1n << 10n)) !== 0n, 'must request VIEW_CHANNEL');
  assert((perms & (1n << 11n)) !== 0n, 'must request SEND_MESSAGES');
  assert((perms & (1n << 16n)) !== 0n, 'must request READ_MESSAGE_HISTORY');
}

const TESTS: [string, () => void | Promise<void>][] = [
  ['short messages pass through unchanged', testShortMessagePassesThrough],
  ['long messages split under the 2000 limit', testLongMessageIsSplitUnderTheLimit],
  ['splitting loses nothing', testNothingIsLost],
  ['code fences survive the split', testCodeFencesSurviveTheSplit],
  ['an unsplittable run still respects the limit', testUnsplittableRunStillFits],
  ['the token appears only in the authorization header', testTokenOnlyAppearsInTheAuthorizationHeader],
  ['a DiscordBot user agent is always sent', testUserAgentIsSet],
  ['replies never ping', testRepliesNeverPing],
  ['only the first chunk threads as a reply', testOnlyTheFirstChunkThreads],
  ['a 429 retry_after is read as seconds', testRateLimitRetryUsesSeconds],
  ['a permanent 429 gives up instead of looping', testRateLimitGivesUpEventually],
  ['the invite asks for least privilege', testInviteUrlIsLeastPrivilege],
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
    ? String(TESTS.length) + ' rest tests passed'
    : String(failed) + ' of ' + String(TESTS.length) + ' rest tests FAILED');
  if (failed > 0) process.exitCode = 1;
}

void main();
