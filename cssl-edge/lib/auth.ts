// cssl-edge/lib/auth.ts · Supabase-Auth wrapper for apocky.com hub
// Per spec/22 : single SSO across all Apocky-projects via JWT issued-by-hub-Supabase.
// Null-fallback when APOCKY_HUB_SUPABASE_URL env-var missing (stage-0 stub mode).

import { createClient, type SupabaseClient } from '@supabase/supabase-js';

let cachedClient: SupabaseClient | null = null;

const AUTH_MUTATION_LOCK = 'apocky-auth-cookie-mutation-v1';
const AUTH_CALLBACK_PROVIDER_LOCK = 'apocky-auth-callback-provider-v1';
const AUTH_MUTATION_STATE_KEY = 'apocky.auth-cookie-mutation.v1';
const AUTH_ATTEMPT_STORAGE_KEY = 'apocky.auth-attempt.v1';
const AUTH_FENCE_PROTOCOL = 'apocky.auth-fence.v1';

interface BrowserAuthMutationState {
  readonly schema_version: 'apocky.auth-cookie-mutation.v1';
  readonly generation: string;
  readonly phase: 'active' | 'signing-out' | 'signed-out';
}

interface BrowserAuthMutationRead {
  readonly available: boolean;
  readonly state: BrowserAuthMutationState | null;
}

const AUTH_COOKIE_MIRROR_TIMEOUT_MS = 8_000;
const AUTH_MUTATION_LOCK_WAIT_MS = 2_000;

export type BrowserAuthAttemptMode = 'fresh' | 'refresh';
export type AuthSessionMirrorResult =
  | { readonly status: 'established' }
  | { readonly status: 'not_established' }
  | { readonly status: 'commit_uncertain' };

const SESSION_ESTABLISHED: AuthSessionMirrorResult = { status: 'established' };
const SESSION_NOT_ESTABLISHED: AuthSessionMirrorResult = { status: 'not_established' };
const SESSION_COMMIT_UNCERTAIN: AuthSessionMirrorResult = { status: 'commit_uncertain' };

interface BrowserAuthAttempt {
  readonly schema_version: 'apocky.auth-attempt.v1';
  readonly mode: BrowserAuthAttemptMode;
  readonly ticket: string;
  readonly expires_at_ms: number;
}

function freshAuthGeneration(): string {
  if (typeof crypto?.randomUUID === 'function') return crypto.randomUUID();
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return [...bytes].map(value => value.toString(16).padStart(2, '0')).join('');
}

function readAuthMutationState(): BrowserAuthMutationRead {
  try {
    const raw = localStorage.getItem(AUTH_MUTATION_STATE_KEY);
    if (!raw) return { available: true, state: null };
    const parsed = JSON.parse(raw) as Partial<BrowserAuthMutationState>;
    const state = parsed.schema_version === 'apocky.auth-cookie-mutation.v1'
      && typeof parsed.generation === 'string'
      && ['active', 'signing-out', 'signed-out'].includes(parsed.phase ?? '')
      ? parsed as BrowserAuthMutationState
      : null;
    return state ? { available: true, state } : { available: false, state: null };
  } catch {
    return { available: false, state: null };
  }
}

function writeAuthMutationState(state: BrowserAuthMutationState): boolean {
  try {
    localStorage.setItem(AUTH_MUTATION_STATE_KEY, JSON.stringify(state));
    const written = readAuthMutationState();
    return written.available
      && written.state?.generation === state.generation
      && written.state.phase === state.phase;
  } catch {
    return false;
  }
}

interface BrowserAuthLockManager {
  request<U>(name: string, options: { signal: AbortSignal }, callback: () => Promise<U>): Promise<U>;
}

function browserAuthLockManager(): BrowserAuthLockManager | null {
  return (navigator as Navigator & {
    locks?: BrowserAuthLockManager;
  }).locks ?? null;
}

