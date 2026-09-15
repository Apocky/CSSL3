import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const read = (path: string): string => readFileSync(resolve(process.cwd(), path), 'utf8');
// There is one chat interface now. The member contract splits cleanly in two: the ROOM owns
// recovery and the reader-facing actions, the LANE owns the member transport — and the lane
// delegates to the same validated client this file has always asserted against.
const room = read('components/apocrypha/ApocryphaChat.tsx');
const lanes = read('lib/apocrypha/chat-lanes.ts');
const client = read('lib/apocrypha/member-chat-client.ts');
const server = read('lib/apocrypha/member-chat.ts');
const publicPage = read('pages/apocrypha.tsx');
const adminPage = read('pages/admin/chat.tsx');

assert.match(lanes, /fetchMemberChatHistoryPage/, 'signed-in chat must load durable paged server history');
assert.match(lanes, /submitMemberChatJob/, 'send must create a durable member job');
assert.match(lanes, /fetchMemberChatJob/, 'accepted jobs must be read back through the validated client');
assert.match(lanes, /isMemberChatUuid\(id\)/, 'member conversation ids must stay validated before use');
assert.match(room, /TERMINAL|snapshot\.done/, 'accepted jobs must be followed to a terminal state');

// Reload recovery, written BEFORE the network mutation. This is the guarantee that makes an
// interrupted send recoverable rather than silently lost, so it is asserted by ORDER, not presence.
assert.match(
  room,
  /writeStored\(activeJobKey\(laneId\), pending\);[\s\S]*?await lane\.send\(/,
  'reload recovery must be written before the network mutation',
);
assert.match(room, /Send the same message again/, 'an ambiguous send must expose an idempotent recovery action');
assert.match(room, /Stopped waiting\. Your message is saved/, 'stopping the browser wait must not imply job cancellation');
assert.match(client, /Your sign-in expired\. Sign in again to continue\./, 'expired sessions must receive a plain recovery message');
assert.match(room, /notice\.startsWith\('Your sign-in'\)[\s\S]*?Sign in again/, 'expired sessions must expose a direct recovery action');
assert.doesNotMatch(room + lanes, /\/api\/mobile\//, 'member chat must not use the legacy mobile adapter');
assert.doesNotMatch(room, /Connection details|Support code|Request reference/, 'the primary chat must not expose operator diagnostics');
assert.match(
  room,
  /isDefiniteRefusal\(sendError\)[\s\S]*?dropStored\(activeJobKey\(laneId\)\)[\s\S]*?setDraft\(text\)/,
  'definitive pre-acceptance rejection must restore an editable draft',
);
// The converse, and the more important half: anything NOT a definite refusal keeps the record, so
// an unresolved send is offered back instead of being treated as a failure that never happened.
assert.match(room, /setUnresolved\(pending\)/, 'an unresolved send must be retained for recovery');
assert.match(room, /status !== 408 && status !== 429/, 'a timeout or a throttle must not be read as a definite refusal');

assert.match(client, /conversation_id=\$\{encodeURIComponent\(conversationId\)\}[\s\S]*?\/api\/apocrypha\/member\/history\?\$\{query\}/, 'history helper must bind the member history route to its conversation query');
assert.match(client, /['"]\/api\/apocrypha\/member\/jobs['"]/, 'submit helper must use the member job route');
assert.match(client, /\/api\/apocrypha\/member\/jobs\/\$\{encodeURIComponent\(jobId\)\}/, 'poll helper must bind reads to the accepted job id');
assert.match(client, /20 \* 60_000/, 'polling must allow a long-running Qwen response');
assert.match(client, /Promise\.race\(\[operation, deadline, callerAbort\]\)/, 'the absolute deadline and caller abort must cover fetch and response decoding');
const controlPattern = (source: string): string | undefined => source
  .match(/const DISALLOWED_MESSAGE_CONTROL_RE = (\/[^\r\n]+\/u);/)?.[1];
assert.equal(controlPattern(client), controlPattern(server), 'browser and server must reject the same control characters');

// One interface, reached by three lanes. The old assertion here required the page to SWAP
// components on sign-in, which is exactly the defect this replaced: the signed-out room had been
// redesigned and the signed-in one had not, so signing in visibly downgraded the product.
assert.match(publicPage, /<ApocryphaChat lane=\{lane\}/, 'every reader gets the same chat component');
assert.match(
  publicPage,
  /owner \? ownerLane\(authFetch\) : account \? memberLane\(authFetch\) : guestLane\(\)/,
  'entitlement selects a transport, not a different chat surface',
);
assert.doesNotMatch(publicPage, /<ChatThread|<AccountChat|<GuestChat/, 'no second chat surface may return');
assert.match(adminPage, /<ApocryphaChat/, 'the dedicated admin route uses the same chat component');
assert.match(adminPage, /ownerLane\(authFetch\)/, 'the admin route keeps the owner transport');

console.log('account-chat.test: 26/26 passed');
