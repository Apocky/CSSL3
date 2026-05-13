import { getAuthClient, persistSessionToCookie } from './auth';

interface AuthSessionResult {
  data?: {
    session?: {
      access_token: string;
      refresh_token?: string | null;
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
  stub?: boolean;
  reason?: string;
}

function withTimeout<T>(promise: Promise<T>, timeoutMs: number): Promise<T> {
  return Promise.race([
    promise,
    new Promise<never>((_, reject) => setTimeout(() => reject(new Error('timeout')), timeoutMs)),
  ]);
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

  const client = getAuthClient();
  if (!client) return { handled: true, ok: false, stub: true, reason: 'auth client is not configured' };

  try {
    let result: AuthSessionResult;
    if (params.code) {
      result = await withTimeout(client.auth.exchangeCodeForSession(params.code), 10_000) as AuthSessionResult;
    } else if (params.accessToken && params.refreshToken) {
      result = await withTimeout(
        client.auth.setSession({ access_token: params.accessToken, refresh_token: params.refreshToken }),
        10_000,
      ) as AuthSessionResult;
    } else {
      result = await withTimeout(client.auth.getSession(), 5_000) as AuthSessionResult;
    }

    if (result.error || !result.data?.session) {
      return {
        handled: true,
        ok: false,
        reason: result.error?.message ?? 'no session found',
      };
    }

    persistSessionToCookie(
      result.data.session.access_token,
      result.data.session.refresh_token ?? undefined,
    );
    clearAuthCallbackFromLocation();
    return { handled: true, ok: true };
  } catch (err: unknown) {
    const isTimeout = err instanceof Error && err.message === 'timeout';
    return {
      handled: true,
      ok: false,
      reason: isTimeout ? 'sign-in timed out' : String(err),
    };
  }
}