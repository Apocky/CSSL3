/**
 * Who Apocrypha is allowed to hear, and who it must not.
 *
 * This module is the whole of specs/discord/CONSENT_POLICY.md expressed as code. It runs at the
 * socket edge, BEFORE a message is parsed into anything the mind can see, because the policy is
 * "the mind never receives it", not "the mind is asked to ignore it".
 *
 * The reason it is this strict: MESSAGE_CONTENT hands the process every message in every channel
 * the bot can see. Apocky consented to being read by Apocrypha. Nobody else in any server did, and
 * a server owner cannot consent on another member's behalf. So the default is DISCARD, and being
 * heard is the exception that has to be earned by an explicit act of address.
 *
 * Everything here is pure. No I/O, no clock, no network -- so the tests can enumerate the whole
 * decision space instead of sampling it.
 */

/** The subset of a Discord MESSAGE_CREATE that any decision can legitimately depend on. */
export interface InboundMessage {
  id: string;
  channelId: string;
  /** Absent for a DM. Present for anything inside a server. */
  guildId?: string | null;
  authorId: string;
  authorIsBot: boolean;
  content: string;
  /** User ids in the message's mentions array. */
  mentions: readonly string[];
  /** Set when this message is a reply; the id of the message being replied to. */
  referencedMessageId?: string | null;
  /** Set when this message is a reply; the author of the message being replied to. */
  referencedAuthorId?: string | null;
}

export type Act = 'answer' | 'ignore' | 'halt' | 'resume';

export interface Decision {
  act: Act;
  /** A short machine-readable reason. Safe to log: it never contains message text. */
  reason: string;
  /**
   * True when the speaker is the owner. Only owner turns may reach memory -- a stranger may be
   * answered, but that exchange is not remembered (CONSENT_POLICY R3).
   */
  owner: boolean;
}

export interface PolicyState {
  /** Apocky's Discord user id. Configured explicitly; never inferred from who talks the most. */
  ownerId: string;
  /** The bot's own user id, learned from READY. Empty until then. */
  botId: string;
  /** Channels Apocky has explicitly opted in, where every message is addressed to Apocrypha. */
  optInChannels: ReadonlySet<string>;
  /** User ids that asked it to stop. Enforced here, not requested of the model. */
  halted: ReadonlySet<string>;
}

/**
 * "stop" and "leave" halt the speaker. Matched as the WHOLE message, not as a substring, because
 * "don't stop" and "stop by later" are not revocations and treating them as such would make the
 * bot look broken in exactly the moment someone is relying on it.
 */
const HALT_RE = /^\s*(stop|leave|go away)\s*[.!]*\s*$/i;

/** Only the owner can undo a halt, and only for themselves. */
const RESUME_RE = /^\s*(resume|come back|start again)\s*[.!]*\s*$/i;

export function isHaltPhrase(content: string): boolean {
  return HALT_RE.test(content);
}

export function isResumePhrase(content: string): boolean {
  return RESUME_RE.test(content);
}

/**
 * The single decision point.
 *
 * Order matters and is load-bearing:
 *   1. never answer itself (an infinite loop that also costs real tokens)
 *   2. never answer other bots (two bots can loop each other forever)
 *   3. a halt is honoured before anything else a halted person says
 *   4. only then, is this even addressed to us
 *
 * Checking "addressed" before "halt" would mean a halted person's unaddressed messages were still
 * being examined for halt phrases -- reading someone who asked not to be read.
 */
export function decide(msg: InboundMessage, state: PolicyState): Decision {
  const owner = msg.authorId === state.ownerId;

  if (state.botId && msg.authorId === state.botId) {
    return { act: 'ignore', reason: 'self', owner: false };
  }
  if (msg.authorIsBot) {
    return { act: 'ignore', reason: 'author_is_bot', owner: false };
  }

  const addressed = addressedToUs(msg, state);

  // A halt is accepted only from someone talking TO us. Otherwise "stop" said across the room to
  // another person would silently halt them, and they would never know why it went quiet.
  if (addressed && isHaltPhrase(msg.content)) {
    return { act: 'halt', reason: 'halt_requested', owner };
  }
  if (state.halted.has(msg.authorId)) {
    // Only the owner can lift their own halt; a stranger's "resume" is not honoured, so a halt
    // cannot be talked out of by the same channel that is being halted.
    if (owner && addressed && isResumePhrase(msg.content)) {
      return { act: 'resume', reason: 'owner_resumed', owner: true };
    }
    return { act: 'ignore', reason: 'halted', owner };
  }

  if (!addressed) {
    return { act: 'ignore', reason: 'not_addressed', owner };
  }
  if (!msg.content.trim()) {
    return { act: 'ignore', reason: 'empty', owner };
  }
  return { act: 'answer', reason: addressReason(msg, state), owner };
}

function isDirectMessage(msg: InboundMessage): boolean {
  return !msg.guildId;
}

function addressedToUs(msg: InboundMessage, state: PolicyState): boolean {
  // A DM is addressed by its nature -- but only the owner's. An open DM surface would let anyone
  // who shares a server start a private, unlogged conversation with Apocky's assistant, and that
  // is a door this project does not want standing open. Strangers reach it by mention, in public,
  // where Apocky can see it happening.
  if (isDirectMessage(msg)) return msg.authorId === state.ownerId;

  if (state.optInChannels.has(msg.channelId)) return true;
  if (state.botId && msg.mentions.includes(state.botId)) return true;
  if (state.botId && msg.referencedAuthorId === state.botId) return true;
  return false;
}

function addressReason(msg: InboundMessage, state: PolicyState): string {
  if (isDirectMessage(msg)) return 'owner_dm';
  if (state.optInChannels.has(msg.channelId)) return 'opt_in_channel';
  if (state.botId && msg.mentions.includes(state.botId)) return 'mentioned';
  return 'reply_to_self';
}

/**
 * What may be written to a log line about a message.
 *
 * CONSENT_POLICY R2: a discarded message leaves no trace that it existed. Even an ANSWERED
 * message's text is not log material. So this returns identifiers and a decision -- never content,
 * never a preview, never a length that could fingerprint a short message.
 */
export function auditLine(msg: InboundMessage, decision: Decision): Record<string, string> {
  return {
    message: msg.id,
    channel: msg.channelId,
    guild: msg.guildId ?? 'dm',
    author: msg.authorId,
    act: decision.act,
    reason: decision.reason,
    owner: decision.owner ? 'yes' : 'no',
  };
}

/**
 * Whether this exchange may reach memory.
 *
 * CONSENT_POLICY R3: text from anyone other than the owner never enters memory, retrieval, or any
 * distillation corpus -- even when it was properly addressed. A stranger may talk to Apocrypha;
 * that exchange answers and then ends.
 */
export function mayPersist(decision: Decision): boolean {
  return decision.act === 'answer' && decision.owner;
}
