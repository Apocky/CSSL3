import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { runInNewContext } from 'node:vm';
import ts from 'typescript';
import { withDeadline } from '@/lib/apocrypha/deadline';
import type { MemberChatStorage } from '@/lib/apocrypha/member-chat-client';

class MemoryStorage implements MemberChatStorage {
  private readonly values = new Map<string, string>();
  getItem(key: string): string | null { return this.values.get(key) ?? null; }
  setItem(key: string, value: string): void { this.values.set(key, value); }
  removeItem(key: string): void { this.values.delete(key); }
}

// § Actual session admission + actual page controller ; async hook harness ≠ browser/auth runtime proof
export async function main(root: string): Promise<void> {
  const compile = (source: string) => ts.transpileModule(source, { compilerOptions: {
    module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, jsx: ts.JsxEmit.ReactJSX,
  } }).outputText;
  let checks = 0;
  const equal = (actual: unknown, expected: unknown, message: string): void => { assert.equal(actual, expected, message); checks += 1; };
  const browserAuthExports: Record<string, any> = {};
  let protectedFetches = 0;
  let mirrorAttempts = 0;
  runInNewContext(compile(readFileSync(join(root, 'lib/browser-auth.ts'), 'utf8')), {
    exports: browserAuthExports,
    Headers,
    fetch: async () => { protectedFetches += 1; return { ok: true }; },
    require(name: string) {
      if (name.endsWith('/auth')) return {
        getAuthClient: () => ({ auth: { getSession: async () => ({ data: { session: { access_token: 'browser-token' } } }) } }),
        persistSessionToCookie: async () => { mirrorAttempts += 1; return new Promise<boolean>(() => undefined); },
      };
      return {};
    },
  });
  await browserAuthExports.authFetch('/api/protected');
  equal(protectedFetches, 1, 'protected request starts without waiting for a second session-mirror verification');
  equal(mirrorAttempts, 0, 'ordinary protected requests do not remint the session cookie');
  let account: Record<string, unknown> = { user: { id: 'owner' }, owner_conversation: true, authorized: true };
  let authorized = true;
  let hangSiteFetch = false;
  let siteFetchDelayMs = 0;
  let siteFetchCount = 0;
  const sessionExports: Record<string, any> = {};
  runInNewContext(compile(readFileSync(join(root, 'components/hub/SiteSession.tsx'), 'utf8')), { exports: sessionExports, require(name: string) {
      if (name === 'react') return { createContext: () => ({}) };
      if (name.endsWith('/auth')) return { getAuthClient: () => ({ auth: { getSession: async () => ({ data: { session: {} } }) } }) };
      if (name.endsWith('/browser-auth')) return { authFetch: async (url: string) => {
        siteFetchCount += 1;
        if (hangSiteFetch) return new Promise<never>(() => undefined);
        if (siteFetchDelayMs > 0) await new Promise((resolve) => setTimeout(resolve, siteFetchDelayMs));
        return { ok: true, json: async () => url === '/api/auth/me' ? account : { authorized } };
      } };
      if (name.endsWith('/apocrypha/deadline')) return { withDeadline };
      return {};
    } });
  let session = await sessionExports.resolveSiteAccess();
  equal(session.ownerConversation, true, 'browser bearer session admits the server-verified owner');
  equal(siteFetchCount, 1, 'combined identity response avoids a duplicate admin verification request');
  account = { user: { id: 'operator' }, owner_conversation: false, authorized: true };
  session = await sessionExports.resolveSiteAccess();
  equal(session.ownerConversation, false, 'admin access alone does not admit another owner conversation');
  account = { user: null, owner_conversation: true, authorized: true };
  equal((await sessionExports.resolveSiteAccess()).ownerConversation, false, 'capability requires authenticated identity');
  account = { user: { id: 'owner' }, owner_conversation: true, authorized: false }; authorized = false;
  equal((await sessionExports.resolveSiteAccess()).ownerConversation, false, 'failed admin admission stays closed');
  account = { user: { id: 'owner' }, owner_conversation: true, authorized: true };
  siteFetchDelayMs = 30;
  session = await sessionExports.resolveSiteAccess(50);
  equal(session.access, 'owner', 'one combined verification completes inside the browser deadline');
  siteFetchDelayMs = 0;
  hangSiteFetch = true;
  const deadlineStarted = Date.now();
  session = await sessionExports.resolveSiteAccess(10);
  equal(session.access, 'unavailable', 'hung session admission terminates in an explicit unavailable state');
  equal(Date.now() - deadlineStarted < 100, true, 'hung session admission settles before the outer browser watchdog');
  hangSiteFetch = false;

  type Tree = { type: unknown; props: Record<string, any> };
  type Session = { access: string; ownerConversation: boolean; authenticated: boolean; subjectKey: string | null };
  const jsx = (type: unknown, props: Record<string, any>): Tree => ({ type, props });
  const pageSource = compile(readFileSync(join(root, 'pages/apocrypha.tsx'), 'utf8'));
  const ownerSession: Session = { access: 'owner', ownerConversation: true, authenticated: true, subjectKey: 'f1000000-0000-4000-8000-000000000101' };
  function children(value: unknown): Tree[] {
    if (Array.isArray(value)) return value.flatMap(children);
    if (!value || typeof value !== 'object' || !('props' in value)) return [];
    const node = value as Tree;
    const rendered = typeof node.type === 'function' ? (node.type as (props: Record<string, unknown>) => Tree)(node.props) : null;
    return [node, ...children(rendered), ...children(node.props.children)];
  }
  // One chat interface, so the surface is no longer "which component rendered" but "which LANE the
  // page handed it". That is the security-relevant fact: entitlement now travels as a transport,
  // and an unresolved session must never be handed the owner one.
  function surface(tree: Tree): string {
    const node = children(tree).find(item => item.type === 'Conversation');
    if (node) return String((node.props.lane as { id?: string } | undefined)?.id ?? 'unknown');
    if (children(tree).some(item => item.type === 'main' && item.props.className === 'page'
      && children(item).some(child => child.props.role === 'alert'))) return 'session-error';
    throw new Error('Page has no conversation surface at all.');
  }
  function harness(initialSession: Session, ssr = false) {
    const pageExports: Record<string, any> = {};
    const cells: unknown[] = [];
    const memos: Array<{ dependencies: readonly unknown[]; value: unknown } | undefined> = [];
    const effects: Array<{ dependencies: readonly unknown[]; cleanup?: () => void } | undefined> = [];
    let cursor = 0;
    let memoCursor = 0;
    let effectCursor = 0;
    let scheduled: Array<() => void> = [];
    let currentSession = initialSession;
    const same = (left: readonly unknown[] | undefined, right: readonly unknown[]): boolean =>
      Boolean(left && left.length === right.length && right.every((value, index) => Object.is(value, left[index])));
    runInNewContext(pageSource, { exports: pageExports, window: { localStorage: new MemoryStorage() },
      setTimeout: (callback: () => void, delay: number) => setTimeout(callback, Math.min(delay, 10)), clearTimeout,
      require(name: string) {
        if (name === 'react/jsx-runtime') return { jsx, jsxs: jsx, Fragment: 'Fragment' };
        if (name === 'react') return {
          useState(initial: unknown) { const slot = cursor++; if (!(slot in cells)) cells[slot] = initial; return [cells[slot], (next: unknown) => { cells[slot] = typeof next === 'function' ? (next as (p: unknown) => unknown)(cells[slot]) : next; }]; },
          useRef(initial: unknown) { const slot = cursor++; if (!(slot in cells)) cells[slot] = { current: initial }; return cells[slot]; },
          useMemo(factory: () => unknown, dependencies: readonly unknown[]) {
            const slot = memoCursor++;
            const previous = memos[slot];
            if (!previous || !same(previous.dependencies, dependencies)) memos[slot] = { dependencies, value: factory() };
            return memos[slot]!.value;
          },
          useEffect(effect: () => void | (() => void), dependencies: readonly unknown[]) {
            const slot = effectCursor++; const previous = effects[slot];
            if (previous && same(previous.dependencies, dependencies)) return;
            scheduled.push(() => { previous?.cleanup?.(); const cleanup = effect(); effects[slot] = { dependencies, ...(typeof cleanup === 'function' ? { cleanup } : {}) }; });
          },
        };
        if (name === '@/components/hub/SiteSession') return { useSiteSession: () => currentSession };
        // Marker lanes: the point of the assertions below is WHICH one the page chose.
        if (name === '@/lib/apocrypha/chat-lanes') return {
          ownerLane: () => ({ id: 'owner' }),
          memberLane: () => ({ id: 'member' }),
          guestLane: () => ({ id: 'guest' }),
        };
        if (name === '@/lib/browser-auth') return { authFetch: async () => ({ ok: true }) };
        if (name === '@/components/apocrypha/ApocryphaChat') return { __esModule: true, default: 'Conversation' };
        if (name === 'next/link') return { __esModule: true, default: 'Link' };
        if (name === '@/styles/AccountChat.module.css') return { __esModule: true, default: new Proxy({}, { get: (_t, key) => String(key) }) };
        return {};
      } });
    function render(): Tree { cursor = 0; memoCursor = 0; effectCursor = 0; scheduled = []; const tree = pageExports.default({ ownerConversation: ssr }) as Tree; for (const effect of scheduled) effect(); return tree; }
    return {
      render,
      setSession(next: Session) { currentSession = next; },
      async flush(): Promise<Tree> { await new Promise<void>(resolve => setImmediate(resolve)); return render(); },
    };
  }

  const ownerRoom = harness(ownerSession);
  equal(surface(await ownerRoom.flush()), 'owner', 'a verified owner is handed the owner transport');

  const memberRoom = harness({ access: 'member', ownerConversation: false, authenticated: true, subjectKey: 'f1000000-0000-4000-8000-000000000102' });
  equal(surface(await memberRoom.flush()), 'member', 'a verified member is handed the account-scoped transport');

  const guestRoom = harness({ access: 'signed-out', ownerConversation: false, authenticated: false, subjectKey: null });
  equal(surface(await guestRoom.flush()), 'guest', 'a signed-out visitor still reaches the room, on the open transport');

  // The security assertion this file exists for. `ssr = true` is a request that WAS server-bound to
  // the owner; if the live session has not confirmed it, the owner transport must not be handed out.
  const unresolved = harness({ access: 'checking', ownerConversation: false, authenticated: false, subjectKey: null }, true);
  const unresolvedSurface = surface(unresolved.render());
  equal(unresolvedSurface === 'owner', false, 'stale request-time admission must not hand over the owner transport while the session is unresolved');
  equal(unresolvedSurface, 'guest', 'an unresolved session falls back to the least-privileged transport');

  const unavailable = harness({ access: 'checking', ownerConversation: false, authenticated: false, subjectKey: null }, true);
  unavailable.render();
  await new Promise(resolve => setTimeout(resolve, 20));
  const tree = unavailable.render();
  equal(surface(tree), 'session-error', 'never-resolving auth state reaches a visible terminal error before the browser watchdog');
  const recoveryLinks = children(tree).filter(node => node.type === 'Link').map(node => node.props.href);
  equal(recoveryLinks.includes('/login?next=%2Fapocrypha'), true, 'terminal account error exposes the sign-in recovery path');

  // Switching identity must re-derive the transport, not keep the previous one alive.
  const switched = harness(ownerSession);
  await switched.flush();
  switched.setSession({ access: 'member', ownerConversation: false, authenticated: true, subjectKey: 'f1000000-0000-4000-8000-000000000103' });
  equal(surface(await switched.flush()), 'member', 'losing owner admission immediately drops the owner transport');

  console.log('apocrypha-owner-browser-session: ' + checks + ' session and transport-entitlement assertions passed; browser acceptance separate');
}

void main(process.cwd()).catch((error: unknown) => {
  console.error(error);
  process.exit(1);
});
