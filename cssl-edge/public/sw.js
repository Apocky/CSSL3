// apocky.com service worker.
//
// WHY THIS EXISTS
// ---------------
// The site already serves a complete, valid PWA manifest - standalone display,
// 192 and 512 icons, theme and background colours - so it advertises itself as
// installable. Chrome will not offer installation without a service worker
// carrying a fetch handler, so that promise could not be kept. This keeps it.
//
// THE FAILURE THIS MUST NOT CAUSE
// -------------------------------
// A service worker that serves stale application code after a deploy is worse
// than no service worker at all: the site looks updated to whoever deployed it
// and is not updated for anyone else, and nothing reports an error. That has
// already happened on this project once, from a browser cache rather than a
// worker, and it cost real time to find.
//
// So the strategies here are chosen around one rule: NOTHING whose URL can
// change meaning is ever served from cache before the network is tried.
//
//   /_next/static/*   cache-first   - safe, and ONLY because these URLs are
//                                     content-hashed. The filename contains a
//                                     digest of the bytes, so a given URL can
//                                     never mean something new. Changing the
//                                     code changes the URL.
//   navigations       network-first - the page must be live. Cache is a
//                                     fallback for being offline, nothing more.
//   everything else   network-first - same reasoning, no exceptions worth the
//                                     risk.
//   /api/*            never cached  - responses are per-user and per-moment.
//   /brain/*          not handled   - /brain-sw.js owns that scope. See
//                                     lib/brain/mini-brain.ts.
//   /.well-known/*    never cached  - assetlinks is read by Chrome to verify
//                                     the Android app; a stale copy there
//                                     silently breaks app verification.
//
// The Apocrypha TWA loads this origin, so "navigations are network-first" is
// also what keeps the Android app showing live content.

const VERSION = 'apocky-v1';
const RUNTIME = `${VERSION}-runtime`;
const IMMUTABLE = `${VERSION}-immutable`;
const OFFLINE_URL = '/';

// Paths this worker declines to handle at all.
function notOurs(url) {
  return (
    url.pathname.startsWith('/api/')
    || url.pathname.startsWith('/brain')
    || url.pathname.startsWith('/.well-known/')
    || url.pathname.startsWith('/_next/image')
  );
}

// Content-hashed build output. The digest is in the path, so the URL is a
// permanent name for exactly these bytes.
function immutable(url) {
  return url.pathname.startsWith('/_next/static/');
}

self.addEventListener('install', (event) => {
  // Warm only the offline fallback. Precaching a list of app URLs is how a
  // worker ends up serving a page from a build nobody is running any more.
  event.waitUntil(
    caches
      .open(RUNTIME)
      .then((cache) => cache.add(new Request(OFFLINE_URL, { cache: 'reload' })))
      .catch(() => undefined)
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((names) =>
        Promise.all(
          names
            // Only this worker's own caches. `brain-` and anything else on the
            // origin belongs to somebody else and deleting it would break them.
            .filter((n) => n.startsWith('apocky-') && !n.startsWith(VERSION))
            .map((n) => caches.delete(n)),
        ),
      )
      .then(() => self.clients.claim()),
  );
});

self.addEventListener('fetch', (event) => {
  const { request } = event;
  if (request.method !== 'GET') return;

  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;
  if (notOurs(url)) return;

  if (immutable(url)) {
    event.respondWith(cacheFirst(request));
    return;
  }
  event.respondWith(networkFirst(request));
});

/// Only for URLs that cannot change meaning.
async function cacheFirst(request) {
  const cache = await caches.open(IMMUTABLE);
  const hit = await cache.match(request);
  if (hit) return hit;
  const response = await fetch(request);
  if (response && response.ok) cache.put(request, response.clone());
  return response;
}

/// The network decides. Cache answers only when the network cannot.
async function networkFirst(request) {
  const cache = await caches.open(RUNTIME);
  try {
    const response = await fetch(request);
    // Opaque and error responses are not stored: caching a 404 or a redirect
    // to a login page is how a worker starts serving the wrong thing to
    // everyone who comes back later.
    if (response && response.ok && response.type === 'basic') {
      cache.put(request, response.clone());
    }
    return response;
  } catch (err) {
    const hit = await cache.match(request);
    if (hit) return hit;
    if (request.mode === 'navigate') {
      const shell = await cache.match(OFFLINE_URL);
      if (shell) return shell;
    }
    throw err;
  }
}
