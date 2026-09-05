import { accountReference, accountSigningKey, signAccountRequest, validAccountTarget } from './account-grant';
import { fetchBridge } from '../bridge/queue';
import { bridgeConfigured } from '../bridge/crypto';

const ACCOUNT_RUNTIME_ORIGIN = 'https://apocrypha.apocky.com';
const ACCESS_HEADER_VALUE_RE = /^[\x21-\x7e]{1,4096}$/;

export class AccountRuntimeError extends Error {
  constructor(readonly code: string, readonly publicStatus: 404 | 502 | 503 | 504 = 502) { super(code); }
}

export function accountRuntimeOrigin(): string {
  if (typeof window !== 'undefined') throw new AccountRuntimeError('ACCOUNT_CONFIGURATION_INVALID', 503);
  const raw = process.env.APOCV4_ACCOUNT_RUNTIME_URL;
  if (!raw) throw new AccountRuntimeError('ACCOUNT_SERVICE_UNAVAILABLE', 503);
  if (raw === ACCOUNT_RUNTIME_ORIGIN) return raw;
  if (process.env.NODE_ENV !== 'production' && /^http:\/\/127\.0\.0\.1:[1-9][0-9]{3,4}$/.test(raw)) {
    const port = Number(new URL(raw).port);
    if (port <= 65535) return raw;
  }
  throw new AccountRuntimeError('ACCOUNT_CONFIGURATION_INVALID', 503);
}

function accountAccessHeaders(origin: string): Record<string, string> {
  if (origin !== ACCOUNT_RUNTIME_ORIGIN) return {};
  const clientId = process.env.APOCV4_ACCOUNT_CF_ACCESS_CLIENT_ID;
  const clientSecret = process.env.APOCV4_ACCOUNT_CF_ACCESS_CLIENT_SECRET;
  if (!clientId || !clientSecret || !ACCESS_HEADER_VALUE_RE.test(clientId) || !ACCESS_HEADER_VALUE_RE.test(clientSecret)) {
    throw new AccountRuntimeError('ACCOUNT_SERVICE_UNAVAILABLE', 503);
  }
  return { 'CF-Access-Client-Id': clientId, 'CF-Access-Client-Secret': clientSecret };
}

export function accountRuntimeConfigured(): boolean {
  if (process.env.APOCV4_ACCOUNT_RUNTIME_TRANSPORT === 'outbound-bridge') {
    return bridgeConfigured();
  }
  try { accountAccessHeaders(accountRuntimeOrigin()); accountSigningKey().key.fill(0); return true; } catch { return false; }
}

async function boundedJson(response: Response, allowNotFound = false): Promise<Record<string, unknown>> {
  if ((!response.ok && !(allowNotFound && response.status === 404)) || response.redirected
    || response.headers.get('content-type')?.split(';', 1)[0]?.trim() !== 'application/json') {
    throw new AccountRuntimeError('ACCOUNT_UPSTREAM_UNVERIFIED');
  }
  const reader = response.body?.getReader();
  if (!reader) throw new AccountRuntimeError('ACCOUNT_RESPONSE_EMPTY');
  const chunks: Uint8Array[] = [];
  let bytes = 0;
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      bytes += next.value.byteLength;
      if (bytes > 262_144) throw new AccountRuntimeError('ACCOUNT_RESPONSE_TOO_LARGE');
      chunks.push(next.value);
    }
  } finally { await reader.cancel().catch(() => undefined); reader.releaseLock(); }
  let value: unknown;
  try { value = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(Buffer.concat(chunks))); }
  catch { throw new AccountRuntimeError('ACCOUNT_RESPONSE_INVALID'); }
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new AccountRuntimeError('ACCOUNT_RESPONSE_INVALID');
  return value as Record<string, unknown>;
}

export async function callAccountRuntime(input: {
  subject: string; method: 'GET' | 'POST'; target: string; body?: Record<string, string>;
}): Promise<Record<string, unknown>> {
  if (!validAccountTarget(input.method, input.target)) throw new AccountRuntimeError('ACCOUNT_REQUEST_INVALID');
  const bridged = process.env.APOCV4_ACCOUNT_RUNTIME_TRANSPORT === 'outbound-bridge';
  const origin = bridged ? null : accountRuntimeOrigin();
  const accessHeaders = origin === null ? {} : accountAccessHeaders(origin);
  const signing = bridged ? null : accountSigningKey();
  const body = Buffer.from(input.body ? JSON.stringify(input.body) : '', 'utf8');
  const grant = signing === null ? null : signAccountRequest({ ...input, body, ...signing });
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), input.method === 'POST' ? 115_000 : 15_000);
  try {
    const init: RequestInit = {
      method: input.method, headers: { ...accessHeaders, Authorization: `Bearer ${grant}`, Accept: 'application/json',
        ...(input.method === 'POST' ? { 'Content-Type': 'application/json' } : {}) },
      ...(input.method === 'POST' ? { body } : {}),
      signal: controller.signal, cache: 'no-store', redirect: 'manual',
    };
    const response = bridged
      ? await fetchBridge({ channel: 'account', subject: input.subject, method: input.method,
        target: input.target, body, signal: controller.signal })
      : await fetch(`${origin}${input.target}`, init);
    const allowNotFound = input.method === 'GET' && input.target.startsWith('/v1/account/sessions?session_id=');
    const result = await boundedJson(response, allowNotFound);
    if (result.account_ref !== accountReference(input.subject)) throw new AccountRuntimeError('ACCOUNT_RESPONSE_SCOPE_MISMATCH');
    if (response.status === 404) {
      if (allowNotFound && result.code === 'ACCOUNT_SESSION_NOT_FOUND') throw new AccountRuntimeError('ACCOUNT_SESSION_NOT_FOUND', 404);
      throw new AccountRuntimeError('ACCOUNT_UPSTREAM_UNVERIFIED');
    }
    return result;
  } catch (error) {
    if (error instanceof AccountRuntimeError) throw error;
    throw new AccountRuntimeError(controller.signal.aborted ? 'ACCOUNT_RESPONSE_TIMEOUT' : 'ACCOUNT_SERVICE_UNAVAILABLE',
      controller.signal.aborted ? 504 : 502);
  } finally { clearTimeout(timer); signing?.key.fill(0); }
}
