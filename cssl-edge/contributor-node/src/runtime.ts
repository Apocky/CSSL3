import {
  createHash,
  createPublicKey,
  generateKeyPairSync,
  sign as cryptoSign,
  verify as cryptoVerify,
  KeyObject,
} from 'node:crypto';
import { performance } from 'node:perf_hooks';

/**
 * Local-only contributor runtime.
 *
 * This module has no network, filesystem, process-spawn, shell, git, chat, or
 * vault integration.  A controller may hand a signed lease to the process;
 * the process verifies and executes only the tiny deterministic task below.
 */

export const CONTRIBUTOR_NETWORK_ENABLED = false as const;
export const CONTRIBUTOR_NODE_SCHEMA = 'apocrypha.contributor.node.v1' as const;
export const CONTRIBUTOR_LEASE_SCHEMA = 'apocrypha.contributor.lease.v1' as const;
export const CONTRIBUTOR_RESULT_SCHEMA = 'apocrypha.contributor.result.v1' as const;
export const CONTRIBUTOR_UNINSTALL_SCHEMA = 'apocrypha.contributor.uninstall.v1' as const;

export interface ContributorNodePolicy {
  readonly maxLeaseBytes: number;
  readonly maxTaskBytes: number;
  readonly maxResultBytes: number;
  readonly maxElements: number;
  readonly maxOperations: number;
  readonly maxWallMs: number;
  readonly maxLeaseTtlMs: number;
  readonly clockSkewMs: number;
  readonly maxReplayEntries: number;
}

export const DEFAULT_CONTRIBUTOR_POLICY: ContributorNodePolicy = Object.freeze({
  maxLeaseBytes: 128 * 1024,
  maxTaskBytes: 64 * 1024,
  maxResultBytes: 64 * 1024,
  maxElements: 4_096,
  maxOperations: 2_000_000,
  maxWallMs: 2_000,
  maxLeaseTtlMs: 5 * 60 * 1_000,
  clockSkewMs: 30 * 1_000,
  maxReplayEntries: 512,
});
export type ContributorNodeMode = 'paused' | 'ready' | 'running' | 'revoked' | 'uninstalled';

export type ContributorNodeErrorCode =
  | 'CONTROLLER_KEY_REQUIRED'
  | 'CONTROLLER_KEY_INVALID'
  | 'CONTROLLER_KEY_ID_MISMATCH'
  | 'NODE_SIGNING_KEY_REQUIRED'
  | 'NODE_SIGNING_KEY_INVALID'
  | 'ED25519_UNAVAILABLE'
  | 'WORKER_NOT_OPTED_IN'
  | 'WORKER_PAUSED'
  | 'WORKER_REVOKED'
  | 'WORKER_UNINSTALLED'
  | 'CONCURRENCY_LIMIT'
  | 'LEASE_SCHEMA_INVALID'
  | 'LEASE_SIGNATURE_INVALID'
  | 'LEASE_REPLAY'
  | 'LEASE_REPLAY_WINDOW_FULL'
  | 'LEASE_NODE_MISMATCH'
  | 'LEASE_NOT_YET_VALID'
  | 'LEASE_EXPIRED'
  | 'LEASE_TTL_EXCEEDED'
  | 'LEASE_INPUT_LIMIT'
  | 'TASK_SCHEMA_INVALID'
  | 'TASK_INPUT_LIMIT'
  | 'TASK_OPERATION_LIMIT'
  | 'TASK_DEADLINE'
  | 'TASK_PAUSED'
  | 'TASK_REVOKED'
  | 'TASK_UNINSTALLED'
  | 'TASK_NUMERIC_OVERFLOW'
  | 'TASK_FAILED'
  | 'RESULT_SCHEMA_INVALID'
  | 'RESULT_SIGNATURE_INVALID'
  | 'RESULT_OUTPUT_LIMIT'
  | 'VALUE_NOT_FINITE';

export class ContributorNodeError extends Error {
  readonly code: ContributorNodeErrorCode;

