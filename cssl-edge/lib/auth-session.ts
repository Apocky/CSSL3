import type { NextApiRequest } from 'next';
import { authFenceCookieNames, freshLogoutFence } from './auth-fence';

const MAX_SESSION_SECONDS = 60 * 60;

function first(value: string | string[] | undefined): string | null {
  const item = Array.isArray(value) ? value[0] : value;
  return item?.split(',')[0]?.trim() || null;
}

export function requestOrigin(req: NextApiRequest): string | null {
  const host = first(req.headers['x-forwarded-host']) ?? first(req.headers.host);
  if (!host) return null;
  const proto = first(req.headers['x-forwarded-proto']) ?? (host.startsWith('localhost') || host.startsWith('127.0.0.1') ? 'http' : 'https');
  try {
    return new URL(`${proto}://${host}`).origin;
  } catch {
    return null;
  }
}

export function hasSameOrigin(req: NextApiRequest): boolean {
  const expected = requestOrigin(req);
  const presented = first(req.headers.origin);
  if (!expected || !presented) return false;
  try {
    return new URL(presented).origin === expected;
  } catch {
    return false;
  }
}

export function bearerFromRequest(req: NextApiRequest): string | null {
  const header = first(req.headers.authorization);
  if (!header?.startsWith('Bearer ')) return null;
  const token = header.slice('Bearer '.length).trim();
  return token || null;
}

export function jwtLifetimeSeconds(token: string, nowMs = Date.now()): number | null {
  const encoded = token.split('.')[1];
  if (!encoded) return null;
  try {
    const payload = JSON.parse(Buffer.from(encoded, 'base64url').toString('utf8')) as { exp?: unknown };
    if (typeof payload.exp !== 'number' || !Number.isFinite(payload.exp)) return null;
    const remaining = Math.floor(payload.exp - nowMs / 1000);
    if (remaining <= 30 || remaining > MAX_SESSION_SECONDS) return null;
    return remaining;
  } catch {
    return null;
  }
}

export function sessionCookies(token: string, maxAge: number, production: boolean, sessionBinding: string): string[] {
  const secure = production ? '; Secure' : '';
  const activeName = production ? '__Host-apocky-access-token' : 'apocky-access-token';
  const alternateName = production ? 'apocky-access-token' : '__Host-apocky-access-token';
  const bindingNames = authFenceCookieNames(production);
  const alternateBindingName = production ? 'apocky-session-v2' : '__Host-apocky-session-v2';
  return [
    `${activeName}=${encodeURIComponent(token)}; Path=/; Max-Age=${maxAge}; HttpOnly${secure}; SameSite=Strict`,
    `${alternateName}=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Strict`,
    `${bindingNames.session}=${encodeURIComponent(sessionBinding)}; Path=/; Max-Age=${30 * 24 * 60 * 60}; HttpOnly${secure}; SameSite=Strict`,
    `${alternateBindingName}=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Strict`,
    'sb-access-token=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Strict',
    'sb-refresh-token=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Strict',
  ];
}

export function clearedSessionCookies(
  production = process.env.NODE_ENV === 'production',
  nextFence = freshLogoutFence(),
): string[] {
  const secure = production ? '; Secure' : '';
  const fenceNames = authFenceCookieNames(production);
  const alternateFenceName = production ? 'apocky-logout-v1' : '__Host-apocky-logout-v1';
  return [
    '__Host-apocky-access-token=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Strict',
    'apocky-access-token=; Path=/; Max-Age=0; HttpOnly; SameSite=Strict',
    '__Host-apocky-session-v2=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Strict',
    'apocky-session-v2=; Path=/; Max-Age=0; HttpOnly; SameSite=Strict',
    `${fenceNames.fence}=${encodeURIComponent(nextFence)}; Path=/; Max-Age=${180 * 24 * 60 * 60}; HttpOnly${secure}; SameSite=Strict`,
    `${alternateFenceName}=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Strict`,
    'sb-access-token=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Strict',
    'sb-refresh-token=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Strict',
  ];
}
