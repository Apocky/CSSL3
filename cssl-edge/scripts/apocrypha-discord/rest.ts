/**
 * The Discord REST half: sending, typing indicators, and the two things that bite.
 *
 * BITE ONE -- the 2000 character limit. Discord rejects a longer message outright; there is no
 * documented auto-split. Apocrypha answers are frequently longer than that and frequently contain
 * fenced code, so chunkMessage() splits on real boundaries and REOPENS a code fence it had to cut
 * through. Splitting mid-fence renders the second half as prose, which for a coding assistant is
 * the difference between an answer and a mess.
 *
 * BITE TWO -- rate limits. Discord returns 429 with retry_after in SECONDS (a float), while the
 * X-RateLimit-Reset-After header is also seconds. Treating either as milliseconds produces a bot
 * that hammers straight through its own backoff. The global limit is 50 requests/second; per-route
 * buckets are advertised in headers. The widely repeated "5 messages per 5 seconds per channel"
 * is NOT in the documentation, so it is not hardcoded here -- the headers are obeyed instead.
 *
 * The User-Agent is mandatory. Discord's Cloudflare layer rejects requests without a
 * `DiscordBot (url, version)` agent, and the failure looks like a network problem rather than a
 * missing header.
 */

const API = 'https://discord.com/api/v10';
const USER_AGENT = 'DiscordBot (https://apocky.com, 1.0)';

/** Discord's hard cap on a single message. */
export const MESSAGE_LIMIT = 2000;

export type LogFn = (event: string, fields?: Record<string, string | number>) => void;

export interface RestOptions {
  token: string;
  log?: LogFn;
  fetchImpl?: typeof fetch;
  /** Injected in tests so retry paths do not actually sleep. */
  sleepImpl?: (ms: number) => Promise<void>;
  maxRetries?: number;
}

const FENCE = /^```/;

/**
 * Split a reply into Discord-sized pieces.
 *
 * Preference order for a split point: paragraph break, then line break, then a hard cut. A hard
 * cut is only reached by a single unbroken 2000-character run, which in practice means a base64
 * blob or a minified file.
 *
 * Fence state is tracked across the split so a chunk never ends inside an open code fence.
 */
export function chunkMessage(text: string, limit = MESSAGE_LIMIT): string[] {
  const body = text.trim();
  if (!body) return [];
  if (body.length <= limit) return [body];

  const chunks: string[] = [];
  let rest = body;
  let openFence: string | null = null;

  while (rest.length > 0) {
    // Room for the fence we may have to re-open at the top of the NEXT chunk and close at the
    // bottom of this one. 8 characters covers "```\n" twice.
    const budget = limit - (openFence ? openFence.length + 4 : 0) - 4;
    if (rest.length <= (openFence ? budget : limit)) {
      chunks.push(openFence ? openFence + '\n' + rest : rest);
      break;
    }

    const window = rest.slice(0, openFence ? budget : limit - 4);
    let cut = window.lastIndexOf('\n\n');
    if (cut < window.length * 0.4) cut = window.lastIndexOf('\n');
    if (cut < window.length * 0.4) cut = window.length;

    let piece = rest.slice(0, cut);
    rest = rest.slice(cut).replace(/^\n+/, '');

    const prefix = openFence ? openFence + '\n' : '';
    // Scanned from scratch, NOT from the previous state: `prefix` already re-opens the
    // fence, so seeding with the old state made that re-open read as a CLOSE and the
    // chunk went out with an unterminated fence. Observed, not theorised -- chunk 1 of
    // the fixture came back with exactly one fence marker.
    openFence = fenceStateAfter(prefix + piece);
    if (openFence) piece = piece.replace(/\n+$/, '') + '\n```';
    chunks.push(prefix + piece);
  }
  return chunks.filter((c) => c.trim().length > 0);
}

/** The fence opener still in effect at the end of this self-contained text, or null. */
function fenceStateAfter(text: string): string | null {
  let open: string | null = null;
  for (const line of text.split('\n')) {
    if (!FENCE.test(line)) continue;
    // A bare ``` closes an open fence; anything else opens one (carrying its language tag).
    if (open) open = null;
    else open = line.trimEnd();
  }
  return open;
}

export interface SendOptions {
  /** Makes the message a threaded reply to this message id. */
  replyTo?: string;
}

export class DiscordRest {
  private readonly log: LogFn;
  private readonly fetchImpl: typeof fetch;
  private readonly sleepImpl: (ms: number) => Promise<void>;

