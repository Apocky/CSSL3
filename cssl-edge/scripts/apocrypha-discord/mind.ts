/**
 * The responder: how a Discord message becomes an Apocrypha answer.
 *
 * It calls the LOCAL MIND SERVICE (scripts/apocrypha-mind/server.ts), not the engine and not the
 * cloud job queue. That choice is the whole point of this file, so it is worth writing down:
 *
 *   - The engine at 19128 is a raw language model. Talking to it directly would give Discord a
 *     SECOND Apocrypha with no persona and no memory -- a different entity wearing the same name.
 *   - The cloud job queue is how the website asks, but the worker is pull-only: it claims jobs
 *     from www.apocky.com and accepts nothing inbound. There is no way to hand it a message.
 *   - The mind service is the one surface that takes "here is a message" and returns "here is
 *     Apocrypha", assembling persona plus admitted memory before it forwards to the engine.
 *
 * So Discord and the website reach the same mind. That was the requirement.
 *
 * WHEN THE MIND IS DOWN, this says so. It does not quietly fall back to the bare engine, because
 * a fallback that answers in a different voice with no memory is worse than an honest silence --
 * it looks like Apocrypha having a bad day rather than a service being off.
 */

export interface MindTurn {
  role: 'system' | 'user' | 'assistant';
  content: string;
}

export interface MindAnswer {
  ok: boolean;
  text: string;
  /** Present when ok is false: a short operator-facing reason, safe to log. */
  failure?: string;
  durationMs: number;
}

export interface MindOptions {
  baseUrl: string;
  model: string;
  timeoutMs: number;
  fetchImpl?: typeof fetch;
}

/**
 * What the person sees when the mind service is not running. It names the actual cause, because
 * "something went wrong" would send Apocky looking in the wrong place.
 */
const DOWN_MESSAGE =
  'My mind service is not answering, so there is nothing behind this bridge right now. '
  + 'It lives at %URL%. Everything else here is working -- this is the one piece that is off.';

const TIMEOUT_MESSAGE =
  'I did not finish that one in time (%SECONDS%s). The engine may be loading a model, or the '
  + 'question may be long enough to need more room than this bridge allows.';

export class MindClient {
  private readonly fetchImpl: typeof fetch;

  constructor(private readonly opts: MindOptions) {
    this.fetchImpl = opts.fetchImpl ?? fetch;
  }

  /** Is the mind service up? Used by the bridge's own /health so a probe shows the real cause. */
  async healthy(): Promise<boolean> {
    try {
      const res = await this.fetchImpl(this.opts.baseUrl + '/health', {
        signal: AbortSignal.timeout(3_000),
      });
      return res.ok;
    } catch {
      return false;
    }
  }

  async ask(turns: readonly MindTurn[]): Promise<MindAnswer> {
    const started = Date.now();
    let res: Response;
    try {
      res = await this.fetchImpl(this.opts.baseUrl + '/v1/chat/completions', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          model: this.opts.model,
          messages: turns,
          stream: false,
        }),
        signal: AbortSignal.timeout(this.opts.timeoutMs),
      });
    } catch (err) {
      const timedOut = (err as Error).name === 'TimeoutError'
        || (err as Error).name === 'AbortError';
      return {
        ok: false,
        durationMs: Date.now() - started,
        failure: timedOut ? 'mind_timeout' : 'mind_unreachable',
        text: timedOut
          ? TIMEOUT_MESSAGE.replace('%SECONDS%', String(Math.round(this.opts.timeoutMs / 1000)))
          : DOWN_MESSAGE.replace('%URL%', this.opts.baseUrl),
      };
    }

    if (!res.ok) {
      return {
        ok: false,
        durationMs: Date.now() - started,
        failure: 'mind_http_' + String(res.status),
        text: 'My mind service answered with HTTP ' + String(res.status)
          + '. That is a local service problem, not a question I refused.',
      };
    }

    let text: string;
    try {
      const body = (await res.json()) as {
        choices?: { message?: { content?: string } }[];
      };
      text = (body.choices?.[0]?.message?.content ?? '').trim();
    } catch {
      return {
        ok: false,
        durationMs: Date.now() - started,
        failure: 'mind_bad_json',
        text: 'My mind service returned something I could not parse.',
      };
    }

    if (!text) {
      // An empty 200 is its own failure mode, and it has bitten this project before: a silent
      // reply reads as "ignored you" when it actually means the engine produced nothing.
      return {
        ok: false,
        durationMs: Date.now() - started,
        failure: 'mind_empty',
        text: 'My mind service returned an empty answer. That is a fault on my side, not a refusal.',
      };
    }

    return { ok: true, text, durationMs: Date.now() - started };
  }
}
