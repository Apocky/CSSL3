import {
  createHash,
  createHmac,
  randomBytes,
  randomUUID,
  timingSafeEqual,
} from 'node:crypto';
import type { NextApiRequest } from 'next';

export const AUTH_FENCE_PROTOCOL = 'apocky.auth-fence.v1';
export type AuthAttemptMode = 'fresh' | 'refresh';

const UUID_V4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/iu;
const TOKEN_PART = /^[A-Za-z0-9_-]{1,4096}$/u;
const ATTEMPT_TTL_MS = 15 * 60_000;
const SESSION_BINDING_TTL_MS = 30 * 24 * 60 * 60_000;
const INITIAL_FENCE = 'origin';

interface AuthAttemptPayload {
  readonly schema_version: typeof AUTH_FENCE_PROTOCOL;
  readonly kind: 'attempt';
  readonly mode: AuthAttemptMode;
  readonly fence: string;
  readonly nonce: string;
  readonly issued_at_ms: number;
  readonly expires_at_ms: number;
}

interface AuthSessionBindingPayload {
  readonly schema_version: typeof AUTH_FENCE_PROTOCOL;
  readonly kind: 'session';
  readonly fence: string;
  readonly subject_digest: string;
  readonly provider_session_digest: string;
  readonly issued_at_ms: number;
  readonly expires_at_ms: number;
}

export interface VerifiedJwtSessionClaims {
  readonly subject: string;
  readonly providerSessionId: string;
  readonly issuedAtMs: number;
  readonly expiresAtMs: number;
  readonly authenticationMethods: readonly {
    readonly method: string;
    readonly timestampMs: number;
  }[];
}

const INTERACTIVE_AUTHENTICATION_METHODS = new Set([
  'oauth',
  'password',
  'otp',
  'totp',
  'sso/saml',
  'magiclink',
  'email/signup',
]);

function secretBytes(explicit?: string): Buffer {
  const raw = explicit ?? process.env.APOCKY_AUTH_FENCE_SECRET
    ?? (process.env.NODE_ENV !== 'production' ? 'apocky-development-auth-fence-secret-v1' : '');
  if (raw !== raw.trim() || Buffer.byteLength(raw, 'utf8') < 32 || Buffer.byteLength(raw, 'utf8') > 8_192) {
    throw new Error('AUTH_FENCE_SECRET_UNAVAILABLE');
  }
  return createHmac('sha256', raw).update(AUTH_FENCE_PROTOCOL, 'utf8').digest();
}

function digest(value: string): string {
  return createHash('sha256').update(value, 'utf8').digest('hex');
}

function cookieNames(production: boolean): { readonly fence: string; readonly session: string } {
  return production
    ? { fence: '__Host-apocky-logout-v1', session: '__Host-apocky-session-v2' }
    : { fence: 'apocky-logout-v1', session: 'apocky-session-v2' };
}

function exactCookie(req: NextApiRequest, name: string): string | null | undefined {
  const header = req.headers.cookie ?? '';
  const values = header.split(';').flatMap((part) => {
    const separator = part.indexOf('=');
    if (separator < 0 || part.slice(0, separator).trim() !== name) return [];
    try { return [decodeURIComponent(part.slice(separator + 1).trim())]; } catch { return ['']; }
  });
  if (values.length === 0) return null;
  return values.length === 1 && values[0] ? values[0] : undefined;
}

export function currentAuthFence(
  req: NextApiRequest,
  production = process.env.NODE_ENV === 'production',
): string | null {
  const names = cookieNames(production);
  const value = exactCookie(req, names.fence);
  if (value === undefined) return null;
  if (value === null) return INITIAL_FENCE;
  return UUID_V4.test(value) ? value.toLowerCase() : null;
}

function sign(payload: AuthAttemptPayload | AuthSessionBindingPayload, explicitSecret?: string): string {
  const encoded = Buffer.from(JSON.stringify(payload), 'utf8').toString('base64url');
  const signature = createHmac('sha256', secretBytes(explicitSecret)).update(encoded, 'ascii').digest('base64url');
  return `${encoded}.${signature}`;
}

