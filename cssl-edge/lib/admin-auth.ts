import type { NextApiRequest } from 'next';

import { getAuthClient } from './auth';

export interface RequestUser {
  id: string;
  email: string;
  provider: string;
  createdAt: string;
}

export interface RequestUserResult {
  user: RequestUser | null;
  reason?: string;
  authConfigured: boolean;
}

export interface AdminAuthorizationResult extends RequestUserResult {
  authorized: boolean;
}

const DEFAULT_ALLOWLIST = ['apocky13@gmail.com'];

function withTimeout<T>(promise: Promise<T>, timeoutMs: number): Promise<T> {
  return Promise.race([
    promise,
    new Promise<never>((_, reject) => setTimeout(() => reject(new Error('timeout')), timeoutMs)),
  ]);
}

function firstHeaderValue(value: string | string[] | undefined): string | null {
  const first = Array.isArray(value) ? value[0] : value;
  return first?.split(',')[0]?.trim() || null;
}

function readCookie(cookies: string, name: string): string | null {
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const match = cookies.match(new RegExp(`(?:^|;\\s*)${escaped}=([^;]+)`));
  const captured = match?.[1];
  return captured ? decodeURIComponent(captured) : null;
}

function testAdminUser(req: NextApiRequest): RequestUser | null {
  if (process.env.NODE_ENV === 'production') return null;
  if (process.env.LAZARUS_TEST_AUTH_BYPASS !== '1') return null;
  const email = firstHeaderValue(req.headers['x-apocky-test-admin-email']);
  if (!email || !email.includes('@')) return null;
  return {
    id: 'test-admin',
    email,
    provider: 'test',
    createdAt: new Date(0).toISOString(),
  };
}

export function getAdminAllowlist(): string[] {
  const env = process.env.APOCKY_ADMIN_EMAILS;
  if (!env) return DEFAULT_ALLOWLIST;
  return env
    .split(',')
    .map((s) => s.trim().toLowerCase())
    .filter((s) => s.length > 0 && s.includes('@'));
}

export function getAccessTokenFromRequest(req: NextApiRequest): string | null {
  const authHeader = firstHeaderValue(req.headers.authorization);
  if (authHeader?.startsWith('Bearer ')) {
    const token = authHeader.slice('Bearer '.length).trim();
    if (token) return token;
  }
  return readCookie(req.headers.cookie ?? '', 'sb-access-token');
}

export async function getRequestUser(req: NextApiRequest, timeoutMs = 5000): Promise<RequestUserResult> {
  const testUser = testAdminUser(req);
  if (testUser) return { user: testUser, authConfigured: true };

  const accessToken = getAccessTokenFromRequest(req);
  if (!accessToken) {
    return {
      user: null,
      authConfigured: true,
      reason: 'Not signed in · sign in at /login with admin email.',
    };
  }

  const client = getAuthClient();
  if (!client) {
    return {
      user: null,
      authConfigured: false,
      reason: 'Auth service is not configured for this deployment.',
    };
  }

  let result: Awaited<ReturnType<typeof client.auth.getUser>>;
  try {
    result = await withTimeout(client.auth.getUser(accessToken), timeoutMs);
  } catch {
    return {
      user: null,
      authConfigured: true,
      reason: 'Auth lookup timed out · sign in again or retry after Supabase responds.',
    };
  }

  const { data, error } = result;
  if (error || !data?.user) {
    return {
      user: null,
      authConfigured: true,
      reason: 'Session invalid or expired · sign in again.',
    };
  }

  return {
    authConfigured: true,
    user: {
      id: data.user.id,
      email: data.user.email ?? '(no email)',
      provider: data.user.app_metadata?.provider ?? 'email',
      createdAt: data.user.created_at ?? new Date().toISOString(),
    },
  };
}

export async function getAdminAuthorization(req: NextApiRequest, timeoutMs = 5000): Promise<AdminAuthorizationResult> {
  const session = await getRequestUser(req, timeoutMs);
  if (!session.user) return { ...session, authorized: false };

  const authorized = getAdminAllowlist().includes(session.user.email.toLowerCase());
  return {
    ...session,
    authorized,
    reason: authorized ? undefined : 'Email not on admin allowlist.',
  };
}