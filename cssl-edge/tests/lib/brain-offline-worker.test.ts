import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';

import { registerMiniBrainOfflineShell } from '../../lib/brain/mini-brain';

const origin = 'https://www.apocky.com';
const currentCache = 'apocky-mini-brain-shell-v2';
const legacyCache = 'apocky-mini-brain-shell-v1';
const handlers = new Map<string, (event: Record<string, unknown>) => void>();
const stores = new Map<string, Map<string, string>>();
const fetches: string[] = [];
let nextResponse: Response | Error = new Error('offline');

function response(body: string, path = '/brain', status = 200, redirected = false): Response {
  const result = new Response(body, { status, headers: { 'content-type': 'text/html' } });
  Object.defineProperties(result, {
    url: { value: `${origin}${path}` },
    type: { value: 'basic' },
    redirected: { value: redirected },
  });
  return result;
}

const cachesFixture = {
  async keys(): Promise<string[]> { return [...stores.keys()]; },
  async delete(name: string): Promise<boolean> { return stores.delete(name); },
  async open(name: string) {
    if (!stores.has(name)) stores.set(name, new Map());
    const entries = stores.get(name)!;
    return {
      async put(key: string | { url: string }, value: Response): Promise<void> {
        entries.set(typeof key === 'string' ? new URL(key, origin).href : key.url, await value.text());
      },
      async match(key: string): Promise<Response | undefined> {
        const body = entries.get(new URL(key, origin).href);
        return body === undefined ? undefined : response(body);
      },
      async addAll(): Promise<void> {},
    };
  },
  async match(key: string): Promise<Response | undefined> {
    for (const entries of stores.values()) {
      const body = entries.get(new URL(key, origin).href);
      if (body !== undefined) return response(body);
    }
    return undefined;
  },
};

runInNewContext(readFileSync('public/brain-sw.js', 'utf8'), {
  URL, Response,
  self: {
    location: { origin },
    addEventListener: (name: string, handler: (event: Record<string, unknown>) => void) => handlers.set(name, handler),
    skipWaiting: async () => undefined,
  },
  caches: cachesFixture,
  fetch: async (request: { url: string }): Promise<Response> => {
    fetches.push(request.url);
    if (nextResponse instanceof Error) throw nextResponse;
    return nextResponse;
  },
});

async function dispatch(path: string, method = 'GET', mode = 'navigate'): Promise<Response | undefined> {
  let pending: Promise<Response> | undefined;
  handlers.get('fetch')!({
    request: { url: new URL(path, origin).href, method, mode },
    respondWith: (value: Promise<Response>) => { pending = value; },
  });
  return pending;
}

async function main(): Promise<void> {
  for (const path of ['/apocrypha', '/account', '/login', '/brain/other', '/api/brain/snapshot', '/api/mobile/turn', '/_next/data/private.json', 'https://other.example/brain']) {
    assert.equal(await dispatch(path), undefined, `${path} must bypass the private worker`);
  }
  assert.equal(await dispatch('/brain', 'POST'), undefined);
  assert.equal(fetches.length, 0, 'bypassed requests must not trigger worker fetches');

  const ownerShell = '<script id="__NEXT_DATA__">{"props":{"pageProps":{"serverAccess":"owner"}}}</script>';
  nextResponse = response(ownerShell);
  assert.equal((await dispatch('/brain'))?.status, 200);
  assert.equal(stores.get(currentCache)?.get(`${origin}/brain`), ownerShell);
  assert.equal(stores.get(currentCache)?.has(`${origin}/apocrypha`), false);

  for (const denied of [
    response('forbidden shell'),
    response(ownerShell, '/login', 200, true),
    response(ownerShell, '/brain', 503),
  ]) {
    nextResponse = denied;
    assert.equal(await dispatch('/brain'), denied, 'network responses must remain authoritative');
    assert.equal(stores.get(currentCache)?.get(`${origin}/brain`), ownerShell, 'denied/redirected responses must not overwrite the sealed owner shell');
  }
  nextResponse = new Error('offline');
  assert.equal(await (await dispatch('/brain'))?.text(), ownerShell, 'only the exact private page restores its offline shell');
  assert.equal(await dispatch('/apocrypha'), undefined);

  stores.set(legacyCache, new Map());
  stores.set('other-provider-cache', new Map());
  let activation: Promise<unknown> | undefined;
  handlers.get('activate')!({ waitUntil: (pending: Promise<unknown>) => { activation = pending; } });
  await activation;
  assert.equal(stores.has(legacyCache), false);
  assert.equal(stores.has(currentCache), true);
  assert.equal(stores.has('other-provider-cache'), true);

  const originalGlobals = Object.fromEntries(['navigator', 'location', 'caches'].map(name => [name, Object.getOwnPropertyDescriptor(globalThis, name)]));
  const removed: string[] = [];
  const registered: Array<{ url: string; scope: string }> = [];
  const worker = (path: string) => ({ scriptURL: `${origin}${path}` });
  const registration = (name: string, scope: string, active: ReturnType<typeof worker>, waiting: ReturnType<typeof worker> | null = null) => ({
    scope: `${origin}${scope}`, active, waiting, installing: null,
    unregister: async () => { removed.push(name); return true; },
  });
  try {
    Object.defineProperties(globalThis, {
      location: { configurable: true, value: { origin } },
      caches: { configurable: true, value: cachesFixture },
      navigator: { configurable: true, value: {
        onLine: false,
        serviceWorker: {
          ready: Promise.resolve({}),
          getRegistrations: async () => [
            registration('legacy-brain', '/', worker('/brain-sw.js')),
            registration('other-provider', '/', worker('/other-sw.js')),
            registration('mixed-transition', '/', worker('/other-sw.js'), worker('/brain-sw.js')),
            registration('current-brain', '/brain', worker('/brain-sw.js')),
          ],
          register: async (url: string, options: { scope: string }) => { registered.push({ url, scope: options.scope }); },
        },
      } },
    });
    stores.set(legacyCache, new Map());
    assert.equal(await registerMiniBrainOfflineShell(), true);
    assert.deepEqual(removed, ['legacy-brain'], 'migration must preserve every unrelated or mixed provider registration');
    assert.deepEqual(registered, [{ url: `${origin}/brain-sw.js`, scope: '/brain' }]);
    assert.equal(stores.has(legacyCache), false);
    assert.equal(stores.has('other-provider-cache'), true);
  } finally {
    for (const name of ['navigator', 'location', 'caches']) {
      const original = originalGlobals[name];
      if (original) Object.defineProperty(globalThis, name, original);
      else Reflect.deleteProperty(globalThis, name);
    }
  }
  console.log('brain-offline-worker.test: OK · private route + owner shell + offline restore + exact registration/cache migration');
}

void main().catch(error => { console.error(error); process.exitCode = 1; });