async function requestBrowserAuthMutationLock<T>(
  lockManager: BrowserAuthLockManager,
  operation: () => Promise<T>,
): Promise<T> {
  const controller = new AbortController();
  let acquired = false;
  const timer = window.setTimeout(() => {
    if (!acquired) controller.abort(new DOMException('Timed out waiting for authentication lock', 'TimeoutError'));
  }, AUTH_MUTATION_LOCK_WAIT_MS);
  try {
    return await lockManager.request(AUTH_MUTATION_LOCK, { signal: controller.signal }, async () => {
      acquired = true;
      window.clearTimeout(timer);
      return operation();
    });
  } finally {
    window.clearTimeout(timer);
  }
}

function writeAuthAttempt(attempt: BrowserAuthAttempt): boolean {
  try {
    const serialized = JSON.stringify(attempt);
    sessionStorage.setItem(AUTH_ATTEMPT_STORAGE_KEY, serialized);
    localStorage.setItem(AUTH_ATTEMPT_STORAGE_KEY, serialized);
    return sessionStorage.getItem(AUTH_ATTEMPT_STORAGE_KEY) === serialized;
  } catch {
    return false;
  }
}

function clearAuthAttempt(): void {
  try { sessionStorage.removeItem(AUTH_ATTEMPT_STORAGE_KEY); } catch { /* unavailable storage cannot retain a reusable ticket */ }
  try { localStorage.removeItem(AUTH_ATTEMPT_STORAGE_KEY); } catch { /* unavailable storage cannot retain a reusable ticket */ }
}

function readAuthAttempt(mode: BrowserAuthAttemptMode): BrowserAuthAttempt | null {
  for (const storage of [typeof sessionStorage === 'undefined' ? null : sessionStorage, typeof localStorage === 'undefined' ? null : localStorage]) {
    if (!storage) continue;
    try {
      const raw = storage.getItem(AUTH_ATTEMPT_STORAGE_KEY);
      if (!raw) continue;
      const parsed = JSON.parse(raw) as Partial<BrowserAuthAttempt>;
      if (
        parsed.schema_version === 'apocky.auth-attempt.v1'
        && parsed.mode === mode
        && typeof parsed.ticket === 'string'
        && parsed.ticket.length >= 80
        && parsed.ticket.length <= 8_192
        && typeof parsed.expires_at_ms === 'number'
        && Number.isFinite(parsed.expires_at_ms)
        && parsed.expires_at_ms > Date.now()
      ) return parsed as BrowserAuthAttempt;
    } catch { /* try the other browser storage */ }
  }
  return null;
}

