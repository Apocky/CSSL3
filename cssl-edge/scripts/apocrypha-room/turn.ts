// The reasoning split both room-loop paths share. Pure: no env, no I/O, safe to import in tests.
//
// Split discipline: explicit reasoning wins; otherwise strip <think> tags; otherwise a
// spill-guarded content. A thought is never posted as an utterance -- the fix for the
// post-think self-correction leak ("Wait, I need to check..." in reply 572, 2026-09-24) is in
// this split, not in gagging reasoning.
//
// Spill mechanism, measured 2026-09-28 on the live engine (--reasoning-budget 600 +
// --reasoning-format auto): when deliberation outruns the budget, the server closes the thinking
// block and the tail of the deliberation arrives as `content`.

export function stripThink(text: string): { reasoning: string; content: string } {
  const match = /^\s*<think>([\s\S]*?)<\/think>\s*/u.exec(text);
  // An unterminated <think> means the token budget ran out mid-thought: that is a thought, never an
  // utterance (row 275 in the lobby on 2026-09-24 was a raw thinking dump for exactly this reason).
  if (!match && /^\s*<think>/u.test(text)) return { reasoning: text.replace(/^\s*<think>/u, '').trim(), content: '' };
  if (!match) return { reasoning: '', content: text.trim() };
  return { reasoning: (match[1] ?? '').trim(), content: text.slice(match[0].length).trim() };
}

/**
 * Thinking that spilled into the answer channel. Markers are the model's own deliberation
 * vocabulary, never answer vocabulary: a real answer does not narrate its plan.
 */
const SPILL_MARKERS = [
  /thinking process:/iu,
  /\b(final check|draft the answer|analyze the request|identify the (fact|persona)|check constraints)\b.*:/iu,
  /^\s*\d+\.\s+\*\*.+\*\*/mu,
  /\bwait,? i need to (check|re-?read|think|verify)\b/iu,
  /\b(let me|i should) (think|reconsider|double-check|verify)\b/iu,
  /\b(attempt|draft)\s+\d+\s*:/iu,
];

export function isThinkingSpill(text: string): boolean {
  return SPILL_MARKERS.some((marker) => marker.test(text));
}

export function splitTurn(reasoning: string, content: string): { reasoning: string; content: string } {
  if (reasoning.trim() !== '') {
    const spare = stripThink(content);
    const mergedReasoning = spare.reasoning !== '' ? `${reasoning.trim()}\n\n${spare.reasoning}` : reasoning.trim();
    if (spare.content !== '' && isThinkingSpill(spare.content)) {
      return { reasoning: `${mergedReasoning}\n\n${spare.content}`.trim(), content: '' };
    }
    return { reasoning: mergedReasoning, content: spare.content };
  }
  const split = stripThink(content);
  if (split.content !== '' && split.reasoning === '' && isThinkingSpill(split.content)) {
    return { reasoning: split.content, content: '' };
  }
  if (split.content !== '' && split.reasoning !== '' && isThinkingSpill(split.content)) {
    return { reasoning: `${split.reasoning}\n\n${split.content}`.trim(), content: '' };
  }
  return split;
}
