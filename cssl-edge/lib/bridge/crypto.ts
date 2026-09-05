import { createCipheriv, createDecipheriv, createHash, createHmac, hkdfSync, randomBytes, randomUUID, timingSafeEqual } from 'node:crypto';
import { ACCOUNT_UUID, CONVERSATION_UUID, validAccountTarget } from '../mobile/account-grant';

export const BRIDGE_REQUEST_LIMIT = 262_144;
export const BRIDGE_RESPONSE_LIMIT = 2 * 1024 * 1024;
export const BRIDGE_JOB_TTL_MS = 600_000;
export const BRIDGE_LEASE_MS = 420_000;
export const BRIDGE_ENVELOPE_SCHEMA = 'apocky.bridge.envelope.v1';
export const BRIDGE_REQUEST_SCHEMA = 'apocky.bridge.request.v1';
export const BRIDGE_RESULT_SCHEMA = 'apocky.bridge.http-result.v1';
const ID = /^[A-Za-z0-9][A-Za-z0-9_.-]{0,63}$/;
const JOB_ID = /^[0-9a-f]{8}-[0-9a-f]{4}-5[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
export const BRIDGE_RESULT_HEADERS = new Set(['content-type', 'content-encoding', 'retry-after',
  'x-apocv4-history-codec', 'x-apocv4-session-binding', 'x-apocv4-auth-mode',
  'x-apocv4-auth-registry-ref', 'x-apocv4-binding-ref', 'x-apocv4-principal-ref',
  'x-apocv4-privacy-partition-ref', 'x-apocv4-effect-scope-ref', 'x-apocv4-rollback-lease-ref']);

export class BridgeError extends Error {
  constructor(readonly code: string, readonly status = 503) { super(code); this.name = 'BridgeError'; }
}
export interface BridgeConfiguration { readonly key: Buffer; readonly keyId: string; readonly workerId: string; readonly ownerSubject: string; }
export function bridgeConfiguration(env: NodeJS.ProcessEnv = process.env): BridgeConfiguration {
  if (typeof window !== 'undefined') throw new BridgeError('BRIDGE_CONFIGURATION_UNAVAILABLE');
  const keyId = env.APOCRYPHA_BRIDGE_KEY_ID ?? '';
  const workerId = env.APOCRYPHA_BRIDGE_WORKER_ID ?? '';
  const ownerSubject = env.APOCRYPHA_BRIDGE_OWNER_USER_ID ?? '';
  const raw = env.APOCRYPHA_BRIDGE_KEY_B64 ?? '';
  const key = Buffer.from(raw, 'base64');
  if (!ID.test(keyId) || !ID.test(workerId) || !ACCOUNT_UUID.test(ownerSubject)
    || key.length !== 32 || key.toString('base64') !== raw) throw new BridgeError('BRIDGE_CONFIGURATION_UNAVAILABLE');
  return { key, keyId, workerId, ownerSubject };
}
export function bridgeConfigured(): boolean { try { bridgeConfiguration().key.fill(0); return true; } catch { return false; } }
export function derivedKey(config: BridgeConfiguration, label: string): Buffer {
  return Buffer.from(hkdfSync('sha256', config.key, Buffer.from('apocky.bridge.v1'), Buffer.from(label), 32));
}
export function sha256(value: Uint8Array): string { return createHash('sha256').update(value).digest('hex'); }
export function bridgeMac(config: BridgeConfiguration, label: string, value: string): string {
  const key = derivedKey(config, label);
  try { return createHmac('sha256', key).update(value, 'utf8').digest('hex'); } finally { key.fill(0); }
}
export function equalMac(a: string, b: string): boolean {
  return /^[a-f0-9]{64}$/.test(a) && /^[a-f0-9]{64}$/.test(b) && timingSafeEqual(Buffer.from(a, 'hex'), Buffer.from(b, 'hex'));
}
export function keyedUuid(config: BridgeConfiguration, label: string, value: string): string {
  const bytes = Buffer.from(bridgeMac(config, label, value).slice(0, 32), 'hex');
  bytes[6] = (bytes[6]! & 15) | 80; bytes[8] = (bytes[8]! & 63) | 128;
  const hex = bytes.toString('hex');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}
export function bridgeSessionId(config: BridgeConfiguration, subject: string): string {
  return keyedUuid(config, 'session-id', `apocky.bridge.session.v1\n${subject}`);
}
export function exactObject(value: unknown, keys: readonly string[]): value is Record<string, unknown> {
  return Boolean(value && typeof value === 'object' && !Array.isArray(value)
    && Object.keys(value).sort().join(',') === [...keys].sort().join(','));
}
export function decodeBase64(value: unknown, limit: number): Buffer {
  if (typeof value !== 'string' || value.length > Math.ceil(limit / 3) * 4) throw new BridgeError('BRIDGE_PAYLOAD_INVALID', 400);
  const bytes = Buffer.from(value, 'base64');
  if (bytes.length > limit || bytes.toString('base64') !== value) throw new BridgeError('BRIDGE_PAYLOAD_INVALID', 400);
  return bytes;
}
export interface BridgeEnvelope {
  schema_version: typeof BRIDGE_ENVELOPE_SCHEMA; key_id: string; direction: 'request' | 'response';
  job_id: string; iv_b64: string; ciphertext_b64: string; tag_b64: string;
}
function aad(config: BridgeConfiguration, direction: string, jobId: string): Buffer {
  return Buffer.from(`${BRIDGE_ENVELOPE_SCHEMA}\n${config.keyId}\n${direction}\n${jobId}`, 'utf8');
}
export function encryptBridge(config: BridgeConfiguration, direction: 'request' | 'response', jobId: string, value: unknown, nonce?: Uint8Array): BridgeEnvelope {
  if (!JOB_ID.test(jobId)) throw new BridgeError('BRIDGE_PAYLOAD_INVALID', 400);
  const plaintext = Buffer.from(JSON.stringify(value), 'utf8');
  const max = direction === 'request' ? 360_000 : 3_000_000;
  if (plaintext.length > max) throw new BridgeError('BRIDGE_PAYLOAD_TOO_LARGE', 413);
  const iv = nonce ? Buffer.from(nonce) : randomBytes(12);
  if (iv.length !== 12) throw new BridgeError('BRIDGE_PAYLOAD_INVALID', 400);
  const key = derivedKey(config, direction);
  try {
    const cipher = createCipheriv('aes-256-gcm', key, iv);
    cipher.setAAD(aad(config, direction, jobId));
    const ciphertext = Buffer.concat([cipher.update(plaintext), cipher.final()]);
    return { schema_version: BRIDGE_ENVELOPE_SCHEMA, key_id: config.keyId, direction, job_id: jobId,
      iv_b64: iv.toString('base64'), ciphertext_b64: ciphertext.toString('base64'), tag_b64: cipher.getAuthTag().toString('base64') };
  } finally { key.fill(0); plaintext.fill(0); }
}
export function decryptBridge(config: BridgeConfiguration, direction: 'request' | 'response', jobId: string, value: unknown): unknown {
  if (!exactObject(value, ['schema_version', 'key_id', 'direction', 'job_id', 'iv_b64', 'ciphertext_b64', 'tag_b64'])
    || value.schema_version !== BRIDGE_ENVELOPE_SCHEMA || value.key_id !== config.keyId
    || value.direction !== direction || value.job_id !== jobId || !JOB_ID.test(jobId)) throw new BridgeError('BRIDGE_PAYLOAD_INVALID', 400);
  const iv = decodeBase64(value.iv_b64, 12); const tag = decodeBase64(value.tag_b64, 16);
  const bytes = decodeBase64(value.ciphertext_b64, direction === 'request' ? 360_000 : 3_000_000);
  if (iv.length !== 12 || tag.length !== 16) throw new BridgeError('BRIDGE_PAYLOAD_INVALID', 400);
  const key = derivedKey(config, direction);
  try {
    const cipher = createDecipheriv('aes-256-gcm', key, iv);
    cipher.setAAD(aad(config, direction, jobId)); cipher.setAuthTag(tag);
    const plaintext = Buffer.concat([cipher.update(bytes), cipher.final()]);
    try { return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(plaintext)); } finally { plaintext.fill(0); }
  } catch { throw new BridgeError('BRIDGE_PAYLOAD_INVALID', 400); } finally { key.fill(0); }
}
export interface BridgeInput { channel: 'owner' | 'account'; subject: string; method: 'GET' | 'POST'; target: string; body: Uint8Array; signal?: AbortSignal; }
export interface BridgeRequest {
  schema_version: typeof BRIDGE_REQUEST_SCHEMA; job_id: string; subject: string; channel: 'owner' | 'account';
  method: 'GET' | 'POST'; target: string; headers: Record<string, string>; body_base64: string; created_at: string; expires_at: string; nonce: string | null;
}
export function validBridgeCodePath(value: unknown): value is string {
  if (typeof value !== 'string' || value.length > 4096 || value !== value.trim()
    || !/^[A-Za-z0-9_.@+ -]+(?:\/[A-Za-z0-9_.@+ -]+)*$/.test(value)) return false;
  const parts = value.split('/');
  return parts[0]?.toLowerCase() !== '.git' && parts.every(part => part !== '.' && part !== '..'
    && !/[ .]$/.test(part) && !/^(?:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])$/i.test(part.split('.')[0]!));
}
export function validBridgeTarget(channel: string, method: string, target: string): boolean {
  if (typeof target !== 'string' || target.length > 1024) return false;
  if (channel === 'account') return validAccountTarget(method, target);
  if (channel !== 'owner') return false;
  if (method === 'POST') return ['/v1/chat', '/v1/code/operations', '/v1/code/operations/rollback'].includes(target);
  if (method !== 'GET') return false;
  if (target === '/health' || target === '/v1/auth/status' || target === '/v1/code/capabilities') return true;
  if (target.startsWith('/v1/code/operations?operation_id=')) return CONVERSATION_UUID.test(target.slice('/v1/code/operations?operation_id='.length));
  if (/^\/v1\/observe\/(status|events|trace|errors|metrics|shards)\?/.test(target)) {
    const query = new URLSearchParams(target.slice(target.indexOf('?') + 1));
    const keys = ['privacy_partition', 'trace_id', 'error_code', 'component', 'cursor', 'limit'];
    const canonical = new URLSearchParams();
    if (query.get('privacy_partition') !== 'owner:apocky') return false;
    for (const key of keys) {
      const values = query.getAll(key);
      if (values.length > 1) return false;
      const value = values[0];
      if (value === undefined) continue;
      if (key === 'limit' ? !/^(?:[1-9][0-9]?|100)$/.test(value)
        : key !== 'privacy_partition' && !/^[A-Za-z0-9][A-Za-z0-9:._/@+-]{0,191}$/.test(value)) return false;
      canonical.set(key, value);
    }
    return canonical.toString() === target.slice(target.indexOf('?') + 1);
  }
  const match = /^\/v1\/chat\/history\?privacy_partition=owner%3Aapocky(?:&conversation_id=([^&]+))?(?:&cursor=([^&]+))?&limit=([1-9]|[12][0-9]|3[0-2])$/.exec(target);
  const historyId = (value: string) => CONVERSATION_UUID.test(value) || JOB_ID.test(value);
  return Boolean(match && (!match[1] || historyId(match[1])) && (!match[2] || (match[1] && historyId(match[2]))));
}
function validateInput(config: BridgeConfiguration, input: BridgeInput): void {
  if (!ACCOUNT_UUID.test(input.subject) || (input.channel === 'owner' && input.subject !== config.ownerSubject)
    || !validBridgeTarget(input.channel, input.method, input.target) || input.body.byteLength > BRIDGE_REQUEST_LIMIT
    || (input.method === 'GET' && input.body.byteLength)) throw new BridgeError('BRIDGE_REQUEST_INVALID', 400);
  if (input.method === 'POST') {
    let body: Record<string, unknown>;
    try { body = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(input.body)) as Record<string, unknown>; }
    catch { throw new BridgeError('BRIDGE_REQUEST_INVALID', 400); }
    if (input.channel === 'owner' && input.target.startsWith('/v1/code/operations')) {
      const rollback = input.target.endsWith('/rollback');
      if (!exactObject(body, rollback ? ['operation_id'] : ['operation_id', 'objective', 'allowed_paths'])
        || typeof body.operation_id !== 'string' || !CONVERSATION_UUID.test(body.operation_id)) throw new BridgeError('BRIDGE_REQUEST_INVALID', 400);
      if (!rollback) {
        if (typeof body.objective !== 'string' || body.objective !== body.objective.trim() || [...body.objective].length < 1 || [...body.objective].length > 32768
          || !Array.isArray(body.allowed_paths) || body.allowed_paths.length < 1 || body.allowed_paths.length > 32
          || !body.allowed_paths.every(validBridgeCodePath)
          || [...new Set(body.allowed_paths)].sort().join('\0') !== body.allowed_paths.join('\0')) throw new BridgeError('BRIDGE_REQUEST_INVALID', 400);
      }
      return;
    }
    if (!body || typeof body !== 'object' || Array.isArray(body) || typeof body.request_id !== 'string' || !(CONVERSATION_UUID.test(body.request_id) || (input.channel === 'owner' && JOB_ID.test(body.request_id)))
      || (input.channel === 'account' && (!exactObject(body, ['text', 'session_id', 'request_id']) || typeof body.session_id !== 'string' || !CONVERSATION_UUID.test(body.session_id) || typeof body.text !== 'string'))
      || (input.channel === 'owner' && (!exactObject(body, ['message', 'conversation_id', 'request_id', 'privacy_partition']) || body.privacy_partition !== 'owner:apocky' || typeof body.conversation_id !== 'string' || !(CONVERSATION_UUID.test(body.conversation_id) || JOB_ID.test(body.conversation_id)) || typeof body.message !== 'string'))) throw new BridgeError('BRIDGE_REQUEST_INVALID', 400);
    const text = input.channel === 'owner' ? body.message : body.text;
    if (typeof text !== 'string' || text !== text.trim() || !text || Buffer.byteLength(text, 'utf8') > 16384) throw new BridgeError('BRIDGE_REQUEST_INVALID', 400);
  }
}
export function bridgeJobId(config: BridgeConfiguration, input: Omit<BridgeInput, 'signal'>, nonce: string | null): string {
  return keyedUuid(config, 'job-id', ['apocky.bridge.job.v1', input.subject, input.channel, input.method, input.target, sha256(input.body), nonce ?? ''].join('\n'));
}
export function createBridgeRequest(config: BridgeConfiguration, input: BridgeInput, now = Date.now(), nonce: string | null = input.method === 'GET' ? randomUUID() : null): BridgeRequest {
  validateInput(config, input);
  if ((input.method === 'GET' && (typeof nonce !== 'string' || !CONVERSATION_UUID.test(nonce))) || (input.method === 'POST' && nonce !== null)) throw new BridgeError('BRIDGE_REQUEST_INVALID', 400);
  return { schema_version: BRIDGE_REQUEST_SCHEMA, job_id: bridgeJobId(config, input, nonce), subject: input.subject,
    channel: input.channel, method: input.method, target: input.target, headers: {}, body_base64: Buffer.from(input.body).toString('base64'),
    created_at: new Date(now).toISOString(), expires_at: new Date(now + BRIDGE_JOB_TTL_MS).toISOString(), nonce };
}
export function validateBridgeRequest(config: BridgeConfiguration, value: unknown): BridgeRequest {
  if (!exactObject(value, ['schema_version', 'job_id', 'subject', 'channel', 'method', 'target', 'headers', 'body_base64', 'created_at', 'expires_at', 'nonce'])
    || value.schema_version !== BRIDGE_REQUEST_SCHEMA || typeof value.subject !== 'string' || typeof value.target !== 'string'
    || (value.channel !== 'owner' && value.channel !== 'account') || (value.method !== 'GET' && value.method !== 'POST')
    || typeof value.created_at !== 'string' || typeof value.expires_at !== 'string') throw new BridgeError('BRIDGE_REQUEST_INVALID', 400);
  if (!value.headers || typeof value.headers !== 'object' || Array.isArray(value.headers)) throw new BridgeError('BRIDGE_REQUEST_INVALID', 400);
  for (const [key, header] of Object.entries(value.headers)) {
    if (key === 'content-type' ? header !== 'application/json' : key !== 'accept' || !['application/json', 'application/vnd.apocv4.chat-history-proof-bundle.v2+json'].includes(header as string)) throw new BridgeError('BRIDGE_REQUEST_INVALID', 400);
  }
  const created = Date.parse(value.created_at); const expires = Date.parse(value.expires_at);
  if (!Number.isFinite(created) || expires - created !== BRIDGE_JOB_TTL_MS) throw new BridgeError('BRIDGE_REQUEST_INVALID', 400);
  const recreated = createBridgeRequest(config, { channel: value.channel, subject: value.subject, method: value.method, target: value.target, body: decodeBase64(value.body_base64, BRIDGE_REQUEST_LIMIT) }, created, value.nonce as string | null);
  if (recreated.job_id !== value.job_id || recreated.created_at !== value.created_at || recreated.expires_at !== value.expires_at) throw new BridgeError('BRIDGE_REQUEST_INVALID', 400);
  return value as unknown as BridgeRequest;
}
export interface BridgeHttpResult { schema_version: typeof BRIDGE_RESULT_SCHEMA; job_id: string; status: number; headers: Record<string, string>; body_base64: string; completed_at: string; }
export function validateBridgeResult(jobId: string, value: unknown): BridgeHttpResult {
  if (!exactObject(value, ['schema_version', 'job_id', 'status', 'headers', 'body_base64', 'completed_at'])
    || value.schema_version !== BRIDGE_RESULT_SCHEMA || value.job_id !== jobId || typeof value.status !== 'number'
    || !Number.isInteger(value.status) || value.status < 200 || value.status > 599 || !value.headers || typeof value.headers !== 'object' || Array.isArray(value.headers)
    || typeof value.completed_at !== 'string' || !Number.isFinite(Date.parse(value.completed_at))) throw new BridgeError('BRIDGE_RESULT_INVALID', 400);
  const body = decodeBase64(value.body_base64, BRIDGE_RESPONSE_LIMIT);
  if ([204, 205, 304].includes(value.status) && body.length) throw new BridgeError('BRIDGE_RESULT_INVALID', 400);
  for (const [key, header] of Object.entries(value.headers)) {
    if (!BRIDGE_RESULT_HEADERS.has(key) || typeof header !== 'string' || !/^[\x20-\x7e]{1,256}$/.test(header)) throw new BridgeError('BRIDGE_RESULT_INVALID', 400);
  }
  return value as unknown as BridgeHttpResult;
}
