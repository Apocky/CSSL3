const DEFAULT_AUTH_RETURN_PATH = '/account';

function normalizeLegacyPath(path: string): string {
  if (path === '/chat') return '/admin/chat';
  if (path.startsWith('/chat?')) return `/admin/chat${path.slice('/chat'.length)}`;
  if (path.startsWith('/chat#')) return `/admin/chat${path.slice('/chat'.length)}`;
  return path;
}

export function normalizeAuthReturnPath(value: unknown, fallback = DEFAULT_AUTH_RETURN_PATH): string {
  if (typeof value !== 'string') return fallback;
  const raw = value.trim();
  if (!raw || raw.startsWith('//')) return fallback;
  if (/^https?:\/\//i.test(raw)) return fallback;
  if (!raw.startsWith('/')) return fallback;

  try {
    const url = new URL(raw, 'https://apocky.local');
    const normalized = normalizeLegacyPath(`${url.pathname}${url.search}${url.hash}`);
    if (normalized === '/' || normalized.startsWith('/api/')) return fallback;
    if (normalized.startsWith('/auth/callback') || normalized.startsWith('/login') || normalized.startsWith('/register')) return fallback;
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
  const normalized = normalizeAuthReturnPath(returnPath, '/admin/chat');
  return `/login?next=${encodeURIComponent(normalized)}`;
}