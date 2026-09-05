import { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';

import { getAuthClient } from '../../lib/auth';
import { withDeadline } from '../../lib/apocrypha/deadline';
import { authFetch } from '../../lib/browser-auth';

export type SiteAccessState = 'checking' | 'signed-out' | 'member' | 'owner' | 'unavailable';

interface SiteSessionValue {
  access: SiteAccessState;
  authenticated: boolean;
  refresh: () => Promise<void>;
}

const SiteSessionContext = createContext<SiteSessionValue | null>(null);
const ACCESS_DEADLINE_MS = 15_000;

async function resolveSiteAccess(): Promise<SiteAccessState> {
  try {
    return await withDeadline((async (): Promise<SiteAccessState> => {
      let browserAuthenticated = false;
      const client = getAuthClient();

      if (client) {
        try {
          const { data } = await client.auth.getSession();
          browserAuthenticated = Boolean(data.session);
        } catch {
          browserAuthenticated = false;
        }
      }

      let serverAuthenticated = false;
      try {
        const response = await authFetch('/api/auth/me', { cache: 'no-store' });
        if (!response.ok) return browserAuthenticated ? 'unavailable' : 'signed-out';
        const payload = await response.json() as { user?: unknown };
        serverAuthenticated = Boolean(payload.user);
      } catch {
        if (!browserAuthenticated) return 'unavailable';
      }

      if (!serverAuthenticated && !browserAuthenticated) return 'signed-out';

      try {
        const response = await authFetch('/api/admin/check', { cache: 'no-store' });
        if (!response.ok) return 'unavailable';
        const payload = await response.json() as { authorized?: unknown };
        return payload.authorized === true ? 'owner' : 'member';
      } catch {
        return 'unavailable';
      }
    })(), ACCESS_DEADLINE_MS);
  } catch {
    return 'unavailable';
  }
}

export function SiteSessionProvider({ children }: { children: React.ReactNode }): JSX.Element {
  const [access, setAccess] = useState<SiteAccessState>('checking');

  const refresh = useCallback(async () => {
    setAccess('checking');
    setAccess(await resolveSiteAccess());
  }, []);

  useEffect(() => {
    let current = true;
    void resolveSiteAccess().then((next) => {
      if (current) setAccess(next);
    });
    return () => { current = false; };
  }, []);

  const value = useMemo<SiteSessionValue>(() => ({
    access,
    authenticated: access === 'member' || access === 'owner',
    refresh,
  }), [access, refresh]);

  return <SiteSessionContext.Provider value={value}>{children}</SiteSessionContext.Provider>;
}

export function useSiteSession(): SiteSessionValue {
  const value = useContext(SiteSessionContext);
  if (!value) {
    throw new Error('useSiteSession must be used within SiteSessionProvider');
  }
  return value;
}