function readSharedAuthAttempt(mode: BrowserAuthAttemptMode): BrowserAuthAttempt | null {
  if (typeof localStorage === 'undefined') return null;
  try {
    const raw = localStorage.getItem(AUTH_ATTEMPT_STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<BrowserAuthAttempt>;
    if (
      parsed.schema_version === 'apocky.auth-attempt.v1'
      && parsed.mode === mode
      && typeof parsed.ticket === 'string'
      && parsed.ticket.length >= 80
      && parsed.ticket.length <= 8_192
      && typeof parsed.expires_at_ms === 'number'
      && Number.isFinite(parsed.expires_at_ms)
      && parsed.expires_at_ms > Date.now()
    ) return parsed as BrowserAuthAttempt;
  } catch { /* unavailable shared storage retains fail-closed cleanup */ }
  return null;
}

export function currentAuthenticationAttempt(mode: BrowserAuthAttemptMode): string | null {
  return readAuthAttempt(mode)?.ticket ?? null;
}

export async function beginAuthenticationAttempt(mode: BrowserAuthAttemptMode): Promise<string | null> {
  if (typeof fetch === 'undefined') return null;
  const lockManager = browserAuthLockManager();
  if (!lockManager) return null;
  try {
    return await requestBrowserAuthMutationLock(lockManager, async () => {
      const mutation = readAuthMutationState();
      if (!mutation.available || mutation.state?.phase === 'signing-out') return null;
      if (mode === 'refresh' && mutation.state?.phase === 'signed-out') return null;
      const controller = new AbortController();
      const timeout = window.setTimeout(() => controller.abort(new DOMException('Timed out', 'TimeoutError')), AUTH_COOKIE_MIRROR_TIMEOUT_MS);
      try {
        const response = await fetch('/api/auth/attempt', {
          method: 'POST',
          credentials: 'same-origin',
          cache: 'no-store',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ mode }),
          signal: controller.signal,
        });
        if (!response.ok) return null;
        const payload = await response.json() as Record<string, unknown>;
        const expiresAtMs = typeof payload.expires_at === 'string' ? Date.parse(payload.expires_at) : Number.NaN;
        const providerStartDelayMs = typeof payload.provider_start_delay_ms === 'number'
          ? payload.provider_start_delay_ms
          : Number.NaN;
        if (
          payload.schema_version !== AUTH_FENCE_PROTOCOL
          || payload.status !== 'ready'
          || payload.mode !== mode
          || typeof payload.ticket !== 'string'
          || payload.ticket.length < 80
          || payload.ticket.length > 8_192
          || !Number.isFinite(expiresAtMs)
          || expiresAtMs <= Date.now()
          || !Number.isInteger(providerStartDelayMs)
          || providerStartDelayMs < 0
          || providerStartDelayMs > 1_000
        ) return null;
        const attempt: BrowserAuthAttempt = {
          schema_version: 'apocky.auth-attempt.v1',
          mode,
          ticket: payload.ticket,
          expires_at_ms: expiresAtMs,
        };
        if (!writeAuthAttempt(attempt)) return null;
        if (providerStartDelayMs > 0) {
          await new Promise<void>(resolve => window.setTimeout(resolve, providerStartDelayMs));
        }
        return attempt.ticket;
      } finally {
        window.clearTimeout(timeout);
      }
    });
  } catch {
    return null;
  }
}

async function withBrowserAuthMutationLock<T>(operation: () => Promise<T>): Promise<T> {
  const lockManager = browserAuthLockManager();
  if (!lockManager) throw new Error('APOCKY_AUTH_MUTATION_LOCK_UNAVAILABLE');
  return requestBrowserAuthMutationLock(lockManager, operation);
}

export function browserAuthMutationAllowsProtectedOpen(): boolean {
  const read = readAuthMutationState();
  return read.available && (!read.state || read.state.phase === 'active');
}

export async function whileBrowserAuthMutationActive(operation: () => Promise<boolean>): Promise<boolean> {
  try {
    return await withBrowserAuthMutationLock(async () => {
      const before = readAuthMutationState();
      if (!before.available || (before.state && before.state.phase !== 'active')) return false;
      const result = await operation();
      const after = readAuthMutationState();
      if (!after.available) return false;
      const unchanged = before.state
        ? after.state?.phase === 'active' && after.state.generation === before.state.generation
        : after.state === null;
      return result && unchanged;
    });
  } catch {
    return false;
  }
}

const DEFAULT_AUTH_ORIGIN = 'https://www.apocky.com';
const TRUSTED_AUTH_HOSTS = new Set([
  'apocky.com',
  'www.apocky.com',
  'apocky-com.vercel.app',
  'cssl-edge.vercel.app',
]);

type AuthRedirectHeaders = {
  origin?: string | string[];
  host?: string | string[];
  'x-forwarded-host'?: string | string[];
  'x-forwarded-proto'?: string | string[];
};

function firstHeaderValue(value: string | string[] | undefined): string | null {
  const first = Array.isArray(value) ? value[0] : value;
  return first?.split(',')[0]?.trim() || null;
}

function isLocalAuthHost(hostname: string): boolean {
  return hostname === 'localhost' || hostname === '127.0.0.1' || hostname === '::1' || hostname === '[::1]';
}

