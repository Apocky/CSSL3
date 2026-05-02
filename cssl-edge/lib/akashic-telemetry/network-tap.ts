// § Akashic-Webpage-Records · network-tap.ts
// Intercepts fetch() + XMLHttpRequest · stamps net.fail / net.slow cells.
// Self-route exempted to avoid feedback-loop. Σ-mask redact applied to URL
// query-strings before emit.
//
// Bit-pack philosophy : monkey-patch once · idempotent · zero-runtime-cost
// when consent_tier == none (gate denies before any fetch payload built).

import { capture } from './client';
import { redactString } from './sigma-mask';

let installed = false;
const SLOW_MS = 3000;

function isSelfRoute(url: string): boolean {
  return url.includes('/api/akashic/');
}

// fetch tap · wraps window.fetch ; preserves return-shape.
function installFetchTap(): void {
  if (typeof window === 'undefined' || typeof window.fetch !== 'function') return;
  const orig = window.fetch.bind(window);
  window.fetch = (async (
    input: RequestInfo | URL,
    init?: RequestInit
  ): Promise<Response> => {
    const url = typeof input === 'string'
      ? input
      : input instanceof URL
        ? input.href
        : input.url;
    const method = (init?.method ?? 'GET').toUpperCase();
    if (isSelfRoute(url)) return orig(input, init);

    const t0 = (typeof performance !== 'undefined' && performance.now) ? performance.now() : Date.now();
    try {
      const res = await orig(input, init);
      const t1 = (typeof performance !== 'undefined' && performance.now) ? performance.now() : Date.now();
      const dur = t1 - t0;
      if (!res.ok) {
        capture('net.fail', {
          url: redactString(url),
          method,
          status: res.status,
          duration_ms: Math.round(dur),
        });
      } else if (dur > SLOW_MS) {
        capture('net.slow', {
          url: redactString(url),
          method,
          duration_ms: Math.round(dur),
        });
      }
      return res;
    } catch (err) {
      const t1 = (typeof performance !== 'undefined' && performance.now) ? performance.now() : Date.now();
      capture('net.fail', {
        url: redactString(url),
        method,
        error: (err instanceof Error ? err.message : String(err)),
        duration_ms: Math.round(t1 - t0),
      });
      throw err;
    }
  }) as typeof window.fetch;
}

// XHR tap · monkey-patches send() · captures status + timing on completion.
function installXhrTap(): void {
  if (typeof XMLHttpRequest === 'undefined') return;
  const proto = XMLHttpRequest.prototype;
  const origOpen = proto.open;
  const origSend = proto.send;

  proto.open = function (
    this: XMLHttpRequest,
    method: string,
    url: string | URL,
    async?: boolean,
    user?: string | null,
    password?: string | null
  ): void {
    (this as XMLHttpRequest & { _akashic?: { method: string; url: string; t0: number } })._akashic = {
      method: method.toUpperCase(),
      url: typeof url === 'string' ? url : url.href,
      t0: 0,
    };
    return origOpen.call(this, method, url as string, async ?? true, user ?? null, password ?? null);
  };

  proto.send = function (this: XMLHttpRequest, body?: Document | XMLHttpRequestBodyInit | null): void {
    const meta = (this as XMLHttpRequest & { _akashic?: { method: string; url: string; t0: number } })._akashic;
    if (meta !== undefined && !isSelfRoute(meta.url)) {
      meta.t0 = (typeof performance !== 'undefined' && performance.now) ? performance.now() : Date.now();
      const onLoadEnd = (): void => {
        const t1 = (typeof performance !== 'undefined' && performance.now) ? performance.now() : Date.now();
        const dur = t1 - meta.t0;
        const status = this.status ?? 0;
        if (status === 0 || status >= 400) {
          capture('net.fail', {
            url: redactString(meta.url),
            method: meta.method,
            status,
            duration_ms: Math.round(dur),
          });
        } else if (dur > SLOW_MS) {
          capture('net.slow', {
            url: redactString(meta.url),
            method: meta.method,
            duration_ms: Math.round(dur),
          });
        }
        this.removeEventListener('loadend', onLoadEnd);
      };
      this.addEventListener('loadend', onLoadEnd);
    }
    return origSend.call(this, body ?? null);
  };
}

export function installNetworkTap(): void {
  if (installed) return;
  if (typeof window === 'undefined') return;
  installed = true;
  installFetchTap();
  installXhrTap();
}

export function _resetNetTapForTests(): void {
  installed = false;
}
