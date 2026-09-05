/* Apocrypha Mini Brain shell only. Private API responses and user payloads are never cached here. */
'use strict';

const CACHE = 'apocky-mini-brain-shell-v2';
const CONTROL_PROTOCOL = 'apocky.mini-brain.control.v1';
const LOCK_ACK_TIMEOUT_MS = 1000;
const pendingLocks = new Map();
const STATIC = [
  '/brain-manifest.json',
  '/icons/apocky-v3-192.png',
  '/icons/apocky-v3-512.png',
  '/icons/apocky-maskable-v3-192.png',
  '/icons/apocky-maskable-v3-512.png',
];

self.addEventListener('install', event => {
  event.waitUntil(caches.open(CACHE).then(cache => cache.addAll(STATIC)).then(() => self.skipWaiting()));
});

self.addEventListener('activate', event => {
  event.waitUntil(
    caches.keys()
      .then(keys => Promise.all(keys.filter(key => key.startsWith('apocky-mini-brain-shell-') && key !== CACHE).map(key => caches.delete(key))))
  );
});

self.addEventListener('fetch', event => {
  const request = event.request;
  if (request.method !== 'GET') return;
  const url = new URL(request.url);
  if (url.origin !== self.location.origin || url.pathname.startsWith('/api/')) return;

  if (request.mode === 'navigate' && url.pathname === '/apocrypha') {
    event.respondWith(
      (async () => {
        try {
          const response = await fetch(request);
          const finalUrl = new URL(response.url);
          if (
            response.ok
            && response.type === 'basic'
            && !response.redirected
            && finalUrl.origin === self.location.origin
            && finalUrl.pathname === '/apocrypha'
          ) {
            const inspection = response.clone();
            const html = await inspection.text();
            if (html.includes('"serverAccess":"owner"')) {
              const cache = await caches.open(CACHE);
              await cache.put('/apocrypha', response.clone());
            }
          }
          return response;
        } catch {
          return (await caches.match('/apocrypha')) ?? Response.error();
        }
      })(),
    );
    return;
  }

  if (url.pathname.startsWith('/_next/static/') || STATIC.includes(url.pathname)) {
    event.respondWith(
      (async () => {
        const cached = await caches.match(request);
        if (cached) return cached;
        const response = await fetch(request);
        if (response.ok && response.type === 'basic') {
          const cache = await caches.open(CACHE);
          await cache.put(request, response.clone());
        }
        return response;
      })(),
    );
  }
});

function privateBrainClient(client) {
  try {
    const url = new URL(client.url);
    return url.origin === self.location.origin && (url.pathname === '/apocrypha' || url.pathname === '/brain');
  } catch {
    return false;
  }
}

function finishLock(requestId, status) {
  const pending = pendingLocks.get(requestId);
  if (!pending) return;
  pendingLocks.delete(requestId);
  clearTimeout(pending.timeout);
  try {
    pending.port.postMessage({
      schema_version: CONTROL_PROTOCOL,
      type: 'LOCK_MINI_BRAIN_RESULT',
      request_id: requestId,
      status,
    });
  } catch { /* the caller's bounded fallback remains authoritative */ }
  try { pending.port.close(); } catch { /* MessagePort.close is best-effort */ }
  pending.resolve(status === 'acknowledged');
}

async function relayLock(event, message) {
  const port = event.ports && event.ports[0];
  if (!port || typeof message.request_id !== 'string' || pendingLocks.has(message.request_id)) return;
  const clients = (await self.clients.matchAll({ type: 'window', includeUncontrolled: true })).filter(privateBrainClient);
  if (clients.length === 0) {
    port.postMessage({
      schema_version: CONTROL_PROTOCOL,
      type: 'LOCK_MINI_BRAIN_RESULT',
      request_id: message.request_id,
      status: 'acknowledged',
    });
    return;
  }
  return await new Promise(resolve => {
    const pending = {
      port,
      resolve,
      expected: new Set(clients.map(client => client.id)),
      timeout: null,
    };
    pendingLocks.set(message.request_id, pending);
    pending.timeout = setTimeout(() => finishLock(message.request_id, 'unconfirmed'), LOCK_ACK_TIMEOUT_MS);
    for (const client of clients) {
      try {
        client.postMessage({
          schema_version: CONTROL_PROTOCOL,
          type: 'LOCK_MINI_BRAIN',
          request_id: message.request_id,
        });
      } catch {
        finishLock(message.request_id, 'unconfirmed');
        return;
      }
    }
  });
}

self.addEventListener('message', event => {
  const message = event.data;
  if (message === 'LOCK_MINI_BRAIN') {
    event.waitUntil(caches.delete(CACHE));
    return;
  }
  if (!message || message.schema_version !== CONTROL_PROTOCOL) return;
  if (message.type === 'LOCK_MINI_BRAIN_REQUEST') {
    event.waitUntil(Promise.all([caches.delete(CACHE), relayLock(event, message)]));
    return;
  }
  if (message.type !== 'LOCK_MINI_BRAIN_ACK' || typeof message.request_id !== 'string') return;
  const pending = pendingLocks.get(message.request_id);
  const sourceId = event.source && event.source.id;
  if (!pending || typeof sourceId !== 'string' || !pending.expected.delete(sourceId)) return;
  if (pending.expected.size === 0) finishLock(message.request_id, 'acknowledged');
});
