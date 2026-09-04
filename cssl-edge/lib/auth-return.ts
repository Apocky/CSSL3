const DEFAULT_AUTH_RETURN_PATH = '/account';

// These routes are intentionally retired by middleware. Never complete an
// otherwise-successful sign-in by sending a person into a known 404.
const RETIRED_AUTH_RETURN_EXACT = new Set([
  '/apoc',
  '/apx',
  '/chat',
  '/admin/apex',
  '/admin/apocrypha',
  '/admin/chat',
  '/admin/coder',
  '/admin/cognition',
  '/admin/controls',
  '/admin/diagnostics',
  '/admin/sub-minds',
  '/admin/tools',
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