function verifySigned<T extends AuthAttemptPayload | AuthSessionBindingPayload>(
  token: string,
  explicitSecret?: string,
): T | null {
  if (token.length > 8_192) return null;
  const [encoded, signature, extra] = token.split('.');
  if (!encoded || !signature || extra || !TOKEN_PART.test(encoded) || !/^[A-Za-z0-9_-]{43}$/u.test(signature)) return null;
  const expected = createHmac('sha256', secretBytes(explicitSecret)).update(encoded, 'ascii').digest();
  const presented = Buffer.from(signature, 'base64url');
  if (presented.length !== expected.length || !timingSafeEqual(presented, expected)) return null;
  try {
    const parsed = JSON.parse(Buffer.from(encoded, 'base64url').toString('utf8')) as T;
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

export function issueAuthAttempt(input: {
  readonly req: NextApiRequest;
  readonly mode: AuthAttemptMode;
  readonly nowMs?: number;
  readonly production?: boolean;
  readonly secret?: string;
}): { readonly ticket: string; readonly expiresAtMs: number; readonly providerStartAfterMs: number } | null {
  const nowMs = input.nowMs ?? Date.now();
  const fence = currentAuthFence(input.req, input.production);
  if (!fence) return null;
  if (input.mode === 'refresh' && !readCurrentSessionBinding(input.req, nowMs, input.production, input.secret)) return null;
  const payload: AuthAttemptPayload = {
    schema_version: AUTH_FENCE_PROTOCOL,
    kind: 'attempt',
    mode: input.mode,
    fence,
    nonce: randomBytes(24).toString('base64url'),
    issued_at_ms: nowMs,
    expires_at_ms: nowMs + ATTEMPT_TTL_MS,
  };
  return {
    ticket: sign(payload, input.secret),
    expiresAtMs: payload.expires_at_ms,
    // Supabase JWT iat/amr timestamps have one-second resolution. Starting the
    // provider operation only after this boundary lets the server prove that
    // an interactive credential was minted after this exact attempt.
    providerStartAfterMs: Math.floor(payload.issued_at_ms / 1_000) * 1_000 + 1_000,
  };
}

export function verifyAuthAttempt(input: {
  readonly req: NextApiRequest;
  readonly ticket: string;
  readonly mode: AuthAttemptMode;
  readonly nowMs?: number;
  readonly production?: boolean;
  readonly secret?: string;
}): AuthAttemptPayload | null {
  const nowMs = input.nowMs ?? Date.now();
  const parsed = verifySigned<AuthAttemptPayload>(input.ticket, input.secret);
  const fence = currentAuthFence(input.req, input.production);
  if (
    !parsed
    || parsed.schema_version !== AUTH_FENCE_PROTOCOL
    || parsed.kind !== 'attempt'
    || parsed.mode !== input.mode
    || !fence
    || parsed.fence !== fence
    || !/^[A-Za-z0-9_-]{32}$/u.test(parsed.nonce)
    || !Number.isSafeInteger(parsed.issued_at_ms)
    || !Number.isSafeInteger(parsed.expires_at_ms)
    || parsed.expires_at_ms - parsed.issued_at_ms !== ATTEMPT_TTL_MS
    || parsed.issued_at_ms > nowMs + 5_000
    || parsed.expires_at_ms <= nowMs
  ) return null;
  return parsed;
}

export function jwtSessionClaims(token: string, nowMs = Date.now()): VerifiedJwtSessionClaims | null {
  const encoded = token.split('.')[1];
  if (!encoded) return null;
  try {
    const row = JSON.parse(Buffer.from(encoded, 'base64url').toString('utf8')) as Record<string, unknown>;
    if (
      typeof row.sub !== 'string' || row.sub.length < 1 || row.sub.length > 512
      || typeof row.session_id !== 'string' || row.session_id.length < 1 || row.session_id.length > 512
      || typeof row.iat !== 'number' || !Number.isFinite(row.iat)
      || typeof row.exp !== 'number' || !Number.isFinite(row.exp)
    ) return null;
    const claims = {
      subject: row.sub,
      providerSessionId: row.session_id,
      issuedAtMs: Math.floor(row.iat * 1_000),
      expiresAtMs: Math.floor(row.exp * 1_000),
      authenticationMethods: Array.isArray(row.amr)
        ? row.amr.flatMap((entry) => {
            if (!entry || typeof entry !== 'object' || Array.isArray(entry)) return [];
            const method = (entry as Record<string, unknown>).method;
            const timestamp = (entry as Record<string, unknown>).timestamp;
            return typeof method === 'string'
              && method.length > 0
              && method.length <= 128
              && typeof timestamp === 'number'
              && Number.isFinite(timestamp)
              ? [{ method, timestampMs: Math.floor(timestamp * 1_000) }]
              : [];
          })
        : [],
    };
    return claims.expiresAtMs > nowMs ? claims : null;
  } catch {
    return null;
  }
}

export function hasFreshInteractiveAuthenticationSince(
  claims: VerifiedJwtSessionClaims,
  sinceMs: number,
  nowMs = Date.now(),
): boolean {
  const firstUnambiguousJwtSecond = Math.floor(sinceMs / 1_000) * 1_000 + 1_000;
  const currentServerSecond = Math.floor(nowMs / 1_000) * 1_000;
  if (claims.issuedAtMs < firstUnambiguousJwtSecond || claims.issuedAtMs > currentServerSecond) return false;
  return claims.authenticationMethods.some(method => (
    INTERACTIVE_AUTHENTICATION_METHODS.has(method.method)
    && method.timestampMs >= firstUnambiguousJwtSecond
    && method.timestampMs <= claims.issuedAtMs
    && method.timestampMs <= currentServerSecond
  ));
}

export function issueAuthSessionBinding(input: {
  readonly req: NextApiRequest;
  readonly claims: VerifiedJwtSessionClaims;
  readonly nowMs?: number;
  readonly production?: boolean;
  readonly secret?: string;
}): string | null {
  const nowMs = input.nowMs ?? Date.now();
  const fence = currentAuthFence(input.req, input.production);
  if (!fence) return null;
  return sign({
    schema_version: AUTH_FENCE_PROTOCOL,
    kind: 'session',
    fence,
    subject_digest: digest(`${AUTH_FENCE_PROTOCOL}\u0000subject\u0000${input.claims.subject}`),
    provider_session_digest: digest(`${AUTH_FENCE_PROTOCOL}\u0000provider-session\u0000${input.claims.providerSessionId}`),
    issued_at_ms: nowMs,
    expires_at_ms: nowMs + SESSION_BINDING_TTL_MS,
  }, input.secret);
}

function readCurrentSessionBinding(
  req: NextApiRequest,
  nowMs = Date.now(),
  production = process.env.NODE_ENV === 'production',
  explicitSecret?: string,
): AuthSessionBindingPayload | null {
  const names = cookieNames(production);
  const raw = exactCookie(req, names.session);
  const fence = currentAuthFence(req, production);
  if (!raw || !fence) return null;
  const parsed = verifySigned<AuthSessionBindingPayload>(raw, explicitSecret);
  if (
    !parsed
    || parsed.schema_version !== AUTH_FENCE_PROTOCOL
    || parsed.kind !== 'session'
    || parsed.fence !== fence
    || !/^[0-9a-f]{64}$/u.test(parsed.subject_digest)
    || !/^[0-9a-f]{64}$/u.test(parsed.provider_session_digest)
    || !Number.isSafeInteger(parsed.issued_at_ms)
    || !Number.isSafeInteger(parsed.expires_at_ms)
    || parsed.expires_at_ms - parsed.issued_at_ms !== SESSION_BINDING_TTL_MS
    || parsed.issued_at_ms > nowMs + 5_000
    || parsed.expires_at_ms <= nowMs
  ) return null;
  return parsed;
}

export function authSessionBindingValid(input: {
  readonly req: NextApiRequest;
  readonly claims: VerifiedJwtSessionClaims;
  readonly userId: string;
  readonly nowMs?: number;
  readonly production?: boolean;
  readonly secret?: string;
}): boolean {
  const parsed = readCurrentSessionBinding(input.req, input.nowMs, input.production, input.secret);
  return Boolean(parsed
    && input.claims.subject === input.userId
    && parsed.subject_digest === digest(`${AUTH_FENCE_PROTOCOL}\u0000subject\u0000${input.userId}`)
    && parsed.provider_session_digest === digest(`${AUTH_FENCE_PROTOCOL}\u0000provider-session\u0000${input.claims.providerSessionId}`));
}

export function authFenceCookieNames(production: boolean): { readonly fence: string; readonly session: string } {
  return cookieNames(production);
}

export function freshLogoutFence(): string {
  return randomUUID();
}
