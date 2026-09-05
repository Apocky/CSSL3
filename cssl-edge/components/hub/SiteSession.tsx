import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from 'react';

import { browserAuthMutationAllowsProtectedOpen, getAuthClient } from '../../lib/auth';
import { authFetch } from '../../lib/browser-auth';

export type SiteAccessState = 'checking' | 'signed-out' | 'member' | 'owner' | 'unavailable';

interface SiteSessionValue {
  access: SiteAccessState;
  authenticated: boolean;
  subjectKey: string | null;
  evidenceRevision: number;
  refresh: () => Promise<void>;
}

const SiteSessionContext = createContext<SiteSessionValue | null>(null);

interface ResolvedSiteSession {
  access: SiteAccessState;
  subjectKey: string | null;
}

type SessionFailureKind = 'unauthenticated' | 'invalid-session' | 'upstream-unavailable' | 'unconfigured';

const SESSION_PROBE_TIMEOUT_MS = 1_500;

async function boundedAuthFetch(input: RequestInfo | URL, parentSignal?: AbortSignal): Promise<Response> {
  const controller = new AbortController();
  const abortFromParent = () => controller.abort(parentSignal?.reason ?? new DOMException('Superseded', 'AbortError'));
  if (parentSignal?.aborted) abortFromParent();
  else parentSignal?.addEventListener('abort', abortFromParent, { once: true });
  const timer = window.setTimeout(() => controller.abort(new DOMException('Timed out', 'TimeoutError')), SESSION_PROBE_TIMEOUT_MS);
  try {
    return await authFetch(input, { cache: 'no-store', signal: controller.signal });
  } finally {
    window.clearTimeout(timer);
    parentSignal?.removeEventListener('abort', abortFromParent);
  }
}

async function resolveSiteAccess(signal?: AbortSignal): Promise<ResolvedSiteSession> {
  if (!browserAuthMutationAllowsProtectedOpen()) return { access: 'signed-out', subjectKey: null };
  let browserAuthenticated = false;
  let browserSubjectKey: string | null = null;
  const client = getAuthClient();

  if (client) {
    try {
      const { data } = await client.auth.getSession();
      browserAuthenticated = Boolean(data.session);
      browserSubjectKey = typeof data.session?.user?.id === 'string' && data.session.user.id.length > 0
        ? data.session.user.id
        : null;
    } catch {
      browserAuthenticated = false;
    }
  }

  let serverAuthenticated = false;
  let subjectKey: string | null = null;
  let serverFailureKind: SessionFailureKind | null = null;
  try {
    const response = await boundedAuthFetch('/api/auth/me', signal);
    if (!response.ok) return response.status === 401 || response.status === 403
      ? { access: 'signed-out', subjectKey: null }
      : { access: 'unavailable', subjectKey: browserSubjectKey };
    const payload = await response.json() as { user?: unknown; failureKind?: unknown };
    serverAuthenticated = Boolean(payload.user);
    serverFailureKind = typeof payload.failureKind === 'string'
      && ['unauthenticated', 'invalid-session', 'upstream-unavailable', 'unconfigured'].includes(payload.failureKind)
      ? payload.failureKind as SessionFailureKind
      : null;
    if (payload.user && typeof payload.user === 'object' && !Array.isArray(payload.user)) {
      const candidate = (payload.user as Record<string, unknown>).id;
      if (browserSubjectKey && typeof candidate === 'string' && candidate !== browserSubjectKey) {
        return { access: 'signed-out', subjectKey: null };
      }
      subjectKey = typeof candidate === 'string' && candidate.length > 0 ? candidate : browserSubjectKey;
    }
  } catch {
    return { access: 'unavailable', subjectKey: browserSubjectKey };
  }

  if (!serverAuthenticated) {
    const transient = serverFailureKind === 'upstream-unavailable' || serverFailureKind === 'unconfigured';
    if (browserAuthenticated && transient) return { access: 'unavailable', subjectKey: browserSubjectKey };
    return transient
      ? { access: 'unavailable', subjectKey: null }
      : { access: 'signed-out', subjectKey: null };
  }

  try {
    const response = await boundedAuthFetch('/api/admin/check', signal);
    if (!response.ok) return response.status === 401 || response.status === 403
      ? { access: 'signed-out', subjectKey: null }
      : { access: 'unavailable', subjectKey };
    const payload = await response.json() as { authorized?: unknown; failureKind?: unknown };
    if (payload.authorized === true) return { access: 'owner', subjectKey };
    if (payload.failureKind === 'upstream-unavailable' || payload.failureKind === 'unconfigured') {
      return { access: 'unavailable', subjectKey };
    }
    if (payload.failureKind === 'unauthenticated' || payload.failureKind === 'invalid-session') {
      return { access: 'signed-out', subjectKey: null };
    }
    return { access: 'member', subjectKey };
  } catch {
    return { access: 'unavailable', subjectKey };
  }
}

export function SiteSessionProvider({ children }: { children: React.ReactNode }): JSX.Element {
  const [session, setSession] = useState<ResolvedSiteSession>({ access: 'checking', subjectKey: null });
  const [evidenceRevision, setEvidenceRevision] = useState(0);
  const requestGenerationRef = useRef(0);
  const activeProbeRef = useRef<AbortController | null>(null);

  const refresh = useCallback(async () => {
    const generation = requestGenerationRef.current + 1;
    requestGenerationRef.current = generation;
    activeProbeRef.current?.abort(new DOMException('Superseded', 'AbortError'));
    const controller = new AbortController();
    activeProbeRef.current = controller;
    const resolved = await resolveSiteAccess(controller.signal);
    if (controller.signal.aborted || requestGenerationRef.current !== generation) return;
    activeProbeRef.current = null;
    setSession(resolved);
    setEvidenceRevision(revision => revision + 1);
  }, []);

  useEffect(() => {
    void refresh();
    return () => {
      requestGenerationRef.current += 1;
      activeProbeRef.current?.abort(new DOMException('Unmounted', 'AbortError'));
      activeProbeRef.current = null;
    };
  }, [refresh]);

  useEffect(() => {
    const client = getAuthClient();
    if (!client) return undefined;
    const { data } = client.auth.onAuthStateChange(() => { void refresh(); });
    return () => data.subscription.unsubscribe();
  }, [refresh]);

  const value = useMemo<SiteSessionValue>(() => ({
    access: session.access,
    authenticated: session.access === 'member' || session.access === 'owner',
    subjectKey: session.subjectKey,
    evidenceRevision,
    refresh,
  }), [evidenceRevision, refresh, session]);

  return <SiteSessionContext.Provider value={value}>{children}</SiteSessionContext.Provider>;
}

export function useSiteSession(): SiteSessionValue {
  const value = useContext(SiteSessionContext);
  if (!value) {
    throw new Error('useSiteSession must be used within SiteSessionProvider');
  }
  return value;
}
