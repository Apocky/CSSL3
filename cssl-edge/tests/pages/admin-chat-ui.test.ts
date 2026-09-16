import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

// There is one chat interface. Each contract below is asserted where it now lives: behaviour in
// the room component, layout in its stylesheet, transport in the lane module.
const source = readFileSync(resolve(process.cwd(), 'components/apocrypha/ApocryphaChat.tsx'), 'utf8');
const style = readFileSync(resolve(process.cwd(), 'styles/ApocryphaChat.module.css'), 'utf8');
const lanes = readFileSync(resolve(process.cwd(), 'lib/apocrypha/chat-lanes.ts'), 'utf8');
const cognition = readFileSync(resolve(process.cwd(), 'components/apocrypha/CognitionView.tsx'), 'utf8');
const alias = readFileSync(resolve(process.cwd(), 'pages/chat.tsx'), 'utf8');
const ownerPage = readFileSync(resolve(process.cwd(), 'pages/apocrypha.tsx'), 'utf8');


// Token checks below run against a COMMENT-STRIPPED view of the source. This is not fastidiousness:
// the previous version of testChatAccessibilityContract asserted source.includes('role="log"'), and
// when role="log" was removed from the markup it kept passing -- on the explanatory comment that
// said why it had been removed. A gate that reads prose about the code is not reading the code.
function codeOnly(text: string): string {
  return text
    .replace(/{\/\*[\s\S]*?\*\/}/g, ' ')
    .replace(/\/\*[\s\S]*?\*\//g, ' ')
    .replace(/^[ 	]*\/\/.*$/gm, ' ');
}
const roomCode = codeOnly(source);

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

export function testSupportedConversationSurface(): void {
  assert(lanes.includes("/api/admin/apocrypha/conversations?scope=active"), 'durable active history remains visible');
  assert(source.includes('onClick={() => void loadConv(c.id)}'), 'saved conversations remain selectable');
  for (const token of ['onContextMenu', 'aria-haspopup="menu"', 'mutateConversation', "method: 'PATCH'", "'archived'", "'trash'"]) {
    assert(!source.includes(token) && !lanes.includes(token), `unsupported conversation control must stay hidden: ${token}`);
  }
}

export function testResponsiveSidebarContract(): void {
  for (const token of [
    'COMPACT_CHAT_QUERY',
    'styles.sidebarBackdrop',
    'aria-expanded={sidebarOpen}',
    'aria-modal={compactViewport || undefined}',
    'handleSidebarKeyDown',
  ]) {
    assert(source.includes(token), `responsive sidebar contract missing: ${token}`);
  }
  for (const token of ['.sidebarBackdrop', '@media (max-width: 767px)', 'max-width: calc(100vw - 44px)']) {
    assert(style.includes(token), `responsive sidebar layout missing: ${token}`);
  }
}

export function testChatAccessibilityContract(): void {
  for (const token of [
    'role="status"',
    'role="alert"',
    'aria-label="Message Apocrypha"',
    'aria-describedby="apocrypha-composer-help"',
    // The transcript is a landmark you can reach and scroll, not a thing that talks.
    'role="region"',
    'aria-label="Messages"',
    'tabIndex={0}',
  ]) {
    assert(roomCode.includes(token), `chat accessibility contract missing: ${token}`);
  }

  // The transcript MUST NOT be a live region. It carried role="log" AND aria-live="polite" while
  // the poll rewrote the entire accumulated answer every 250ms, so a screen reader read the reply
  // from the top, was interrupted, and began again, for as long as the answer took. role="log"
  // carries an IMPLICIT polite live region, so removing only the aria-live attribute would change
  // nothing -- which is why both are banned here and why this comment exists. Do not "restore" it.
  assert(!roomCode.includes('role="log"'), 'the transcript must not be a live region: role="log" carries an implicit polite region');

  // Exactly one live region in the room, and it is the settled announcer: sr-only, role="status",
  // written once per finished turn rather than on every poll tick.
  const liveRegions = roomCode.match(/aria-live=/g) ?? [];
  assert(liveRegions.length === 1, `the room must declare exactly one live region, found ${liveRegions.length}`);
  const announcer = roomCode.split(String.fromCharCode(10)).find((line) => line.includes('aria-live=')) ?? '';
  assert(announcer.includes('role="status"'), 'the one live region must be the settled announcer');
  assert(announcer.includes('styles.srOnly'), 'the settled announcer must be screen-reader-only, not a visible banner');
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
  assert(!alias.includes('ApocryphaChat'), 'the alias must not create a second chat surface');
  assert(style.includes('height: 100dvh'), 'the room uses the dynamic viewport');
  // The point of the whole unification: the page renders ONE chat component, for everyone.
  assert(ownerPage.includes('<ApocryphaChat'), 'the page uses the one chat component');
  for (const retired of ['ChatThread', 'AccountChat', 'GuestChat']) {
    assert(!ownerPage.includes(`<${retired}`), `a second chat surface came back: ${retired}`);
  }
  assert((ownerPage.match(/<ApocryphaChat/gu) ?? []).length === 1, 'the page renders the room exactly once');
}

export function testSettingsSurfaceIsGatedAndPresentationOnly(): void {
  assert(source.includes('aria-controls="apocrypha-settings"'), 'settings control must expose a target');
  assert(source.includes('id="apocrypha-settings"'), 'settings panel must have stable id');
  assert(source.includes('Show tool and run trace'), 'trace visibility toggle missing');
  assert(source.includes('can.trace ?'), 'the trace toggle is offered only where the lane grants it');
  assert(source.includes('model, authority, and security policy remain server-controlled'), 'settings boundary must be explicit');
}

testSupportedConversationSurface();
testResponsiveSidebarContract();
testChatAccessibilityContract();
testCognitionResponsiveAccessibilityContract();
testSingleLiveChatPath();
testSettingsSurfaceIsGatedAndPresentationOnly();
console.log('admin-chat-ui.test : OK · 6 tests passed');
