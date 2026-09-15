const DEFAULT_AUTH_RETURN_PATH = '/account';

// Paths that do not resolve for an authorized owner. Never complete an otherwise-successful
// sign-in by sending a person somewhere that 404s — they would land on /account with no
// explanation and have to retype the URL they were already on.
//
// The old comment said these were "intentionally retired by middleware", and that claim is what
// let the list rot: middleware.ts retires /apoc and /apx and nothing else, but this listed eight
// /admin/* consoles and /chat as well. All nine resolve live — the consoles return 200 and /chat
// 308s to /apocrypha, an explicitly allowed return target. The effect was that signing in from any
// admin console silently dropped the owner on /account instead of the page they were on.
//
// Check membership against whether a page FILE exists, not against middleware.
const RETIRED_AUTH_RETURN_EXACT = new Set([
  '/apoc',
  '/apx',
]);

const RETIRED_AUTH_RETURN_PREFIXES = ['/apoc/', '/apocrypha/', '/apx/', '/chat/', '/admin/apocrypha/'] as const;

export function isAvailableAuthReturnPath(pathname: string): boolean {
  return !RETIRED_AUTH_RETURN_EXACT.has(pathname)
    && !RETIRED_AUTH_RETURN_PREFIXES.some((prefix) => pathname.startsWith(prefix));
}

export function normalizeAuthReturnPath(value: unknown, fallback = DEFAULT_AUTH_RETURN_PATH): string {
  if (typeof value !== 'string') return fallback;
  const raw = value.trim();
  if (!raw || raw.startsWith('//')) return fallback;
  if (/^https?:\/\//i.test(raw)) return fallback;
  if (!raw.startsWith('/')) return fallback;

  try {
    const url = new URL(raw, 'https://apocky.local');
    const normalized = `${url.pathname}${url.search}${url.hash}`;
    if (normalized === '/' || normalized.startsWith('/api/')) return fallback;
    if (normalized.startsWith('/auth/callback') || normalized.startsWith('/login') || normalized.startsWith('/register')) return fallback;
    if (!isAvailableAuthReturnPath(url.pathname)) return fallback;
    return normalized;
  } catch {
    return fallback;
  }
}

export function buildAuthCallbackUrl(origin: string, returnPath: string): string {
  const url = new URL('/auth/callback', origin);
  const normalized = normalizeAuthReturnPath(returnPath);
  if (normalized !== DEFAULT_AUTH_RETURN_PATH) url.searchParams.set('next', normalized);
  return url.toString();
}

export function loginHrefForReturnPath(returnPath: string): string {
  const normalized = normalizeAuthReturnPath(returnPath);
  return `/login?next=${encodeURIComponent(normalized)}`;
}
