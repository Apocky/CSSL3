import {
  createHash,
  createPrivateKey,
  createPublicKey,
  generateKeyPairSync,
  sign as cryptoSign,
  verify as cryptoVerify,
  type KeyObject,
} from 'node:crypto';
import { chmod, mkdir, readFile, writeFile } from 'node:fs/promises';
import { homedir, platform as osPlatform } from 'node:os';
import { join } from 'node:path';

import {
  ContributorWorker,
  type ContributorTask,
  type ResultEnvelope,
  type ContributorNodePolicy,
} from './runtime.js';

export const CONTRIBUTOR_CLIENT_SCHEMA = 'apocrypha.contributor.client.v1' as const;
export const CONTRIBUTOR_LEASE_POLL_SCHEMA = 'apocrypha.contributor.lease-poll.v1' as const;
export const CONTRIBUTOR_ENROLLMENT_SCHEMA = 'apocrypha.contributor.enrollment.v1' as const;
export const CONTRIBUTOR_ENROLLMENT_RECEIPT_SCHEMA = 'apocrypha.contributor.enrollment-receipt.v1' as const;
export const CONTRIBUTOR_LEASE_DISPATCH_SCHEMA = 'apocrypha.contributor.lease-dispatch.v1' as const;
export const CONTRIBUTOR_RESULT_SUBMISSION_SCHEMA = 'apocrypha.contributor.result-submission.v1' as const;
export const CONTRIBUTOR_RESULT_RECEIPT_SCHEMA = 'apocrypha.contributor.result-receipt.v1' as const;

const IDENTIFIER = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/;
const KEY_ID = /^[A-Za-z0-9][A-Za-z0-9._:-]{2,127}$/;
const REQUEST_ID = /^[A-Za-z0-9][A-Za-z0-9._:-]{7,127}$/;
const IDEMPOTENCY = /^[A-Za-z0-9][A-Za-z0-9._:-]{7,199}$/;
const SIGNATURE = /^[A-Za-z0-9_-]{86}$/;
const BASE64 = /^[A-Za-z0-9+/]+={0,2}$/;

export type ContributorClientPlatform = 'windows-x64' | 'macos-arm64' | 'linux-x64' | 'android' | 'ios';

export interface ContributorNodeIdentity {
  readonly schema_version: typeof CONTRIBUTOR_CLIENT_SCHEMA;
  readonly node_id: string;
  readonly node_key_id: string;
  readonly private_key_pem: string;
  readonly public_key_spki_b64: string;
}

export interface ContributorNetworkClientOptions {
  readonly baseUrl: string;
  readonly controllerKeyId: string;
  readonly controllerPublicKey: string | KeyObject;
  readonly identity: ContributorNodeIdentity;
  readonly platform: ContributorClientPlatform;
  readonly dataDir: string;
  readonly policy?: Partial<ContributorNodePolicy>;
  readonly fetchImpl?: typeof fetch;
  readonly now?: () => number;
}

export interface ContributorRunReceipt {
  readonly node_id: string;
  readonly dispatch_id: string;
  readonly lease_id: string;
  readonly output: number | null;
  readonly result_status: 'accepted';
}

export class ContributorClientError extends Error {
  readonly code: string;
  readonly status: number | null;

  constructor(code: string, message = code, status: number | null = null) {
    super(message);
    this.name = 'ContributorClientError';
    this.code = code;
    this.status = status;
  }
}

