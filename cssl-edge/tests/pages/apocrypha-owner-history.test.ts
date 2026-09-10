import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { runInNewContext } from 'node:vm';

import ts from 'typescript';

type Tree = { type: unknown; props: Record<string, any> };

const CONVERSATION_ID = '11111111-1111-4111-8111-111111111111';
const REQUEST_ID = '22222222-2222-4222-8222-222222222222';
const JOB_ID = '33333333-3333-4333-8333-333333333333';
const ACTIVE_JOB_KEY = 'apocky.apocrypha.active-job.v1';

class MemoryStorage {
  private readonly values = new Map<string, string>();
  getItem(key: string): string | null { return this.values.get(key) ?? null; }
  setItem(key: string, value: string): void { this.values.set(key, value); }
  removeItem(key: string): void { this.values.delete(key); }
}

function children(value: unknown): Tree[] {
  if (Array.isArray(value)) return value.flatMap(children);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Tree;
  const rendered = typeof node.type === 'function'
    ? (node.type as (props: Record<string, unknown>) => unknown)(node.props)
    : null;
  return [node, ...children(rendered), ...children(node.props.children)];
}

function text(value: unknown): string {
  if (Array.isArray(value)) return value.map(text).join('');
  if (value && typeof value === 'object' && 'props' in value) {
    const node = value as Tree;
    const rendered = typeof node.type === 'function'
      ? (node.type as (props: Record<string, unknown>) => unknown)(node.props)
      : null;
    return text(rendered) + text(node.props.children);
  }
  return typeof value === 'string' || typeof value === 'number' ? String(value) : '';
}

