import {
  currentAuthenticationAttempt,
  getAbortableAuthCallbackClient,
  persistSessionToCookie,
  type AuthSessionMirrorResult,
} from './auth';

interface AuthSessionResult {
  data?: {
    session?: {
      access_token: string;
      refresh_token?: string | null;
      user?: { id?: unknown };
    } | null;
  };
  error?: {
    message: string;
  } | null;
}

export interface AuthCallbackParams {
  hasCallback: boolean;
  error: string | null;
  code: string | null;
  accessToken: string | null;
  refreshToken: string | null;
}

export interface ConsumeAuthCallbackResult {
  handled: boolean;
  ok: boolean;
  freshSession?: {
    accessToken: string;
    subjectKey: string;
    authAttempt: string;
  };
  mirrorStatus?: AuthSessionMirrorResult['status'];
  providerSessionUncertain?: boolean;
  providerSessionAuthAttempt?: string;
  stub?: boolean;
  reason?: string;
}

function withTimeout<T>(promise: Promise<T>, timeoutMs: number, onTimeout: () => void): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    let settled = false;
    let timedOut = false;
    const finish = (callback: () => void): void => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      callback();
    };
    const timer = setTimeout(() => {
      if (settled) return;
      timedOut = true;
      try {
        onTimeout();
      } catch {
        finish(() => reject(new Error('timeout')));
      }
    }, timeoutMs);
    void promise.then(
      value => finish(() => timedOut ? reject(new Error('timeout')) : resolve(value)),
      error => finish(() => reject(timedOut ? new Error('timeout') : error)),
    );
  });
}

export function readAuthCallbackParams(search: string, hash: string): AuthCallbackParams {
  const query = new URLSearchParams(search);
  const hashParams = new URLSearchParams(hash.replace(/^#/, ''));
  const error =
    query.get('error_description') ??
    query.get('error') ??
    hashParams.get('error_description') ??
    hashParams.get('error');
  const code = query.get('code');
  const accessToken = hashParams.get('access_token');
  const refreshToken = hashParams.get('refresh_token');
  return {
    hasCallback: Boolean(error || code || accessToken || refreshToken),
    error,
    code,
    accessToken,
    refreshToken,
  };
}

export function clearAuthCallbackFromLocation(): void {
  if (typeof location === 'undefined' || typeof history === 'undefined') return;
  const url = new URL(location.href);
  for (const key of ['code', 'error', 'error_description', 'state']) url.searchParams.delete(key);
  const hashParams = new URLSearchParams(url.hash.replace(/^#/, ''));
  const hashHasAuth = ['access_token', 'refresh_token', 'expires_in', 'token_type', 'type', 'error', 'error_description']
    .some((key) => hashParams.has(key));
  if (hashHasAuth) url.hash = '';
  const next = `${url.pathname}${url.search}${url.hash}`;
  history.replaceState(null, document.title, next || '/');
}

export async function consumeAuthCallbackFromLocation(): Promise<ConsumeAuthCallbackResult> {
  if (typeof location === 'undefined') return { handled: false, ok: false };
  const params = readAuthCallbackParams(location.search, location.hash);
  if (!params.hasCallback) return { handled: false, ok: false };

  if (params.error) return { handled: true, ok: false, reason: `provider rejected sign-in · ${params.error}` };

  const authAttempt = currentAuthenticationAttempt('fresh');
  if (!authAttempt) {
    return { handled: true, ok: false, reason: 'secure authentication attempt is missing or expired' };
  }
  const providerController = new AbortController();
  const client = getAbortableAuthCallbackClient(providerController.signal);
  if (!client) return { handled: true, ok: false, stub: true, reason: 'auth client is not configured' };

  let providerMutationDispatched = false;
  try {
    let result: AuthSessionResult;
    if (params.code) {
      providerMutationDispatched = true;
      result = await withTimeout(
        client.auth.exchangeCodeForSession(params.code),
        10_000,
        () => providerController.abort(new DOMException('Provider handoff timed out', 'TimeoutError')),
      ) as AuthSessionResult;
    } else if (params.accessToken && params.refreshToken) {
      providerMutationDispatched = true;
      result = await withTimeout(
        client.auth.setSession({ access_token: params.accessToken, refresh_token: params.refreshToken }),
        10_000,
        () => providerController.abort(new DOMException('Provider handoff timed out', 'TimeoutError')),
      ) as AuthSessionResult;
    } else {
      return { handled: true, ok: false, reason: 'incomplete authentication response' };
    }

    if (result.error || !result.data?.session) {
      return {
        handled: true,
        ok: false,
        providerSessionUncertain: providerMutationDispatched,
        providerSessionAuthAttempt: providerMutationDispatched ? authAttempt : undefined,
        reason: result.error?.message ?? 'no session found',
      };
    }

    const freshSession = {
      accessToken: result.data.session.access_token,
      subjectKey: result.data.session.user?.id,
      authAttempt,
    };
    if (typeof freshSession.subjectKey !== 'string' || freshSession.subjectKey.length === 0) {
      return {
        handled: true,
        ok: false,
        providerSessionUncertain: true,
        providerSessionAuthAttempt: authAttempt,
        reason: 'verified session subject is unavailable',
      };
    }
    const mirrored = await persistSessionToCookie(result.data.session.access_token, {
      reauthenticated: true,
      authAttempt,
    });
    if (mirrored.status !== 'established') {
      return {
        handled: true,
        ok: false,
        freshSession: freshSession as { accessToken: string; subjectKey: string; authAttempt: string },
        mirrorStatus: mirrored.status,
        reason: mirrored.status === 'commit_uncertain'
          ? 'server session commit could not be confirmed (AUTH_SESSION_COMMIT_UNCERTAIN)'
          : 'server session boundary is unavailable',
      };
    }
    clearAuthCallbackFromLocation();
    return {
      handled: true,
      ok: true,
      freshSession: freshSession as { accessToken: string; subjectKey: string; authAttempt: string },
    };
  } catch (err: unknown) {
    const isTimeout = err instanceof Error && err.message === 'timeout';
    return {
      handled: true,
      ok: false,
      providerSessionUncertain: providerMutationDispatched,
      providerSessionAuthAttempt: providerMutationDispatched ? authAttempt : undefined,
      reason: isTimeout ? 'sign-in timed out' : String(err),
    };
  }
}