function isVercelPreviewHost(hostname: string): boolean {
  return hostname.endsWith('.vercel.app') && (hostname.startsWith('apocky-') || hostname.startsWith('cssl-edge-'));
}

function isTrustedRequestOrigin(origin: string): boolean {
  try {
    const url = new URL(origin);
    if (url.protocol === 'https:' && TRUSTED_AUTH_HOSTS.has(url.hostname)) return true;
    if (url.protocol === 'https:' && isVercelPreviewHost(url.hostname)) return true;
    if (url.protocol === 'http:' && isLocalAuthHost(url.hostname) && process.env.NODE_ENV !== 'production') return true;
  } catch {
    return false;
  }
  return false;
}

function requestOriginFromHeaders(headers?: AuthRedirectHeaders): string {
  const forwardedHost = firstHeaderValue(headers?.['x-forwarded-host']);
  const host = forwardedHost ?? firstHeaderValue(headers?.host);
  if (!host) return DEFAULT_AUTH_ORIGIN;

  const forwardedProto = firstHeaderValue(headers?.['x-forwarded-proto']);
  const proto = forwardedProto ?? (host.startsWith('localhost') || host.startsWith('127.0.0.1') ? 'http' : 'https');
  const origin = `${proto}://${host}`;
  return isTrustedRequestOrigin(origin) ? new URL(origin).origin : DEFAULT_AUTH_ORIGIN;
}

function isTrustedRedirectTarget(url: URL, requestOrigin: string): boolean {
  if (url.protocol === 'https:' && TRUSTED_AUTH_HOSTS.has(url.hostname)) return true;
  if (url.origin === requestOrigin && isTrustedRequestOrigin(requestOrigin)) return true;
  return false;
}

export function resolveAuthRedirect(redirectTo: unknown, headers?: AuthRedirectHeaders): string {
  const requestOrigin = requestOriginFromHeaders(headers);
  const fallback = new URL('/account', requestOrigin).toString();

  if (typeof redirectTo !== 'string' || !redirectTo.trim()) return fallback;

  try {
    const target = new URL(redirectTo.trim(), requestOrigin);
    if (!isTrustedRedirectTarget(target, requestOrigin)) return fallback;
    if (target.protocol === 'http:' && !isLocalAuthHost(target.hostname)) return fallback;
    return target.toString();
  } catch {
    return fallback;
  }
}

/**
 * Returns the apocky-hub Supabase client OR null if env vars missing.
 * Pages/routes MUST handle null-case gracefully (show stub-mode UI).
 */
export function getAuthClient(): SupabaseClient | null {
  if (cachedClient) return cachedClient;
  const url = process.env.APOCKY_HUB_SUPABASE_URL ?? process.env.NEXT_PUBLIC_SUPABASE_URL;
  const anonKey = process.env.APOCKY_HUB_SUPABASE_ANON_KEY ?? process.env.NEXT_PUBLIC_SUPABASE_ANON_KEY;
  if (!url || !anonKey) return null;
  cachedClient = createClient(url, anonKey, {
    auth: {
      persistSession: true,
      autoRefreshToken: true,
      // The callback page explicitly exchanges PKCE ?code= for a session. Keeping
      // background URL detection enabled can race that exchange in Next dev mode.
      detectSessionInUrl: false,
      // pkce keeps the OAuth verifier in browser storage; OAuth must start client-side.
      flowType: 'pkce',
    },
  });
  return cachedClient;
}

function abortError(signal: AbortSignal): unknown {
  return signal.reason ?? new DOMException('Authentication request was cancelled', 'AbortError');
}

