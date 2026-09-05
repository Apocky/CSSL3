// Top-level App wrapper. Public telemetry is intentionally disabled while its
// server-verifiable consent and purge authority are rebuilt.

import type { AppProps } from 'next/app';
import { useEffect } from 'react';
import { useRouter } from 'next/router';
import 'katex/dist/katex.min.css';
import '@/styles/apocky-system.css';
import {
  akashicDisable,
  AkashicErrorBoundary,
} from '@/lib/akashic-telemetry';
import SiteShell from '@/components/SiteShell';
import { SiteSessionProvider } from '@/components/hub/SiteSession';

// Auth, admin, and the immersive entity/chat pages render bare (their own chrome / clean for OAuth).
// Everything else gets the global nav + footer so the whole site is navigable.
function isBare(pathname: string): boolean {
  return (
    pathname === '/login' ||
    pathname === '/register' ||
    pathname.startsWith('/auth') ||
    pathname.startsWith('/admin') ||
    pathname === '/apocrypha' ||
    pathname === '/chat' ||
    pathname.startsWith('/shawn')
  );
}

export default function App({ Component, pageProps }: AppProps): JSX.Element {
  const router = useRouter();

  useEffect(() => {
    akashicDisable();
    return () => akashicDisable();
  }, []);

  const bare = isBare(router.pathname);
  const content = (
    <>
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
