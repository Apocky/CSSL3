// The chat capability strip. The load-bearing property is NEGATIVE: a tool button writes a prompt
// into the composer and stops. If one of these ever calls send(), the site starts speaking on the
// visitor's behalf, which is the one thing a chat must never do. A comment saying so is not a gate;
// this is.

import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';

const read = (rel: string) => fs.readFileSync(path.join(process.cwd(), rel), 'utf8');

const strip = read('components/apocrypha/ToolStrip.tsx');
const chat = read('components/apocrypha/ApocryphaChat.tsx');
const css = read('styles/ApocryphaChat.module.css');

// -- the negative invariant ---------------------------------------------------------------------
assert.ok(!strip.includes('send('), 'ToolStrip must never call send() -- a tool fills the composer, the visitor sends');
assert.ok(!strip.includes('onSubmit'), 'ToolStrip must not submit a form');
assert.ok(!strip.includes('fetch('), 'ToolStrip must not reach the network on its own');
assert.ok(strip.includes('onInsert'), 'ToolStrip hands its prompt back to the composer');

// -- every tool is complete and usable ----------------------------------------------------------
const ids = [...strip.matchAll(/id: '([a-z_]+)'/g)].map((m) => m[1]!);
const labels = [...strip.matchAll(/label: '([^']+)'/g)].map((m) => m[1]!);
const hints = [...strip.matchAll(/hint: '([^']+)'/g)].map((m) => m[1]!);
const prompts = [...strip.matchAll(/prompt: '([^']+)'/g)].map((m) => m[1]!);

assert.ok(ids.length >= 5, `expected at least 5 chat tools, found ${ids.length}`);
assert.equal(new Set(ids).size, ids.length, 'tool ids must be unique -- React keys depend on it');
assert.equal(labels.length, ids.length, 'every tool needs a visible label');
assert.equal(hints.length, ids.length, 'every tool needs a hint, which becomes its title attribute');
assert.equal(prompts.length, ids.length, 'every tool needs a prompt to insert');
for (const prompt of prompts) {
  assert.ok(prompt.length > 20, `prompt too thin to be useful: "${prompt}"`);
  // The visitor's own words come last, so the caret lands where they type.
  assert.ok(prompt.endsWith(' ') || prompt.endsWith(': '), `prompt must hand over mid-sentence: "${prompt}"`);
}

// -- the agent is owner-gated, and is a link not a trigger --------------------------------------
assert.ok(strip.includes('canAgent'), 'the coding agent entry must be gated');
assert.ok(strip.includes('canAgent ?'), 'the agent link renders only when canAgent is true');
assert.ok(strip.includes('href="/work"'), 'the agent entry points at the work console');
assert.ok(chat.includes('canAgent={can.trace}'), 'only the owner lane (trace capability) sees the coding agent');

// -- wired into the room, above the composer, always present ------------------------------------
assert.ok(chat.includes('<ToolStrip'), 'the chat renders the strip');
assert.ok(chat.includes('disabled={streaming}'), 'tools are inert while a turn is in flight');
const stripAt = chat.indexOf('<ToolStrip');
const formAt = chat.indexOf('className={styles.composer}');
assert.ok(stripAt > 0 && formAt > 0 && stripAt < formAt, 'the strip sits above the composer, not inside the empty state');

// -- it cannot push the composer off a phone -----------------------------------------------------
assert.ok(css.includes('.tools'), 'the strip has styles');
assert.ok(css.includes('overflow-x: auto'), 'the strip scrolls sideways rather than wrapping and shoving the composer away');
assert.ok(css.includes('min-height: 44px'), 'touch targets reach 44px on coarse pointers');

console.log(`chat-tools.test : OK - ${ids.length} tools, none can send, agent owner-gated, strip above composer`);

// -- per-message row: the same negative invariant, one level down ------------------------------
// The tool strip may not send on your behalf; neither may the per-message handles. "Ask again"
// appends a NEW turn -- it cannot replace the old answer, because nothing in ChatLane can -- so it
// refills the composer and stops. The label says "Ask again" and not "Regenerate" for exactly that
// reason, and this gate is what keeps the two honest with each other.
const askAgain = chat.slice(chat.indexOf('const askAgain'), chat.indexOf('const askAgain') + 700);
assert.ok(askAgain.length > 100, 'askAgain must exist');
assert.ok(!askAgain.includes('send('), 'Ask again must never send -- it refills the composer and stops');
assert.ok(askAgain.includes('setDraft('), 'Ask again must put the question back in the composer');
assert.ok(chat.includes("disabled={Boolean(draft.trim()) || streaming}"), 'Ask again must not clobber a draft in progress');
assert.ok(!chat.includes('>Regenerate<'), 'the label must not promise a replacement this lane cannot perform');

// Copy hands over the RAW source, never the rendered HTML: a code block must paste back with its
// fences intact, and the transcript copy must stay raw markdown.
assert.ok(chat.includes('copyText(message.text,'), 'per-message copy must copy the raw message source');
assert.ok(chat.includes('copyText(transcriptOf(messages)'), 'transcript copy must stay raw');

// Timestamps cost no vertical space: the <time> lives inside the existing author line.
const whoLine = chat.slice(chat.indexOf('className={styles.who}'), chat.indexOf('className={styles.who}') + 400);
assert.ok(whoLine.includes('<time'), 'the timestamp belongs in the author line, not on a row of its own');

console.log('chat-tools.test : OK - tools and per-message handles both refuse to send on your behalf');

// -- the strip must not be squeezable ----------------------------------------------------------
// Observed on a real phone with a long answer on screen: .tools was the only row in the flex
// column without a flex declaration, so it defaulted to `flex: 0 1 auto`, collapsed under a full
// transcript, and clipped its own 44px buttons to slivers above the composer.
const toolsRule = css.slice(css.indexOf('.tools {'), css.indexOf('.tools {') + 500);
assert.ok(toolsRule.includes('flex: 0 0 auto'), 'the strip must not shrink when the transcript fills the column');

// The model writes `* item` far more often than `- item`; matching only the hyphen sent asterisks
// to the reader as literal punctuation.
const md = read('lib/markdown.ts');
assert.ok(md.includes('/^[-*+] (.+)$/'), 'all three CommonMark bullet markers must make list items');
assert.ok(md.includes('testMarkdownStarBullets'), 'the star-bullet behaviour carries its own test');
