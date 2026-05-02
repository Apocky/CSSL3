// § Akashic-Webpage-Records · _app.tsx
// Top-level App wrapper · wires Akashic-Webpage-Records on every page mount +
// installs the AkashicErrorBoundary at the React root + renders the
// first-visit AkashicConsent overlay.
//
// Substrate-flavor : the App-component is the trunk · every page is a branch ·
// the ErrorBoundary catches any spore-fall in any branch · the Records remember.

import type { AppProps } from 'next/app';
import { useEffect } from 'react';
import {
  akashicInstall,
  AkashicErrorBoundary,
  capture,
} from '@/lib/akashic-telemetry';
import AkashicConsent from '@/components/AkashicConsent';

// Wire window.onerror + window.onunhandledrejection to capture(). These
// fallback layers catch errors that escape the React-tree (stage-3
// defense-in-depth). Idempotent ; Next preserves window across navigations.
let _global_listeners_attached = false;
function attachGlobalErrorListeners(): void {
  if (_global_listeners_attached) return;
  if (typeof window === 'undefined') return;
  _global_listeners_attached = true;

  window.addEventListener('error', (ev: ErrorEvent) => {
    capture('page.error', {
      message: ev.message ?? 'unknown',
      source: ev.filename ?? '',
      line: ev.lineno ?? 0,
      col: ev.colno ?? 0,
      stack: ev.error instanceof Error ? (ev.error.stack ?? '').slice(0, 4000) : '',
    });
  });

  window.addEventListener('unhandledrejection', (ev: PromiseRejectionEvent) => {
    const reason = ev.reason;
    capture('promise.unhandled', {
      message:
        reason instanceof Error
          ? reason.message
          : typeof reason === 'string'
            ? reason
            : JSON.stringify(reason),
      stack: reason instanceof Error ? (reason.stack ?? '').slice(0, 4000) : '',
    });
  });
}

export default function App({ Component, pageProps }: AppProps): JSX.Element {
  useEffect(() => {
    // Install once · idempotent. Pull build-time env-vars (Next exposes
    // anything prefixed NEXT_PUBLIC_ to client). Vercel auto-injects
    // VERCEL_GIT_COMMIT_SHA + we mirror it to NEXT_PUBLIC_* via next.config.
    akashicInstall({
      dpl_id:
        (typeof process !== 'undefined'
          ? process.env['NEXT_PUBLIC_VERCEL_DEPLOYMENT_ID']
          : undefined) ?? 'local-dev',
      commit_sha:
        (typeof process !== 'undefined'
          ? process.env['NEXT_PUBLIC_VERCEL_GIT_COMMIT_SHA']
          : undefined) ?? 'unknown',
      build_time:
        (typeof process !== 'undefined'
          ? process.env['NEXT_PUBLIC_BUILD_TIME']
          : undefined) ?? 'unknown',
    });
    attachGlobalErrorListeners();

    // Drain any errors captured by the early-error inline-script in
    // _document.tsx (those land in window.__akashic_pre_init).
    try {
      const pre = (window as unknown as { __akashic_pre_init?: Array<Record<string, unknown>> })
        .__akashic_pre_init;
      if (Array.isArray(pre) && pre.length > 0) {
        for (const e of pre) {
          capture('page.error', {
            message: typeof e['message'] === 'string' ? e['message'] : 'pre-hydrate',
            source: typeof e['source'] === 'string' ? e['source'] : '',
            line: typeof e['line'] === 'number' ? e['line'] : 0,
            col: typeof e['col'] === 'number' ? e['col'] : 0,
            stack: typeof e['stack'] === 'string' ? e['stack'].slice(0, 4000) : '',
            phase: 'pre-hydrate', // distinguishes from post-hydrate errors
          });
        }
        (window as unknown as { __akashic_pre_init?: Array<Record<string, unknown>> })
          .__akashic_pre_init = [];
      }
    } catch {
      // never break user-flow on telemetry-bridge
    }
  }, []);

  return (
    <AkashicErrorBoundary>
      <AkashicConsent />
      <Component {...pageProps} />
    </AkashicErrorBoundary>
  );
}
