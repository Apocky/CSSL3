/**
 * A Discord gateway client with no dependencies.
 *
 * Node 25 ships a WHATWG WebSocket and fetch, so discord.js and ws buy nothing here except a
 * dependency this project's policy says to justify before adding. What they would normally buy is
 * the reconnect state machine, which is the part that actually matters, so it is written out
 * below rather than trusted to a library nobody here would read.
 *
 * Protocol: gateway v10, JSON encoding, no compression (zlib-stream and zstd-stream would each
 * reintroduce a native dependency for a payload volume measured in bytes per minute).
 *
 * THE PART THAT IS EASY TO GET WRONG, and why each piece is here:
 *
 *   - The FIRST heartbeat must be jittered by a random fraction of the interval. Every bot that
 *     skips this heartbeats in lockstep with every other bot after a gateway restart, which is
 *     the thundering herd the jitter exists to break up.
 *
 *   - A heartbeat that is never ACKed means the socket is a ZOMBIE: still open at the TCP level,
 *     receiving nothing. Without an ack check the bot sits there looking healthy and silently
 *     deaf. Recovery requires closing with a code that is NOT 1000/1001, because a clean close
 *     tells Discord to destroy the session and a destroyed session cannot be resumed.
 *
 *   - Six close codes are FATAL and must never be retried: 4004 (bad token), 4010 (bad shard),
 *     4011 (sharding required), 4012 (bad API version), 4013 (bad intents), 4014 (a privileged
 *     intent that is not enabled in the developer portal). Every one is a configuration error.
 *     Retrying them produces an infinite loop that burns the 1000-IDENTIFY-per-day budget and
 *     tells the operator nothing. 4014 in particular is the one that WILL happen the first time,
 *     because MESSAGE_CONTENT is off by default.
 */

export const OP = {
  DISPATCH: 0,
  HEARTBEAT: 1,
  IDENTIFY: 2,
  RESUME: 6,
  RECONNECT: 7,
  INVALID_SESSION: 9,
  HELLO: 10,
  HEARTBEAT_ACK: 11,
} as const;

/** Configuration errors. Reconnecting cannot fix any of them, so the bridge stops and says why. */
export const FATAL_CLOSE_CODES: ReadonlyMap<number, string> = new Map([
  [4004, 'the bot token was rejected'],
  [4010, 'invalid shard'],
  [4011, 'sharding required'],
  [4012, 'invalid gateway API version'],
  [4013, 'invalid intents'],
  [4014, 'a privileged intent is not enabled for this application in the developer portal'],
]);

/** Session is gone; reconnect from scratch rather than trying to resume it. */
const RESET_CLOSE_CODES: ReadonlySet<number> = new Set([4007, 4009]);

/** Any 4xxx that is not 1000/1001 keeps the session alive for a resume. */
const ZOMBIE_CLOSE = 4900;

export const INTENTS = {
  GUILDS: 1 << 0,
  GUILD_MESSAGES: 1 << 9,
  DIRECT_MESSAGES: 1 << 12,
  MESSAGE_CONTENT: 1 << 15,
} as const;

/**
 * Guild text + DMs + message content. 37377.
 * MESSAGE_CONTENT is PRIVILEGED: without it, content arrives EMPTY in guilds (DMs are unaffected),
 * which looks exactly like a working bot that has gone mute. Enabling it is a portal toggle.
 */
export const TEXT_INTENTS = INTENTS.GUILDS | INTENTS.GUILD_MESSAGES
  | INTENTS.DIRECT_MESSAGES | INTENTS.MESSAGE_CONTENT;

export interface GatewayPayload {
  op: number;
  d?: unknown;
  s?: number | null;
  t?: string | null;
}

export type LogFn = (event: string, fields?: Record<string, string | number>) => void;

export interface GatewayOptions {
  token: string;
  intents?: number;
  /** Base gateway URL. Overridden in tests to point at the loopback fake. */
  url?: string;
  onEvent: (type: string, data: Record<string, unknown>) => void;
  onReady?: (botId: string) => void;
  /** Called once when the failure is a config error no retry can fix. The bridge then exits. */
  onFatal?: (code: number, explanation: string) => void;
  log?: LogFn;
  random?: () => number;
  socketFactory?: (url: string) => WebSocket;
  maxBackoffMs?: number;
  /** Test hook: stop after this many connections so a test cannot loop forever. */
  maxConnections?: number;
}

