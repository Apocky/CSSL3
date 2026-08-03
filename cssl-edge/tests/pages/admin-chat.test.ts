import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

function source(path: string): string {
  return readFileSync(resolve(process.cwd(), path), 'utf8');
}

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

const page = source('pages/admin/chat.tsx');
const thread = source('components/apocrypha/ChatThread.tsx');
const route = source('pages/api/admin/apocrypha/chat.ts');
const controls = source('pages/admin/controls.tsx');
const layout = source('components/AdminLayout.tsx');

assert(page.includes('<ChatThread />'), 'admin chat uses the canonical V2 chat component');
assert(page.includes('adminAuthorized'), 'admin chat remains owner-gated');
assert(thread.includes('conversation_id: conversationId'), 'chat sends the retained conversation ID');
assert(thread.includes('request_id: requestId'), 'chat sends one stable client-turn request ID');
assert(thread.includes('setRetryTurn({ text, requestId })'), 'failed turn retains its stable request ID');
assert(thread.includes('send(retryTurn)'), 'retry reuses the retained request ID');
assert(thread.includes('PENDING_TURN_STORAGE_KEY'), 'pending turn survives a tab reload');
assert(thread.includes('writePendingTurn(conversationId, { text, requestId })'), 'turn identity persists before dispatch');
assert(thread.includes('readPendingTurn(resolved)'), 'pending turn is recovered on remount');
assert(thread.includes('response.status === 409'), 'conflicts retain the same retry identity');
assert(thread.includes('message.id !== localMessageId'), 'non-retryable optimistic messages are removed');
assert(route.includes('isOpaqueConversationId(body.conversation_id)'), 'route requires conversation identity');
assert(route.includes('isOpaqueClientRequestId(body.request_id)'), 'route requires client-turn identity');
assert(route.includes('submitRuntimeChat'), 'route calls only the direct Apocv4 chat runtime');
assert(route.includes('requestId: scopedRequestId'), 'route forwards the scoped request identity');
assert(route.includes('privacyPartition: OWNER_PRIVACY_PARTITION'), 'route fixes the server-owned privacy partition');
assert(route.includes('runtime.request_id === scopedRequestId'), 'route verifies the echoed runtime identity');
assert(route.includes("runtime.outcome === 'completed'"), 'route requires the exact completed outcome');
assert(route.includes("authority.effect_authority === 'NONE'"), 'route rejects effect authority');
assert(route.includes("authority.tool_authority === 'NONE'"), 'route rejects tool authority');
assert(route.includes("kind: 'runtime.chat.completed'"), 'route emits completed operational telemetry');
assert(route.includes("'runtime.chat.contract_rejected'"), 'route emits contract rejection telemetry');
assert(!route.includes('fetchApocryphaV2'), 'route no longer uses the Cloudflare-era V2 transport');
assert(!route.includes('transition_id'), 'route does not synthesize a nonexistent transition');
assert(!route.includes('state_root'), 'route does not synthesize a nonexistent state root');
assert(route.includes("conversation_history: 'not_retained_by_public_interface'"), 'route projects the browser retention boundary');
assert(route.includes('model_id: modelId'), 'route exposes the validated model identity to the browser');
assert(route.includes('response_digest: responseDigest'), 'route exposes the validated response digest to the browser');
assert(thread.includes("body.duplicate_effect_protection === 'not_applicable_no_effect_authority'"), 'chat validates the no-effect retry boundary');
assert(thread.includes('data-capability-effect-authority="NONE"'), 'chat truthfully advertises no effect authority');
assert(thread.includes('data-capability-tool-authority="NONE"'), 'chat truthfully advertises no tool authority');
assert(!controls.includes('/api/admin/apocrypha/chat'), 'controls never reinterpret chat as a command channel');
assert(!controls.includes('TRIGGER KILL-SWITCH'), 'unimplemented kill control is absent');
assert(controls.includes('No V2 effect control is exposed'), 'controls disclose the authority boundary');
assert(layout.includes('one committed final response'), 'navigation does not advertise synthetic streaming');
assert(layout.includes('V2 effect controls unavailable'), 'navigation does not advertise a false kill control');

console.log('admin-chat.test : OK · direct runtime identity, evidence, and no-effect boundary passed');
