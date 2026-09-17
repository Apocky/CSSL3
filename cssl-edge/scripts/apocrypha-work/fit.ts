// Keep a turn inside the context window, and SAY when it did not fit.
//
// The failure this exists to stop is silent. llama.cpp clips an oversized prompt rather than
// refusing it, so the first thing to fall off the front is the system prompt -- the instructions.
// The coder then behaves like a worse model for no visible reason, and the log says `truncated = 1`
// somewhere nobody is reading. T98 names this directly: "if you send me a large enough prompt, I'm
// just going to..." clip it.
//
// It is reachable in ordinary use, not in theory. read_file accepts a 512 KB file, tool results are
// capped at 24,000 characters (~7k tokens), the agent loop pushes messages and never removes any,
// and a slot holds 16,384. TWO large reads plus the ~3.5k system-and-tools prompt is already over.
//
// What must survive, in order:
//   1. the system prompt   -- lose it and the agent forgets what it is doing
//   2. the operator's task -- lose it and the answer is to a question nobody asked
//   3. the most recent work -- what it just learned matters more than its first step
//
// Tool messages are dropped with the assistant message that requested them. A `tool` message whose
// matching assistant `tool_calls` entry is gone is an orphan, and the engine rejects the request:
// trimming carelessly turns a degraded turn into a failed one.

export interface FitMessage {
  readonly role: 'system' | 'user' | 'assistant' | 'tool';
  readonly content?: string;
  readonly tool_calls?: ReadonlyArray<{ id: string; type: 'function'; function: { name: string; arguments: string } }>;
  readonly tool_call_id?: string;
}

export interface FitReport {
  readonly messages: FitMessage[];
  /** How many assistant+tool groups were dropped whole. */
  readonly droppedGroups: number;
  /** How many surviving tool results had their content shortened. */
  readonly shortened: number;
  readonly estimatedTokensBefore: number;
  readonly estimatedTokensAfter: number;
  readonly fits: boolean;
}

/**
 * Characters per token, deliberately LOW.
 *
 * English averages nearer 4 and code nearer 3. Guessing low overestimates the token count, which
 * trims slightly early; guessing high would let a prompt through that silently clips. The direction
 * of the error is the whole point.
 */
const CHARS_PER_TOKEN = 3;

/** Per-message overhead for role tags and the chat template's own scaffolding. */
const MESSAGE_OVERHEAD_TOKENS = 4;

export function estimateTokens(message: FitMessage): number {
  let chars = (message.content ?? '').length;
  for (const call of message.tool_calls ?? []) {
    chars += call.function.name.length + call.function.arguments.length;
  }
  return Math.ceil(chars / CHARS_PER_TOKEN) + MESSAGE_OVERHEAD_TOKENS;
}

export function estimateTotal(messages: readonly FitMessage[]): number {
  return messages.reduce((sum, message) => sum + estimateTokens(message), 0);
}

/**
 * Trim `messages` to fit `contextTokens`, leaving room for the reply.
 *
 * Returns a report rather than just messages, so the caller can TELL the operator that history was
 * dropped. A context guard that trims silently has replaced one invisible failure with another.
 */
export function fitMessages(
  messages: readonly FitMessage[],
  options: { contextTokens: number; reserveTokens: number; overheadTokens?: number },
): FitReport {
  // `overheadTokens` is everything the engine will send that is NOT in `messages` -- above all the
  // TOOL SCHEMAS, which engine.ts passes as a separate `tools` array. Missing this is not academic:
  // the first version counted only messages, judged a turn to fit, and the engine rejected the
  // request at 17,617 tokens against a 16,384 window. The tool block IS part of the prompt.
  const overhead = options.overheadTokens ?? 0;
  const before = estimateTotal(messages) + overhead;
  // 8% headroom: the estimate is a character heuristic, not a tokeniser, and being a little under
  // costs nothing while being over costs the system prompt.
  const budget = Math.floor((options.contextTokens - options.reserveTokens) * 0.92);
  if (before <= budget || messages.length === 0) {
    return { messages: [...messages], droppedGroups: 0, shortened: 0, estimatedTokensBefore: before, estimatedTokensAfter: before, fits: before <= options.contextTokens };
  }

  // Group the transcript: a system or user message stands alone; an assistant message owns every
  // tool message that answers it.
  const groups: FitMessage[][] = [];
  for (const message of messages) {
    const current = groups[groups.length - 1];
    if (message.role === 'tool' && current !== undefined && current[0]?.role === 'assistant') {
      current.push(message);
    } else {
      groups.push([message]);
    }
  }

  // Protected: the system prompt, the LAST user message -- which is the task actually being worked
  // on, not necessarily the first in the transcript -- and the MOST RECENT exchange.
  //
  // That last one is not a nicety. Dropping oldest-first until the budget is met will happily drop
  // every exchange when each is individually large, including the one the model just produced and
  // is about to reason from. Protecting it means an oversized newest result gets SHORTENED below
  // instead, which keeps the thread of work intact. A test caught this; the first implementation
  // dropped all three exchanges and left the agent with no idea what it had just done.
  const lastUser = groups.map((g) => g[0]?.role).lastIndexOf('user');
  const lastExchange = groups.length - 1;
  const protectedIndex = (index: number): boolean =>
    (index === 0 && groups[0]?.[0]?.role === 'system')
    || index === lastUser
    || index === lastExchange;

  const keep = groups.map(() => true);
  let running = before;

  // Drop OLDEST first: recent work is what the next step depends on.
  for (let index = 0; index < groups.length && running > budget; index += 1) {
    if (protectedIndex(index)) continue;
    keep[index] = false;
    running -= estimateTotal(groups[index] ?? []);
  }

  let survivors = groups.filter((_, index) => keep[index]).flat();
  const droppedGroups = keep.filter((k) => !k).length;

  // Still over? Then a single protected message is itself too big -- a huge file read in the very
  // last exchange, say. Shorten tool content rather than drop a protected message, because a
  // truncated-but-labelled result is usable and a missing instruction is not.
  let shortened = 0;
  if (running > budget) {
    survivors = survivors.map((message) => {
      if (running <= budget || message.role !== 'tool' || !message.content) return message;
      const excessTokens = running - budget;
      const keepChars = Math.max(500, message.content.length - excessTokens * CHARS_PER_TOKEN);
      if (keepChars >= message.content.length) return message;
      const cut = `${message.content.slice(0, keepChars)}\n\n[...${message.content.length - keepChars} characters dropped: this result did not fit the context window. Read a narrower line range if you need the rest.]`;
      running -= Math.ceil((message.content.length - cut.length) / CHARS_PER_TOKEN);
      shortened += 1;
      return { ...message, content: cut };
    });
  }

  const after = estimateTotal(survivors) + overhead;
  return {
    messages: survivors,
    droppedGroups,
    shortened,
    estimatedTokensBefore: before,
    estimatedTokensAfter: after,
    fits: after <= options.contextTokens,
  };
}
