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
assert(route.includes('isOpaqueConversationId(body.conversation_id)'), 'route requires conversation identity');
assert(route.includes('isOpaqueClientRequestId(body.request_id)'), 'route requires client-turn identity');
assert(route.includes("upstreamPath: '/v2/turn'"), 'route calls only the V2 body');
assert(!controls.includes('/api/admin/apocrypha/chat'), 'controls never reinterpret chat as a command channel');
assert(!controls.includes('TRIGGER KILL-SWITCH'), 'unimplemented kill control is absent');
assert(controls.includes('No V2 effect control is exposed'), 'controls disclose the authority boundary');
assert(layout.includes('one committed final response'), 'navigation does not advertise synthetic streaming');
assert(layout.includes('V2 effect controls unavailable'), 'navigation does not advertise a false kill control');

console.log('admin-chat.test : OK · V2 chat identity and truthful control boundary passed');
