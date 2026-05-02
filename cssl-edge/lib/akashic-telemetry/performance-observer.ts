// § Akashic-Webpage-Records · performance-observer.ts
// PerformanceObserver wrapper · Web Vitals + long-tasks + resource-timing.
// Each metric stamps a perf.* cell. Pure-DOM ; no React deps.
//
// Targets (Google CWV thresholds) :
//   LCP ≤ 2.5s · FID ≤ 100ms · CLS ≤ 0.1 · INP ≤ 200ms · TTFB ≤ 600ms
//
// Stage-0 implements LCP / FCP / CLS / Long-Tasks / Resource-Timing inline ·
// FID + INP layered via interactive-event listener. Future : import web-vitals
// pkg if appetite allows ; right now zero-dep.

import { capture } from './client';

type AnyPerfEntry = PerformanceEntry & {
  value?: number;
  hadRecentInput?: boolean;
  startTime?: number;
  duration?: number;
  responseStart?: number;
  domContentLoadedEventEnd?: number;
  loadEventEnd?: number;
  initiatorType?: string;
  transferSize?: number;
  responseEnd?: number;
};

let cls_value = 0;
let cls_session_value = 0;
let cls_session_first_ts = 0;
let cls_session_last_ts = 0;
let inp_max = 0;
let installed = false;

function observe(types: string[], cb: (e: AnyPerfEntry) => void): PerformanceObserver | null {
  if (typeof PerformanceObserver === 'undefined') return null;
  try {
    const po = new PerformanceObserver((list) => {
      for (const e of list.getEntries()) cb(e as AnyPerfEntry);
    });
    po.observe({ entryTypes: types });
    return po;
  } catch {
    return null;
  }
}

// LCP : last-largest-contentful-paint wins. Report on visibilitychange.
function installLCP(): void {
  let last_lcp = 0;
  observe(['largest-contentful-paint'], (e) => {
    last_lcp = e.startTime ?? 0;
  });
  // Report when page becomes hidden (CWV best practice).
  if (typeof document !== 'undefined') {
    const report = (): void => {
      if (last_lcp > 0) {
        capture('perf.lcp', {
          value: Math.round(last_lcp),
          url: location.href,
          viewport: { w: window.innerWidth, h: window.innerHeight },
          connection: getConnectionType(),
        });
      }
    };
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'hidden') report();
    });
    window.addEventListener('pagehide', report);
  }
}

function installFCP(): void {
  observe(['paint'], (e) => {
    if (e.name === 'first-contentful-paint') {
      capture('perf.fcp', {
        value: Math.round(e.startTime ?? 0),
        url: location.href,
        viewport: { w: window.innerWidth, h: window.innerHeight },
      });
    }
  });
}

// CLS : cumulative-layout-shift sessions (5s window · 1s gap).
function installCLS(): void {
  observe(['layout-shift'], (e) => {
    if (e.hadRecentInput === true) return;
    const v = e.value ?? 0;
    const t = e.startTime ?? 0;
    if (
      cls_session_value !== 0 &&
      (t - cls_session_last_ts > 1000 || t - cls_session_first_ts > 5000)
    ) {
      // session ended ; promote if it was bigger
      if (cls_session_value > cls_value) cls_value = cls_session_value;
      cls_session_value = 0;
    }
    if (cls_session_value === 0) cls_session_first_ts = t;
    cls_session_value += v;
    cls_session_last_ts = t;
  });
  if (typeof document !== 'undefined') {
    const report = (): void => {
      const final = Math.max(cls_value, cls_session_value);
      if (final > 0) {
        capture('perf.cls', {
          value: Math.round(final * 1000) / 1000, // 3-decimal precision
          url: location.href,
          viewport: { w: window.innerWidth, h: window.innerHeight },
        });
      }
    };
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'hidden') report();
    });
    window.addEventListener('pagehide', report);
  }
}

// FID-ish (uses 'first-input' entry-type when available).
function installFID(): void {
  observe(['first-input'], (e) => {
    const ps = (e as unknown as { processingStart?: number }).processingStart ?? 0;
    const fid = ps - (e.startTime ?? 0);
    if (fid >= 0) {
      capture('perf.fid', {
        value: Math.round(fid),
        url: location.href,
        viewport: { w: window.innerWidth, h: window.innerHeight },
      });
    }
  });
}

// INP : longest interaction. Approximation : track every event-timing entry,
// emit max on visibilitychange.
function installINP(): void {
  observe(['event'], (e) => {
    const dur = e.duration ?? 0;
    if (dur > inp_max) inp_max = dur;
  });
  if (typeof document !== 'undefined') {
    const report = (): void => {
      if (inp_max > 0) {
        capture('perf.inp', {
          value: Math.round(inp_max),
          url: location.href,
          viewport: { w: window.innerWidth, h: window.innerHeight },
        });
      }
    };
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'hidden') report();
    });
    window.addEventListener('pagehide', report);
  }
}

// TTFB · derived from navigation-timing.
function installTTFB(): void {
  observe(['navigation'], (e) => {
    const ttfb = e.responseStart ?? 0;
    if (ttfb >= 0) {
      capture('perf.ttfb', {
        value: Math.round(ttfb),
        url: location.href,
        viewport: { w: window.innerWidth, h: window.innerHeight },
        connection: getConnectionType(),
      });
    }
  });
}

// Long-Tasks (≥ 50ms · main-thread blocking).
function installLongTasks(): void {
  observe(['longtask'], (e) => {
    capture('perf.long_task', {
      value: Math.round(e.duration ?? 0),
      url: location.href,
      viewport: { w: window.innerWidth, h: window.innerHeight },
    });
  });
}

// Resource-Timing : surface failed (transferSize=0 · status≥400 inferred) +
// slow (>2s) resources. Filters out the akashic-batch endpoint to avoid loops.
function installResourceTiming(): void {
  observe(['resource'], (e) => {
    const url = e.name ?? '';
    if (url.includes('/api/akashic/')) return; // ignore self
    const dur = e.duration ?? 0;
    const xfer = e.transferSize ?? 0;
    if (dur > 2000) {
      capture('perf.resource_slow', {
        url,
        duration_ms: Math.round(dur),
        kind: e.initiatorType ?? 'unknown',
      });
    }
    // transferSize = 0 + duration > 0 = likely-failed (CORS or 4xx with empty body)
    // Heuristic only ; net.fail handles the precise case.
    if (xfer === 0 && dur > 100 && (e.initiatorType === 'fetch' || e.initiatorType === 'xmlhttprequest')) {
      capture('perf.resource_fail', {
        url,
        duration_ms: Math.round(dur),
        kind: e.initiatorType,
      });
    }
  });
}

function getConnectionType(): string {
  try {
    const nav = navigator as unknown as { connection?: { effectiveType?: string } };
    return nav.connection?.effectiveType ?? 'unknown';
  } catch {
    return 'unknown';
  }
}

// ─── public · install all observers (idempotent) ──────────────────────────
export function installPerformanceObservers(): void {
  if (installed) return;
  if (typeof window === 'undefined') return;
  installed = true;
  installLCP();
  installFCP();
  installCLS();
  installFID();
  installINP();
  installTTFB();
  installLongTasks();
  installResourceTiming();
}

// test-only
export function _resetPerfForTests(): void {
  installed = false;
  cls_value = 0;
  cls_session_value = 0;
  inp_max = 0;
}
