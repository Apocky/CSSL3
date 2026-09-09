import {
  createHash,
  createPublicKey,
  sign as cryptoSign,
  verify as cryptoVerify,
  type KeyObject,
} from 'node:crypto';

import {
  canonicalJson,
  parseLeaseEnvelope,
  signLease,
  verifyResultEnvelope,
  type ContributorTask,
  type LeaseEnvelope,
  type LeaseInput,
  type PublicKeyInput,
  type ResultEnvelope,
} from '../../contributor-node/src/runtime';

/**
 * Authenticated controller transport for the local contributor-node runtime.
 *
 * This module deliberately has no default persistence, network client, or
 * route side effects.  A production route must supply a durable transactional
 * store and an already-authenticated operator boundary.  The memory store is
 * test-only and is never selected implicitly by a deployment.
 */

export const CONTRIBUTOR_ENROLLMENT_SCHEMA = 'apocrypha.contributor.enrollment.v1' as const;
export const CONTRIBUTOR_ENROLLMENT_RECEIPT_SCHEMA = 'apocrypha.contributor.enrollment-receipt.v1' as const;
export const CONTRIBUTOR_LEASE_REQUEST_SCHEMA = 'apocrypha.contributor.lease-request.v1' as const;
export const CONTRIBUTOR_LEASE_POLL_SCHEMA = 'apocrypha.contributor.lease-poll.v1' as const;
export const CONTRIBUTOR_LEASE_DISPATCH_SCHEMA = 'apocrypha.contributor.lease-dispatch.v1' as const;
export const CONTRIBUTOR_RESULT_SUBMISSION_SCHEMA = 'apocrypha.contributor.result-submission.v1' as const;
export const CONTRIBUTOR_RESULT_RECEIPT_SCHEMA = 'apocrypha.contributor.result-receipt.v1' as const;
export const CONTRIBUTOR_REVOKE_SCHEMA = 'apocrypha.contributor.revoke.v1' as const;
export const CONTRIBUTOR_REVOKE_RECEIPT_SCHEMA = 'apocrypha.contributor.revoke-receipt.v1' as const;

export const CONTRIBUTOR_ENROLLMENT_TTL_MS = 5 * 60 * 1_000;
export const CONTRIBUTOR_CLOCK_SKEW_MS = 30 * 1_000;
export const CONTRIBUTOR_CAPABILITIES = ['vector_dot'] as const;
export const CONTRIBUTOR_PLATFORMS = ['windows-x64', 'macos-arm64', 'linux-x64', 'android', 'ios'] as const;

export type ContributorCapability = (typeof CONTRIBUTOR_CAPABILITIES)[number];
export type ContributorPlatform = (typeof CONTRIBUTOR_PLATFORMS)[number];
export type ContributorNodeStatus = 'active' | 'revoked';

export type ContributorTransportErrorCode =
  | 'TRANSPORT_SCHEMA_INVALID'
  | 'TRANSPORT_SIGNATURE_INVALID'
  | 'TRANSPORT_PUBLIC_KEY_INVALID'
  | 'TRANSPORT_KEY_ID_INVALID'
  | 'TRANSPORT_NODE_ID_INVALID'
  | 'TRANSPORT_NODE_KEY_MISMATCH'
  | 'TRANSPORT_NODE_NOT_ENROLLED'
  | 'TRANSPORT_NODE_REVOKED'
  | 'TRANSPORT_TIME_INVALID'
  | 'TRANSPORT_ENROLLMENT_EXPIRED'
  | 'TRANSPORT_ENROLLMENT_REPLAY'
  | 'TRANSPORT_IDEMPOTENCY_REQUIRED'
  | 'TRANSPORT_IDEMPOTENCY_CONFLICT'
  | 'TRANSPORT_LEASE_INVALID'
  | 'TRANSPORT_LEASE_UNKNOWN'
  | 'TRANSPORT_RESULT_INVALID'
  | 'TRANSPORT_RESULT_REPLAY'
  | 'TRANSPORT_REVOCATION_UNCONFIGURED'
  | 'TRANSPORT_STORE_UNAVAILABLE';

export class ContributorTransportError extends Error {
  readonly code: ContributorTransportErrorCode;

  constructor(code: ContributorTransportErrorCode, message: string = code) {
    super(message);
    this.name = 'ContributorTransportError';
    this.code = code;
  }
}

export interface EnrollmentRequestPayload {
  readonly schema_version: typeof CONTRIBUTOR_ENROLLMENT_SCHEMA;
  readonly request_id: string;
  readonly node_id: string;
  readonly node_key_id: string;
  readonly node_public_key_spki_b64: string;
  readonly platform: ContributorPlatform;
  readonly capabilities: readonly ContributorCapability[];
  readonly consent_revision: string;
  readonly issued_at: number;
  readonly expires_at: number;
}

export interface EnrollmentRequest extends EnrollmentRequestPayload {
  readonly signature_b64: string;
}

export interface EnrollmentReceiptPayload {
  readonly schema_version: typeof CONTRIBUTOR_ENROLLMENT_RECEIPT_SCHEMA;
  readonly request_id: string;
  readonly enrollment_id: string;
  readonly node_id: string;
  readonly node_key_id: string;
  readonly controller_key_id: string;
  readonly status: 'active';
  readonly revision: number;
  readonly request_hash: string;
  readonly issued_at: number;
  readonly expires_at: number;
}

export interface EnrollmentReceipt extends EnrollmentReceiptPayload {
  readonly signature_b64: string;
}

export interface LeaseIssueRequestPayload {
  readonly schema_version: typeof CONTRIBUTOR_LEASE_REQUEST_SCHEMA;
  readonly request_id: string;
  readonly idempotency_key: string;
  readonly node_id: string;
  readonly issued_at: number;
  readonly expires_at: number;
  readonly attempt: number;
  readonly task: ContributorTask;
}

export interface LeaseIssueRequest extends LeaseIssueRequestPayload {
  readonly signature_b64?: never;
}

/** Public node-authenticated lease request.  The node key id is bound to the
 * enrolled public key before the controller signs a dispatch. */
export interface LeasePollRequestPayload {
  readonly schema_version: typeof CONTRIBUTOR_LEASE_POLL_SCHEMA;
  readonly request_id: string;
  readonly idempotency_key: string;
  readonly node_id: string;
  readonly node_key_id: string;
  readonly issued_at: number;
  readonly expires_at: number;
  readonly attempt: number;
  readonly task: ContributorTask;
}

export interface LeasePollRequest extends LeasePollRequestPayload {
  readonly signature_b64: string;
}

export interface LeaseDispatchPayload {
  readonly schema_version: typeof CONTRIBUTOR_LEASE_DISPATCH_SCHEMA;
  readonly dispatch_id: string;
  readonly request_id: string;
  readonly idempotency_key: string;
  readonly node_id: string;
  readonly lease: LeaseEnvelope;
}

