import { randomUUID } from 'node:crypto';
import type { SamplingProfile } from '../../lib/apocrypha/sampling';
import type { ServerResponse } from 'node:http';
import type { WorkAgent } from './agent';
import type { EngineMessage } from './engine';
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

const REPLAY_PAGE_SIZE = 1_000;
const HEARTBEAT_MS = 15_000;
const STREAM_DRAIN_MS = 5_000;
const TERMINAL = new Set(['done', 'failed', 'cancelled']);

interface ActiveTurn {
  readonly turn: WorkTurn;
  readonly controller: AbortController;
  readonly pendingConsent: Map<string, (decision: ConsentDecision) => void>;
  readonly consentIds: Set<string>;
  readonly completion: Promise<void>;
}

interface Channel {
  streams: Set<ServerResponse>;
  active: ActiveTurn | null;
  pending: Promise<void>;
  failure: Error | null;
}

export class TurnRunner {
  private readonly config: WorkConfig;
  private readonly agent: WorkAgent;
  private readonly store: SessionStore;
  private readonly channels = new Map<string, Channel>();
  private readonly heartbeat: NodeJS.Timeout;
  private stopped = false;

  constructor(config: WorkConfig, agent: WorkAgent, store: SessionStore) {
    this.config = config;
    this.agent = agent;
    this.store = store;
    // Proxies and browsers drop an idle event stream; a long model prefill can be silent for
    // minutes, so the channel says something even when the agent has not.
    this.heartbeat = setInterval(() => {
      for (const channel of this.channels.values()) {
        for (const stream of channel.streams) this.write(channel, stream, ': ping\n\n');
      }
    }, HEARTBEAT_MS);
    this.heartbeat.unref();
  }

  private channel(sessionId: string): Channel {
    const existing = this.channels.get(sessionId);
    if (existing) return existing;
    const created: Channel = { streams: new Set(), active: null, pending: Promise.resolve(), failure: null };
    this.channels.set(sessionId, created);
    return created;
  }

  private enqueue<Result>(channel: Channel, operation: () => Promise<Result>): Promise<Result> {
    const pending = channel.pending.then(operation);
    channel.pending = pending.then(() => undefined, () => undefined);
    return pending;
  }

  private write(channel: Channel, stream: ServerResponse, payload: string, replay = false): boolean {
    try {
      if (!stream.destroyed && !stream.writableEnded) {
        const writable = stream.write(payload);
        if (writable || replay) return writable;
      }
    } catch (error) {
      log('warn', 'work.stream.failed', { error: error instanceof Error ? error.message : String(error) });
    }
    channel.streams.delete(stream);
    try { stream.end(); }
    catch (error) { log('warn', 'work.stream.close_failed', { error: error instanceof Error ? error.message : String(error) }); }
    return false;
  }

  private async writeReplay(channel: Channel, response: ServerResponse, payload: string): Promise<boolean> {
    if (this.write(channel, response, payload, true)) return true;
    if (response.destroyed || response.writableEnded) return false;
    return new Promise((resolve) => {
      const finish = (ready: boolean) => {
        clearTimeout(timeout);
        response.off('drain', drained);
        response.off('close', closed);
        response.off('error', closed);
        resolve(ready);
      };
      const drained = () => finish(true);
      const closed = () => finish(false);
      const timeout = setTimeout(() => {
        finish(false);
        this.write(channel, response, '');
        response.end();
      }, STREAM_DRAIN_MS);
      timeout.unref();
      response.once('drain', drained);
      response.once('close', closed);
      response.once('error', closed);
    });
  }

  private publish(channel: Channel, event: WorkEvent): void {
    const payload = `id: ${event.seq}\ndata: ${JSON.stringify(event)}\n\n`;
    for (const stream of channel.streams) this.write(channel, stream, payload);
  }

  activeCount(): number {
    return [...this.channels.values()].filter((channel) => channel.active !== null).length;
  }