function record(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function canonicalize(value: unknown): unknown {
  if (value === null || typeof value === 'string' || typeof value === 'boolean') return value;
  if (typeof value === 'number') {
    if (!Number.isFinite(value)) throw new ContributorClientError('CLIENT_SCHEMA_INVALID');
    return Object.is(value, -0) ? 0 : value;
  }
  if (Array.isArray(value)) return value.map(canonicalize);
  if (typeof value === 'object') {
    const source = value as Record<string, unknown>;
    return Object.fromEntries(Object.keys(source).sort().map((key) => [key, canonicalize(source[key])]));
  }
  throw new ContributorClientError('CLIENT_SCHEMA_INVALID');
}

function canonicalJson(value: unknown): string {
  return JSON.stringify(canonicalize(value));
}

function signature(payload: unknown, privateKey: KeyObject): string {
  return cryptoSign(null, Buffer.from(canonicalJson(payload), 'utf8'), privateKey).toString('base64url');
}

function validSignature(value: unknown): value is string {
  return typeof value === 'string' && SIGNATURE.test(value);
}

function verify(payload: unknown, value: unknown, publicKey: KeyObject): boolean {
  return validSignature(value)
    && cryptoVerify(null, Buffer.from(canonicalJson(payload), 'utf8'), publicKey, Buffer.from(value, 'base64url'));
}

function spkiB64(key: KeyObject): string {
  return Buffer.from(key.export({ type: 'spki', format: 'der' })).toString('base64');
}

function privateKey(value: string): KeyObject {
  try {
    const key = createPrivateKey(value);
    if (key.type !== 'private' || key.asymmetricKeyType !== 'ed25519') throw new Error('key');
    return key;
  } catch {
    throw new ContributorClientError('NODE_SIGNING_KEY_INVALID');
  }
}

function publicKey(value: string | KeyObject): KeyObject {
  try {
    const key = typeof value === 'string' ? createPublicKey(value) : value;
    if (key.type !== 'public' || key.asymmetricKeyType !== 'ed25519') throw new Error('key');
    return key;
  } catch {
    throw new ContributorClientError('CONTROLLER_KEY_INVALID');
  }
}

function boundedIdentity(value: unknown): ContributorNodeIdentity {
  const source = record(value);
  if (!source
    || source.schema_version !== CONTRIBUTOR_CLIENT_SCHEMA
    || typeof source.node_id !== 'string' || !IDENTIFIER.test(source.node_id)
    || typeof source.node_key_id !== 'string' || !KEY_ID.test(source.node_key_id)
    || typeof source.private_key_pem !== 'string' || source.private_key_pem.length > 16_384
    || typeof source.public_key_spki_b64 !== 'string' || !BASE64.test(source.public_key_spki_b64)
    || source.public_key_spki_b64.length > 512) {
    throw new ContributorClientError('CLIENT_IDENTITY_INVALID');
  }
  const privateKeyObject = privateKey(source.private_key_pem);
  if (spkiB64(createPublicKey(privateKeyObject)) !== source.public_key_spki_b64) {
    throw new ContributorClientError('NODE_SIGNING_KEY_INVALID');
  }
  return {
    schema_version: CONTRIBUTOR_CLIENT_SCHEMA,
    node_id: source.node_id,
    node_key_id: source.node_key_id,
    private_key_pem: source.private_key_pem,
    public_key_spki_b64: source.public_key_spki_b64,
  };
}

function defaultDataDir(): string {
  if (process.env.APOCRYPHA_NODE_DATA_DIR) return process.env.APOCRYPHA_NODE_DATA_DIR;
  if (osPlatform() === 'win32' && process.env.APPDATA) return join(process.env.APPDATA, 'Apocrypha', 'contributor');
  if (osPlatform() === 'darwin') return join(homedir(), 'Library', 'Application Support', 'Apocrypha', 'contributor');
  return join(process.env.XDG_STATE_HOME ?? join(homedir(), '.local', 'state'), 'apocrypha', 'contributor');
}

export function identityPath(dataDir = defaultDataDir()): string {
  return join(dataDir, 'identity.json');
}

export async function loadOrCreateIdentity(
  dataDir = defaultDataDir(),
  nodeId = `node-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 12)}`,
  nodeKeyId = 'node-local-v1',
): Promise<ContributorNodeIdentity> {
  await mkdir(dataDir, { recursive: true, mode: 0o700 });
  const path = identityPath(dataDir);
  try {
    const existing = boundedIdentity(JSON.parse(await readFile(path, 'utf8')) as unknown);
    await chmod(path, 0o600);
    return existing;
  } catch (error) {
    if (error instanceof ContributorClientError) throw error;
    // ENOENT is the only creation path; an unreadable or malformed existing
    // identity is never silently replaced.
    const code = error && typeof error === 'object' && 'code' in error
      ? String((error as { code?: unknown }).code ?? '')
      : '';
    if (code !== 'ENOENT') throw new ContributorClientError('CLIENT_IDENTITY_UNREADABLE');
  }
  if (!IDENTIFIER.test(nodeId) || !KEY_ID.test(nodeKeyId)) throw new ContributorClientError('CLIENT_IDENTITY_INVALID');
  const pair = generateKeyPairSync('ed25519');
  const identity: ContributorNodeIdentity = {
    schema_version: CONTRIBUTOR_CLIENT_SCHEMA,
    node_id: nodeId,
    node_key_id: nodeKeyId,
    private_key_pem: pair.privateKey.export({ type: 'pkcs8', format: 'pem' }).toString(),
    public_key_spki_b64: spkiB64(pair.publicKey),
  };
  try {
    await writeFile(path, `${JSON.stringify(identity)}\n`, { encoding: 'utf8', mode: 0o600, flag: 'wx' });
  } catch (error) {
    const code = error && typeof error === 'object' && 'code' in error
      ? String((error as { code?: unknown }).code ?? '')
      : '';
    if (code !== 'EEXIST') throw new ContributorClientError('CLIENT_IDENTITY_WRITE_FAILED');
    return boundedIdentity(JSON.parse(await readFile(path, 'utf8')) as unknown);
  }
  await chmod(path, 0o600);
  return identity;
}

function requestId(prefix: string, now: number): string {
  const value = `${prefix}-${now}-${Math.random().toString(36).slice(2, 12)}`;
  if (!REQUEST_ID.test(value)) throw new ContributorClientError('CLIENT_REQUEST_INVALID');
  return value;
}

function idempotencyKey(now: number): string {
  const value = `poll-idempotency-${now}-${Math.random().toString(36).slice(2, 12)}`;
  if (!IDEMPOTENCY.test(value)) throw new ContributorClientError('CLIENT_REQUEST_INVALID');
  return value;
}

function enrollmentPayload(identity: ContributorNodeIdentity, platform: ContributorClientPlatform, now: number) {
  return {
    schema_version: CONTRIBUTOR_ENROLLMENT_SCHEMA,
    request_id: requestId('enroll', now),
    node_id: identity.node_id,
    node_key_id: identity.node_key_id,
    node_public_key_spki_b64: identity.public_key_spki_b64,
    platform,
    capabilities: ['vector_dot'] as const,
    consent_revision: 'public-node-v1',
    issued_at: now - 1_000,
    expires_at: now + 120_000,
  };
}

function pollPayload(identity: ContributorNodeIdentity, task: ContributorTask, now: number) {
  return {
    schema_version: CONTRIBUTOR_LEASE_POLL_SCHEMA,
    request_id: requestId('poll', now),
    idempotency_key: idempotencyKey(now),
    node_id: identity.node_id,
    node_key_id: identity.node_key_id,
    issued_at: now - 1_000,
    expires_at: now + 30_000,
    attempt: 0,
    task,
  };
}

function enrollmentReceiptPayload(value: Record<string, unknown>) {
  const { signature_b64: _signature, ...payload } = value;
  return payload;
}

function dispatchPayload(value: Record<string, unknown>) {
  const { signature_b64: _signature, ...payload } = value;
  return payload;
}

function receiptPayload(value: Record<string, unknown>) {
  const { signature_b64: _signature, ...payload } = value;
  return payload;
}

function sha256Json(value: unknown): string {
  return createHash('sha256').update(canonicalJson(value), 'utf8').digest('hex');
}

function normalizeBaseUrl(value: string): string {
  try {
    const url = new URL(value);
    if (url.protocol !== 'https:' && url.hostname !== 'localhost' && url.hostname !== '127.0.0.1') {
      throw new Error('protocol');
    }
    return url.toString().replace(/\/$/, '');
  } catch {
    throw new ContributorClientError('CLIENT_BASE_URL_INVALID');
  }
}

export class ContributorNetworkClient {
  readonly identity: ContributorNodeIdentity;
  readonly platform: ContributorClientPlatform;
  readonly dataDir: string;
  private readonly baseUrl: string;
  private readonly controllerKeyId: string;
  private readonly controllerPublicKey: KeyObject;
  private readonly nodePrivateKey: KeyObject;
  private readonly fetchImpl: typeof fetch;
  private readonly now: () => number;
  private readonly worker: ContributorWorker;

  constructor(options: ContributorNetworkClientOptions) {
    this.identity = boundedIdentity(options.identity);
    this.platform = options.platform;
    this.dataDir = options.dataDir;
    this.baseUrl = normalizeBaseUrl(options.baseUrl);
    if (!KEY_ID.test(options.controllerKeyId)) throw new ContributorClientError('CONTROLLER_KEY_ID_INVALID');
    this.controllerKeyId = options.controllerKeyId;
    this.controllerPublicKey = publicKey(options.controllerPublicKey);
    this.nodePrivateKey = privateKey(this.identity.private_key_pem);
    this.fetchImpl = options.fetchImpl ?? fetch;
    this.now = options.now ?? (() => Date.now());
    this.worker = new ContributorWorker({
      nodeId: this.identity.node_id,
      controllerKeyId: this.controllerKeyId,
      controllerPublicKey: this.controllerPublicKey,
      nodeKeyId: this.identity.node_key_id,
      nodeSigningKey: this.nodePrivateKey,
      policy: options.policy,
      clock: this.now,
    });
  }

  private async post<T>(path: string, payload: unknown): Promise<T> {
    let response: Response;
    try {
      response = await this.fetchImpl(`${this.baseUrl}${path}`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(payload),
      });
    } catch {
      throw new ContributorClientError('CLIENT_NETWORK_UNAVAILABLE');
    }
    let body: unknown;
    try { body = await response.json(); } catch { throw new ContributorClientError('CLIENT_RESPONSE_INVALID', 'invalid server response', response.status); }
    const source = record(body);
    if (!response.ok || source?.ok !== true) {
      const code = typeof source?.transport_code === 'string'
        ? source.transport_code
        : typeof source?.code === 'string' ? source.code : 'CLIENT_REMOTE_REJECTED';
      throw new ContributorClientError(code, typeof source?.error === 'string' ? source.error : code, response.status);
    }
    if (!('result' in source)) throw new ContributorClientError('CLIENT_RESPONSE_INVALID', 'missing result', response.status);
    return source.result as T;
  }

  async enroll(): Promise<void> {
    const now = this.now();
    const payload = enrollmentPayload(this.identity, this.platform, now);
    const request = { ...payload, signature_b64: signature(payload, this.nodePrivateKey) };
    const receipt = record(await this.post<unknown>('/api/apocrypha/contributor/enroll', request));
    if (!receipt
      || receipt.schema_version !== CONTRIBUTOR_ENROLLMENT_RECEIPT_SCHEMA
      || receipt.node_id !== this.identity.node_id
      || receipt.node_key_id !== this.identity.node_key_id
      || !verify(enrollmentReceiptPayload(receipt), receipt.signature_b64, this.controllerPublicKey)) {
      throw new ContributorClientError('CLIENT_ENROLLMENT_RECEIPT_INVALID');
    }
  }

  async poll(task: ContributorTask): Promise<Record<string, unknown>> {
    const now = this.now();
    const payload = pollPayload(this.identity, task, now);
    const request = { ...payload, signature_b64: signature(payload, this.nodePrivateKey) };
    const dispatch = record(await this.post<unknown>('/api/apocrypha/contributor/poll', request));
    if (!dispatch
      || dispatch.schema_version !== CONTRIBUTOR_LEASE_DISPATCH_SCHEMA
      || dispatch.node_id !== this.identity.node_id
      || !record(dispatch.lease)
      || !verify(dispatchPayload(dispatch), dispatch.signature_b64, this.controllerPublicKey)) {
      throw new ContributorClientError('CLIENT_LEASE_DISPATCH_INVALID');
    }
    return dispatch;
  }

  async runOnce(task: ContributorTask): Promise<ContributorRunReceipt> {
    await this.enroll();
    const dispatch = await this.poll(task);
    this.worker.optIn();
    const result: ResultEnvelope = await this.worker.run(dispatch.lease, this.now());
    const submissionPayload = {
      schema_version: CONTRIBUTOR_RESULT_SUBMISSION_SCHEMA,
      dispatch_id: dispatch.dispatch_id,
      request_id: dispatch.request_id,
      idempotency_key: dispatch.idempotency_key,
      node_id: this.identity.node_id,
      result,
    };
    const submission = { ...submissionPayload, signature_b64: signature(submissionPayload, this.nodePrivateKey) };
    const receipt = record(await this.post<unknown>('/api/apocrypha/contributor/result', submission));
    if (!receipt
      || receipt.schema_version !== CONTRIBUTOR_RESULT_RECEIPT_SCHEMA
      || receipt.dispatch_id !== dispatch.dispatch_id
      || !verify(receiptPayload(receipt), receipt.signature_b64, this.controllerPublicKey)) {
      throw new ContributorClientError('CLIENT_RESULT_RECEIPT_INVALID');
    }
    return {
      node_id: this.identity.node_id,
      dispatch_id: dispatch.dispatch_id as string,
      lease_id: (dispatch.lease as Record<string, unknown>).lease_id as string,
      output: result.output,
      result_status: 'accepted',
    };
  }
}

export function publicKeyPemFromSpkiB64(value: string): string {
  if (!BASE64.test(value)) throw new ContributorClientError('CONTROLLER_KEY_INVALID');
  const der = Buffer.from(value, 'base64');
  return createPublicKey({ key: der, format: 'der', type: 'spki' }).export({ type: 'spki', format: 'pem' }).toString();
}
