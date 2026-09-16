import { randomUUID } from 'node:crypto';
import type { SamplingProfile } from '../../lib/apocrypha/sampling';
import type { ServerResponse } from 'node:http';
import type { WorkAgent } from './agent';
import { log } from './log';
import type { SessionStore } from './sessions';
import type {
  ConsentDecision,
  ConsentRequest,
  WorkConfig,
  WorkEvent,
  WorkSession,
  WorkTurn,
} from './types';

const REPLAY_LIMIT = 400;
const HEARTBEAT_MS = 15_000;

interface ActiveTurn {
  readonly turn: WorkTurn;
  readonly controller: AbortController;
  readonly pendingConsent: Map<string, (decision: ConsentDecision) => void>;
}

interface Channel {
  seq: number;
  recent: WorkEvent[];
  streams: Set<ServerResponse>;
  active: ActiveTurn | null;
}

export class TurnRunner {
  private readonly config: WorkConfig;
  private readonly agent: WorkAgent;
  private readonly store: SessionStore;
  private readonly channels = new Map<string, Channel>();
  private readonly heartbeat: NodeJS.Timeout;

  constructor(config: WorkConfig, agent: WorkAgent, store: SessionStore) {
    this.config = config;
    this.agent = agent;
    this.store = store;
    // Proxies and browsers drop an idle event stream; a long model prefill can be silent for
    // minutes, so the channel says something even when the agent has not.
    this.heartbeat = setInterval(() => {
      for (const channel of this.channels.values()) {
        for (const stream of channel.streams) stream.write(': ping\n\n');
      }
    }, HEARTBEAT_MS);
    this.heartbeat.unref();
  }

  private channel(sessionId: string): Channel {
    const existing = this.channels.get(sessionId);
    if (existing) return existing;
    const created: Channel = { seq: 0, recent: [], streams: new Set(), active: null };
    this.channels.set(sessionId, created);
    return created;
  }

  activeCount(): number {
    return [...this.channels.values()].filter((channel) => channel.active !== null).length;
  }

  attachStream(sessionId: string, response: ServerResponse): void {
    const channel = this.channel(sessionId);
    response.writeHead(200, {
      'content-type': 'text/event-stream; charset=utf-8',
      'cache-control': 'no-store',
      connection: 'keep-alive',
      'x-accel-buffering': 'no',
    });
    for (const event of channel.recent) response.write(`data: ${JSON.stringify(event)}\n\n`);
    response.write(': attached\n\n');
    channel.streams.add(response);
    response.on('close', () => { channel.streams.delete(response); });
  }

  private emit(sessionId: string, event: Omit<WorkEvent, 'seq' | 'at'>): void {
    const channel = this.channel(sessionId);
    channel.seq += 1;
    const full: WorkEvent = { seq: channel.seq, at: new Date().toISOString(), ...event };
    channel.recent.push(full);
    if (channel.recent.length > REPLAY_LIMIT) channel.recent.splice(0, channel.recent.length - REPLAY_LIMIT);
    const payload = `data: ${JSON.stringify(full)}\n\n`;
    for (const stream of channel.streams) stream.write(payload);
    void this.store.appendEvent(sessionId, full).catch((error) => {
      log('warn', 'work.event.persist_failed', { session: sessionId, error: error instanceof Error ? error.message : 'unknown' });
    });
  }

  async start(session: WorkSession, prompt: string, sampling?: SamplingProfile): Promise<WorkTurn> {
    const channel = this.channel(session.id);
    if (channel.active) throw new Error('a turn is already running in this session');

    const turn: WorkTurn = {
      id: randomUUID(),
      sessionId: session.id,
      prompt,
      phase: 'queued',
      startedAt: new Date().toISOString(),
      toolCalls: [],
      output: '',
    };
    const controller = new AbortController();
    channel.active = { turn, controller, pendingConsent: new Map() };

    const history = await this.store.history(session.id);
    this.emit(session.id, { kind: 'session', data: { turn_id: turn.id, prompt, phase: 'queued' } });

    const timeout = setTimeout(() => controller.abort(), this.config.turnTimeoutMs);
    void this.agent.run(
      session,
      turn,
      history,
      (event) => this.emit(session.id, event),
      (request) => this.awaitConsent(session.id, request),
      controller.signal,
      sampling,
    ).catch((error) => {
      turn.phase = 'failed';
      turn.error = error instanceof Error ? error.message : String(error);
      turn.endedAt = new Date().toISOString();
      log('error', 'work.turn.failed', { session: session.id, turn: turn.id, error: turn.error });
      this.emit(session.id, { kind: 'error', data: { message: turn.error, code: 'TURN_FAILED' } });
    }).finally(() => {
      clearTimeout(timeout);
      channel.active = null;
      this.emit(session.id, {
        kind: 'phase',
        data: {
          phase: turn.phase,
          terminal: true,
          tool_calls: turn.toolCalls.length,
          usage: turn.usage ?? null,
        },
      });
      void this.store.addTurn(session.id, turn).catch((error) => {
        log('error', 'work.turn.persist_failed', { session: session.id, error: error instanceof Error ? error.message : 'unknown' });
      });
    });

    return turn;
  }

  /**
   * Block the agent until the operator decides, or the turn is cancelled.
   *
   * There is deliberately no timeout that auto-approves. An unanswered request stays unanswered;
   * the operator walking away must never become consent by default.
   */
  private awaitConsent(sessionId: string, request: ConsentRequest): Promise<ConsentDecision> {
    const channel = this.channel(sessionId);
    const active = channel.active;
    if (!active) return Promise.resolve('deny');
    return new Promise<ConsentDecision>((resolve) => {
      active.pendingConsent.set(request.id, resolve);
      active.controller.signal.addEventListener('abort', () => {
        if (active.pendingConsent.delete(request.id)) resolve('deny');
      }, { once: true });
    });
  }

  resolveConsent(requestId: string, decision: ConsentDecision): boolean {
    for (const channel of this.channels.values()) {
      const resolver = channel.active?.pendingConsent.get(requestId);
      if (!resolver) continue;
      channel.active?.pendingConsent.delete(requestId);
      resolver(decision);
      return true;
    }
    return false;
  }

  cancel(sessionId: string): boolean {
    const active = this.channels.get(sessionId)?.active;
    if (!active) return false;
    active.controller.abort();
    return true;
  }

  stopAll(): void {
    clearInterval(this.heartbeat);
    for (const channel of this.channels.values()) {
      channel.active?.controller.abort();
      for (const stream of channel.streams) stream.end();
    }
  }
}