const DEFAULT_URL = 'wss://gateway.discord.gg/?v=10&encoding=json';

export class DiscordGateway {
  private ws: WebSocket | null = null;
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null;
  private firstBeatTimer: ReturnType<typeof setTimeout> | null = null;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;

  private sequence: number | null = null;
  private sessionId: string | null = null;
  private resumeUrl: string | null = null;
  private ackPending = false;
  private attempt = 0;
  private connections = 0;
  private stopped = false;

  botId = '';

  private readonly log: LogFn;
  private readonly random: () => number;
  private readonly makeSocket: (url: string) => WebSocket;

  constructor(private readonly opts: GatewayOptions) {
    this.log = opts.log ?? (() => {});
    this.random = opts.random ?? Math.random;
    this.makeSocket = opts.socketFactory ?? ((url) => new WebSocket(url));
  }

  get intents(): number {
    return this.opts.intents ?? TEXT_INTENTS;
  }

  /** True once READY or RESUMED has landed. */
  get connected(): boolean {
    return this.ws?.readyState === 1 && this.sessionId !== null;
  }

  connect(): void {
    if (this.stopped) return;
    if (this.opts.maxConnections && this.connections >= this.opts.maxConnections) {
      this.log('gateway.connect.capped', { connections: this.connections });
      return;
    }
    this.connections += 1;

    // A resume goes back to the URL READY handed us; a fresh identify goes to the base URL.
    const target = this.sessionId && this.resumeUrl
      ? this.resumeUrl
      : (this.opts.url ?? DEFAULT_URL);
    this.log('gateway.connecting', { attempt: this.attempt, resuming: this.sessionId ? 1 : 0 });

    let socket: WebSocket;
    try {
      socket = this.makeSocket(target);
    } catch (err) {
      this.log('gateway.socket_failed', { error: String((err as Error).message) });
      this.scheduleReconnect();
      return;
    }
    this.ws = socket;

    socket.addEventListener('message', (ev) => {
      let payload: GatewayPayload;
      try {
        payload = JSON.parse(String((ev as MessageEvent).data)) as GatewayPayload;
      } catch {
        this.log('gateway.bad_json');
        return;
      }
      this.onPayload(payload);
    });
    // 'error' carries no useful detail in undici and always precedes 'close'; close does the work.
    socket.addEventListener('error', () => this.log('gateway.socket_error'));
    socket.addEventListener('close', (ev) => this.onClose((ev as CloseEvent).code));
  }

  /** Shut down for good. Idempotent. */
  stop(): void {
    this.stopped = true;
    this.clearTimers();
    if (this.reconnectTimer) clearTimeout(this.reconnectTimer);
    this.reconnectTimer = null;
    try {
      this.ws?.close(1000);
    } catch {
      /* already closing */
    }
    this.ws = null;
  }

  private clearTimers(): void {
    if (this.heartbeatTimer) clearInterval(this.heartbeatTimer);
    if (this.firstBeatTimer) clearTimeout(this.firstBeatTimer);
    this.heartbeatTimer = null;
    this.firstBeatTimer = null;
  }

  private send(payload: GatewayPayload): void {
    if (this.ws?.readyState !== 1) return;
    this.ws.send(JSON.stringify(payload));
  }

  private onPayload(payload: GatewayPayload): void {
    if (typeof payload.s === 'number') this.sequence = payload.s;

    switch (payload.op) {
      case OP.HELLO: {
        const interval = Number((payload.d as { heartbeat_interval?: number })?.heartbeat_interval);
        this.startHeartbeat(Number.isFinite(interval) && interval > 0 ? interval : 41_250);
        if (this.sessionId) this.sendResume();
        else this.sendIdentify();
        return;
      }
      case OP.HEARTBEAT:
        // Discord can demand one out of band; answer immediately, do not wait for the interval.
        this.sendHeartbeat();
        return;
      case OP.HEARTBEAT_ACK:
        this.ackPending = false;
        return;
      case OP.RECONNECT:
        this.log('gateway.server_asked_reconnect');
        this.reconnectNow();
        return;
      case OP.INVALID_SESSION: {
        const resumable = payload.d === true;
        this.log('gateway.invalid_session', { resumable: resumable ? 1 : 0 });
        if (!resumable) this.forgetSession();
        // Discord asks for a 1-5s wait here so a fleet of bots does not re-identify in unison.
        this.reconnectNow(1_000 + Math.floor(this.random() * 4_000));
        return;
      }
      case OP.DISPATCH:
        this.onDispatch(payload);
        return;
      default:
        return;
    }
  }

