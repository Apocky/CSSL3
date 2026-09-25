/**
 * The bridge: gateway in, policy, mind, REST out.
 *
 * Message flow, and the order is the policy:
 *
 *   MESSAGE_CREATE -> decide() -> [discard]            most messages end here, unread
 *                              -> [halt / resume]      enforced here, not asked of the model
 *                              -> answer:
 *                                   announce once per channel   (CONSENT_POLICY R4)
 *                                   typing indicator, refreshed while thinking
 *                                   history ONLY if the speaker is the owner  (R3)
 *                                   mind.ask -> reply, chunked to Discord's 2000 limit
 *
 * A message that decide() discards is never stored, never embedded, never counted, and its text
 * never reaches a log line. The only trace is an audit record of identifiers and the decision.
 */
import { DiscordGateway, type LogFn } from './gateway';
import { DiscordRest } from './rest';
import { MindClient, type MindTurn } from './mind';
import {
  auditLine,
  decide,
  mayPersist,
  type Decision,
  type InboundMessage,
  type PolicyState,
} from './policy';
import type { BridgeConfig } from './config';

/** How much of a conversation Apocrypha carries between turns, for the owner only. */
const HISTORY_TURNS = 12;

/** Discord's typing indicator lasts 10 s; refresh a little inside that. */
const TYPING_REFRESH_MS = 8_000;

const ANNOUNCEMENT =
  'I am Apocrypha, an AI. I only read messages addressed to me -- a mention, a reply to me, '
  + 'or a channel Apocky has opted in -- and I do not remember anyone except him. '
  + 'Say **stop** and I will not reply to you again.';

export interface BridgeState {
  halted: string[];
  announced: string[];
}

export interface StateStore {
  read: () => BridgeState;
  write: (state: BridgeState) => void;
}

/** In-memory store, used by tests and as the fallback when the state dir is unwritable. */
export function memoryStore(initial?: Partial<BridgeState>): StateStore {
  let state: BridgeState = {
    halted: initial?.halted ?? [],
    announced: initial?.announced ?? [],
  };
  return {
    read: () => ({ halted: [...state.halted], announced: [...state.announced] }),
    write: (next) => {
      state = { halted: [...next.halted], announced: [...next.announced] };
    },
  };
}

export interface BridgeDeps {
  config: BridgeConfig;
  rest: DiscordRest;
  mind: MindClient;
  store: StateStore;
  log?: LogFn;
  /** Overridden in tests to point the gateway at a loopback fake. */
  gatewayUrl?: string;
  socketFactory?: (url: string) => WebSocket;
  onFatal?: (code: number, explanation: string) => void;
}

export class Bridge {
  readonly gateway: DiscordGateway;

  private readonly log: LogFn;
  private readonly halted: Set<string>;
  private readonly announced: Set<string>;
  private readonly history = new Map<string, MindTurn[]>();
  private readonly busy = new Set<string>();

  /** Counters, exposed on /health so "is it doing anything" has an answer. */
  readonly stats = { seen: 0, answered: 0, ignored: 0, halted: 0, failed: 0 };

  constructor(private readonly deps: BridgeDeps) {
    this.log = deps.log ?? (() => {});
    const persisted = deps.store.read();
    this.halted = new Set(persisted.halted);
    this.announced = new Set(persisted.announced);

    this.gateway = new DiscordGateway({
      token: deps.config.token,
      url: deps.gatewayUrl,
      socketFactory: deps.socketFactory,
      log: this.log,
      onFatal: deps.onFatal,
      onEvent: (type, data) => {
        if (type === 'MESSAGE_CREATE') void this.onMessage(data);
      },
    });
  }

  start(): void {
    this.gateway.connect();
  }

  stop(): void {
    this.gateway.stop();
  }

  private policyState(): PolicyState {
    return {
      ownerId: this.deps.config.ownerId,
      botId: this.gateway.botId,
      optInChannels: this.deps.config.optInChannels,
      halted: this.halted,
    };
  }

  private persist(): void {
    try {
      this.deps.store.write({
        halted: [...this.halted],
        announced: [...this.announced],
      });
    } catch (err) {
      // A halt that cannot be persisted is still enforced for this process lifetime. Say so
      // loudly rather than pretending the write happened.
      this.log('bridge.state_write_failed', { error: String((err as Error).message) });
    }
  }