function authFetchBoundTo(signal: AbortSignal): typeof fetch {
  return (async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    if (signal.aborted) throw abortError(signal);
    const controller = new AbortController();
    const inheritedSignal = init?.signal
      ?? (typeof Request !== 'undefined' && input instanceof Request ? input.signal : null);
    const abortFromFence = (): void => controller.abort(abortError(signal));
    const abortFromRequest = (): void => {
      if (inheritedSignal) controller.abort(abortError(inheritedSignal));
    };
    signal.addEventListener('abort', abortFromFence, { once: true });
    inheritedSignal?.addEventListener('abort', abortFromRequest, { once: true });
    if (signal.aborted) abortFromFence();
    else if (inheritedSignal?.aborted) abortFromRequest();
    try {
      return await fetch(input, { ...init, signal: controller.signal });
    } finally {
      signal.removeEventListener('abort', abortFromFence);
      inheritedSignal?.removeEventListener('abort', abortFromRequest);
    }
  }) as typeof fetch;
}

/**
 * A callback-scoped client whose provider network work can be cancelled before
 * auth-js persists a session. The ordinary cached client intentionally remains
 * separate so a timed-out callback cannot keep mutating its session in the
 * background after the UI has locked the private Brain and closed auth.
 */
export function getAbortableAuthCallbackClient(signal: AbortSignal): SupabaseClient | null {
  const url = process.env.APOCKY_HUB_SUPABASE_URL ?? process.env.NEXT_PUBLIC_SUPABASE_URL;
  const anonKey = process.env.APOCKY_HUB_SUPABASE_ANON_KEY ?? process.env.NEXT_PUBLIC_SUPABASE_ANON_KEY;
  if (!url || !anonKey) return null;
  return createClient(url, anonKey, {
    auth: {
      persistSession: true,
      autoRefreshToken: false,
      detectSessionInUrl: false,
      flowType: 'pkce',
      // Callback mutations share their own abortable cross-tab lock. Reusing
      // auth-js's default storage-key lock would contend with the cached client
      // and trigger its 5s lock-steal path; a no-op lock would let two callback
      // tabs race writes to the same session storage.
      lock: async <R>(_name: string, _acquireTimeout: number, operation: () => Promise<R>): Promise<R> => {
        const lockManager = browserAuthLockManager();
        if (!lockManager) throw new Error('APOCKY_AUTH_CALLBACK_LOCK_UNAVAILABLE');
        return lockManager.request(AUTH_CALLBACK_PROVIDER_LOCK, { signal }, operation);
      },
    },
    global: { fetch: authFetchBoundTo(signal) },
  });
}

// Ask the same-origin server to validate the bearer and issue a short-lived,
// HttpOnly session mirror. The refresh token never enters a cookie.
export async function persistSessionToCookie(
  accessToken: string,
  options: { reauthenticated?: boolean; authAttempt?: string } = {},
): Promise<AuthSessionMirrorResult> {
  if (typeof fetch === 'undefined') return SESSION_NOT_ESTABLISHED;
  const mode: BrowserAuthAttemptMode = options.reauthenticated ? 'fresh' : 'refresh';
  const authAttempt = options.authAttempt ?? readAuthAttempt(mode)?.ticket ?? null;
  if (!authAttempt) return SESSION_NOT_ESTABLISHED;
  const observed = readAuthMutationState();
  if (
    !observed.available
    || observed.state?.phase === 'signing-out'
    || (!options.reauthenticated && observed.state?.phase === 'signed-out')
  ) return SESSION_NOT_ESTABLISHED;
  let requestDispatched = false;
  try {
    return await withBrowserAuthMutationLock(async () => {
    const before = readAuthMutationState();
    if (
      !before.available
      || before.state?.phase === 'signing-out'
      || (!options.reauthenticated && before.state?.phase === 'signed-out')
    ) return SESSION_NOT_ESTABLISHED;
    const controller = new AbortController();
    const timeout = window.setTimeout(() => controller.abort(new DOMException('Timed out', 'TimeoutError')), AUTH_COOKIE_MIRROR_TIMEOUT_MS);
    try {
      requestDispatched = true;
      const response = await fetch('/api/auth/session', {
        method: 'POST',
        headers: {
          Authorization: `Bearer ${accessToken}`,
          'Content-Type': 'application/json',
          'X-Apocky-Auth-Protocol': AUTH_FENCE_PROTOCOL,
          'X-Apocky-Auth-Attempt': authAttempt,
        },
        credentials: 'same-origin',
        cache: 'no-store',
        body: JSON.stringify({ mode }),
        signal: controller.signal,
      });
      if (!response.ok) return SESSION_NOT_ESTABLISHED;
      const after = readAuthMutationState();
      if (!after.available) return SESSION_COMMIT_UNCERTAIN;
      const unchanged = before.state
        ? after.state?.generation === before.state.generation && after.state.phase === before.state.phase
        : after.state === null;
      if (!unchanged) return SESSION_COMMIT_UNCERTAIN;
      if (options.reauthenticated) {
        return writeAuthMutationState({
          schema_version: 'apocky.auth-cookie-mutation.v1',
          generation: freshAuthGeneration(),
          phase: 'active',
        }) ? SESSION_ESTABLISHED : SESSION_COMMIT_UNCERTAIN;
      }
      return SESSION_ESTABLISHED;
    } catch {
      return requestDispatched ? SESSION_COMMIT_UNCERTAIN : SESSION_NOT_ESTABLISHED;
    } finally {
      window.clearTimeout(timeout);
    }
    });
  } catch {
    return requestDispatched ? SESSION_COMMIT_UNCERTAIN : SESSION_NOT_ESTABLISHED;
  }
}