  async attachStream(sessionId: string, response: ServerResponse, after = 0): Promise<void> {
    const channel = this.channel(sessionId);
    let closed = response.destroyed || response.writableEnded;
    const detach = () => { closed = true; channel.streams.delete(response); };
    response.on('close', detach);
    response.on('error', detach);
    try {
      await this.enqueue(channel, async () => {
        if (this.stopped) throw new Error('turn runner is stopped');
        let cursor = after;
        let events = await this.store.eventsAfter(sessionId, cursor, REPLAY_PAGE_SIZE);
        if (closed) return;
        response.writeHead(200, {
          'content-type': 'text/event-stream; charset=utf-8',
          'cache-control': 'no-store',
          connection: 'keep-alive',
          'x-accel-buffering': 'no',
        });
        for (;;) {
          for (const event of events) {
            if (closed || !await this.writeReplay(channel, response, `id: ${event.seq}\ndata: ${JSON.stringify(event)}\n\n`)) return;
            cursor = event.seq;
          }
          if (events.length < REPLAY_PAGE_SIZE) break;
          events = await this.store.eventsAfter(sessionId, cursor, REPLAY_PAGE_SIZE);
        }
        if (closed || !await this.writeReplay(channel, response, ': attached\n\n')) return;
        channel.streams.add(response);
        if (channel.failure) this.write(channel, response, this.failureFrame(channel.failure));
      });
    } catch (error) {
      detach();
      response.off('close', detach);
      response.off('error', detach);
      throw error;
    }
  }

  private failureFrame(error: Error): string {
    return `event: persistence_error\ndata: ${JSON.stringify({ code: 'PERSISTENCE_FAILED', message: error.message, durable: false, needs_reconciliation: true })}\n\n`;
  }

  private persistenceFailed(sessionId: string, error: unknown): void {
    const channel = this.channel(sessionId);
    const failure = error instanceof Error ? error : new Error(String(error));
    if (!channel.failure) {
      channel.failure = failure;
      log('error', 'work.persistence.failed', { session: sessionId, error: failure.message });
      for (const stream of channel.streams) this.write(channel, stream, this.failureFrame(failure));
    }
    if (channel.active) {
      channel.active.turn.phase = 'failed';
      channel.active.turn.error = `Persistence failure: ${failure.message}; outcome unknown, reconciliation required.`;
      channel.active.turn.endedAt ??= new Date().toISOString();
    }
    channel.active?.controller.abort();
  }

  private emit(sessionId: string, event: Omit<WorkEvent, 'seq' | 'at'>): void {
    const channel = this.channel(sessionId);
    try {
      const input = { ...structuredClone(event), at: new Date().toISOString() };
      void this.enqueue(channel, async () => {
        if (channel.failure) return;
        const committed = await this.store.appendEvent(sessionId, input);
        this.publish(channel, committed);
      }).catch((error) => { this.persistenceFailed(sessionId, error); });
    } catch (error) {
      this.persistenceFailed(sessionId, error);
    }
  }

  async start(session: WorkSession, prompt: string, sampling?: SamplingProfile): Promise<WorkTurn> {
    if (this.stopped) throw new Error('turn runner is stopped');
    const channel = this.channel(session.id);
    if (channel.failure) throw new Error(`Session persistence failed; reopen and reconcile before continuing: ${channel.failure.message}`);
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
    let complete!: () => void;
    const completion = new Promise<void>((resolve) => { complete = resolve; });
    const active: ActiveTurn = { turn, controller, pendingConsent: new Map(), consentIds: new Set(), completion };
    channel.active = active;
    let history: EngineMessage[];
    try {
      history = await this.store.history(session.id);
      await this.enqueue(channel, async () => {
        if (this.stopped) throw new Error('turn runner is stopped');
        const initial = await this.store.beginTurn(session.id, turn);
        this.publish(channel, initial);
      });
    } catch (error) {
      controller.abort();
      if (channel.active === active) channel.active = null;
      complete();
      throw error;
    }
    void this.runActive(session, active, history, sampling).then(complete, (error) => {
      try { this.persistenceFailed(session.id, error); }
      finally {
        if (channel.active === active) channel.active = null;
        complete();
      }
    });
    return turn;
  }

