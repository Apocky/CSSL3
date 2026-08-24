// § Akashic-Webpage-Records · _app.tsx
// Top-level App wrapper · wires Akashic-Webpage-Records on every page mount +
// installs the AkashicErrorBoundary at the React root + renders the
// first-visit AkashicConsent banner.
//
// Substrate-flavor : the App-component is the trunk · every page is a branch ·
// the ErrorBoundary catches any spore-fall in any branch · the Records remember.

import type { AppProps } from 'next/app';
import { useEffect } from 'react';
import { useRouter } from 'next/router';
import 'katex/dist/katex.min.css';
import '@/styles/apocky-system.css';
import {
  akashicInstall,
  akashicDisable,
  AkashicErrorBoundary,
  CONSENT_CHANGE_EVENT,
  capture,
  capturePageView,
  isTelemetryBlackoutPath,
} from '@/lib/akashic-telemetry';
import AkashicConsent from '@/components/AkashicConsent';
import SiteShell from '@/components/SiteShell';
import { SiteSessionProvider } from '@/components/hub/SiteSession';

// Auth, admin, and the live social room render bare (their own chrome / clean for OAuth).
// Everything else gets the global nav + footer so the whole site is navigable.
function isBare(pathname: string): boolean {
  return (
    pathname === '/login' ||
    pathname === '/register' ||
    pathname.startsWith('/auth') ||
    pathname.startsWith('/admin') ||
    pathname === '/clearing' ||
    pathname.startsWith('/clearing/') ||
    pathname.startsWith('/shawn')
  );
}

// Wire window.onerror + window.onunhandledrejection to capture(). These
// fallback layers catch errors that escape the React-tree (stage-3
// defense-in-depth). Idempotent ; Next preserves window across navigations.
let _global_listeners_attached = false;
const handleGlobalError = (ev: ErrorEvent): void => {
  capture('page.error', {
    message: ev.message ?? 'unknown',
    source: ev.filename ?? '',
    line: ev.lineno ?? 0,
    col: ev.colno ?? 0,
    stack: ev.error instanceof Error ? (ev.error.stack ?? '').slice(0, 4000) : '',
  });
};

const handleGlobalRejection = (ev: PromiseRejectionEvent): void => {
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
};

function attachGlobalErrorListeners(): void {
  if (_global_listeners_attached) return;
  if (typeof window === 'undefined') return;
  _global_listeners_attached = true;
  window.addEventListener('error', handleGlobalError);
  window.addEventListener('unhandledrejection', handleGlobalRejection);
}

function detachGlobalErrorListeners(): void {
  if (!_global_listeners_attached) return;
  if (typeof window !== 'undefined') {
    window.removeEventListener('error', handleGlobalError);
    window.removeEventListener('unhandledrejection', handleGlobalRejection);
  }
  _global_listeners_attached = false;
}

export default function App({ Component, pageProps }: AppProps): JSX.Element {
  const router = useRouter();

  useEffect(() => {
    const opts = {
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
    };

    type EarlyWindow = Window & {
      __akashic_pre_init?: Array<Record<string, unknown>>;
      __akashic_pre_init_cleanup?: () => void;
    };

    const cleanupEarlyListeners = (): void => {
      try {
        (window as EarlyWindow).__akashic_pre_init_cleanup?.();
      } catch {
        // never break user-flow on telemetry cleanup
      }
    };

    const clearEarlyBuffer = (): void => {
      try {
        (window as EarlyWindow).__akashic_pre_init = [];
      } catch {
        // never break user-flow on telemetry cleanup
      }
    };

    const drainEarlyBuffer = (): void => {
      const pre = (window as EarlyWindow).__akashic_pre_init;
      if (!Array.isArray(pre) || pre.length === 0) return;
      for (const e of pre) {
        capture('page.error', {
          message: typeof e['message'] === 'string' ? e['message'] : 'pre-hydrate',
          source: typeof e['source'] === 'string' ? e['source'] : '',
          line: typeof e['line'] === 'number' ? e['line'] : 0,
          col: typeof e['col'] === 'number' ? e['col'] : 0,
          stack: typeof e['stack'] === 'string' ? e['stack'].slice(0, 4000) : '',
          phase: 'pre-hydrate',
        });
      }
      clearEarlyBuffer();
    };

    const suspend = (): void => {
      cleanupEarlyListeners();
      clearEarlyBuffer();
      detachGlobalErrorListeners();
      akashicDisable();
    };

    const reconcile = (): boolean => {
      cleanupEarlyListeners();
      const active = akashicInstall(opts);
      if (!active || isTelemetryBlackoutPath()) {
        clearEarlyBuffer();
        detachGlobalErrorListeners();
        return false;
      }
      attachGlobalErrorListeners();
      drainEarlyBuffer();
      return true;
    };

    const handleRouteStart = (url: string): void => {
      if (isTelemetryBlackoutPath(url)) suspend();
    };

    const handleRouteComplete = (url: string): void => {
      if (reconcile()) capturePageView(url, 'client');
    };

    reconcile();
    window.addEventListener(CONSENT_CHANGE_EVENT, reconcile);
    router.events.on('routeChangeStart', handleRouteStart);
    router.events.on('routeChangeComplete', handleRouteComplete);
    router.events.on('routeChangeError', reconcile);

    return () => {
      suspend();
      window.removeEventListener(CONSENT_CHANGE_EVENT, reconcile);
      router.events.off('routeChangeStart', handleRouteStart);
      router.events.off('routeChangeComplete', handleRouteComplete);
      router.events.off('routeChangeError', reconcile);
    };
  }, [router.events]);

  const bare = isBare(router.pathname);
  const content = (
    <>
      <AkashicConsent />
      {bare ? (
        <Component {...pageProps} />
      ) : (
        <SiteShell>
          <Component {...pageProps} />
        </SiteShell>
      )}
    </>
  );
  return (
    <AkashicErrorBoundary>
      <SiteSessionProvider>{content}</SiteSessionProvider>
    </AkashicErrorBoundary>
  );
}
