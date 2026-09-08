import { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';

import { getAuthClient } from '../../lib/auth';
import { authFetch } from '../../lib/browser-auth';
import { withDeadline } from '../../lib/apocrypha/deadline';

export const SITE_SESSION_RESOLUTION_DEADLINE_MS = 4_000;

export type SiteAccessState = 'checking' | 'signed-out' | 'member' | 'owner' | 'unavailable';

interface SiteSessionValue {
  access: SiteAccessState;
  authenticated: boolean;
  subjectKey: string | null;
  ownerConversation?: boolean;
  refresh: () => Promise<void>;
}

const SiteSessionContext = createContext<SiteSessionValue | null>(null);

interface ResolvedSiteSession {
  access: SiteAccessState;
  subjectKey: string | null;
  ownerConversation?: boolean;
}

async function resolveSiteAccessUnchecked(): Promise<ResolvedSiteSession> {
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
  let subjectKey: string | null = null;
  let ownerConversation = false;
  try {
    const response = await authFetch('/api/auth/me', { cache: 'no-store' });
    if (!response.ok) return { access: browserAuthenticated ? 'unavailable' : 'signed-out', subjectKey: null };
    const payload = await response.json() as { user?: unknown; owner_conversation?: unknown };
    serverAuthenticated = Boolean(payload.user);
    ownerConversation = serverAuthenticated && payload.owner_conversation === true;
    if (payload.user && typeof payload.user === 'object' && !Array.isArray(payload.user)) {
      const candidate = (payload.user as Record<string, unknown>).id;
      subjectKey = typeof candidate === 'string' && candidate.length > 0 ? candidate : null;
    }
  } catch {
    if (!browserAuthenticated) return { access: 'unavailable', subjectKey: null };
  }

  if (!serverAuthenticated && !browserAuthenticated) return { access: 'signed-out', subjectKey: null };

  try {
    const response = await authFetch('/api/admin/check', { cache: 'no-store' });
    if (!response.ok) return { access: 'member', subjectKey };
    const payload = await response.json() as { authorized?: unknown };
    return { access: payload.authorized === true ? 'owner' : 'member', subjectKey, ownerConversation: payload.authorized === true && ownerConversation };
  } catch {
    return { access: 'member', subjectKey };
  }
}

export async function resolveSiteAccess(
  deadlineMs = SITE_SESSION_RESOLUTION_DEADLINE_MS,
): Promise<ResolvedSiteSession> {
  try {
    return await withDeadline(resolveSiteAccessUnchecked(), deadlineMs);
  } catch {
    return { access: 'unavailable', subjectKey: null };
  }
}

export function SiteSessionProvider({ children }: { children: React.ReactNode }): JSX.Element {
  const [session, setSession] = useState<ResolvedSiteSession>({ access: 'checking', subjectKey: null });

  const refresh = useCallback(async () => {
    setSession(await resolveSiteAccess());
  }, []);

  useEffect(() => {
    let current = true;
    void resolveSiteAccess().then((next) => {
      if (current) setSession(next);
    });
    return () => { current = false; };
  }, []);

  useEffect(() => {
    const client = getAuthClient();
    if (!client) return undefined;
    let queuedRefresh: ReturnType<typeof setTimeout> | null = null;
    const { data } = client.auth.onAuthStateChange(() => {
      // Supabase dispatches this callback while its auth state is locked. Defer
      // the read so refresh() cannot wait on the callback that invoked it.
      if (queuedRefresh !== null) return;
      queuedRefresh = setTimeout(() => {
        queuedRefresh = null;
        void refresh();
      }, 0);
    });
    return () => {
      if (queuedRefresh !== null) clearTimeout(queuedRefresh);
      data.subscription.unsubscribe();
    };
  }, [refresh]);

  const value = useMemo<SiteSessionValue>(() => ({
    access: session.access,
    authenticated: session.access === 'member' || session.access === 'owner',
    subjectKey: session.subjectKey,
    ownerConversation: session.ownerConversation === true,
    refresh,
  }), [refresh, session]);

  return <SiteSessionContext.Provider value={value}>{children}</SiteSessionContext.Provider>;
}

export function useSiteSession(): SiteSessionValue {
  const value = useContext(SiteSessionContext);
  if (!value) {
    throw new Error('useSiteSession must be used within SiteSessionProvider');
  }
  return value;
}