function occurrences(value: string, needle: string): number {
  return value.split(needle).length - 1;
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

function sameDependencies(left: readonly unknown[] | undefined, right: readonly unknown[]): boolean {
  return Boolean(left && left.length === right.length && right.every((value, index) => Object.is(value, left[index])));
}

function createHarness(
  source: string,
  authFetch: (url: string, init?: RequestInit) => Promise<Response>,
  storage: MemoryStorage,
) {
  const state: unknown[] = [];
  const callbacks: Array<{ dependencies: readonly unknown[]; value: (...args: any[]) => any } | undefined> = [];
  const effects: Array<{ dependencies: readonly unknown[]; cleanup?: () => void } | undefined> = [];
  let stateCursor = 0;
  let callbackCursor = 0;
  let effectCursor = 0;
  let scheduled: Array<() => void> = [];
  let uuidCursor = 0;
  let timerCursor = 0;
  const timers = new Map<number, () => void>();
  const uuids = [CONVERSATION_ID, REQUEST_ID];
  const jsx = (type: unknown, props: Record<string, any>): Tree => ({ type, props });
  const media = {
    matches: false,
    addEventListener() {},
    removeEventListener() {},
  };
  const windowValue = {
    localStorage: storage,
    crypto: { randomUUID: () => uuids[uuidCursor++] ?? REQUEST_ID },
    matchMedia: () => media,
    setTimeout(callback: () => void) {
      const id = ++timerCursor;
      timers.set(id, callback);
      return id;
    },
    clearTimeout(id: number) {
      timers.delete(id);
    },
    innerWidth: 1280,
    innerHeight: 800,
  };
  const documentValue = {
    activeElement: null,
    addEventListener() {},
    removeEventListener() {},
  };
  const module = { exports: {} as { ChatThread: () => Tree } };
  const compiled = ts.transpileModule(source, {
    compilerOptions: {
      target: ts.ScriptTarget.ES2022,
      module: ts.ModuleKind.CommonJS,
      jsx: ts.JsxEmit.ReactJSX,
      esModuleInterop: true,
    },
  }).outputText;
  runInNewContext(compiled, {
    module,
    exports: module.exports,
    window: windowValue,
    document: documentValue,
    Node: class {},
    AbortController,
    Date,
    setTimeout,
    clearTimeout,
    requestAnimationFrame: (callback: () => void) => { callback(); return 1; },
    cancelAnimationFrame() {},
    require(name: string) {
      if (name === 'react') return {
        useState(initial: unknown) {
          const slot = stateCursor++;
          if (!(slot in state)) state[slot] = typeof initial === 'function' ? (initial as () => unknown)() : initial;
          return [state[slot], (next: unknown) => {
            state[slot] = typeof next === 'function' ? (next as (previous: unknown) => unknown)(state[slot]) : next;
          }];
        },
        useRef(initial: unknown) {
          const slot = stateCursor++;
          if (!(slot in state)) state[slot] = { current: initial };
          return state[slot];
        },
        useCallback(value: (...args: any[]) => any, dependencies: readonly unknown[]) {
          const slot = callbackCursor++;
          const previous = callbacks[slot];
          if (!previous || !sameDependencies(previous.dependencies, dependencies)) {
            callbacks[slot] = { dependencies, value };
          }
          return callbacks[slot]!.value;
        },
        useEffect(effect: () => void | (() => void), dependencies: readonly unknown[]) {
          const slot = effectCursor++;
          const previous = effects[slot];
          if (previous && sameDependencies(previous.dependencies, dependencies)) return;
          scheduled.push(() => {
            previous?.cleanup?.();
            const cleanup = effect();
            effects[slot] = { dependencies, ...(typeof cleanup === 'function' ? { cleanup } : {}) };
          });
        },
      };
      if (name === 'react/jsx-runtime') return { jsx, jsxs: jsx, Fragment: 'Fragment' };
      if (name.endsWith('/browser-auth')) return { authFetch };
      if (name.endsWith('/ApocryphaAvatar')) return { ApocryphaAvatar: 'ApocryphaAvatar' };
      throw new Error(`Unexpected ChatThread dependency: ${name}`);
    },
  }, { filename: 'ChatThread.tsx' });

  return {
    render(): Tree {
      stateCursor = 0;
      callbackCursor = 0;
      effectCursor = 0;
      scheduled = [];
      const tree = module.exports.ChatThread();
      for (const effect of scheduled) effect();
      return tree;
    },
    async flush(): Promise<Tree> {
      await new Promise<void>((resolveFlush) => setImmediate(resolveFlush));
      return this.render();
    },
    advanceTimers(): number {
      const pending = [...timers.values()];
      timers.clear();
      for (const callback of pending) callback();
      return pending.length;
    },
    unmount(): void {
      for (const effect of effects) effect?.cleanup?.();
    },
  };
}

async function main(): Promise<void> {
  const source = readFileSync(resolve(process.cwd(), 'components/apocrypha/ChatThread.tsx'), 'utf8');
  assert.match(
    source,
    /activeJob\.conversationId[\s\S]*?snapshot\.job\.request\?\.conversation_id[\s\S]*?snapshot\.job\.id/,
    'pre-patch in-flight jobs recover their durable conversation identity from the job UUID',
  );
  const storage = new MemoryStorage();
  const calls: Array<{ url: string; init?: RequestInit }> = [];
  const statusReads: string[] = [];
  let admitted = false;
  let jobStatus: 'queued' | 'leased' | 'succeeded' = 'queued';
  let delayedDetail: Promise<void> | null = null;
  let releaseDelayedDetail: (() => void) | null = null;
  const priorSummary = {
    id: CONVERSATION_ID,
    title: 'Durable conversation',
    last_active_iso: '2026-09-08T12:01:00.000Z',
    message_count: 2,
    state: 'active',
  };
  const priorMessages = [
    { id: 'prior:user', role: 'user', text: 'Earlier durable question.', ts_iso: '2026-09-08T12:00:00.000Z', tool_trace: [] },
    { id: 'prior:apocrypha', role: 'apocrypha', text: 'Earlier durable answer.', ts_iso: '2026-09-08T12:01:00.000Z', tool_trace: [] },
  ];
  const pendingMessage = {
    id: `${JOB_ID}:user`, role: 'user', text: 'Continue while this job is in flight.',
    ts_iso: '2026-09-08T12:02:00.000Z', tool_trace: [],
  };
  const finalAnswer = 'One durable final answer.';
  const authFetch = async (url: string, init?: RequestInit): Promise<Response> => {
    calls.push({ url, ...(init ? { init } : {}) });
    if (url === '/api/admin/apocrypha/jobs') {
      const body = JSON.parse(String(init?.body ?? '{}')) as Record<string, unknown>;
      assert.equal(body.conversation_id, CONVERSATION_ID, 'follow-up admission preserves the selected durable conversation UUID');
      admitted = true;
      return jsonResponse({ ok: true, accepted: true, conversation_id: CONVERSATION_ID, job: { id: JOB_ID } }, 202);
    }
    if (url === `/api/admin/apocrypha/jobs/${JOB_ID}`) {
      statusReads.push(jobStatus);
      return jsonResponse({
        ok: true,
        job: { id: JOB_ID, status: jobStatus, request: { conversation_id: CONVERSATION_ID } },
        chunks: [],
        revisions: jobStatus === 'succeeded'
          ? [{ content: finalAnswer, provenance: {}, usage: {} }]
          : [],
      });
    }
    if (url === '/api/admin/apocrypha/conversations?scope=active') {
      return jsonResponse({
        upstream_status: 200,
        data: {
          conversations: [{
            ...priorSummary,
            message_count: priorMessages.length + (admitted ? 1 : 0) + (jobStatus === 'succeeded' ? 1 : 0),
          }],
        },
      });
    }
    if (url === `/api/admin/apocrypha/conversations?id=${CONVERSATION_ID}`) {
      if (delayedDetail) await delayedDetail;
      const messages = admitted
        ? [
            ...priorMessages,
            pendingMessage,
            ...(jobStatus === 'succeeded' ? [{
              id: `${JOB_ID}:apocrypha`,
              role: 'apocrypha',
              text: 'Bounded stored prefix only.',
              ts_iso: '2026-09-08T12:03:00.000Z',
              tool_trace: [],
              truncated: true,
            }] : []),
          ]
        : priorMessages;
      return jsonResponse({ upstream_status: 200, data: { conversation: priorSummary, messages } });
    }
    throw new Error(`Unexpected ChatThread request: ${url}`);
  };

  const first = createHarness(source, authFetch, storage);
  let tree = first.render();
  tree = await first.flush();
  tree = await first.flush();
  assert.match(text(tree), /Earlier durable question\./, 'initial mount restores the prior durable user turn');
  assert.match(text(tree), /Earlier durable answer\./, 'initial mount restores the prior durable answer');
  const textarea = children(tree).find((node) => node.type === 'textarea' && node.props['aria-label'] === 'Message Apocrypha');
  assert.ok(textarea, 'composer renders');
  textarea.props.onChange({ target: { value: pendingMessage.text } });
  tree = first.render();
  const send = children(tree).find((node) => node.type === 'button' && node.props['aria-label'] === 'Send message');
  assert.ok(send, 'send control renders');
  send.props.onClick();
  await new Promise<void>((resolveFlush) => setImmediate(resolveFlush));
  tree = first.render();
  assert.match(text(tree), new RegExp(`conv #${CONVERSATION_ID}`), 'accepted receipt immediately selects the durable conversation UUID');
  const activeRecord = JSON.parse(storage.getItem(ACTIVE_JOB_KEY) ?? '{}') as Record<string, unknown>;
  assert.equal(activeRecord.conversationId, CONVERSATION_ID, 'accepted receipt journals the durable conversation UUID with the active job');

  tree = await first.flush();
  assert.deepEqual(statusReads, ['queued'], 'the accepted job remains queued before the remount');
  assert.match(text(tree), /Earlier durable question\./, 'prior durable user turn remains visible before remount');
  assert.match(text(tree), /Earlier durable answer\./, 'prior durable answer remains visible before remount');
  assert.match(text(tree), /Accepted\. Waiting for the local Apocrypha node/, 'queued status remains visible before remount');
  assert.notEqual(storage.getItem(ACTIVE_JOB_KEY), null, 'queued job remains recoverable in the active-job journal');
  first.unmount();

  jobStatus = 'leased';
  const detailReadsBeforeRemount = calls.filter((call) => (
    call.url === `/api/admin/apocrypha/conversations?id=${CONVERSATION_ID}`
  )).length;
  const reloaded = createHarness(source, authFetch, storage);
  reloaded.render();
  tree = await reloaded.flush();
  tree = await reloaded.flush();
  assert.match(text(tree), /Earlier durable question\./, 'in-flight remount keeps the prior durable user turn visible');
  assert.match(text(tree), /Earlier durable answer\./, 'in-flight remount keeps the prior durable answer visible');
  assert.equal(occurrences(text(tree), pendingMessage.text), 1, 'in-flight remount restores the accepted prompt exactly once');
  assert.match(text(tree), /The Apocrypha node has claimed this thought/, 'remount reconnects to the running job');
  assert.deepEqual(statusReads, ['queued', 'leased'], 'the remounted component resumes polling the accepted job');
  assert.ok(
    calls.filter((call) => call.url === `/api/admin/apocrypha/conversations?id=${CONVERSATION_ID}`).length
      > detailReadsBeforeRemount,
    'in-flight remount reloads the durable conversation detail before composing transient job state',
  );

  jobStatus = 'succeeded';
  assert.equal(reloaded.advanceTimers(), 1, 'the running job has one scheduled follow-up poll');
  tree = await reloaded.flush();
  tree = await reloaded.flush();
  assert.equal(occurrences(text(tree), finalAnswer), 1, 'the terminal answer appears exactly once after in-flight recovery');
  assert.match(text(tree), /Earlier durable answer\./, 'terminal completion preserves the prior durable history');
  assert.equal(storage.getItem(ACTIVE_JOB_KEY), null, 'terminal job clears only the transient active-job journal');
  tree = await reloaded.flush();
  assert.equal(occurrences(text(tree), finalAnswer), 1, 'a settled rerender does not duplicate the terminal answer');
  reloaded.unmount();

  storage.setItem(ACTIVE_JOB_KEY, JSON.stringify({
    id: JOB_ID,
    prompt: pendingMessage.text,
    submittedAt: pendingMessage.ts_iso,
  }));
  jobStatus = 'leased';
  const legacyReload = createHarness(source, authFetch, storage);
  legacyReload.render();
  tree = await legacyReload.flush();
  tree = await legacyReload.flush();
  tree = await legacyReload.flush();
  assert.match(text(tree), /Earlier durable question\./, 'a pre-patch active-job journal recovers prior durable history');
  assert.match(text(tree), /Earlier durable answer\./, 'a pre-patch journal restores the prior answer after resolving its conversation id');
  assert.equal(occurrences(text(tree), pendingMessage.text), 1, 'a pre-patch journal preserves its active prompt exactly once');
  legacyReload.unmount();

  storage.setItem(ACTIVE_JOB_KEY, JSON.stringify({
    id: JOB_ID,
    prompt: pendingMessage.text,
    submittedAt: pendingMessage.ts_iso,
  }));
  jobStatus = 'succeeded';
  delayedDetail = new Promise<void>((resolveDetail) => { releaseDelayedDetail = resolveDetail; });
  const terminalReload = createHarness(source, authFetch, storage);
  tree = terminalReload.render();
  tree = await terminalReload.flush();
  assert.match(text(tree), /Reconnected\. Apocrypha is continuing this answer/, 'first-poll terminal recovery waits for durable history');
  assert.notEqual(storage.getItem(ACTIVE_JOB_KEY), null, 'terminal recovery keeps its journal while history is unresolved');
  assert.ok(releaseDelayedDetail, 'the terminal hydration request is held for the ordering regression');
  (releaseDelayedDetail as () => void)();
  delayedDetail = null;
  tree = await terminalReload.flush();
  tree = await terminalReload.flush();
  tree = await terminalReload.flush();
  assert.equal(occurrences(text(tree), finalAnswer), 1, 'first-poll terminal recovery preserves the full terminal revision exactly once');
  assert.doesNotMatch(text(tree), /Bounded stored prefix only\./, 'late bounded hydration cannot overwrite the full terminal revision');
  assert.match(text(tree), /Earlier durable answer\./, 'first-poll terminal recovery retains earlier conversation history');
  assert.equal(storage.getItem(ACTIVE_JOB_KEY), null, 'first-poll terminal recovery clears its journal only after composition');
  terminalReload.unmount();

  console.log('apocrypha-owner-history UI test: in-flight durable remount and exactly-once terminal recovery OK');
}

void main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