  /** Parse a raw MESSAGE_CREATE into only the fields a decision may depend on. */
  static parse(data: Record<string, unknown>): InboundMessage | null {
    const id = typeof data.id === 'string' ? data.id : '';
    const channelId = typeof data.channel_id === 'string' ? data.channel_id : '';
    const author = data.author as { id?: string; bot?: boolean } | undefined;
    if (!id || !channelId || !author?.id) return null;
    const mentions = Array.isArray(data.mentions)
      ? (data.mentions as { id?: string }[]).map((m) => String(m?.id ?? '')).filter(Boolean)
      : [];
    const ref = data.referenced_message as
      { id?: string; author?: { id?: string } } | undefined;
    return {
      id,
      channelId,
      guildId: typeof data.guild_id === 'string' ? data.guild_id : null,
      authorId: String(author.id),
      authorIsBot: author.bot === true,
      content: typeof data.content === 'string' ? data.content : '',
      mentions,
      referencedMessageId: ref?.id ?? null,
      referencedAuthorId: ref?.author?.id ?? null,
    };
  }

  async onMessage(data: Record<string, unknown>): Promise<void> {
    const msg = Bridge.parse(data);
    if (!msg) return;
    this.stats.seen += 1;

    const decision = decide(msg, this.policyState());
    this.log('bridge.decision', auditLine(msg, decision));

    switch (decision.act) {
      case 'halt':
        this.halted.add(msg.authorId);
        this.history.delete(msg.channelId);
        this.persist();
        this.stats.halted += 1;
        await this.deps.rest.sendMessage(
          msg.channelId,
          'Stopped. I will not reply to you again unless Apocky turns me back on.',
          { replyTo: msg.id },
        );
        return;
      case 'resume':
        this.halted.delete(msg.authorId);
        this.persist();
        await this.deps.rest.sendMessage(msg.channelId, 'Back.', { replyTo: msg.id });
        return;
      case 'ignore':
        this.stats.ignored += 1;
        return;
      case 'answer':
        await this.answer(msg, decision);
        return;
      default:
        return;
    }
  }

  private async answer(msg: InboundMessage, decision: Decision): Promise<void> {
    if (this.busy.has(msg.channelId)) {
      this.log('bridge.busy', { channel: msg.channelId });
      await this.deps.rest.sendMessage(
        msg.channelId,
        'Still working on the last one. Ask again when it lands.',
        { replyTo: msg.id },
      );
      return;
    }
    this.busy.add(msg.channelId);

    // R4: say what this is, once, before the first answer in any channel or DM.
    if (!this.announced.has(msg.channelId)) {
      this.announced.add(msg.channelId);
      this.persist();
      await this.deps.rest.sendMessage(msg.channelId, ANNOUNCEMENT);
    }

    const typing = this.keepTyping(msg.channelId);
    try {
      const persistable = mayPersist(decision);
      const prior = persistable ? (this.history.get(msg.channelId) ?? []) : [];
      const turns: MindTurn[] = [...prior, { role: 'user', content: msg.content }];

      const answer = await this.deps.mind.ask(turns);
      if (!answer.ok) {
        this.stats.failed += 1;
        this.log('bridge.mind_failed', {
          channel: msg.channelId,
          failure: answer.failure ?? 'unknown',
          ms: answer.durationMs,
        });
      } else {
        this.stats.answered += 1;
        this.log('bridge.answered', { channel: msg.channelId, ms: answer.durationMs });
        if (persistable) {
          // Only the owner's exchanges are carried forward. A stranger's turn is answered and
          // then forgotten -- it never becomes context for anyone, including themselves.
          const carried: MindTurn[] = [
            ...turns,
            { role: 'assistant', content: answer.text },
          ];
          this.history.set(msg.channelId, carried.slice(-HISTORY_TURNS));
        }
      }
      await this.deps.rest.sendMessage(msg.channelId, answer.text, { replyTo: msg.id });
    } finally {
      typing.stop();
      this.busy.delete(msg.channelId);
    }
  }

  /** Holds the typing indicator up for as long as the mind is thinking. */
  private keepTyping(channelId: string): { stop: () => void } {
    void this.deps.rest.triggerTyping(channelId);
    const timer = setInterval(() => {
      void this.deps.rest.triggerTyping(channelId);
    }, TYPING_REFRESH_MS);
    return {
      stop: () => {
        clearInterval(timer);
      },
    };
  }
}