export async function withBrowserAuthSignOut<T>(
  operation: () => Promise<T>,
  onFenced?: () => Promise<void>,
): Promise<T> {
  const generation = freshAuthGeneration();
  writeAuthMutationState({ schema_version: 'apocky.auth-cookie-mutation.v1', generation, phase: 'signing-out' });
  clearAuthAttempt();
  await onFenced?.();
  const operationWithFinalState = async (): Promise<T> => {
    try {
      return await operation();
    } finally {
      const current = readAuthMutationState();
      if (current.available && current.state?.generation === generation) {
        writeAuthMutationState({ ...current.state, phase: 'signed-out' });
      }
    }
  };
  const lockManager = browserAuthLockManager();
  if (!lockManager) return operationWithFinalState();
  try {
    return await requestBrowserAuthMutationLock(lockManager, operationWithFinalState);
  } catch {
    // signing-out was published before acquisition. If the lock mechanism is
    // stalled, the abort cancels its queued callback and direct cleanup is safer
    // than leaving the authenticated browser and server sessions alive.
    return operationWithFinalState();
  }
}

async function boundedAuthenticationCleanup(operation: Promise<boolean>, timeoutMs = 5_000): Promise<boolean> {
  return new Promise((resolve) => {
    let settled = false;
    const finish = (value: boolean): void => {
      if (settled) return;
      settled = true;
      window.clearTimeout(timer);
      resolve(value);
    };
    const timer = window.setTimeout(() => finish(false), timeoutMs);
    void operation.then(finish, () => finish(false));
  });
}

export async function closeAuthenticationAfterPrivateLockFailure(): Promise<boolean> {
  const client = getAuthClient();
  let browserCleared = !client;
  let serverCleared = false;
  try {
    const completed = await boundedAuthenticationCleanup(withBrowserAuthSignOut(async () => {
      [browserCleared, serverCleared] = await Promise.all([
        client
          ? boundedAuthenticationCleanup(client.auth.signOut({ scope: 'local' }).then(result => !result.error))
          : Promise.resolve(true),
        boundedAuthenticationCleanup(fetch('/api/auth/logout', {
          method: 'POST',
          credentials: 'same-origin',
          cache: 'no-store',
        }).then(response => response.ok)),
      ]);
    }).then(() => true), 14_000);
    if (!completed) return false;
  } catch {
    return false;
  }
  return browserCleared && serverCleared;
}