export interface LeaseDispatch extends LeaseDispatchPayload {
  readonly signature_b64: string;
}

export interface ResultSubmissionPayload {
  readonly schema_version: typeof CONTRIBUTOR_RESULT_SUBMISSION_SCHEMA;
  readonly dispatch_id: string;
  readonly request_id: string;
  readonly idempotency_key: string;
  readonly node_id: string;
  readonly result: ResultEnvelope;
}

export interface ResultSubmission extends ResultSubmissionPayload {
  readonly signature_b64: string;
}

export interface ResultReceiptPayload {
  readonly schema_version: typeof CONTRIBUTOR_RESULT_RECEIPT_SCHEMA;
  readonly dispatch_id: string;
  readonly request_id: string;
  readonly idempotency_key: string;
  readonly node_id: string;
  readonly lease_id: string;
  readonly result_hash: string;
  readonly status: 'accepted';
  readonly accepted_at: number;
}

export interface ResultReceipt extends ResultReceiptPayload {
  readonly signature_b64: string;
}

export interface RevokeRequestPayload {
  readonly schema_version: typeof CONTRIBUTOR_REVOKE_SCHEMA;
  readonly request_id: string;
  readonly node_id: string;
  readonly reason: string;
  readonly issued_at: number;
  readonly expires_at: number;
  readonly operator_key_id: string;
}

export interface RevokeRequest extends RevokeRequestPayload {
  readonly signature_b64: string;
}

export interface RevokeReceiptPayload {
  readonly schema_version: typeof CONTRIBUTOR_REVOKE_RECEIPT_SCHEMA;
  readonly request_id: string;
  readonly node_id: string;
  readonly controller_key_id: string;
  readonly status: 'revoked' | 'already_revoked';
  readonly revision: number;
  readonly reason: string;
  readonly revoked_at: number;
}

export interface RevokeReceipt extends RevokeReceiptPayload {
  readonly signature_b64: string;
}

export interface ContributorNodeRecord {
  readonly node_id: string;
  readonly node_key_id: string;
  readonly node_public_key_spki_b64: string;
  readonly platform: ContributorPlatform;
  readonly capabilities: readonly ContributorCapability[];
  readonly status: ContributorNodeStatus;
  readonly revision: number;
  readonly enrolled_at: number;
  readonly revoked_at: number | null;
  readonly revoke_reason: string | null;
}

export interface EnrollmentReplay {
  readonly request_hash: string;
  readonly receipt: EnrollmentReceipt;
}

export interface RevokeReplay {
  readonly request_hash: string;
  readonly receipt: RevokeReceipt;
}

export interface LeaseReplay {
  readonly request_hash: string;
  readonly dispatch: LeaseDispatch;
}

export interface ResultReplay {
  readonly submission_hash: string;
  readonly receipt: ResultReceipt;
}

/**
 * The production adapter must make transaction() cover every read/write in a
 * controller operation.  This prevents two concurrent retries from issuing
 * different leases for the same node/idempotency key.
 */
export interface ContributorTransportStore {
  transaction<T>(operation: () => Promise<T>): Promise<T>;
  getNode(nodeId: string): Promise<ContributorNodeRecord | null>;
  putNode(node: ContributorNodeRecord): Promise<void>;
  getEnrollmentReplay(requestId: string): Promise<EnrollmentReplay | null>;
  putEnrollmentReplay(requestId: string, replay: EnrollmentReplay): Promise<void>;
  getRevokeReplay(requestId: string): Promise<RevokeReplay | null>;
  putRevokeReplay(requestId: string, replay: RevokeReplay): Promise<void>;
  getLeaseReplay(nodeId: string, idempotencyKey: string): Promise<LeaseReplay | null>;
  getLeaseByDispatch(dispatchId: string): Promise<LeaseReplay | null>;
  putLeaseReplay(nodeId: string, idempotencyKey: string, replay: LeaseReplay): Promise<void>;
  getResultReplay(dispatchId: string): Promise<ResultReplay | null>;
  putResultReplay(dispatchId: string, replay: ResultReplay): Promise<void>;
}

const IDENTIFIER = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/;
const KEY_ID = /^[A-Za-z0-9][A-Za-z0-9._:-]{2,127}$/;
const REQUEST_ID = /^[A-Za-z0-9][A-Za-z0-9._:-]{7,127}$/;
const IDEMPOTENCY_KEY = /^[A-Za-z0-9][A-Za-z0-9._:-]{7,199}$/;
const HASH = /^[0-9a-f]{64}$/;
const BASE64 = /^[A-Za-z0-9+/]+={0,2}$/;
const BASE64URL = /^[A-Za-z0-9_-]{86}$/;
const REASON = /^[\x20-\x7e]{1,256}$/;

