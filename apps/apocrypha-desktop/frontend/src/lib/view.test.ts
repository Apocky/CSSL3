import assert from 'node:assert/strict';
import test from 'node:test';
import {
  canSend,
  conversationLabel,
  EMPTY_VIEW,
  MAX_TEXT_BYTES,
  promptBytes,
  scopeNote,
  sendBlockedReason,
  type View,
} from './view.ts';

const signedIn: View = { ...EMPTY_VIEW, configured: true, signed_in: true, email: 'person@example.com' };

test('a signed-in account with a real draft may send', () => {
  assert.equal(canSend(signedIn, 'hello', false), true);
  assert.equal(sendBlockedReason(signedIn, 'hello', false), null);
});

test('an empty or whitespace draft is not sendable', () => {
  assert.equal(canSend(signedIn, '', false), false);
  assert.equal(canSend(signedIn, '   \n\t ', false), false);
});

test('an unconfirmed reply closes the composer instead of resending', () => {
  const pending: View = { ...signedIn, pending_request: '0f9c1d2e-3a4b-4c6d-8e8f-90a1b2c3d4e5' };
  assert.equal(canSend(pending, 'hello again', false), false);
  assert.match(sendBlockedReason(pending, 'hello again', false) ?? '', /unconfirmed/);
});

test('a refused account is told to start a new conversation', () => {
  const denied: View = { ...signedIn, access_denied: true };
  assert.equal(canSend(denied, 'hello', false), false);
  assert.match(sendBlockedReason(denied, 'hello', false) ?? '', /new one/);
});

test('signing in is required before sending', () => {
  assert.equal(canSend(EMPTY_VIEW, 'hello', false), false);
  assert.match(sendBlockedReason(EMPTY_VIEW, 'hello', false) ?? '', /Sign in/);
});

test('work in flight closes the composer', () => {
  assert.equal(canSend(signedIn, 'hello', true), false);
});

test('the byte budget matches the service, not the character count', () => {
  assert.equal(promptBytes('  hi  '), 2);
  assert.equal(promptBytes('é'), 2);
  assert.equal(promptBytes('👋'), 4);
  const atLimit = 'x'.repeat(MAX_TEXT_BYTES);
  assert.equal(canSend(signedIn, atLimit, false), true);
  const overLimit = `${atLimit}x`;
  assert.equal(canSend(signedIn, overLimit, false), false);
  assert.match(sendBlockedReason(signedIn, overLimit, false) ?? '', /16 KB/);
  // Four-byte characters must count as four, or a rejected message would look
  // sendable right up to the moment the service refuses it.
  const emoji = '👋'.repeat(MAX_TEXT_BYTES / 4);
  assert.equal(promptBytes(emoji), MAX_TEXT_BYTES);
  assert.equal(canSend(signedIn, `${emoji}x`, false), false);
});

test('conversation labels stay short and never render as blank', () => {
  assert.equal(conversationLabel({ id: 'a', title: '  a   title  ' }), 'a title');
  assert.equal(conversationLabel({ id: 'a', title: '   ' }), 'Untitled conversation');
  const long = conversationLabel({ id: 'a', title: 'y'.repeat(200) });
  assert.equal(long.length, 64);
  assert.ok(long.endsWith('…'));
});

test('a partial history scope is surfaced rather than hidden', () => {
  assert.equal(scopeNote(signedIn), null);
  assert.match(scopeNote({ ...signedIn, history_scope: 'latest_conversation_only' }) ?? '', /most recent/);
  assert.equal(scopeNote({ ...EMPTY_VIEW, history_scope: 'latest_conversation_only' }), null);
});