export type AuthenticationAttemptCloseResult<T> =
  | { readonly status: 'closed' | 'failed'; readonly beforeCloseResult?: T }
  | { readonly status: 'superseded' };

/**
 * Close an uncertain callback only while its original fresh-auth ticket is
 * still the globally shared attempt. The ticket comparison, private-state
 * invalidation, and provider/server cleanup share AUTH_MUTATION_LOCK with
 * beginAuthenticationAttempt(), so an older callback cannot erase a newer
 * successful sign-in from another tab.
 */
export async function closeAuthenticationAttemptAfterPrivateLockFailure<T>(
  expectedFreshAttempt: string,
  beforeClose: () => Promise<T>,
): Promise<AuthenticationAttemptCloseResult<T>> {
  const client = getAuthClient();
  const operation = async (): Promise<AuthenticationAttemptCloseResult<T>> => {
    const sharedAttempt = readSharedAuthAttempt('fresh');
    if (sharedAttempt && sharedAttempt.ticket !== expectedFreshAttempt) {
      return { status: 'superseded' };
    }

    const generation = freshAuthGeneration();
    writeAuthMutationState({ schema_version: 'apocky.auth-cookie-mutation.v1', generation, phase: 'signing-out' });
    clearAuthAttempt();

    let beforeCloseResult: T | undefined;
    try {
      beforeCloseResult = await beforeClose();
    } catch { /* provider/server cleanup must still run */ }

    let browserCleared = !client;
    let serverCleared = false;
    try {
      [browserCleared, serverCleared] = await Promise.all([
        client
          ? boundedAuthenticationCleanup(client.auth.signOut({ scope: 'local' }).then(result => !result.error))
          : Promise.resolve(true),
        boundedAuthenticationCleanup(fetch('/api/auth/logout', {
          method: 'POST',
          credentials: 'same-origin',
          cache: 'no-store',
        }).then(response => response.ok)),
      ]);
    } finally {
      const current = readAuthMutationState();
      if (current.available && current.state?.generation === generation) {
        writeAuthMutationState({ ...current.state, phase: 'signed-out' });
      }
    }
    return {
      status: browserCleared && serverCleared ? 'closed' : 'failed',
      ...(beforeCloseResult === undefined ? {} : { beforeCloseResult }),
    };
  };

  const lockManager = browserAuthLockManager();
  if (!lockManager) return operation();
  try {
    return await requestBrowserAuthMutationLock(lockManager, operation);
  } catch {
    const sharedAttempt = readSharedAuthAttempt('fresh');
    if (sharedAttempt && sharedAttempt.ticket !== expectedFreshAttempt) {
      return { status: 'superseded' };
    }
    return operation();
  }
}

/** Auth-provider configuration · what's available · what's required to enable. */
export const AUTH_PROVIDERS = [
  { id: 'google', label: 'Google', enabled: true, gradient: '#4285f4' },
  { id: 'apple', label: 'Apple', enabled: true, gradient: '#000000' },
  { id: 'github', label: 'GitHub', enabled: true, gradient: '#24292e' },
  { id: 'discord', label: 'Discord', enabled: true, gradient: '#5865f2' },
  { id: 'twitter', label: 'X / Twitter', enabled: false, gradient: '#000000' },
  { id: 'spotify', label: 'Spotify', enabled: false, gradient: '#1db954' },
] as const;

export type AuthProviderId = typeof AUTH_PROVIDERS[number]['id'];

/** Apocky's external channels (not OAuth, just profile links). */
export const APOCKY_CHANNELS = [
  { label: '@noneisone.oneisall (medium)', href: 'https://medium.com/@noneisone.oneisall' },
  { label: 'ko-fi.com/oneinfinity', href: 'https://ko-fi.com/oneinfinity' },
  { label: 'patreon.com/0ne1nfinity', href: 'https://www.patreon.com/0ne1nfinity' },
  { label: 'github.com/Apocky', href: 'https://github.com/Apocky' },
] as const;

