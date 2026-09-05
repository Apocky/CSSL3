/* Apocrypha Mini Brain shell only. Private API responses and user payloads are never cached here. */
'use strict';

const CACHE = 'apocky-mini-brain-shell-v2';
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
      .then(keys => Promise.all(keys.filter(key => key === 'apocky-mini-brain-shell-v1').map(key => caches.delete(key))))
  );
});

self.addEventListener('fetch', event => {
  const request = event.request;
  if (request.method !== 'GET') return;
  const url = new URL(request.url);
  if (url.origin !== self.location.origin || url.pathname.startsWith('/api/')) return;

  if (request.mode === 'navigate' && url.pathname === '/brain') {
    event.respondWith(
      fetch(request)
        .then(async response => {
          if (
            response.ok && response.type === 'basic' && !response.redirected
            && new URL(response.url).origin === self.location.origin
            && new URL(response.url).pathname === '/brain'
            && response.headers.get('content-type')?.includes('text/html')
          ) {
            const html = await response.clone().text();
            if (html.includes('"serverAccess":"owner"')) {
              await caches.open(CACHE).then(cache => cache.put('/brain', response.clone())).catch(() => {});
            }
          }
          return response;
        })
        .catch(async () => (await caches.match('/brain')) ?? Response.error()),
    );
    return;
  }

  if (url.pathname.startsWith('/_next/static/') || STATIC.includes(url.pathname)) {
    event.respondWith(
      caches.match(request).then(cached => cached ?? fetch(request).then(response => {
        if (response.ok && response.type === 'basic') {
          const copy = response.clone();
          void caches.open(CACHE).then(cache => cache.put(request, copy));
        }
        return response;
      })),
    );
  }
});

self.addEventListener('message', event => {
  if (event.data === 'LOCK_MINI_BRAIN') event.waitUntil(caches.delete(CACHE));
});
