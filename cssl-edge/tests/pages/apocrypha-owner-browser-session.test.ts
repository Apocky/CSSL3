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
  let account: Record<string, unknown> = { user: { id: 'owner' }, owner_conversation: true };
  let authorized = true;
  let hangSiteFetch = false;
  const sessionExports: Record<string, any> = {};
  runInNewContext(compile(readFileSync(join(root, 'components/hub/SiteSession.tsx'), 'utf8')), { exports: sessionExports, require(name: string) {
      if (name === 'react') return { createContext: () => ({}) };
      if (name.endsWith('/auth')) return { getAuthClient: () => ({ auth: { getSession: async () => ({ data: { session: {} } }) } }) };
      if (name.endsWith('/browser-auth')) return { authFetch: async (url: string) => {
        if (hangSiteFetch) return new Promise<never>(() => undefined);
        return { ok: true, json: async () => url === '/api/auth/me' ? account : { authorized } };
      } };
      if (name.endsWith('/apocrypha/deadline')) return { withDeadline };
      return {};
    } });
  let session = await sessionExports.resolveSiteAccess();
  equal(session.ownerConversation, true, 'browser bearer session admits the server-verified owner');
  account = { user: { id: 'operator' }, owner_conversation: false };
  session = await sessionExports.resolveSiteAccess();
  equal(session.ownerConversation, false, 'admin access alone does not admit another owner conversation');
  account = { user: null, owner_conversation: true };
  equal((await sessionExports.resolveSiteAccess()).ownerConversation, false, 'capability requires authenticated identity');
  account = { user: { id: 'owner' }, owner_conversation: true }; authorized = false;
  equal((await sessionExports.resolveSiteAccess()).ownerConversation, false, 'failed admin admission stays closed');
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
    function render(): Tree { cursor = 0; effectCursor = 0; scheduled = []; const tree = pageExports.default({ ownerConversation: ssr }) as Tree; for (const effect of scheduled) effect(); return tree; }
    return {
      render,
      setSession(next: Session) { currentSession = next; },
      async flush(): Promise<Tree> { await new Promise<void>(resolve => setImmediate(resolve)); return render(); },
    };
  }

  const clear = harness(ownerSession, async () => null);
  equal(surface(clear.render()), 'checking', 'owner waits for the saved-journal decision');
  equal(surface(await clear.flush()), 'owner', 'verified owner with clear journal reaches the owner conversation');

  const pendingRecord = { session_id: 'f1000000-0000-4000-8000-000000000001', request_id: 'f1000000-0000-4000-8000-000000000099', text: 'A saved fixture message.' };
  let saved: unknown = pendingRecord;
  const pending = harness(ownerSession, async () => saved);
  pending.render(); let tree = await pending.flush();
  equal(surface(tree), 'account', 'existing saved account message retains the account controller');
  equal(handoff(tree).props.disabled, true, 'pending reply disables owner handoff');

  const ownerReloadStorage = new MemoryStorage();
  const ownerReloadPending: MemberChatPendingSubmission = {
    conversation_id: 'f1000000-0000-4000-8000-000000000001',
    request_id: 'f1000000-0000-4000-8000-000000000099',
    message: 'A saved fixture message.',
    created_at: '2026-09-08T12:00:00.000Z',
  };
  ownerReloadStorage.setItem(
    `apocky.member-chat.pending.v1.${encodeURIComponent(ownerSession.subjectKey!)}`,
    JSON.stringify(ownerReloadPending),
  );
  const ownerReload = harness(ownerSession, readMemberChatPending, false, ownerReloadStorage);
  ownerReload.render();
  equal(surface(await ownerReload.flush()), 'account', 'reloaded durable member submission keeps the owner on the account recovery surface');

  saved = null; accountView(tree).props.onPendingChange(false); tree = pending.render();
  equal(surface(tree), 'account', 'resolution alone never swaps the active controller');
  equal(handoff(tree).props.disabled, false, 'resolution makes an explicit owner handoff available');
  handoff(tree).props.onClick(); tree = await pending.flush();
  equal(surface(tree), 'owner', 'explicit handoff checks journal again before opening owner conversation');

  const unavailable = harness(ownerSession, async () => { throw new Error('Fixture journal unavailable'); });
  unavailable.render(); tree = await unavailable.flush();
  equal(surface(tree), 'account', 'unavailable journal preserves account recovery surface');
  equal(handoff(tree).props.disabled, true, 'unverified journal cannot authorize owner handoff');

  const hangingJournal = harness(ownerSession, async () => new Promise<never>(() => undefined));
  equal(surface(hangingJournal.render()), 'checking', 'owner journal starts in a bounded checking state');
  await new Promise(resolve => setTimeout(resolve, 20));
  tree = hangingJournal.render();
  equal(surface(tree), 'account', 'hung owner journal terminates on the account recovery surface');
  equal(handoff(tree).props.disabled, true, 'timed-out journal cannot authorize owner handoff');

  saved = pendingRecord; const reappeared = harness(ownerSession, async () => saved);
  reappeared.render(); tree = await reappeared.flush();
  accountView(tree).props.onPendingChange(false); tree = reappeared.render();
  handoff(tree).props.onClick(); tree = await reappeared.flush();
  equal(surface(tree), 'account', 'fresh journal read refuses handoff while a pending message still exists');
  equal(handoff(tree).props.disabled, true, 'refused fresh read restores pending handoff guard');

  const operator = harness({ ...ownerSession, ownerConversation: false }, async () => null);
  operator.render(); equal(surface(await operator.flush()), 'account', 'operator cannot select the owner surface');
  const signedOut = harness({ access: 'signed-out', ownerConversation: false, authenticated: false, subjectKey: null }, async () => null, true);
  equal(surface(signedOut.render()), 'account', 'stale SSR admission does not outlive sign out');
  const checking = harness({ access: 'checking', ownerConversation: false, authenticated: false, subjectKey: null }, async () => null, true);
  equal(surface(checking.render()), 'account', 'checking identity remains inside the public account controller instead of rendering private contents');
  await new Promise(resolve => setTimeout(resolve, 20));
  tree = checking.render();
  equal(surface(tree), 'session-error', 'never-resolving Supabase auth/session state reaches a visible terminal error before the browser watchdog');
  const recoveryLinks = children(tree).filter(node => node.type === 'Link').map(node => node.props.href);
  equal(recoveryLinks.includes('/login?next=%2Fapocrypha'), true, 'terminal account error exposes the sign-in recovery path');

  let resolveOld: (value: unknown) => void = () => undefined;
  const oldRead = new Promise<unknown>(resolve => { resolveOld = resolve; });
  const switched = harness(ownerSession, async subject => subject === ownerSession.subjectKey ? oldRead : null);
  switched.render(); await switched.flush();
  switched.setSession({ ...ownerSession, subjectKey: 'f1000000-0000-4000-8000-000000000102' });
  switched.render(); tree = await switched.flush();
  equal(surface(tree), 'owner', 'new account uses its own completed journal check');
  resolveOld(pendingRecord); tree = await switched.flush();
  equal(surface(tree), 'owner', 'late former-account pending result cannot overwrite the new account decision');

  console.log('apocrypha-owner-browser-session: ' + checks + ' session and async controller assertions passed; browser acceptance separate');
}
