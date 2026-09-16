// Room preferences — and whether anything actually reads them.
//
// The settings panel before this had one control, sat inside the message log, and could not be
// closed except by finding its button again. The failure to guard against now is subtler: a panel
// full of controls that store a value nothing consumes. So this asserts both halves — the value
// round-trips, AND the component reads it at the point it would have to.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import {
  DEFAULT_CHAT_PREFS, isSendKey, readChatPrefs, transcriptOf, writeChatPrefs, type ChatPrefs,
} from '@/lib/apocrypha/chat-prefs';

const room = readFileSync(resolve(process.cwd(), 'components/apocrypha/ApocryphaChat.tsx'), 'utf8');

// Source-text gates get fooled by comments in BOTH directions: a positive assertion can pass on
// prose that merely quotes the code (that is how role="log" survived its own removal in
// admin-chat-ui.test.ts), and a negative assertion can FAIL on a comment explaining why the banned
// phrasing was retired. Both are the same mistake -- reading prose about the code as the code.
const roomCode = room
  .replace(/{\/\*[\s\S]*?\*\/}/g, ' ')
  .replace(/\/\*[\s\S]*?\*\//g, ' ')
  .replace(/^[ 	]*\/\/.*$/gm, ' ');
const style = readFileSync(resolve(process.cwd(), 'styles/ApocryphaChat.module.css'), 'utf8');

class Memory {
  private readonly values = new Map<string, string>();
  getItem(key: string): string | null { return this.values.get(key) ?? null; }
  setItem(key: string, value: string): void { this.values.set(key, value); }
}

// ── storage round-trips, and survives everything it will actually meet ─────────────────────────
const store = new Memory();
assert.deepEqual(readChatPrefs(store), DEFAULT_CHAT_PREFS, 'an empty store reads as defaults');

const chosen: ChatPrefs = { enterSends: false, textSize: 'large', showTrace: true, calmMotion: true, preset: 'precise' };
writeChatPrefs(chosen, store);
assert.deepEqual(readChatPrefs(store), chosen, 'a choice round-trips exactly');

store.setItem('apx.chat.prefs.v1', '{not json');
assert.deepEqual(readChatPrefs(store), DEFAULT_CHAT_PREFS, 'corrupt storage reads as defaults, not as a crash');
store.setItem('apx.chat.prefs.v1', '{"textSize":"enormous","enterSends":"yes","junk":1}');
assert.deepEqual(readChatPrefs(store), DEFAULT_CHAT_PREFS, 'unknown values and unknown keys fall back');
assert.deepEqual(
  readChatPrefs({ getItem() { throw new Error('blocked'); } }),
  DEFAULT_CHAT_PREFS,
  'a browser that refuses storage still gets a working room',
);
// Must not throw: a private window blocks writes, and a preference is not worth an exception.
writeChatPrefs(chosen, { setItem() { throw new Error('blocked'); } });

// ── the send key ──────────────────────────────────────────────────────────────────────────────
const on: ChatPrefs = { ...DEFAULT_CHAT_PREFS, enterSends: true };
const off: ChatPrefs = { ...DEFAULT_CHAT_PREFS, enterSends: false };
const key = (over: Partial<KeyboardEvent>) =>
  ({ key: 'Enter', shiftKey: false, ctrlKey: false, metaKey: false, ...over }) as KeyboardEvent;

assert.equal(isSendKey(key({}), on), true, 'Enter sends when Enter sends');
assert.equal(isSendKey(key({ shiftKey: true }), on), false, 'Shift+Enter is a new line');
assert.equal(isSendKey(key({}), off), false, 'Enter is a new line when that is the choice');
assert.equal(isSendKey(key({ shiftKey: true }), off), false, 'Shift+Enter is still a new line');
// The universal shortcut works under BOTH settings: someone who turned Enter off still expects it,
// and someone who left it on loses nothing.
assert.equal(isSendKey(key({ ctrlKey: true }), off), true, 'Ctrl+Enter sends when Enter does not');
assert.equal(isSendKey(key({ metaKey: true }), off), true, 'Cmd+Enter sends when Enter does not');
assert.equal(isSendKey(key({ ctrlKey: true }), on), true, 'Ctrl+Enter also sends when Enter does');
assert.equal(isSendKey(key({ key: 'a' }), on), false, 'an ordinary key is not a send');
assert.equal(isSendKey(key({ key: 'Escape' }), on), false, 'Escape is not a send');

// ── transcript ────────────────────────────────────────────────────────────────────────────────
assert.equal(
  transcriptOf([
    { role: 'user', text: 'A question.', at: new Date(0) },
    { role: 'apocrypha', text: 'An answer.', at: new Date(0) },
  ]),
  'You:\nA question.\n\nApocrypha:\nAn answer.',
  'a copied conversation names who said what',
);
assert.equal(transcriptOf([]), '', 'an empty conversation copies as nothing');

// ── every stored preference is CONSUMED somewhere ─────────────────────────────────────────────
//
// A control that writes a value nothing reads is worse than no control: it reports that something
// happened. Each of these names the place the preference has to take effect.
assert.ok(room.includes('isSendKey(event, prefs)'), 'the composer must honour the send preference');
assert.ok(room.includes('prefs.showTrace'), 'the trace preference must gate the trace');
assert.ok(room.includes('prefs.calmMotion ? styles.caretStill : styles.caret'), 'calm motion must reach the caret');
assert.ok(room.includes("prefs.textSize === 'large'"), 'text size must reach the conversation column');
assert.ok(style.includes('.conversationLarge'), 'the large size must have something to apply');
assert.ok(style.includes('.caretStill'), 'calm motion must have a still caret to apply');
assert.ok(room.includes('writeChatPrefs(next)'), 'a change must be persisted when it is made, not when the panel closes');

// ── the panel is a panel ──────────────────────────────────────────────────────────────────────
assert.ok(room.includes('role="dialog"'), 'settings is a dialog');
assert.ok(room.includes('aria-haspopup="dialog"'), 'the control says what it opens');
assert.ok(room.includes('aria-label="Close settings"'), 'it can be closed without hunting for the toggle');
assert.ok(room.includes("event.key !== 'Escape'") || room.includes("event.key === 'Escape'"), 'Escape closes it');
assert.ok(room.includes('settingsPanelRef'), 'a click outside closes it');
assert.ok(style.includes('.settings {\n  position: absolute'), 'the panel floats rather than displacing the conversation');
// It used to be rendered INSIDE the scrolling message column, so it scrolled away with the thread.
const conversationAt = room.indexOf('aria-label="Apocrypha conversation"');
const settingsAt = room.indexOf('id="apocrypha-settings"');
assert.ok(settingsAt > 0 && conversationAt > settingsAt, 'the panel is not nested inside the scrolling conversation');

// ── a destructive control appears only where it can tell the truth, and asks twice ────────────
//
// Bound to the CONDITION rather than to one spelling of the button, because the markup changed the
// moment the control grew a confirm step and the invariant did not.
// Anchored on the button's own label, not on the capability check: `!can.durableHistory ? <button`
// also matches the settings-toggle block higher in the file, which silently sliced the wrong region.
const forgetAt = room.indexOf('Forget it on this device');
const forgetBlock = room.slice(Math.max(0, forgetAt - 500), forgetAt + 200);
assert.ok(forgetAt > 0, 'the forget control must exist');
assert.ok(forgetBlock.includes('!can.durableHistory'), 'the forget control is gated on the lane, not on identity');
assert.ok(forgetBlock.includes('forgetLocalThread'), 'forgetting a thread is offered only where the thread really is local');
// On the guest lane this is the ONLY copy. One press used to destroy it, from a button dressed
// exactly like "Copy transcript" beside it.
// Assert the BEHAVIOUR, not the vocabulary. An earlier version of this line only checked that the
// word "armedForget" appeared near the button -- which it still did after the confirm was deleted,
// because the label ternary mentions it. Ban the direct binding instead: that is the defect.
assert.ok(
  !roomCode.includes('onClick={forgetLocalThread}'),
  'destroying the only copy of a thread must take two presses, not one direct binding',
);
assert.ok(forgetBlock.includes('armedForget'), 'the forget control must carry its confirm state');
assert.ok(room.includes("'Really forget? This cannot be undone'"), 'the second press must say what it does');
assert.ok(room.includes('setArmedForget(false)'), 'the confirm must disarm itself rather than sitting armed');

// ── the guest footnote must not promise confidentiality it does not have ──────────────────────
//
// The guest lane POSTs each message plus the last 12 turns to /api/apocrypha/guest/chat, because
// something has to answer it. "This conversation stays in this browser" read as a privacy promise.
// What stays local is the RECORD; the words travel. The copy has to say both halves.
assert.ok(
  room.includes('Your messages are sent to Apocrypha to be answered'),
  'the guest footnote must say that messages are sent, not only that the thread is kept locally',
);
assert.ok(
  !roomCode.includes('This conversation stays'),
  'the superseded confidentiality-sounding phrasing must not return',
);

// ── the room is not a dead end ────────────────────────────────────────────────────────────────
//
// The header used to choose BETWEEN the conversations toggle and the link home, so signing in
// removed the only way out and the page became a trap. They are independent controls; the test
// pins that they both exist rather than trusting the markup to stay that way.
assert.ok(
  room.includes('aria-controls="apocrypha-conversations"'),
  'the conversations toggle exists',
);
assert.ok(
  room.includes('<Link href="/" className={styles.brand} aria-label="Apocky home">'),
  'the way home exists',
);
const togglesAt = room.indexOf('aria-controls="apocrypha-conversations"');
const brandAt = room.indexOf('className={styles.brand}');
assert.ok(brandAt > togglesAt, 'the way home sits beside the toggle, not as its alternative');
// The defect exactly: the brand link appearing as the ELSE of the toggle's ternary. Matching the
// `: <Link ... brand` junction rather than mere proximity, because the corrected code legitimately
// has `: null}` followed by that same link a few characters later.
assert.ok(
  !/:\s*<Link href="\/" className=\{styles\.brand\}/u.test(room),
  'the way home must not be the ELSE branch of having conversations',
);
// The DEFECT this protects against is a dead end: a full-height room with no way out. That is
// unchanged. What changed is where "out" goes -- the site is Apocrypha now (owner instruction
// 2026-09-15), so the way out leads further into Apocrypha instead of to four other projects.
for (const href of ['/', '/download/apocrypha', '/account']) {
  assert.ok(room.includes(`<Link href="${href}">`), `the room links back to ${href}`);
}
for (const gone of ['/tools', '/words', '/conversations', '/codex-apockalypsis']) {
  assert.ok(!room.includes(`<Link href="${gone}">`), `the room must no longer advertise ${gone}`);
}
assert.ok(room.includes('aria-label="Apocrypha"'), 'the return links are a named landmark');
assert.ok(style.includes('.returnLinks'), 'the return links are styled rather than raw');

// ── the browser lane must not import a server module ──────────────────────────────────────────
//
// chat-lanes.ts runs in the browser. Importing a single constant from lib/apocrypha/guest-chat.ts
// — a server module that imports node:crypto — made webpack try to bundle node:crypto for the
// client and broke the production build. tsc and the test suite BOTH passed; only `next build`
// caught it, which is why this guard exists at all.
const lanes = readFileSync(resolve(process.cwd(), 'lib/apocrypha/chat-lanes.ts'), 'utf8');
// member-chat-client is deliberately absent: the browser lane imports it on purpose, and it is
// client-safe. What must never appear is a module that reaches for node builtins.
const SERVER_ONLY = ['guest-chat', 'job-control', 'node:crypto', 'node:fs', '@supabase/supabase-js'];
for (const mod of SERVER_ONLY) {
  assert.ok(
    !new RegExp(`from '[^']*${mod}`, 'u').test(lanes),
    `chat-lanes runs in the browser and must not import ${mod}`,
  );
}
assert.ok(
  /from '@\/lib\/apocrypha\/chat-limits'/u.test(lanes),
  'the shared size ceilings come from the dependency-free module',
);

console.log('chat-prefs.test : OK · preferences persist and are consumed; the room has a way out');