  constructor(code: ContributorNodeErrorCode, message: string = code) {
    super(message);
    this.name = 'ContributorNodeError';
    this.code = code;
  }
}

export interface VectorDotTask {
  readonly kind: 'vector_dot';
  readonly left: readonly number[];
  readonly right: readonly number[];
}

export type ContributorTask = VectorDotTask;

export interface LeaseInput {
  readonly schema_version: typeof CONTRIBUTOR_LEASE_SCHEMA;
  readonly key_id: string;
  readonly lease_id: string;
  readonly node_id: string;
  readonly issued_at: number;
  readonly expires_at: number;
  readonly attempt: number;
  readonly task: ContributorTask;
}

export interface LeaseEnvelope extends LeaseInput {
  readonly signature_b64: string;
}

export interface ResultEnvelope {
  readonly schema_version: typeof CONTRIBUTOR_RESULT_SCHEMA;
  readonly key_id: string;
  readonly lease_id: string;
  readonly node_id: string;
  readonly attempt: number;
  readonly started_at: number;
  readonly finished_at: number;
  readonly ok: boolean;
  readonly output: number | null;
  readonly error_code: ContributorNodeErrorCode | null;
  readonly error_message: string | null;
  readonly signature_b64: string;
}

export interface ContributorWorkerStatus {
  readonly schema_version: typeof CONTRIBUTOR_NODE_SCHEMA;
  readonly node_id: string;
  readonly mode: ContributorNodeMode;
  readonly network_enabled: false;
  readonly active_lease_id: string | null;
  readonly replay_entries: number;
  readonly node_signing_key_present: boolean;
  readonly limits: ContributorNodePolicy;
}

export interface UninstallReceipt {
  readonly schema_version: typeof CONTRIBUTOR_UNINSTALL_SCHEMA;
  readonly node_id: string;
  readonly state_cleared: true;
  readonly replay_entries_cleared: true;
  readonly node_signing_key_cleared: true;
}

export type PublicKeyInput = KeyObject | string | Buffer;

const IDENTIFIER = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/;
const KEY_ID = /^[A-Za-z0-9][A-Za-z0-9._:-]{2,127}$/;
const ERROR_CODE = /^[A-Z0-9_]{3,64}$/;
const RESULT_ERROR_CODES = new Set<ContributorNodeErrorCode>([
  'CONTROLLER_KEY_REQUIRED', 'CONTROLLER_KEY_INVALID', 'CONTROLLER_KEY_ID_MISMATCH',
  'NODE_SIGNING_KEY_REQUIRED', 'NODE_SIGNING_KEY_INVALID', 'ED25519_UNAVAILABLE',
  'WORKER_NOT_OPTED_IN', 'WORKER_PAUSED', 'WORKER_REVOKED', 'WORKER_UNINSTALLED',
  'CONCURRENCY_LIMIT', 'LEASE_SCHEMA_INVALID', 'LEASE_SIGNATURE_INVALID', 'LEASE_REPLAY',
  'LEASE_REPLAY_WINDOW_FULL', 'LEASE_NODE_MISMATCH', 'LEASE_NOT_YET_VALID', 'LEASE_EXPIRED',
  'LEASE_TTL_EXCEEDED', 'LEASE_INPUT_LIMIT', 'TASK_SCHEMA_INVALID', 'TASK_INPUT_LIMIT',
  'TASK_OPERATION_LIMIT', 'TASK_DEADLINE', 'TASK_PAUSED', 'TASK_REVOKED', 'TASK_UNINSTALLED',
  'TASK_NUMERIC_OVERFLOW', 'TASK_FAILED', 'RESULT_SCHEMA_INVALID', 'RESULT_SIGNATURE_INVALID',
  'RESULT_OUTPUT_LIMIT', 'VALUE_NOT_FINITE',
]);