/** Profile-linkable social channels the player can attach to their profile. */
export const PROFILE_LINKABLE = [
  { id: 'medium', label: 'Medium', placeholder: '@yourhandle' },
  { id: 'twitter', label: 'X / Twitter', placeholder: '@yourhandle' },
  { id: 'bluesky', label: 'Bluesky', placeholder: 'yourhandle.bsky.social' },
  { id: 'mastodon', label: 'Mastodon', placeholder: '@yourhandle@instance' },
  { id: 'github', label: 'GitHub', placeholder: 'yourhandle' },
  { id: 'youtube', label: 'YouTube', placeholder: '@yourchannel' },
  { id: 'twitch', label: 'Twitch', placeholder: 'yourhandle' },
  { id: 'kofi', label: 'Ko-fi', placeholder: 'yourhandle' },
  { id: 'patreon', label: 'Patreon', placeholder: 'yourhandle' },
  { id: 'website', label: 'Personal site', placeholder: 'https://you.example' },
] as const;

export type LinkableId = typeof PROFILE_LINKABLE[number]['id'];

/** Magic-link sign-in. Returns true on success, false on stub-mode. */
export async function signInWithMagicLink(email: string, redirectTo: string): Promise<{ ok: boolean; reason?: string }> {
  const client = getAuthClient();
  if (!client) {
    return { ok: false, reason: 'stub-mode · APOCKY_HUB_SUPABASE_URL not set' };
  }
  const { error } = await client.auth.signInWithOtp({
    email,
    options: { emailRedirectTo: redirectTo },
  });
  if (error) return { ok: false, reason: error.message };
  return { ok: true };
}

/** OAuth sign-in. Redirects browser. */
export async function signInWithOAuth(provider: AuthProviderId, redirectTo: string): Promise<{ ok: boolean; reason?: string }> {
  const client = getAuthClient();
  if (!client) {
    return { ok: false, reason: 'stub-mode · APOCKY_HUB_SUPABASE_URL not set' };
  }
  const { error } = await client.auth.signInWithOAuth({
    provider: provider as 'google' | 'apple' | 'github' | 'discord',
    options: { redirectTo },
  });
  if (error) return { ok: false, reason: error.message };
  return { ok: true };
}

export async function signOut(): Promise<{ ok: boolean }> {
  const client = getAuthClient();
  if (!client) return { ok: true }; // no-op in stub mode
  await client.auth.signOut();
  return { ok: true };
}

export async function getCurrentUser(): Promise<{
  email: string;
  id: string;
  provider: string;
  createdAt: string;
} | null> {
  const client = getAuthClient();
  if (!client) return null;
  const { data, error } = await client.auth.getUser();
  if (error || !data.user) return null;
  return {
    email: data.user.email ?? '(no email)',
    id: data.user.id,
    provider: data.user.app_metadata?.provider ?? 'unknown',
    createdAt: data.user.created_at ?? new Date().toISOString(),
  };
}

/** Inline tests · exercised via npm test scripts. */
if (require.main === module) {
  // Smoke tests
  const stubClient = getAuthClient();
  console.log('§ auth smoke-test');
  console.log('  client present @', !!stubClient ? '✓' : '✗ stub-mode');
  console.log('  AUTH_PROVIDERS count =', AUTH_PROVIDERS.length);
  console.log('  PROFILE_LINKABLE count =', PROFILE_LINKABLE.length);
  console.log('  APOCKY_CHANNELS count =', APOCKY_CHANNELS.length);
  console.log('  expected ≥ 4 providers · ≥ 8 linkables · 4 channels');
  if (AUTH_PROVIDERS.length < 4) process.exit(1);
  if (PROFILE_LINKABLE.length < 8) process.exit(1);
  if (APOCKY_CHANNELS.length !== 4) process.exit(1);
  console.log('✓ all smoke checks passed');
}