  private onDispatch(payload: GatewayPayload): void {
    const data = (payload.d ?? {}) as Record<string, unknown>;
    if (payload.t === 'READY') {
      this.attempt = 0;
      this.sessionId = String(data.session_id ?? '');
      const url = data.resume_gateway_url;
      this.resumeUrl = typeof url === 'string' && url ? url : null;
      const user = data.user as { id?: string } | undefined;
      this.botId = String(user?.id ?? '');
      this.log('gateway.ready', { bot: this.botId, session: this.sessionId });
      this.opts.onReady?.(this.botId);
      return;
    }
    if (payload.t === 'RESUMED') {
      this.attempt = 0;
      this.log('gateway.resumed');
      return;
    }
    if (payload.t) this.opts.onEvent(payload.t, data);
  }

  private startHeartbeat(intervalMs: number): void {
    this.clearTimers();
    this.ackPending = false;
    // The first beat is jittered across the whole interval. This is required, not cosmetic.
    const jitter = Math.floor(intervalMs * this.random());
    this.log('gateway.heartbeat.scheduled', { interval: intervalMs, first: jitter });
    this.firstBeatTimer = setTimeout(() => {
      this.sendHeartbeat();
      this.heartbeatTimer = setInterval(() => this.sendHeartbeat(), intervalMs);
    }, jitter);
  }

  private sendHeartbeat(): void {
    if (this.ackPending) {
      // Nothing came back from the last one: the socket is open but dead. Close with a non-1000
      // code so the session survives and can be resumed on the next connection.
      this.log('gateway.zombie_detected');
      this.clearTimers();
      try {
        this.ws?.close(ZOMBIE_CLOSE);
      } catch {
        this.onClose(ZOMBIE_CLOSE);
      }
      return;
    }
    this.ackPending = true;
    this.send({ op: OP.HEARTBEAT, d: this.sequence });
  }

  private sendIdentify(): void {
    this.log('gateway.identify', { intents: this.intents });
    this.send({
      op: OP.IDENTIFY,
      d: {
        token: this.opts.token,
        intents: this.intents,
        properties: { os: 'windows', browser: 'apocrypha-bridge', device: 'apocrypha-bridge' },
      },
    });
  }

  private sendResume(): void {
    this.log('gateway.resume', { session: this.sessionId ?? '' });
    this.send({
      op: OP.RESUME,
      d: { token: this.opts.token, session_id: this.sessionId, seq: this.sequence },
    });
  }

  private forgetSession(): void {
    this.sessionId = null;
    this.resumeUrl = null;
    this.sequence = null;
  }

  private onClose(code: number): void {
    this.clearTimers();
    this.ws = null;
    if (this.stopped) return;

    const fatal = FATAL_CLOSE_CODES.get(code);
    if (fatal) {
      this.stopped = true;
      this.log('gateway.fatal', { code, reason: fatal });
      this.opts.onFatal?.(code, fatal);
      return;
    }
    if (RESET_CLOSE_CODES.has(code)) this.forgetSession();
    this.log('gateway.closed', { code, resumable: this.sessionId ? 1 : 0 });
    this.scheduleReconnect();
  }

  private reconnectNow(delayMs = 0): void {
    try {
      this.ws?.close(ZOMBIE_CLOSE);
    } catch {
      /* socket already gone; onClose will not fire, so drive it below */
    }
    if (!this.ws) this.scheduleReconnect(delayMs);
  }

  private scheduleReconnect(delayMs?: number): void {
    if (this.stopped || this.reconnectTimer) return;
    this.attempt += 1;
    const cap = this.opts.maxBackoffMs ?? 60_000;
    const backoff = delayMs ?? Math.min(cap, 1_000 * 2 ** Math.min(this.attempt - 1, 6));
    const wait = delayMs ?? Math.floor(backoff * (0.5 + this.random() * 0.5));
    this.log('gateway.reconnect.scheduled', { attempt: this.attempt, wait });
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      this.connect();
    }, wait);
  }
}