function asRecord(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function exactKeys(value: Record<string, unknown>, expected: readonly string[]): boolean {
  const actual = Object.keys(value).sort();
  const sortedExpected = [...expected].sort();
  return actual.length === sortedExpected.length
    && actual.every((item, index) => item === sortedExpected[index]);
}

function fail(code: ContributorNodeErrorCode, message: string = code): never {
  throw new ContributorNodeError(code, message);
}

function canonicalize(value: unknown): unknown {
  if (value === null || typeof value === 'string' || typeof value === 'boolean') return value;
  if (typeof value === 'number') {
    if (!Number.isFinite(value)) fail('VALUE_NOT_FINITE');
    return value;
  }
  if (Array.isArray(value)) return value.map(canonicalize);
  if (typeof value === 'object') {
    const record = value as Record<string, unknown>;
    return Object.fromEntries(
      Object.keys(record).sort().map((key) => [key, canonicalize(record[key])]),
    );
  }
  fail('LEASE_SCHEMA_INVALID', 'Unsupported value in canonical envelope.');
}

export function canonicalJson(value: unknown): string {
  const serialized = JSON.stringify(canonicalize(value));
  if (typeof serialized !== 'string') fail('LEASE_SCHEMA_INVALID', 'Envelope is not JSON serializable.');
  return serialized;
}

function byteLength(value: unknown): number {
  return Buffer.byteLength(canonicalJson(value), 'utf8');
}

function boundedIdentifier(value: unknown, code: ContributorNodeErrorCode): string {
  if (typeof value !== 'string' || !IDENTIFIER.test(value)) fail(code);
  return value;
}

function boundedKeyId(value: unknown, code: ContributorNodeErrorCode): string {
  if (typeof value !== 'string' || !KEY_ID.test(value)) fail(code);
  return value;
}

function boundedInteger(value: unknown, minimum: number, maximum: number, code: ContributorNodeErrorCode): number {
  if (!Number.isSafeInteger(value) || Number(value) < minimum || Number(value) > maximum) fail(code);
  return Number(value);
}

function boundedMessage(value: unknown): string {
  if (typeof value !== 'string' || value.length < 1 || value.length > 256) fail('RESULT_SCHEMA_INVALID');
  return value;
}

function decodeSignature(value: unknown, code: ContributorNodeErrorCode): Buffer {
  if (typeof value !== 'string' || value.length < 80 || value.length > 96 || !/^[A-Za-z0-9_-]+$/.test(value)) {
    fail(code);
  }
  const padded = `${value}${'='.repeat((4 - (value.length % 4)) % 4)}`;
  const decoded = Buffer.from(padded, 'base64');
  if (decoded.length !== 64 || decoded.toString('base64url') !== value) fail(code);
  return decoded;
}

function ed25519PublicKey(input: PublicKeyInput): KeyObject {
  try {
    if (input instanceof KeyObject) {
      if (input.asymmetricKeyType !== 'ed25519' || input.type !== 'public') fail('CONTROLLER_KEY_INVALID');
      return input;
    }
    const key = createPublicKey(input);
    if (key.asymmetricKeyType !== 'ed25519' || key.type !== 'public') fail('CONTROLLER_KEY_INVALID');
    return key;
  } catch (error) {
    if (error instanceof ContributorNodeError) throw error;
    fail('CONTROLLER_KEY_INVALID');
  }
}

function ed25519PrivateKey(input: KeyObject | undefined, missingCode: ContributorNodeErrorCode): KeyObject {
  if (!input) fail(missingCode);
  if (input.asymmetricKeyType !== 'ed25519' || input.type !== 'private') fail('NODE_SIGNING_KEY_INVALID');
  return input;
}

function signEnvelope(payload: unknown, privateKey: KeyObject): string {
  try {
    return cryptoSign(null, Buffer.from(canonicalJson(payload), 'utf8'), privateKey).toString('base64url');
  } catch {
    fail('ED25519_UNAVAILABLE');
  }
}

function verifyEnvelope(payload: unknown, signature: unknown, publicKey: KeyObject): boolean {
  try {
    return cryptoVerify(
      null,
      Buffer.from(canonicalJson(payload), 'utf8'),
      publicKey,
      decodeSignature(signature, 'LEASE_SIGNATURE_INVALID'),
    );
  } catch (error) {
    if (error instanceof ContributorNodeError && error.code === 'LEASE_SIGNATURE_INVALID') return false;
    return false;
  }
}

function taskPayload(task: ContributorTask): VectorDotTask {
  return { kind: 'vector_dot', left: [...task.left], right: [...task.right] };
}

function parseTask(value: unknown, policy: ContributorNodePolicy): ContributorTask {
  const source = asRecord(value);
  if (!source || !exactKeys(source, ['kind', 'left', 'right']) || source.kind !== 'vector_dot') {
    fail('TASK_SCHEMA_INVALID');
  }
  if (!Array.isArray(source.left) || !Array.isArray(source.right)) fail('TASK_SCHEMA_INVALID');
  const left = source.left as unknown[];
  const right = source.right as unknown[];
  if (left.length < 1 || right.length < 1 || left.length !== right.length || left.length > policy.maxElements) {
    fail('TASK_INPUT_LIMIT');
  }
  const values = [...left, ...right];
  if (values.some((value) => typeof value !== 'number' || !Number.isFinite(value))) fail('TASK_SCHEMA_INVALID');
  if (byteLength(source) > policy.maxTaskBytes) fail('TASK_INPUT_LIMIT');
  if (left.length * 2 > policy.maxOperations) fail('TASK_OPERATION_LIMIT');
  return {
    kind: 'vector_dot',
    left: left as number[],
    right: right as number[],
  };
}

function leasePayload(lease: LeaseInput): LeaseInput {
  return {
    schema_version: CONTRIBUTOR_LEASE_SCHEMA,
    key_id: lease.key_id,
    lease_id: lease.lease_id,
    node_id: lease.node_id,
    issued_at: lease.issued_at,
    expires_at: lease.expires_at,
    attempt: lease.attempt,
    task: taskPayload(lease.task),
  };
}

function resultPayload(result: Omit<ResultEnvelope, 'signature_b64'>): Omit<ResultEnvelope, 'signature_b64'> {
  return {
    schema_version: CONTRIBUTOR_RESULT_SCHEMA,
    key_id: result.key_id,
    lease_id: result.lease_id,
    node_id: result.node_id,
    attempt: result.attempt,
    started_at: result.started_at,
    finished_at: result.finished_at,
    ok: result.ok,
    output: result.output,
    error_code: result.error_code,
    error_message: result.error_message,
  };
}

function parseLeaseInput(value: unknown, policy: ContributorNodePolicy): LeaseInput {
  const source = asRecord(value);
  if (!source || !exactKeys(source, [
    'schema_version', 'key_id', 'lease_id', 'node_id', 'issued_at', 'expires_at', 'attempt', 'task',
  ]) || source.schema_version !== CONTRIBUTOR_LEASE_SCHEMA) {
    fail('LEASE_SCHEMA_INVALID');
  }
  const input: LeaseInput = {
    schema_version: CONTRIBUTOR_LEASE_SCHEMA,
    key_id: boundedKeyId(source.key_id, 'LEASE_SCHEMA_INVALID'),
    lease_id: boundedIdentifier(source.lease_id, 'LEASE_SCHEMA_INVALID'),
    node_id: boundedIdentifier(source.node_id, 'LEASE_SCHEMA_INVALID'),
    issued_at: boundedInteger(source.issued_at, 0, 9_000_000_000_000, 'LEASE_SCHEMA_INVALID'),
    expires_at: boundedInteger(source.expires_at, 0, 9_000_000_000_000, 'LEASE_SCHEMA_INVALID'),
    attempt: boundedInteger(source.attempt, 0, 1_000_000, 'LEASE_SCHEMA_INVALID'),
    task: parseTask(source.task, policy),
  };
  if (input.expires_at <= input.issued_at) fail('LEASE_TTL_EXCEEDED');
  if (input.expires_at - input.issued_at > policy.maxLeaseTtlMs) fail('LEASE_TTL_EXCEEDED');
  return input;
}

export function parseLeaseEnvelope(value: unknown, policy: ContributorNodePolicy = DEFAULT_CONTRIBUTOR_POLICY): LeaseEnvelope {
  const source = asRecord(value);
  if (!source || !exactKeys(source, [
    'schema_version', 'key_id', 'lease_id', 'node_id', 'issued_at', 'expires_at', 'attempt', 'task', 'signature_b64',
  ])) {
    fail('LEASE_SCHEMA_INVALID');
  }
  const input = parseLeaseInput({
    schema_version: source.schema_version,
    key_id: source.key_id,
    lease_id: source.lease_id,
    node_id: source.node_id,
    issued_at: source.issued_at,
    expires_at: source.expires_at,
    attempt: source.attempt,
    task: source.task,
  }, policy);
  decodeSignature(source.signature_b64, 'LEASE_SIGNATURE_INVALID');
  const envelope: LeaseEnvelope = { ...input, signature_b64: source.signature_b64 as string };
  if (byteLength(envelope) > policy.maxLeaseBytes) fail('LEASE_INPUT_LIMIT');
  return envelope;
}

export function signLease(input: LeaseInput, controllerPrivateKey: KeyObject): LeaseEnvelope {
  const privateKey = ed25519PrivateKey(controllerPrivateKey, 'CONTROLLER_KEY_REQUIRED');
  const normalized = parseLeaseInput(input, DEFAULT_CONTRIBUTOR_POLICY);
  return { ...normalized, signature_b64: signEnvelope(leasePayload(normalized), privateKey) };
}

function parseResultEnvelope(value: unknown, policy: ContributorNodePolicy): ResultEnvelope {
  const source = asRecord(value);
  if (!source || !exactKeys(source, [
    'schema_version', 'key_id', 'lease_id', 'node_id', 'attempt', 'started_at', 'finished_at',
    'ok', 'output', 'error_code', 'error_message', 'signature_b64',
  ]) || source.schema_version !== CONTRIBUTOR_RESULT_SCHEMA) {
    fail('RESULT_SCHEMA_INVALID');
  }
  const ok = source.ok;
  if (typeof ok !== 'boolean') fail('RESULT_SCHEMA_INVALID');
  const output = source.output;
  if (ok && (typeof output !== 'number' || !Number.isFinite(output))) fail('RESULT_SCHEMA_INVALID');
  if (!ok && output !== null) fail('RESULT_SCHEMA_INVALID');
  const errorCode = source.error_code;
  const errorMessage = source.error_message;
  if (ok && (errorCode !== null || errorMessage !== null)) fail('RESULT_SCHEMA_INVALID');
  if (!ok && (typeof errorCode !== 'string' || !ERROR_CODE.test(errorCode)
    || !RESULT_ERROR_CODES.has(errorCode as ContributorNodeErrorCode)
    || typeof errorMessage !== 'string')) {
    fail('RESULT_SCHEMA_INVALID');
  }
  const result: ResultEnvelope = {
    schema_version: CONTRIBUTOR_RESULT_SCHEMA,
    key_id: boundedKeyId(source.key_id, 'RESULT_SCHEMA_INVALID'),
    lease_id: boundedIdentifier(source.lease_id, 'RESULT_SCHEMA_INVALID'),
    node_id: boundedIdentifier(source.node_id, 'RESULT_SCHEMA_INVALID'),
    attempt: boundedInteger(source.attempt, 0, 1_000_000, 'RESULT_SCHEMA_INVALID'),
    started_at: boundedInteger(source.started_at, 0, 9_000_000_000_000, 'RESULT_SCHEMA_INVALID'),
    finished_at: boundedInteger(source.finished_at, 0, 9_000_000_000_000, 'RESULT_SCHEMA_INVALID'),
    ok,
    output: output as number | null,
    error_code: errorCode as ContributorNodeErrorCode | null,
    error_message: errorMessage === null ? null : boundedMessage(errorMessage),
    signature_b64: source.signature_b64 as string,
  };
  if (result.finished_at < result.started_at) fail('RESULT_SCHEMA_INVALID');
  decodeSignature(result.signature_b64, 'RESULT_SIGNATURE_INVALID');
  if (byteLength(result) > policy.maxResultBytes) fail('RESULT_OUTPUT_LIMIT');
  return result;
}

export function verifyResultEnvelope(
  value: unknown,
  nodePublicKey: PublicKeyInput,
  expectedNodeId?: string,
  policy: ContributorNodePolicy = DEFAULT_CONTRIBUTOR_POLICY,
): ResultEnvelope {
  const result = parseResultEnvelope(value, policy);
  const publicKey = ed25519PublicKey(nodePublicKey);
  if (expectedNodeId !== undefined && result.node_id !== expectedNodeId) fail('RESULT_SIGNATURE_INVALID');
  const { signature_b64: _signature, ...payload } = result;
  if (!verifyEnvelope(resultPayload(payload), result.signature_b64, publicKey)) fail('RESULT_SIGNATURE_INVALID');
  return result;
}

export function createLocalIdentity(nodeId: string, keyId = 'node-local-v1'): {
  readonly node_id: string;
  readonly key_id: string;
  readonly public_key: KeyObject;
  readonly private_key: KeyObject;
} {
  boundedIdentifier(nodeId, 'NODE_SIGNING_KEY_INVALID');
  boundedKeyId(keyId, 'NODE_SIGNING_KEY_INVALID');
  try {
    const pair = generateKeyPairSync('ed25519');
    return { node_id: nodeId, key_id: keyId, public_key: pair.publicKey, private_key: pair.privateKey };
  } catch {
    fail('ED25519_UNAVAILABLE');
  }
}

function failureCode(error: unknown): ContributorNodeErrorCode {
  return error instanceof ContributorNodeError ? error.code : 'TASK_FAILED';
}

function failureMessage(error: unknown): string {
  if (error instanceof ContributorNodeError) return error.code;
  return 'TASK_FAILED';
}

function yieldToLoop(): Promise<void> {
  return new Promise((resolve) => setImmediate(resolve));
}

export interface ContributorWorkerOptions {
  readonly nodeId: string;
  readonly controllerKeyId: string;
  readonly controllerPublicKey: PublicKeyInput;
  readonly nodeKeyId?: string;
  readonly nodeSigningKey?: KeyObject;
  readonly policy?: Partial<ContributorNodePolicy>;
  readonly clock?: () => number;
}

function normalizePolicy(input: Partial<ContributorNodePolicy> | undefined): ContributorNodePolicy {
  const candidate = { ...DEFAULT_CONTRIBUTOR_POLICY, ...(input ?? {}) };
  const integerFields: Array<[keyof ContributorNodePolicy, number, number]> = [
    ['maxLeaseBytes', 1_024, 1_024 * 1_024],
    ['maxTaskBytes', 256, 256 * 1_024],
    ['maxResultBytes', 256, 256 * 1_024],
    ['maxElements', 1, 65_536],
    ['maxOperations', 1, 50_000_000],
    ['maxWallMs', 1, 10_000],
    ['maxLeaseTtlMs', 1_000, 15 * 60 * 1_000],
    ['clockSkewMs', 0, 5 * 60 * 1_000],
    ['maxReplayEntries', 1, 4_096],
  ];
  for (const [key, minimum, maximum] of integerFields) {
    if (!Number.isSafeInteger(candidate[key]) || Number(candidate[key]) < minimum || Number(candidate[key]) > maximum) {
      fail('TASK_SCHEMA_INVALID', `Invalid policy field: ${String(key)}.`);
    }
  }
  return Object.freeze(candidate);
}

export class ContributorWorker {
  readonly nodeId: string;
  readonly controllerKeyId: string;
  readonly nodeKeyId: string;
  readonly policy: ContributorNodePolicy;
  private readonly controllerPublicKey: KeyObject;
  private readonly clock: () => number;
  private nodeSigningKey: KeyObject | null;
  private mode: ContributorNodeMode = 'paused';
  private activeLeaseId: string | null = null;
  private stopRequested = false;
  private readonly replay = new Map<string, number>();

  constructor(options: ContributorWorkerOptions) {
    this.nodeId = boundedIdentifier(options.nodeId, 'NODE_SIGNING_KEY_INVALID');
    this.controllerKeyId = boundedKeyId(options.controllerKeyId, 'CONTROLLER_KEY_ID_MISMATCH');
    this.nodeKeyId = boundedKeyId(options.nodeKeyId ?? 'node-local-v1', 'NODE_SIGNING_KEY_INVALID');
    this.policy = normalizePolicy(options.policy);
    if (!options.controllerPublicKey) fail('CONTROLLER_KEY_REQUIRED');
    this.controllerPublicKey = ed25519PublicKey(options.controllerPublicKey);
    if (options.nodeSigningKey) {
      this.nodeSigningKey = ed25519PrivateKey(options.nodeSigningKey, 'NODE_SIGNING_KEY_REQUIRED');
    } else {
      try {
        this.nodeSigningKey = generateKeyPairSync('ed25519').privateKey;
      } catch {
        fail('ED25519_UNAVAILABLE');
      }
    }
    this.clock = options.clock ?? (() => Date.now());
  }

  status(): ContributorWorkerStatus {
    return {
      schema_version: CONTRIBUTOR_NODE_SCHEMA,
      node_id: this.nodeId,
      mode: this.mode,
      network_enabled: false,
      active_lease_id: this.activeLeaseId,
      replay_entries: this.replay.size,
      node_signing_key_present: this.nodeSigningKey !== null,
      limits: this.policy,
    };
  }

  optIn(): ContributorWorkerStatus {
    if (this.mode === 'paused') this.mode = 'ready';
    else if (this.mode === 'revoked') fail('WORKER_REVOKED');
    else if (this.mode === 'uninstalled') fail('WORKER_UNINSTALLED');
    return this.status();
  }

  pause(): ContributorWorkerStatus {
    if (this.mode === 'revoked') return this.status();
    if (this.mode === 'uninstalled') return this.status();
    this.stopRequested = true;
    this.mode = 'paused';
    return this.status();
  }

  revoke(): ContributorWorkerStatus {
    if (this.mode !== 'uninstalled') {
      this.stopRequested = true;
      this.mode = 'revoked';
      this.nodeSigningKey = null;
      this.replay.clear();
    }
    return this.status();
  }

  uninstall(): UninstallReceipt {
    this.stopRequested = true;
    this.mode = 'uninstalled';
    this.nodeSigningKey = null;
    this.activeLeaseId = null;
    this.replay.clear();
    return {
      schema_version: CONTRIBUTOR_UNINSTALL_SCHEMA,
      node_id: this.nodeId,
      state_cleared: true,
      replay_entries_cleared: true,
      node_signing_key_cleared: true,
    };
  }

  private pruneReplay(now: number): void {
    for (const [leaseId, expiresAt] of this.replay) {
      if (expiresAt <= now) this.replay.delete(leaseId);
    }
  }

  private ensureReady(): void {
    if (this.mode === 'paused') fail('WORKER_NOT_OPTED_IN');
    if (this.mode === 'revoked') fail('WORKER_REVOKED');
    if (this.mode === 'uninstalled') fail('WORKER_UNINSTALLED');
    if (this.mode === 'running' || this.activeLeaseId !== null) fail('CONCURRENCY_LIMIT');
    if (!this.nodeSigningKey) fail('NODE_SIGNING_KEY_REQUIRED');
  }

  private ensureExecutionAllowed(): void {
    if (this.mode === 'revoked') fail('TASK_REVOKED');
    if (this.mode === 'uninstalled') fail('TASK_UNINSTALLED');
    if (this.stopRequested || this.mode === 'paused') fail('TASK_PAUSED');
  }

  private signedResult(
    lease: LeaseEnvelope,
    startedAt: number,
    finishedAt: number,
    ok: boolean,
    output: number | null,
    errorCode: ContributorNodeErrorCode | null,
    errorMessage: string | null,
  ): ResultEnvelope {
    const key = this.nodeSigningKey;
    if (!key) fail('NODE_SIGNING_KEY_REQUIRED');
    const payload = resultPayload({
      schema_version: CONTRIBUTOR_RESULT_SCHEMA,
      key_id: this.nodeKeyId,
      lease_id: lease.lease_id,
      node_id: this.nodeId,
      attempt: lease.attempt,
      started_at: startedAt,
      finished_at: finishedAt,
      ok,
      output,
      error_code: errorCode,
      error_message: errorMessage,
    });
    const result: ResultEnvelope = { ...payload, signature_b64: signEnvelope(payload, key) };
    if (byteLength(result) > this.policy.maxResultBytes) fail('RESULT_OUTPUT_LIMIT');
    return result;
  }

  private async execute(task: VectorDotTask): Promise<number> {
    const started = performance.now();
    let total = 0;
    for (let index = 0; index < task.left.length; index += 1) {
      if ((index & 127) === 0) {
        this.ensureExecutionAllowed();
        if (performance.now() - started > this.policy.maxWallMs) fail('TASK_DEADLINE');
        await yieldToLoop();
      }
      total += task.left[index]! * task.right[index]!;
      if (!Number.isFinite(total)) fail('TASK_NUMERIC_OVERFLOW');
    }
    this.ensureExecutionAllowed();
    if (performance.now() - started > this.policy.maxWallMs) fail('TASK_DEADLINE');
    return total;
  }

  async run(value: unknown, now = this.clock()): Promise<ResultEnvelope> {
    this.ensureReady();
    const lease = parseLeaseEnvelope(value, this.policy);
    if (lease.key_id !== this.controllerKeyId) fail('CONTROLLER_KEY_ID_MISMATCH');
    if (lease.node_id !== this.nodeId) fail('LEASE_NODE_MISMATCH');
    if (!verifyEnvelope(leasePayload(lease), lease.signature_b64, this.controllerPublicKey)) {
      fail('LEASE_SIGNATURE_INVALID');
    }
    if (now + this.policy.clockSkewMs < lease.issued_at) fail('LEASE_NOT_YET_VALID');
    if (now >= lease.expires_at) fail('LEASE_EXPIRED');
    this.pruneReplay(now);
    if (this.replay.has(lease.lease_id)) fail('LEASE_REPLAY');
    if (this.replay.size >= this.policy.maxReplayEntries) fail('LEASE_REPLAY_WINDOW_FULL');
    this.replay.set(lease.lease_id, lease.expires_at);
    this.activeLeaseId = lease.lease_id;
    this.stopRequested = false;
    this.mode = 'running';
    const startedAt = this.clock();
    try {
      const output = await this.execute(lease.task);
      const finishedAt = this.clock();
      return this.signedResult(lease, startedAt, finishedAt, true, output, null, null);
    } catch (error) {
      const mode = this.mode as ContributorNodeMode;
      if (mode === 'revoked' || mode === 'uninstalled') {
        throw error;
      }
      const code = failureCode(error);
      const message = failureMessage(error);
      return this.signedResult(lease, startedAt, this.clock(), false, null, code, message);
    } finally {
      this.activeLeaseId = null;
      if (this.mode === 'running') this.mode = 'ready';
    }
  }
}

/** Stable digest helper for release tooling; it does not read files or send data. */
export function sha256Text(value: string): string {
  return createHash('sha256').update(value, 'utf8').digest('hex');
}