  constructor(private readonly opts: RestOptions) {
    this.log = opts.log ?? (() => {});
    this.fetchImpl = opts.fetchImpl ?? fetch;
    this.sleepImpl = opts.sleepImpl
      ?? ((ms) => new Promise((resolve) => setTimeout(resolve, ms)));
  }

  private headers(): Record<string, string> {
    return {
      // Never logged. The only place the token is allowed to appear is this header.
      Authorization: 'Bot ' + this.opts.token,
      'Content-Type': 'application/json',
      'User-Agent': USER_AGENT,
    };
  }

  /**
   * One request, with 429 handling. Returns the Response; the caller decides what to do with a
   * non-2xx that is not a rate limit, because a 403 on one channel should not stop the bridge.
   */
  private async request(method: string, path: string, body?: unknown): Promise<Response> {
    const max = this.opts.maxRetries ?? 3;
    for (let attempt = 0; ; attempt += 1) {
      const res = await this.fetchImpl(API + path, {
        method,
        headers: this.headers(),
        body: body === undefined ? undefined : JSON.stringify(body),
      });
      if (res.status !== 429) return res;
      if (attempt >= max) {
        this.log('rest.rate_limited.giving_up', { path, attempts: attempt + 1 });
        return res;
      }
      // retry_after is SECONDS, and a float. Multiply, do not assume milliseconds.
      const waitMs = await retryAfterMs(res);
      this.log('rest.rate_limited', { path, wait_ms: waitMs, attempt: attempt + 1 });
      await this.sleepImpl(waitMs);
    }
  }

  /** Posts a reply, split across as many messages as the 2000-char limit requires. */
  async sendMessage(channelId: string, content: string, opts: SendOptions = {}): Promise<number> {
    const chunks = chunkMessage(content);
    let sent = 0;
    for (const [index, chunk] of chunks.entries()) {
      const body: Record<string, unknown> = { content: chunk };
      // Only the FIRST chunk threads to the original; the rest would spam the reply chip.
      if (opts.replyTo && index === 0) {
        body.message_reference = { type: 0, message_id: opts.replyTo };
        // Never ping on a reply. The person is already looking at the thread.
        body.allowed_mentions = { parse: [] };
      } else {
        body.allowed_mentions = { parse: [] };
      }
      const res = await this.request('POST', '/channels/' + channelId + '/messages', body);
      if (!res.ok) {
        this.log('rest.send_failed', { channel: channelId, status: res.status, chunk: index });
        break;
      }
      sent += 1;
    }
    return sent;
  }

  /** The "Apocrypha is typing..." indicator. Expires after 10 s, so it is re-sent while thinking. */
  async triggerTyping(channelId: string): Promise<boolean> {
    const res = await this.request('POST', '/channels/' + channelId + '/typing');
    if (!res.ok) this.log('rest.typing_failed', { channel: channelId, status: res.status });
    return res.ok;
  }
}

async function retryAfterMs(res: Response): Promise<number> {
  const header = res.headers.get('retry-after');
  const headerSeconds = header === null ? NaN : Number(header);
  if (Number.isFinite(headerSeconds) && headerSeconds >= 0) {
    return Math.ceil(headerSeconds * 1000);
  }
  try {
    const parsed = (await res.clone().json()) as { retry_after?: number };
    if (typeof parsed.retry_after === 'number') return Math.ceil(parsed.retry_after * 1000);
  } catch {
    /* an empty or non-JSON 429 body is allowed; fall through to the default */
  }
  return 1_000;
}

/**
 * The invite URL. Printed at startup so the setup step is one click rather than a documentation
 * hunt.
 *
 * Permissions 274877975552 = VIEW_CHANNEL (1<<10) | SEND_MESSAGES (1<<11)
 *                          | READ_MESSAGE_HISTORY (1<<16) | SEND_MESSAGES_IN_THREADS (1<<38).
 * (1024 + 2048 + 65536 + 274877906944. The value is asserted in rest.test.ts rather than trusted
 * to this comment, which was wrong on first writing.)
 * Deliberately minimal: no manage, no mention-everyone, no administrator.
 */
export const INVITE_PERMISSIONS = String(
  (1 << 10) + (1 << 11) + (1 << 16) + 2 ** 38,
);

export function inviteUrl(applicationId: string): string {
  return 'https://discord.com/api/oauth2/authorize'
    + '?client_id=' + encodeURIComponent(applicationId)
    + '&scope=bot%20applications.commands'
    + '&permissions=' + INVITE_PERMISSIONS;
}
