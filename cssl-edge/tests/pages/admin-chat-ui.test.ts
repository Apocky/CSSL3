import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const source = readFileSync(resolve(process.cwd(), 'components/apocrypha/ChatThread.tsx'), 'utf8');
const cognition = readFileSync(resolve(process.cwd(), 'components/apocrypha/CognitionView.tsx'), 'utf8');
const publicChat = readFileSync(resolve(process.cwd(), 'pages/chat.tsx'), 'utf8');

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

export function testConversationLifecycleSurface(): void {
  for (const token of [
    'onContextMenu',
    'aria-haspopup="menu"',
    'handleMenuKeyDown',
    'menuTriggerRef',
    'restoreMenuFocus',
    'Conversation actions for',
    "'pin'",
    "'archive'",
    "'trash'",
    "'restore'",
  ]) {
    assert(source.includes(token), `conversation lifecycle control missing: ${token}`);
  }
}

export function testResponsiveSidebarContract(): void {
  for (const token of [
    'COMPACT_CHAT_QUERY',
    'chat-sidebar-backdrop',
    'aria-expanded={sidebarOpen}',
    'aria-modal={compactViewport || undefined}',
    'handleSidebarKeyDown',
    'max-width: calc(100vw - 44px)',
  ]) {
    assert(source.includes(token), `responsive sidebar contract missing: ${token}`);
  }
}

export function testChatAccessibilityContract(): void {
  for (const token of [
    'role="log"',
    'role="status"',
    'role="alert"',
    'aria-label="Message Apocrypha"',
    'aria-describedby="apocrypha-composer-help"',
  ]) {
    assert(source.includes(token), `chat accessibility contract missing: ${token}`);
  }
}

export function testCognitionResponsiveAccessibilityContract(): void {
  for (const token of [
    '@media (max-width: 900px)',
    'grid-template-columns: minmax(0, 1fr);',
    'role="progressbar"',
    'aria-label="Filter event stream by event kind"',
    'role="list"',
    "overflowWrap: 'anywhere'",
  ]) {
    assert(cognition.includes(token), `cognition responsive/accessibility contract missing: ${token}`);
  }
}

export function testPublicChatShortViewportContract(): void {
  for (const token of [
    'overflow-y: auto',
    'chat-access-avatar',
    'max-height: 650px',
    '<h1 role="status"',
    'inset: 0;',
    'background-position: 56px 56px;',
  ]) {
    assert(publicChat.includes(token), `public chat short-viewport contract missing: ${token}`);
  }
  for (const forbidden of ['inset: -30%', 'translate3d(56px, 56px, 0)']) {
    assert(!publicChat.includes(forbidden), `decorative grid expands scroll area: ${forbidden}`);
  }
}

export function testSettingsSurfaceIsGatedAndPresentationOnly(): void {
  assert(source.includes('aria-controls="apocrypha-settings"'), 'settings control must expose a target');
  assert(source.includes('id="apocrypha-settings"'), 'settings panel must have stable id');
  assert(source.includes('show tool and run trace'), 'trace visibility toggle missing');
  assert(source.includes('model, authority, and security policy remain server-controlled'), 'settings boundary must be explicit');
}

testConversationLifecycleSurface();
testResponsiveSidebarContract();
testChatAccessibilityContract();
testCognitionResponsiveAccessibilityContract();
testPublicChatShortViewportContract();
testSettingsSurfaceIsGatedAndPresentationOnly();
console.log('admin-chat-ui.test : OK · 6 tests passed');
