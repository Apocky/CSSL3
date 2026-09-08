import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const source = readFileSync(resolve(process.cwd(), 'components/apocrypha/ChatThread.tsx'), 'utf8');
const cognition = readFileSync(resolve(process.cwd(), 'components/apocrypha/CognitionView.tsx'), 'utf8');
const alias = readFileSync(resolve(process.cwd(), 'pages/chat.tsx'), 'utf8');
const ownerPage = readFileSync(resolve(process.cwd(), 'pages/apocrypha.tsx'), 'utf8');

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

export function testSupportedConversationSurface(): void {
  assert(source.includes("/api/admin/apocrypha/conversations?scope=active"), 'durable active history remains visible');
  assert(source.includes('onClick={() => void loadConv(c.id)}'), 'saved conversations remain selectable');
  for (const token of ['onContextMenu', 'aria-haspopup="menu"', 'mutateConversation', "method: 'PATCH'", "'archived'", "'trash'"]) {
    assert(!source.includes(token), `unsupported conversation control must stay hidden: ${token}`);
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

export function testSingleLiveChatPath(): void {
  for (const token of [
    'GetServerSideProps',
    '`/apocrypha${suffix}`',
    'permanent: true',
  ]) {
    assert(alias.includes(token), `chat alias redirect contract missing: ${token}`);
  }
  assert(!alias.includes('ChatThread'), 'the alias must not create a second chat surface');
  assert(ownerPage.includes("height: '100dvh'"), 'the canonical owner chat uses the dynamic viewport');
  assert(ownerPage.includes('<ChatThread />'), 'the canonical owner chat uses the durable UI');
}

export function testSettingsSurfaceIsGatedAndPresentationOnly(): void {
  assert(source.includes('aria-controls="apocrypha-settings"'), 'settings control must expose a target');
  assert(source.includes('id="apocrypha-settings"'), 'settings panel must have stable id');
  assert(source.includes('show tool and run trace'), 'trace visibility toggle missing');
  assert(source.includes('model, authority, and security policy remain server-controlled'), 'settings boundary must be explicit');
}

testSupportedConversationSurface();
testResponsiveSidebarContract();
testChatAccessibilityContract();
testCognitionResponsiveAccessibilityContract();
testSingleLiveChatPath();
testSettingsSurfaceIsGatedAndPresentationOnly();
console.log('admin-chat-ui.test : OK · 6 tests passed');