function asRecord(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function exactKeys(value: Record<string, unknown>, expected: readonly string[]): boolean {
  const actual = Object.keys(value).sort();
  const keys = [...expected].sort();
  return actual.length === keys.length && actual.every((key, index) => key === keys[index]);
}

function fail(code: ContributorTransportErrorCode, message: string = code): never {
  throw new ContributorTransportError(code, message);
}

function boundedIdentifier(value: unknown, code: ContributorTransportErrorCode = 'TRANSPORT_NODE_ID_INVALID'): string {
  if (typeof value !== 'string' || !IDENTIFIER.test(value)) fail(code);
  return value;
}

function boundedKeyId(value: unknown): string {
  if (typeof value !== 'string' || !KEY_ID.test(value)) fail('TRANSPORT_KEY_ID_INVALID');
  return value;
}

function boundedRequestId(value: unknown): string {
  if (typeof value !== 'string' || !REQUEST_ID.test(value)) fail('TRANSPORT_SCHEMA_INVALID');
  return value;
}

function boundedIdempotencyKey(value: unknown): string {
  if (typeof value !== 'string' || !IDEMPOTENCY_KEY.test(value)) fail('TRANSPORT_IDEMPOTENCY_REQUIRED');
  return value;
}

function boundedInteger(value: unknown, minimum: number, maximum: number): number {
  if (!Number.isSafeInteger(value) || Number(value) < minimum || Number(value) > maximum) {
    fail('TRANSPORT_SCHEMA_INVALID');
  }
  return Number(value);
}

function boundedHash(value: unknown): string {
  if (typeof value !== 'string' || !HASH.test(value)) fail('TRANSPORT_SCHEMA_INVALID');
  return value;
}

function decodeBase64(value: unknown, maximumBytes: number): Buffer {
  if (typeof value !== 'string' || value.length < 1 || value.length > Math.ceil(maximumBytes / 3) * 4 + 4
    || !BASE64.test(value)) fail('TRANSPORT_PUBLIC_KEY_INVALID');
  const bytes = Buffer.from(value, 'base64');
  if (bytes.length < 1 || bytes.length > maximumBytes || bytes.toString('base64') !== value) {
    fail('TRANSPORT_PUBLIC_KEY_INVALID');
  }
  return bytes;
}

function decodeSignature(value: unknown): Buffer {
  if (typeof value !== 'string' || !BASE64URL.test(value)) fail('TRANSPORT_SIGNATURE_INVALID');
  const bytes = Buffer.from(value, 'base64url');
  if (bytes.length !== 64 || bytes.toString('base64url') !== value) fail('TRANSPORT_SIGNATURE_INVALID');
  return bytes;
}

function ed25519PrivateKey(value: KeyObject): KeyObject {
  if (!value || value.type !== 'private' || value.asymmetricKeyType !== 'ed25519') {
    fail('TRANSPORT_PUBLIC_KEY_INVALID');
  }
  return value;
}

function ed25519PublicKey(value: PublicKeyInput): KeyObject {
  try {
    const key = value instanceof Object && 'type' in value && 'asymmetricKeyType' in value
      ? value as KeyObject
      : createPublicKey(value);
    if (key.type !== 'public' || key.asymmetricKeyType !== 'ed25519') fail('TRANSPORT_PUBLIC_KEY_INVALID');
    return key;
  } catch (error) {
    if (error instanceof ContributorTransportError) throw error;
    fail('TRANSPORT_PUBLIC_KEY_INVALID');
  }
}

function publicKeyFromSpki(value: string): KeyObject {
  const der = decodeBase64(value, 128);
  try {
    // @types/node currently narrows createPublicKey's object overload too far
    // for the DER SPKI form even though Node accepts it at runtime.
    const key = createPublicKey({ key: der, format: 'der', type: 'spki' } as never);
    return ed25519PublicKey(key);
  } catch {
    fail('TRANSPORT_PUBLIC_KEY_INVALID');
  }
}

export function publicKeySpkiB64(value: PublicKeyInput): string {
  const key = ed25519PublicKey(value);
  try {
    const der = key.export({ type: 'spki', format: 'der' });
    return Buffer.from(der).toString('base64');
  } catch {
    fail('TRANSPORT_PUBLIC_KEY_INVALID');
  }
}

/**
 * Server-side transport binding helper.  The controller boundary needs the
 * same strict SPKI parser used by result verification; exposing this narrow
 * wrapper avoids reimplementing DER/key-shape validation in an adapter.
 */
export function publicKeyFromSpkiB64(value: string): KeyObject {
  return publicKeyFromSpki(value);
}

function signPayload(payload: unknown, privateKey: KeyObject): string {
  try {
    return cryptoSign(null, Buffer.from(canonicalJson(payload), 'utf8'), ed25519PrivateKey(privateKey)).toString('base64url');
  } catch (error) {
    if (error instanceof ContributorTransportError) throw error;
    fail('TRANSPORT_SIGNATURE_INVALID');
  }
}

function verifyPayload(payload: unknown, signature: unknown, publicKey: PublicKeyInput): boolean {
  try {
    return cryptoVerify(null, Buffer.from(canonicalJson(payload), 'utf8'), ed25519PublicKey(publicKey), decodeSignature(signature));
  } catch {
    return false;
  }
}

function hashPayload(payload: unknown): string {
  return createHash('sha256').update(canonicalJson(payload), 'utf8').digest('hex');
}

/** Stable transport hash for operation-level RPC preparation. */
export function contributorPayloadHash(payload: unknown): string {
  return hashPayload(payload);
}

function exactCapabilities(value: unknown): readonly ContributorCapability[] {
  if (!Array.isArray(value) || value.length !== CONTRIBUTOR_CAPABILITIES.length
    || value.some((item) => typeof item !== 'string')
    || value.some((item) => !CONTRIBUTOR_CAPABILITIES.includes(item as ContributorCapability))
    || new Set(value).size !== value.length
    || value.some((item, index) => item !== CONTRIBUTOR_CAPABILITIES[index])) {
    fail('TRANSPORT_SCHEMA_INVALID');
  }
  return [...value] as ContributorCapability[];
}

function platform(value: unknown): ContributorPlatform {
  if (typeof value !== 'string' || !CONTRIBUTOR_PLATFORMS.includes(value as ContributorPlatform)) {
    fail('TRANSPORT_SCHEMA_INVALID');
  }
  return value as ContributorPlatform;
}

function boundedConsent(value: unknown): string {
  if (typeof value !== 'string' || value.length < 1 || value.length > 128 || !REASON.test(value)) {
    fail('TRANSPORT_SCHEMA_INVALID');
  }
  return value;
}

function temporal(issuedAt: number, expiresAt: number, now: number, maxTtl: number, expiredCode: ContributorTransportErrorCode): void {
  if (!Number.isSafeInteger(now) || now < 0 || expiresAt <= issuedAt || expiresAt - issuedAt > maxTtl) {
    fail('TRANSPORT_TIME_INVALID');
  }
  if (now + CONTRIBUTOR_CLOCK_SKEW_MS < issuedAt) fail('TRANSPORT_TIME_INVALID');
  if (now >= expiresAt) fail(expiredCode);
}

function enrollmentPayload(value: EnrollmentRequest | EnrollmentRequestPayload): EnrollmentRequestPayload {
  return {
    schema_version: CONTRIBUTOR_ENROLLMENT_SCHEMA,
    request_id: value.request_id,
    node_id: value.node_id,
    node_key_id: value.node_key_id,
    node_public_key_spki_b64: value.node_public_key_spki_b64,
    platform: value.platform,
    capabilities: [...value.capabilities],
    consent_revision: value.consent_revision,
    issued_at: value.issued_at,
    expires_at: value.expires_at,
  };
}

function parseEnrollment(value: unknown): EnrollmentRequest {
  const source = asRecord(value);
  if (!source || !exactKeys(source, [
    'schema_version', 'request_id', 'node_id', 'node_key_id', 'node_public_key_spki_b64',
    'platform', 'capabilities', 'consent_revision', 'issued_at', 'expires_at', 'signature_b64',
  ]) || source.schema_version !== CONTRIBUTOR_ENROLLMENT_SCHEMA) fail('TRANSPORT_SCHEMA_INVALID');
  const input: EnrollmentRequestPayload = {
    schema_version: CONTRIBUTOR_ENROLLMENT_SCHEMA,
    request_id: boundedRequestId(source.request_id),
    node_id: boundedIdentifier(source.node_id),
    node_key_id: boundedKeyId(source.node_key_id),
    node_public_key_spki_b64: source.node_public_key_spki_b64 as string,
    platform: platform(source.platform),
    capabilities: exactCapabilities(source.capabilities),
    consent_revision: boundedConsent(source.consent_revision),
    issued_at: boundedInteger(source.issued_at, 0, 9_000_000_000_000),
    expires_at: boundedInteger(source.expires_at, 0, 9_000_000_000_000),
  };
  publicKeyFromSpki(input.node_public_key_spki_b64);
  decodeSignature(source.signature_b64);
  return { ...input, signature_b64: source.signature_b64 as string };
}

export function signEnrollmentRequest(input: EnrollmentRequestPayload, nodePrivateKey: KeyObject): EnrollmentRequest {
  const parsed = parseEnrollment({ ...input, signature_b64: signPayload(enrollmentPayload(input), nodePrivateKey) });
  const derived = publicKeySpkiB64(createPublicKey(ed25519PrivateKey(nodePrivateKey)));
  if (derived !== parsed.node_public_key_spki_b64) fail('TRANSPORT_NODE_KEY_MISMATCH');
  return parsed;
}

export function verifyEnrollmentRequest(value: unknown, now = Date.now()): EnrollmentRequest {
  const request = parseEnrollment(value);
  temporal(request.issued_at, request.expires_at, now, CONTRIBUTOR_ENROLLMENT_TTL_MS, 'TRANSPORT_ENROLLMENT_EXPIRED');
  const publicKey = publicKeyFromSpki(request.node_public_key_spki_b64);
  if (!verifyPayload(enrollmentPayload(request), request.signature_b64, publicKey)) fail('TRANSPORT_SIGNATURE_INVALID');
  return request;
}

function receiptPayload(value: EnrollmentReceipt | EnrollmentReceiptPayload): EnrollmentReceiptPayload {
  return {
    schema_version: CONTRIBUTOR_ENROLLMENT_RECEIPT_SCHEMA,
    request_id: value.request_id,
    enrollment_id: value.enrollment_id,
    node_id: value.node_id,
    node_key_id: value.node_key_id,
    controller_key_id: value.controller_key_id,
    status: 'active',
    revision: value.revision,
    request_hash: value.request_hash,
    issued_at: value.issued_at,
    expires_at: value.expires_at,
  };
}

function parseReceipt(value: unknown): EnrollmentReceipt {
  const source = asRecord(value);
  if (!source || !exactKeys(source, [
    'schema_version', 'request_id', 'enrollment_id', 'node_id', 'node_key_id', 'controller_key_id',
    'status', 'revision', 'request_hash', 'issued_at', 'expires_at', 'signature_b64',
  ]) || source.schema_version !== CONTRIBUTOR_ENROLLMENT_RECEIPT_SCHEMA || source.status !== 'active') {
    fail('TRANSPORT_SCHEMA_INVALID');
  }
  return {
    schema_version: CONTRIBUTOR_ENROLLMENT_RECEIPT_SCHEMA,
    request_id: boundedRequestId(source.request_id),
    enrollment_id: boundedIdentifier(source.enrollment_id),
    node_id: boundedIdentifier(source.node_id),
    node_key_id: boundedKeyId(source.node_key_id),
    controller_key_id: boundedKeyId(source.controller_key_id),
    status: 'active',
    revision: boundedInteger(source.revision, 1, 1_000_000_000),
    request_hash: boundedHash(source.request_hash),
    issued_at: boundedInteger(source.issued_at, 0, 9_000_000_000_000),
    expires_at: boundedInteger(source.expires_at, 0, 9_000_000_000_000),
    signature_b64: decodeSignature(source.signature_b64).toString('base64url'),
  };
}

export function verifyEnrollmentReceipt(value: unknown, controllerPublicKey: PublicKeyInput, expectedNodeId?: string): EnrollmentReceipt {
  const receipt = parseReceipt(value);
  if (expectedNodeId !== undefined && receipt.node_id !== expectedNodeId) fail('TRANSPORT_SIGNATURE_INVALID');
  if (!verifyPayload(receiptPayload(receipt), receipt.signature_b64, controllerPublicKey)) fail('TRANSPORT_SIGNATURE_INVALID');
  return receipt;
}

function leaseRequestPayload(value: LeaseIssueRequestPayload): LeaseIssueRequestPayload {
  return {
    schema_version: CONTRIBUTOR_LEASE_REQUEST_SCHEMA,
    request_id: value.request_id,
    idempotency_key: value.idempotency_key,
    node_id: value.node_id,
    issued_at: value.issued_at,
    expires_at: value.expires_at,
    attempt: value.attempt,
    task: value.task,
  };
}

function parseLeaseRequest(value: unknown): LeaseIssueRequest {
  const source = asRecord(value);
  if (!source || !exactKeys(source, [
    'schema_version', 'request_id', 'idempotency_key', 'node_id', 'issued_at', 'expires_at', 'attempt', 'task',
  ]) || source.schema_version !== CONTRIBUTOR_LEASE_REQUEST_SCHEMA) fail('TRANSPORT_LEASE_INVALID');
  return {
    schema_version: CONTRIBUTOR_LEASE_REQUEST_SCHEMA,
    request_id: boundedRequestId(source.request_id),
    idempotency_key: boundedIdempotencyKey(source.idempotency_key),
    node_id: boundedIdentifier(source.node_id),
    issued_at: boundedInteger(source.issued_at, 0, 9_000_000_000_000),
    expires_at: boundedInteger(source.expires_at, 0, 9_000_000_000_000),
    attempt: boundedInteger(source.attempt, 0, 1_000_000),
    task: source.task as ContributorTask,
  };
}

/** Strict parser for the authenticated controller-side lease request. */
export function parseContributorLeaseIssueRequest(value: unknown): LeaseIssueRequest {
  return parseLeaseRequest(value);
}

/**
 * Lease request validation used before an atomic RPC envelope is signed.  The
 * request is authenticated by the route's separate controller bearer gate;
 * it deliberately has no node signature field.
 */
export function verifyContributorLeaseIssueRequest(value: unknown, now = Date.now()): LeaseIssueRequest {
  const request = parseContributorLeaseIssueRequest(value);
  temporal(request.issued_at, request.expires_at, now, CONTRIBUTOR_ENROLLMENT_TTL_MS, 'TRANSPORT_ENROLLMENT_EXPIRED');
  return request;
}

function leasePollPayload(value: LeasePollRequestPayload): LeasePollRequestPayload {
  return {
    schema_version: CONTRIBUTOR_LEASE_POLL_SCHEMA,
    request_id: value.request_id,
    idempotency_key: value.idempotency_key,
    node_id: value.node_id,
    node_key_id: value.node_key_id,
    issued_at: value.issued_at,
    expires_at: value.expires_at,
    attempt: value.attempt,
    task: value.task,
  };
}

function parseLeasePollRequest(value: unknown): LeasePollRequest {
  const source = asRecord(value);
  if (!source || !exactKeys(source, [
    'schema_version', 'request_id', 'idempotency_key', 'node_id', 'node_key_id',
    'issued_at', 'expires_at', 'attempt', 'task', 'signature_b64',
  ]) || source.schema_version !== CONTRIBUTOR_LEASE_POLL_SCHEMA) {
    fail('TRANSPORT_LEASE_INVALID');
  }
  return {
    schema_version: CONTRIBUTOR_LEASE_POLL_SCHEMA,
    request_id: boundedRequestId(source.request_id),
    idempotency_key: boundedIdempotencyKey(source.idempotency_key),
    node_id: boundedIdentifier(source.node_id),
    node_key_id: boundedKeyId(source.node_key_id),
    issued_at: boundedInteger(source.issued_at, 0, 9_000_000_000_000),
    expires_at: boundedInteger(source.expires_at, 0, 9_000_000_000_000),
    attempt: boundedInteger(source.attempt, 0, 1_000_000),
    task: source.task as ContributorTask,
    signature_b64: decodeSignature(source.signature_b64).toString('base64url'),
  };
}

/** Strict parser for the public node-authenticated lease polling request. */
export function parseContributorLeasePollRequest(value: unknown): LeasePollRequest {
  return parseLeasePollRequest(value);
}

export function signLeasePollRequest(
  input: LeasePollRequestPayload,
  nodePrivateKey: KeyObject,
): LeasePollRequest {
  const payload = leasePollPayload(input);
  return { ...payload, signature_b64: signPayload(payload, nodePrivateKey) };
}

/** Verify a poll request against the public key recorded at enrollment. */
export function verifyContributorLeasePollRequest(
  value: unknown,
  nodePublicKey: PublicKeyInput,
  expectedNodeId?: string,
  expectedNodeKeyId?: string,
  now = Date.now(),
): LeasePollRequest {
  const request = parseLeasePollRequest(value);
  temporal(request.issued_at, request.expires_at, now, CONTRIBUTOR_ENROLLMENT_TTL_MS, 'TRANSPORT_ENROLLMENT_EXPIRED');
  if (expectedNodeId !== undefined && request.node_id !== expectedNodeId) fail('TRANSPORT_NODE_KEY_MISMATCH');
  if (expectedNodeKeyId !== undefined && request.node_key_id !== expectedNodeKeyId) fail('TRANSPORT_NODE_KEY_MISMATCH');
  if (!verifyPayload(leasePollPayload(request), request.signature_b64, nodePublicKey)) {
    fail('TRANSPORT_SIGNATURE_INVALID');
  }
  return request;
}

function dispatchPayload(value: LeaseDispatch | LeaseDispatchPayload): LeaseDispatchPayload {
  return {
    schema_version: CONTRIBUTOR_LEASE_DISPATCH_SCHEMA,
    dispatch_id: value.dispatch_id,
    request_id: value.request_id,
    idempotency_key: value.idempotency_key,
    node_id: value.node_id,
    lease: value.lease,
  };
}

function parseDispatch(value: unknown): LeaseDispatch {
  const source = asRecord(value);
  if (!source || !exactKeys(source, [
    'schema_version', 'dispatch_id', 'request_id', 'idempotency_key', 'node_id', 'lease', 'signature_b64',
  ]) || source.schema_version !== CONTRIBUTOR_LEASE_DISPATCH_SCHEMA) fail('TRANSPORT_LEASE_INVALID');
  let lease: LeaseEnvelope;
  try {
    lease = parseLeaseEnvelope(source.lease);
  } catch {
    fail('TRANSPORT_LEASE_INVALID');
  }
  return {
    schema_version: CONTRIBUTOR_LEASE_DISPATCH_SCHEMA,
    dispatch_id: boundedIdentifier(source.dispatch_id),
    request_id: boundedRequestId(source.request_id),
    idempotency_key: boundedIdempotencyKey(source.idempotency_key),
    node_id: boundedIdentifier(source.node_id),
    lease,
    signature_b64: decodeSignature(source.signature_b64).toString('base64url'),
  };
}

export function verifyLeaseDispatch(value: unknown, controllerPublicKey: PublicKeyInput, expectedNodeId?: string, expectedControllerKeyId?: string): LeaseDispatch {
  const dispatch = parseDispatch(value);
  if (!verifyPayload(dispatchPayload(dispatch), dispatch.signature_b64, controllerPublicKey)) fail('TRANSPORT_SIGNATURE_INVALID');
  if (expectedNodeId !== undefined && dispatch.node_id !== expectedNodeId) fail('TRANSPORT_NODE_KEY_MISMATCH');
  if (dispatch.lease.node_id !== dispatch.node_id) fail('TRANSPORT_LEASE_INVALID');
  if (expectedControllerKeyId !== undefined && dispatch.lease.key_id !== expectedControllerKeyId) fail('TRANSPORT_KEY_ID_INVALID');
  const { signature_b64: _leaseSignature, ...leasePayloadValue } = dispatch.lease;
  if (!verifyPayload(leasePayloadValue, dispatch.lease.signature_b64, controllerPublicKey)) fail('TRANSPORT_SIGNATURE_INVALID');
  return dispatch;
}

function resultSubmissionPayload(value: ResultSubmission | ResultSubmissionPayload): ResultSubmissionPayload {
  return {
    schema_version: CONTRIBUTOR_RESULT_SUBMISSION_SCHEMA,
    dispatch_id: value.dispatch_id,
    request_id: value.request_id,
    idempotency_key: value.idempotency_key,
    node_id: value.node_id,
    result: value.result,
  };
}

function parseResultSubmission(value: unknown): ResultSubmission {
  const source = asRecord(value);
  if (!source || !exactKeys(source, [
    'schema_version', 'dispatch_id', 'request_id', 'idempotency_key', 'node_id', 'result', 'signature_b64',
  ]) || source.schema_version !== CONTRIBUTOR_RESULT_SUBMISSION_SCHEMA) fail('TRANSPORT_RESULT_INVALID');
  if (!asRecord(source.result)) fail('TRANSPORT_RESULT_INVALID');
  return {
    schema_version: CONTRIBUTOR_RESULT_SUBMISSION_SCHEMA,
    dispatch_id: boundedIdentifier(source.dispatch_id),
    request_id: boundedRequestId(source.request_id),
    idempotency_key: boundedIdempotencyKey(source.idempotency_key),
    node_id: boundedIdentifier(source.node_id),
    result: source.result as ResultEnvelope,
    signature_b64: decodeSignature(source.signature_b64).toString('base64url'),
  };
}

/** Strict parser for the node-signed result envelope before DB binding. */
export function parseContributorResultSubmission(value: unknown): ResultSubmission {
  return parseResultSubmission(value);
}

export function signResultSubmission(
  input: ResultSubmissionPayload,
  nodePrivateKey: KeyObject,
): ResultSubmission {
  const payload = resultSubmissionPayload(input);
  return { ...payload, signature_b64: signPayload(payload, nodePrivateKey) };
}

export function verifyResultSubmission(
  value: unknown,
  nodePublicKey: PublicKeyInput,
  expectedNodeId?: string,
  expectedNodeKeyId?: string,
): ResultSubmission {
  const submission = parseResultSubmission(value);
  if (expectedNodeId !== undefined && submission.node_id !== expectedNodeId) fail('TRANSPORT_NODE_KEY_MISMATCH');
  const result = submission.result;
  if (result.node_id !== submission.node_id || (expectedNodeKeyId !== undefined && result.key_id !== expectedNodeKeyId)) {
    fail('TRANSPORT_RESULT_INVALID');
  }
  try {
    verifyResultEnvelope(result, nodePublicKey, submission.node_id);
  } catch {
    fail('TRANSPORT_SIGNATURE_INVALID');
  }
  if (!verifyPayload(resultSubmissionPayload(submission), submission.signature_b64, nodePublicKey)) {
    fail('TRANSPORT_SIGNATURE_INVALID');
  }
  return submission;
}

function resultReceiptPayload(value: ResultReceipt | ResultReceiptPayload): ResultReceiptPayload {
  return {
    schema_version: CONTRIBUTOR_RESULT_RECEIPT_SCHEMA,
    dispatch_id: value.dispatch_id,
    request_id: value.request_id,
    idempotency_key: value.idempotency_key,
    node_id: value.node_id,
    lease_id: value.lease_id,
    result_hash: value.result_hash,
    status: 'accepted',
    accepted_at: value.accepted_at,
  };
}

function parseResultReceipt(value: unknown): ResultReceipt {
  const source = asRecord(value);
  if (!source || !exactKeys(source, [
    'schema_version', 'dispatch_id', 'request_id', 'idempotency_key', 'node_id', 'lease_id',
    'result_hash', 'status', 'accepted_at', 'signature_b64',
  ]) || source.schema_version !== CONTRIBUTOR_RESULT_RECEIPT_SCHEMA || source.status !== 'accepted') {
    fail('TRANSPORT_SCHEMA_INVALID');
  }
  return {
    schema_version: CONTRIBUTOR_RESULT_RECEIPT_SCHEMA,
    dispatch_id: boundedIdentifier(source.dispatch_id),
    request_id: boundedRequestId(source.request_id),
    idempotency_key: boundedIdempotencyKey(source.idempotency_key),
    node_id: boundedIdentifier(source.node_id),
    lease_id: boundedIdentifier(source.lease_id),
    result_hash: boundedHash(source.result_hash),
    status: 'accepted',
    accepted_at: boundedInteger(source.accepted_at, 0, 9_000_000_000_000),
    signature_b64: decodeSignature(source.signature_b64).toString('base64url'),
  };
}

export function verifyResultReceipt(value: unknown, controllerPublicKey: PublicKeyInput): ResultReceipt {
  const receipt = parseResultReceipt(value);
  if (!verifyPayload(resultReceiptPayload(receipt), receipt.signature_b64, controllerPublicKey)) fail('TRANSPORT_SIGNATURE_INVALID');
  return receipt;
}

function revokePayload(value: RevokeRequest | RevokeRequestPayload): RevokeRequestPayload {
  return {
    schema_version: CONTRIBUTOR_REVOKE_SCHEMA,
    request_id: value.request_id,
    node_id: value.node_id,
    reason: value.reason,
    issued_at: value.issued_at,
    expires_at: value.expires_at,
    operator_key_id: value.operator_key_id,
  };
}

function parseRevoke(value: unknown): RevokeRequest {
  const source = asRecord(value);
  if (!source || !exactKeys(source, [
    'schema_version', 'request_id', 'node_id', 'reason', 'issued_at', 'expires_at', 'operator_key_id', 'signature_b64',
  ]) || source.schema_version !== CONTRIBUTOR_REVOKE_SCHEMA) fail('TRANSPORT_SCHEMA_INVALID');
  const reason = source.reason;
  if (typeof reason !== 'string' || !REASON.test(reason)) fail('TRANSPORT_SCHEMA_INVALID');
  return {
    schema_version: CONTRIBUTOR_REVOKE_SCHEMA,
    request_id: boundedRequestId(source.request_id),
    node_id: boundedIdentifier(source.node_id),
    reason,
    issued_at: boundedInteger(source.issued_at, 0, 9_000_000_000_000),
    expires_at: boundedInteger(source.expires_at, 0, 9_000_000_000_000),
    operator_key_id: boundedKeyId(source.operator_key_id),
    signature_b64: decodeSignature(source.signature_b64).toString('base64url'),
  };
}

export function signRevokeRequest(input: RevokeRequestPayload, operatorPrivateKey: KeyObject): RevokeRequest {
  const payload = revokePayload(input);
  return { ...payload, signature_b64: signPayload(payload, operatorPrivateKey) };
}

export function verifyRevokeRequest(
  value: unknown,
  operatorPublicKey: PublicKeyInput,
  expectedOperatorKeyId: string,
  now = Date.now(),
): RevokeRequest {
  const request = parseRevoke(value);
  if (request.operator_key_id !== expectedOperatorKeyId) fail('TRANSPORT_KEY_ID_INVALID');
  temporal(request.issued_at, request.expires_at, now, CONTRIBUTOR_ENROLLMENT_TTL_MS, 'TRANSPORT_ENROLLMENT_EXPIRED');
  if (!verifyPayload(revokePayload(request), request.signature_b64, operatorPublicKey)) fail('TRANSPORT_SIGNATURE_INVALID');
  return request;
}

function revokeReceiptPayload(value: RevokeReceipt | RevokeReceiptPayload): RevokeReceiptPayload {
  return {
    schema_version: CONTRIBUTOR_REVOKE_RECEIPT_SCHEMA,
    request_id: value.request_id,
    node_id: value.node_id,
    controller_key_id: value.controller_key_id,
    status: value.status,
    revision: value.revision,
    reason: value.reason,
    revoked_at: value.revoked_at,
  };
}

function parseRevokeReceipt(value: unknown): RevokeReceipt {
  const source = asRecord(value);
  if (!source || !exactKeys(source, [
    'schema_version', 'request_id', 'node_id', 'controller_key_id', 'status', 'revision', 'reason', 'revoked_at', 'signature_b64',
  ]) || source.schema_version !== CONTRIBUTOR_REVOKE_RECEIPT_SCHEMA
    || (source.status !== 'revoked' && source.status !== 'already_revoked')
    || typeof source.reason !== 'string' || !REASON.test(source.reason)) {
    fail('TRANSPORT_SCHEMA_INVALID');
  }
  return {
    schema_version: CONTRIBUTOR_REVOKE_RECEIPT_SCHEMA,
    request_id: boundedRequestId(source.request_id),
    node_id: boundedIdentifier(source.node_id),
    controller_key_id: boundedKeyId(source.controller_key_id),
    status: source.status as 'revoked' | 'already_revoked',
    revision: boundedInteger(source.revision, 1, 1_000_000_000),
    reason: source.reason,
    revoked_at: boundedInteger(source.revoked_at, 0, 9_000_000_000_000),
    signature_b64: decodeSignature(source.signature_b64).toString('base64url'),
  };
}

export function verifyRevokeReceipt(value: unknown, controllerPublicKey: PublicKeyInput): RevokeReceipt {
  const receipt = parseRevokeReceipt(value);
  if (!verifyPayload(revokeReceiptPayload(receipt), receipt.signature_b64, controllerPublicKey)) fail('TRANSPORT_SIGNATURE_INVALID');
  return receipt;
}

export interface ContributorTransportControllerOptions {
  readonly controllerKeyId: string;
  readonly controllerPrivateKey: KeyObject;
  readonly operatorKeyId?: string;
  readonly operatorPublicKey?: PublicKeyInput;
  readonly store: ContributorTransportStore;
  readonly now?: () => number;
}

export class ContributorTransportController {
  readonly controllerKeyId: string;
  private readonly controllerPrivateKey: KeyObject;
  private readonly operatorKeyId: string | null;
  private readonly operatorPublicKey: KeyObject | null;
  private readonly store: ContributorTransportStore;
  private readonly now: () => number;

  constructor(options: ContributorTransportControllerOptions) {
    this.controllerKeyId = boundedKeyId(options.controllerKeyId);
    this.controllerPrivateKey = ed25519PrivateKey(options.controllerPrivateKey);
    this.operatorKeyId = options.operatorKeyId === undefined ? null : boundedKeyId(options.operatorKeyId);
    this.operatorPublicKey = options.operatorPublicKey === undefined ? null : ed25519PublicKey(options.operatorPublicKey);
    if (!options.store || typeof options.store.transaction !== 'function') fail('TRANSPORT_STORE_UNAVAILABLE');
    this.store = options.store;
    this.now = options.now ?? (() => Date.now());
  }

  async enroll(value: unknown): Promise<EnrollmentReceipt> {
    const request = verifyEnrollmentRequest(value, this.now());
    const requestHash = hashPayload(enrollmentPayload(request));
    return this.store.transaction(async () => {
      const replay = await this.store.getEnrollmentReplay(request.request_id);
      if (replay) {
        if (replay.request_hash !== requestHash) fail('TRANSPORT_ENROLLMENT_REPLAY');
        return replay.receipt;
      }
      const existing = await this.store.getNode(request.node_id);
      if (existing?.status === 'revoked') fail('TRANSPORT_NODE_REVOKED');
      if (existing && (existing.node_key_id !== request.node_key_id
        || existing.node_public_key_spki_b64 !== request.node_public_key_spki_b64)) {
        fail('TRANSPORT_NODE_KEY_MISMATCH');
      }
      const revision = (existing?.revision ?? 0) + 1;
      const issuedAt = this.now();
      const receiptPayloadValue: EnrollmentReceiptPayload = {
        schema_version: CONTRIBUTOR_ENROLLMENT_RECEIPT_SCHEMA,
        request_id: request.request_id,
        enrollment_id: `enr-${requestHash.slice(0, 48)}`,
        node_id: request.node_id,
        node_key_id: request.node_key_id,
        controller_key_id: this.controllerKeyId,
        status: 'active',
        revision,
        request_hash: requestHash,
        issued_at: issuedAt,
        expires_at: request.expires_at,
      };
      const receipt: EnrollmentReceipt = {
        ...receiptPayloadValue,
        signature_b64: signPayload(receiptPayloadValue, this.controllerPrivateKey),
      };
      const node: ContributorNodeRecord = {
        node_id: request.node_id,
        node_key_id: request.node_key_id,
        node_public_key_spki_b64: request.node_public_key_spki_b64,
        platform: request.platform,
        capabilities: [...request.capabilities],
        status: 'active',
        revision,
        enrolled_at: existing?.enrolled_at ?? issuedAt,
        revoked_at: null,
        revoke_reason: null,
      };
      await this.store.putNode(node);
      await this.store.putEnrollmentReplay(request.request_id, { request_hash: requestHash, receipt });
      return receipt;
    });
  }

  async issueLease(value: unknown): Promise<LeaseDispatch> {
    const request = parseLeaseRequest(value);
    const now = this.now();
    temporal(request.issued_at, request.expires_at, now, CONTRIBUTOR_ENROLLMENT_TTL_MS, 'TRANSPORT_ENROLLMENT_EXPIRED');
    const requestHash = hashPayload(leaseRequestPayload(request));
    return this.store.transaction(async () => {
      const node = await this.store.getNode(request.node_id);
      if (!node) fail('TRANSPORT_NODE_NOT_ENROLLED');
      if (node.status === 'revoked') fail('TRANSPORT_NODE_REVOKED');
      if (!node.capabilities.includes('vector_dot')) fail('TRANSPORT_LEASE_INVALID');
      const replay = await this.store.getLeaseReplay(request.node_id, request.idempotency_key);
      if (replay) {
        if (replay.request_hash !== requestHash) fail('TRANSPORT_IDEMPOTENCY_CONFLICT');
        return replay.dispatch;
      }
      const leaseId = `lease-${requestHash.slice(0, 56)}`;
      let lease: LeaseEnvelope;
      try {
        lease = signLease({
          schema_version: 'apocrypha.contributor.lease.v1',
          key_id: this.controllerKeyId,
          lease_id: leaseId,
          node_id: request.node_id,
          issued_at: request.issued_at,
          expires_at: request.expires_at,
          attempt: request.attempt,
          task: request.task,
        } satisfies LeaseInput, this.controllerPrivateKey);
      } catch {
        fail('TRANSPORT_LEASE_INVALID');
      }
      const payload: LeaseDispatchPayload = {
        schema_version: CONTRIBUTOR_LEASE_DISPATCH_SCHEMA,
        dispatch_id: `dispatch-${requestHash.slice(0, 48)}`,
        request_id: request.request_id,
        idempotency_key: request.idempotency_key,
        node_id: request.node_id,
        lease,
      };
      const dispatch: LeaseDispatch = { ...payload, signature_b64: signPayload(payload, this.controllerPrivateKey) };
      await this.store.putLeaseReplay(request.node_id, request.idempotency_key, { request_hash: requestHash, dispatch });
      return dispatch;
    });
  }

  async acceptResult(value: unknown): Promise<ResultReceipt> {
    const submission = parseResultSubmission(value);
    return this.store.transaction(async () => {
      const node = await this.store.getNode(submission.node_id);
      if (!node) fail('TRANSPORT_NODE_NOT_ENROLLED');
      if (node.status === 'revoked') fail('TRANSPORT_NODE_REVOKED');
      const leaseReplay = await this.store.getLeaseByDispatch(submission.dispatch_id);
      if (!leaseReplay) fail('TRANSPORT_LEASE_UNKNOWN');
      const dispatch = leaseReplay.dispatch;
      if (dispatch.request_id !== submission.request_id
        || dispatch.idempotency_key !== submission.idempotency_key
        || dispatch.node_id !== submission.node_id
        || submission.result.lease_id !== dispatch.lease.lease_id
        || submission.result.attempt !== dispatch.lease.attempt) {
        fail('TRANSPORT_RESULT_INVALID');
      }
      if (submission.result.started_at + CONTRIBUTOR_CLOCK_SKEW_MS < dispatch.lease.issued_at
        || submission.result.finished_at > dispatch.lease.expires_at + CONTRIBUTOR_CLOCK_SKEW_MS) {
        fail('TRANSPORT_RESULT_INVALID');
      }
      try {
        verifyResultSubmission(submission, publicKeyFromSpki(node.node_public_key_spki_b64), node.node_id, node.node_key_id);
      } catch (error) {
        if (error instanceof ContributorTransportError) throw error;
        fail('TRANSPORT_RESULT_INVALID');
      }
      const resultHash = hashPayload(submission.result);
      const submissionHash = hashPayload(resultSubmissionPayload(submission));
      const replay = await this.store.getResultReplay(submission.dispatch_id);
      if (replay) {
        if (replay.submission_hash !== submissionHash) fail('TRANSPORT_RESULT_REPLAY');
        return replay.receipt;
      }
      const acceptedAt = this.now();
      const payload: ResultReceiptPayload = {
        schema_version: CONTRIBUTOR_RESULT_RECEIPT_SCHEMA,
        dispatch_id: submission.dispatch_id,
        request_id: submission.request_id,
        idempotency_key: submission.idempotency_key,
        node_id: submission.node_id,
        lease_id: submission.result.lease_id,
        result_hash: resultHash,
        status: 'accepted',
        accepted_at: acceptedAt,
      };
      const receipt: ResultReceipt = { ...payload, signature_b64: signPayload(payload, this.controllerPrivateKey) };
      await this.store.putResultReplay(submission.dispatch_id, { submission_hash: submissionHash, receipt });
      return receipt;
    });
  }

  async revoke(value: unknown): Promise<RevokeReceipt> {
    if (!this.operatorKeyId || !this.operatorPublicKey) fail('TRANSPORT_REVOCATION_UNCONFIGURED');
    const request = verifyRevokeRequest(value, this.operatorPublicKey, this.operatorKeyId, this.now());
    const requestHash = hashPayload(revokePayload(request));
    return this.store.transaction(async () => {
      const replay = await this.store.getRevokeReplay(request.request_id);
      if (replay) {
        if (replay.request_hash !== requestHash) fail('TRANSPORT_IDEMPOTENCY_CONFLICT');
        return replay.receipt;
      }
      const existing = await this.store.getNode(request.node_id);
      if (!existing) fail('TRANSPORT_NODE_NOT_ENROLLED');
      const now = this.now();
      const alreadyRevoked = existing.status === 'revoked';
      const revision = existing.revision + (alreadyRevoked ? 0 : 1);
      const receiptPayloadValue: RevokeReceiptPayload = {
        schema_version: CONTRIBUTOR_REVOKE_RECEIPT_SCHEMA,
        request_id: request.request_id,
        node_id: request.node_id,
        controller_key_id: this.controllerKeyId,
        status: alreadyRevoked ? 'already_revoked' : 'revoked',
        revision,
        reason: request.reason,
        revoked_at: existing.revoked_at ?? now,
      };
      const receipt: RevokeReceipt = {
        ...receiptPayloadValue,
        signature_b64: signPayload(receiptPayloadValue, this.controllerPrivateKey),
      };
      if (!alreadyRevoked) {
        await this.store.putNode({
          ...existing,
          status: 'revoked',
          revision,
          revoked_at: receipt.revoked_at,
          revoke_reason: request.reason,
        });
      }
      await this.store.putRevokeReplay(request.request_id, { request_hash: requestHash, receipt });
      return receipt;
    });
  }
}

/** In-memory transactional store for focused unit tests only. */
export class MemoryContributorTransportStore implements ContributorTransportStore {
  private readonly nodes = new Map<string, ContributorNodeRecord>();
  private readonly enrollmentReplays = new Map<string, EnrollmentReplay>();
  private readonly revokeReplays = new Map<string, RevokeReplay>();
  private readonly leaseReplays = new Map<string, LeaseReplay>();
  private readonly dispatches = new Map<string, LeaseReplay>();
  private readonly resultReplays = new Map<string, ResultReplay>();
  private tail: Promise<void> = Promise.resolve();

  async transaction<T>(operation: () => Promise<T>): Promise<T> {
    const prior = this.tail;
    let release!: () => void;
    this.tail = new Promise<void>((resolve) => { release = resolve; });
    await prior;
    try {
      return await operation();
    } finally {
      release();
    }
  }

  async getNode(nodeId: string): Promise<ContributorNodeRecord | null> { return this.nodes.get(nodeId) ?? null; }
  async putNode(node: ContributorNodeRecord): Promise<void> { this.nodes.set(node.node_id, node); }
  async getEnrollmentReplay(requestId: string): Promise<EnrollmentReplay | null> { return this.enrollmentReplays.get(requestId) ?? null; }
  async putEnrollmentReplay(requestId: string, replay: EnrollmentReplay): Promise<void> { this.enrollmentReplays.set(requestId, replay); }
  async getRevokeReplay(requestId: string): Promise<RevokeReplay | null> { return this.revokeReplays.get(requestId) ?? null; }
  async putRevokeReplay(requestId: string, replay: RevokeReplay): Promise<void> { this.revokeReplays.set(requestId, replay); }
  async getLeaseReplay(nodeId: string, idempotencyKey: string): Promise<LeaseReplay | null> {
    return this.leaseReplays.get(`${nodeId}\n${idempotencyKey}`) ?? null;
  }
  async getLeaseByDispatch(dispatchId: string): Promise<LeaseReplay | null> { return this.dispatches.get(dispatchId) ?? null; }
  async putLeaseReplay(nodeId: string, idempotencyKey: string, replay: LeaseReplay): Promise<void> {
    this.leaseReplays.set(`${nodeId}\n${idempotencyKey}`, replay);
    this.dispatches.set(replay.dispatch.dispatch_id, replay);
  }
  async getResultReplay(dispatchId: string): Promise<ResultReplay | null> { return this.resultReplays.get(dispatchId) ?? null; }
  async putResultReplay(dispatchId: string, replay: ResultReplay): Promise<void> { this.resultReplays.set(dispatchId, replay); }
}
