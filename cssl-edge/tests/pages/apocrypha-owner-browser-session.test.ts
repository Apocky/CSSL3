import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { runInNewContext } from 'node:vm';
import ts from 'typescript';
import { withDeadline } from '@/lib/apocrypha/deadline';
import { readMemberChatPending, type MemberChatPendingSubmission, type MemberChatStorage } from '@/lib/apocrypha/member-chat-client';

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
  function surface(tree: Tree): string {
    const nodes = children(tree);
    if (nodes.some(node => node.type === 'OwnerConversation')) return 'owner';
    if (nodes.some(node => node.type === 'AccountConversation')) return 'account';
    if (nodes.some(node => node.type === 'main' && node.props.className === 'page' && children(node).some(child => child.props.role === 'alert'))) return 'session-error';
    if (nodes.some(node => node.type === 'main' && node.props.role === 'status')) return 'checking';
    throw new Error('Page has no expected conversation surface.');
  }
  function accountView(tree: Tree): Tree { const node = children(tree).find(node => node.type === 'AccountConversation'); assert.ok(node); return node; }
  function handoff(tree: Tree): Tree { const node = children(tree).find(node => node.type === 'button' && node.props.children === 'Open your main conversation'); assert.ok(node); return node; }
  function harness(
    initialSession: Session,
    load: (subject: string, storage: MemberChatStorage) => unknown | PromiseLike<unknown>,
    ssr = false,
    storage: MemberChatStorage = new MemoryStorage(),
  ) {
    let currentSession = initialSession;
    const cells: unknown[] = []; let cursor = 0; let effectCursor = 0;
    const effects: Array<{ dependencies: readonly unknown[]; cleanup?: () => void }> = [];
    let scheduled: Array<() => void> = [];
    const pageExports: Record<string, any> = {};
    runInNewContext(pageSource, { exports: pageExports, window: { localStorage: storage }, setTimeout: (callback: () => void, delay: number) => setTimeout(callback, Math.min(delay, 10)), clearTimeout, require(name: string) {
      if (name === 'react/jsx-runtime') return { jsx, jsxs: jsx, Fragment: 'Fragment' };
      if (name === 'react') return {
        useState(initial: unknown) { const slot = cursor++; if (!(slot in cells)) cells[slot] = initial; return [cells[slot], (next: unknown) => { cells[slot] = typeof next === 'function' ? next(cells[slot]) : next; }]; },
        useRef(initial: unknown) { const slot = cursor++; if (!(slot in cells)) cells[slot] = { current: initial }; return cells[slot]; },
        useEffect(effect: () => void | (() => void), dependencies: readonly unknown[]) {
          const slot = effectCursor++; const previous = effects[slot];
          if (previous && dependencies.length === previous.dependencies.length && dependencies.every((value, index) => Object.is(value, previous.dependencies[index]))) return;
          scheduled.push(() => { previous?.cleanup?.(); const cleanup = effect(); effects[slot] = { dependencies, ...(typeof cleanup === 'function' ? { cleanup } : {}) }; });
        },
      };
      if (name === '@/components/hub/SiteSession') return { useSiteSession: () => currentSession };
      if (name === '@/lib/apocrypha/member-chat-client') return { readMemberChatPending: load };
      if (name === '@/lib/apocrypha/deadline') return { withDeadline: (operation: PromiseLike<unknown>, deadlineMs: number) => withDeadline(operation, Math.min(deadlineMs, 10)) };
      if (name === '@/components/brain/BrainExperience') return { default: 'OwnerConversation' };
      if (name === '@/components/apocrypha/ChatThread') return { ChatThread: 'OwnerConversation' };
      if (name === '@/components/apocrypha/AccountChat') return { default: 'AccountConversation' };
      if (name === 'next/link') return { default: 'Link' };
      if (name === '@/styles/AccountChat.module.css') return { default: { page: 'page', header: 'header', brand: 'brand', roomTitle: 'roomTitle', welcome: 'welcome', eyebrow: 'eyebrow', welcomeActions: 'welcomeActions', primary: 'primary', secondary: 'secondary', phoneLink: 'phoneLink' } };
      return {};
    } });
    function render(): Tree { cursor = 0; effectCursor = 0; scheduled = []; const tree = pageExports.default({}) as Tree; for (const effect of scheduled) effect(); return tree; }
    return {
      render,
      setSession(next: Session) { currentSession = next; },
      async flush(): Promise<Tree> { await new Promise<void>(resolve => setImmediate(resolve)); return render(); },
    };
  }

  // One chat interface (owner decision 2026-09-25): the owner gets the account conversation like
  // everyone else; the journal handoff machinery is gone with the second surface.
  const owner = harness(ownerSession, async () => null);
  equal(surface(owner.render()), 'account', 'verified owner opens the account conversation, not a second surface');
  equal(surface(await owner.flush()), 'account', 'owner stays on the account conversation after effects settle');
  equal(children(owner.render()).some(node => node.type === 'button' && node.props.children === 'Open your main conversation'), false, 'no owner handoff button remains');
  const operator = harness({ ...ownerSession, ownerConversation: false }, async () => null);
  operator.render(); equal(surface(await operator.flush()), 'account', 'operator gets the same account conversation');
  const signedOut = harness({ access: 'signed-out', ownerConversation: false, authenticated: false, subjectKey: null }, async () => null, true);
  equal(surface(signedOut.render()), 'account', 'signed-out visitors land in the account controller (its own welcome)');
  const checking = harness({ access: 'checking', ownerConversation: false, authenticated: false, subjectKey: null }, async () => null, true);
  equal(surface(checking.render()), 'account', 'checking identity remains inside the public account controller instead of rendering private contents');
  await new Promise(resolve => setTimeout(resolve, 20));
  const tree = checking.render();
  equal(surface(tree), 'session-error', 'never-resolving Supabase auth/session state reaches a visible terminal error before the browser watchdog');
  const recoveryLinks = children(tree).filter(node => node.type === 'Link').map(node => node.props.href);
  equal(recoveryLinks.includes('/login?next=%2Fapocrypha'), true, 'terminal account error exposes the sign-in recovery path');

  console.log('apocrypha-owner-browser-session: ' + checks + ' session and async controller assertions passed; browser acceptance separate');
}
