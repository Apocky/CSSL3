import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const read = (path: string): string => readFileSync(resolve(process.cwd(), path), 'utf8');
const accountChat = read('components/apocrypha/AccountChat.tsx');
const client = read('lib/apocrypha/member-chat-client.ts');
const server = read('lib/apocrypha/member-chat.ts');
const publicPage = read('pages/apocrypha.tsx');
const adminPage = read('pages/admin/chat.tsx');

assert.match(accountChat, /fetchMemberChatHistoryPage/, 'signed-in chat must load durable paged server history');
assert.match(accountChat, /submitMemberChatJob/, 'send must create a durable member job');
assert.match(accountChat, /pollMemberChatJob/, 'accepted jobs must be followed to a terminal state');
assert.match(accountChat, /isMemberChatUuid\(account\)[\s\S]*?const id = account\.toLowerCase\(\)/, 'the account UUID must be the single cross-device conversation id');
assert.doesNotMatch(accountChat, /New chat|createMemberConversationId|loadMemberConversationId/, 'the public room must not fork browser-local conversations');
assert.match(accountChat, /saveMemberChatPending\(account, localStorage, submission\)[\s\S]*?submitMemberChatJob/, 'reload recovery must be written before the network mutation');
assert.match(accountChat, /Retry same message/, 'an ambiguous send must expose an idempotent recovery action');
assert.match(accountChat, /Stopped waiting\. Your message is saved/, 'stopping the browser wait must not imply job cancellation');
assert.match(client, /Your sign-in expired\. Sign in again to continue\./, 'expired sessions must receive a plain recovery message');
assert.match(accountChat, /notice\.startsWith\('Your sign-in'\)[\s\S]*?Sign in again/, 'expired sessions must expose a direct recovery action');
assert.doesNotMatch(accountChat, /\/api\/mobile\//, 'member chat must not use the legacy mobile adapter');
assert.doesNotMatch(accountChat, /Connection details|Support code|Request reference/, 'the primary chat must not expose operator diagnostics');
assert.match(accountChat, /before: cursor/, 'earlier history must follow the opaque server cursor');
assert.match(accountChat, /loadedHistoryPages >= MEMBER_CHAT_MAX_LOADED_HISTORY_PAGES/, 'earlier history loading must remain bounded');
assert.match(accountChat, /isDefinitivePreAcceptanceRejection[\s\S]*?clearMemberChatPending[\s\S]*?setDraft\(submission\.message\)/, 'definitive pre-acceptance rejection must restore an editable draft');
assert.match(accountChat, /MEMBER_CHAT_JOB_NOT_FOUND[\s\S]*?job_id: _discardedJobId[\s\S]*?Retry the same message/, 'a missing accepted job must drop only its stale job id and offer idempotent retry');

assert.match(client, /conversation_id=\$\{encodeURIComponent\(conversationId\)\}[\s\S]*?\/api\/apocrypha\/member\/history\?\$\{query\}/, 'history helper must bind the member history route to its conversation query');
assert.match(client, /['"]\/api\/apocrypha\/member\/jobs['"]/, 'submit helper must use the member job route');
assert.match(client, /\/api\/apocrypha\/member\/jobs\/\$\{encodeURIComponent\(jobId\)\}/, 'poll helper must bind reads to the accepted job id');
assert.match(client, /20 \* 60_000/, 'polling must allow a long-running Qwen response');
assert.match(client, /Promise\.race\(\[operation, deadline, callerAbort\]\)/, 'the absolute deadline and caller abort must cover fetch and response decoding');
const controlPattern = (source: string): string | undefined => source
  .match(/const DISALLOWED_MESSAGE_CONTROL_RE = (\/[^\r\n]+\/u);/)?.[1];
assert.equal(controlPattern(client), controlPattern(server), 'browser and server must reject the same control characters');

assert.match(publicPage, /showOwner \?[\s\S]*?<ChatThread \/>[\s\S]*?: <AccountChat onPendingChange=/, 'the owner route must retain its durable admin chat while members use AccountChat');
assert.match(adminPage, /<ChatThread \/>/, 'the dedicated admin route must retain the canonical owner chat');

console.log('account-chat.test: 24/24 passed');
