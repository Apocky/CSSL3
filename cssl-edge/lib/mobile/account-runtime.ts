import { accountReference, accountSigningKey, signAccountRequest, validAccountTarget } from './account-grant';
import { CLOUDFLARE_RUNTIME_ORIGIN, fetchCloudflareRuntime } from '../apocv4/cloudflare-runtime-transport';

export class AccountRuntimeError extends Error {
  constructor(readonly code: string, readonly publicStatus: 404 | 502 | 503 | 504 = 502) { super(code); }
}

export function accountRuntimeOrigin(): string {
  const raw = process.env.APOCV4_ACCOUNT_RUNTIME_URL;
  if (!raw) throw new AccountRuntimeError('ACCOUNT_SERVICE_UNAVAILABLE', 503);
  if (raw === CLOUDFLARE_RUNTIME_ORIGIN) return raw;
  if (process.env.NODE_ENV !== 'production' && /^http:\/\/127\.0\.0\.1:[1-9][0-9]{3,4}$/.test(raw)) {
    const port = Number(new URL(raw).port);
    if (port <= 65535) return raw;
  }
  throw new AccountRuntimeError('ACCOUNT_CONFIGURATION_INVALID', 503);
}

export function accountRuntimeConfigured(): boolean {
  try { accountRuntimeOrigin(); accountSigningKey(); return true; } catch { return false; }
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
  const origin = accountRuntimeOrigin();
  const signing = accountSigningKey();
  const body = Buffer.from(input.body ? JSON.stringify(input.body) : '', 'utf8');
  const grant = signAccountRequest({ ...input, body, ...signing });
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), input.method === 'POST' ? 115_000 : 15_000);
  try {
    const init: RequestInit = {
      method: input.method, headers: { Authorization: `Bearer ${grant}`, Accept: 'application/json',
        ...(input.method === 'POST' ? { 'Content-Type': 'application/json' } : {}) },
      ...(input.method === 'POST' ? { body } : {}),
      signal: controller.signal, cache: 'no-store', redirect: 'error',
    };
    const response = origin === CLOUDFLARE_RUNTIME_ORIGIN
      ? await fetchCloudflareRuntime(`${origin}${input.target}`, init)
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
  } finally { clearTimeout(timer); signing.key.fill(0); }
}