  private async runActive(session: WorkSession, active: ActiveTurn, history: EngineMessage[], sampling?: SamplingProfile): Promise<void> {
    const channel = this.channel(session.id);
    const { turn, controller } = active;
    const terminalEvents: Omit<WorkEvent, 'seq' | 'at'>[] = [];
    let terminalData: Record<string, unknown> = {};
    const timeout = setTimeout(() => controller.abort(), this.config.turnTimeoutMs);
    try {
      await this.agent.run(session, turn, history, (event) => {
        const tagged = { ...event, data: { ...event.data, turn_id: turn.id } };
        if (event.kind === 'phase' && TERMINAL.has(String(event.data.phase))) terminalData = structuredClone(tagged.data);
        else if (event.kind === 'error' && TERMINAL.has(turn.phase)) terminalEvents.push(structuredClone(tagged));
        else this.emit(session.id, tagged);
      }, (request) => this.awaitConsent(session.id, request), controller.signal, sampling);
    } catch (error) {
      turn.phase = controller.signal.aborted ? 'cancelled' : 'failed';
      turn.error = error instanceof Error ? error.message : String(error);
      log('error', 'work.turn.failed', { session: session.id, turn: turn.id, error: turn.error });
      terminalEvents.push({ kind: 'error', data: { message: turn.error, code: 'TURN_FAILED' } });
    } finally {
      clearTimeout(timeout);
      await channel.pending;
      for (const resolve of active.pendingConsent.values()) resolve('deny');
      if (channel.failure) {
        turn.phase = 'failed';
        turn.error = `Persistence failure: ${channel.failure.message}; external effects require reconciliation.`;
        terminalEvents.push({ kind: 'error', data: { message: turn.error, code: 'PERSISTENCE_FAILED', needs_reconciliation: true } });
      } else if (controller.signal.aborted) turn.phase = 'cancelled';
      else if (!TERMINAL.has(turn.phase)) {
        turn.phase = 'failed';
        turn.error = 'Agent stopped without a terminal result; reconciliation required.';
      }
      turn.endedAt ??= new Date().toISOString();
      try {
        await this.enqueue(channel, async () => {
          const events = await this.store.finishTurn(session.id, turn, terminalData, terminalEvents);
          if (channel.active === active) channel.active = null;
          for (const event of events) this.publish(channel, event);
        });
      } catch (error) {
        this.persistenceFailed(session.id, error);
      } finally {
        if (channel.active === active) channel.active = null;
      }
    }
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
    if (!active || active.controller.signal.aborted || request.turnId !== active.turn.id
      || active.consentIds.has(request.id)) return Promise.resolve('deny');
    active.consentIds.add(request.id);
    return new Promise<ConsentDecision>((resolve) => {
      const settle = (decision: ConsentDecision) => {
        active.pendingConsent.delete(request.id);
        active.controller.signal.removeEventListener('abort', abort);
        resolve(decision);
      };
      const abort = () => settle('deny');
      active.pendingConsent.set(request.id, settle);
      active.controller.signal.addEventListener('abort', abort, { once: true });
    });
  }

  resolveConsent(requestId: string, decision: ConsentDecision): boolean {
    if (!['allow', 'allow_session', 'deny'].includes(decision)) return false;
    const active = [...this.channels.values()].flatMap((channel) => {
      const turn = channel.active;
      return turn && !turn.controller.signal.aborted && turn.pendingConsent.has(requestId) ? [turn] : [];
    });
    if (active.length !== 1) return false;
    active[0]?.pendingConsent.get(requestId)?.(decision);
    return true;
  }

  cancel(sessionId: string): boolean {
    const active = this.channels.get(sessionId)?.active;
    if (!active) return false;
    active.controller.abort();
    return true;
  }

  async stopAll(): Promise<void> {
    this.stopped = true;
    clearInterval(this.heartbeat);
    const completions: Promise<void>[] = [];
    for (const channel of this.channels.values()) {
      channel.active?.controller.abort();
      if (channel.active) completions.push(channel.active.completion);
      for (const stream of channel.streams) {
        try { stream.end(); }
        catch (error) { log('warn', 'work.stream.close_failed', { error: error instanceof Error ? error.message : String(error) }); }
      }
      channel.streams.clear();
    }
    await Promise.all(completions);
  }
}
