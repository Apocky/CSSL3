import { createHash, createPrivateKey, createPublicKey, randomUUID, timingSafeEqual, type KeyObject } from 'node:crypto';

import type { NextApiRequest, NextApiResponse } from 'next';

import { getAdminAuthorization, type AdminAuthorizationResult } from '@/lib/admin-auth';
import { envelope, logHit } from '@/lib/response';

import {
  ContributorTransportController,
  ContributorTransportError,
  type EnrollmentReceipt,
  type LeaseDispatch,
  type RevokeReceipt,
  type ResultReceipt,
} from '@/lib/apocrypha/contributor-transport';
import {
  createSupabaseContributorTransportStore,
  type ContributorTransportAtomicEnrollmentInput,
  type ContributorTransportAtomicLeaseInput,
  type ContributorTransportAtomicOperations,
  type ContributorTransportAtomicResultInput,
  type ContributorTransportAtomicRevokeInput,
  type ContributorTransportStoreAvailability,
} from '@/lib/apocrypha/contributor-transport-supabase';
import { CONTRIBUTOR_NODE_MANIFEST } from '@/lib/apocrypha/contributor-node';

/**
 * HTTP boundary for the contributor transport.
 *
 * The default production path is intentionally fail-closed.  A controller is
 * available only when an Ed25519 signing key, a durable transactional store,
 * and (for internal lease issuance) a controller bearer-token digest exist.
 * Tests may inject a fully constructed controller; production never selects
 * MemoryContributorTransportStore or a generated/default key.
 */

export const CONTRIBUTOR_HTTP_MAX_BODY_BYTES = 128 * 1024;
export const CONTRIBUTOR_HTTP_ROUTE_SCHEMA = 'apocrypha.contributor.http.v1' as const;

export type ContributorEndpoint = 'enroll' | 'lease' | 'result' | 'revoke' | 'status';

export interface ContributorRateLimitRequest {
  readonly endpoint: ContributorEndpoint;
  readonly key: string;
  readonly method: string;
}

export interface ContributorRateLimitDecision {
  readonly allowed: boolean;
  readonly retry_after_seconds?: number;
}

/** A real deployment must inject a durable/global limiter; no local default. */
export interface ContributorRateLimiter {
  check(input: ContributorRateLimitRequest): ContributorRateLimitDecision | Promise<ContributorRateLimitDecision>;
}

/**
 * Controller preparation is deliberately explicit: the RPC adapter persists
 * an already-validated, controller-signed operation envelope.  This seam is
 * the only place allowed to turn an HTTP request into one of those envelopes.
 * The default production resolver never invents a preparer or a key.
 */
export interface ContributorAtomicRouteController {
  readonly atomicRpcCapable: true;
  /** True only when the adapter also has a generic callback transaction. */
  readonly genericTransactionCapable: boolean;
  readonly controllerSigningConfigured: boolean;
  readonly operatorConfigured: boolean;
  readonly operations: ContributorTransportAtomicOperations;
  readonly prepare: {
    enrollment(value: unknown): Promise<ContributorTransportAtomicEnrollmentInput>;
    lease(value: unknown): Promise<ContributorTransportAtomicLeaseInput>;
    result(value: unknown): Promise<ContributorTransportAtomicResultInput>;
    revoke(value: unknown): Promise<ContributorTransportAtomicRevokeInput>;
  };
}

export interface ContributorRouteDependencies {
  /** Test seam only; never populated by the default production resolver. */
  readonly controller?: ContributorTransportController;
  /**
   * Test seam or a separately-bound production controller.  The controller
   * must prepare/sign each request and call the operation-level Supabase RPC;
   * a generic transaction wrapper is not accepted as an atomic substitute.
   */
  readonly atomicController?: ContributorAtomicRouteController;
  /** Test seam or a real externally-owned abuse-control adapter. */
  readonly rateLimiter?: ContributorRateLimiter;
  /** SHA-256 of the internal controller bearer token; raw token is never read from this field. */
  readonly controllerTokenSha256?: string;
  /** Test seam for the existing verified owner/admin session boundary. */
  readonly authorize?: (req: NextApiRequest) => Promise<AdminAuthorizationResult>;
}

interface ContributorErrorBody {
  readonly ok: false;
  readonly code: string;
  readonly error: string;
  readonly transport_code?: string;
  readonly correlation_id: string;
  readonly served_by: string;
  readonly ts: string;
}

interface ContributorSuccessBody<T> {
  readonly ok: true;
  readonly result?: T;
  readonly status?: ContributorStatusPayload;
  readonly correlation_id: string;
  readonly served_by: string;
  readonly ts: string;
}

export interface ContributorStatusPayload {
  readonly schema_version: typeof CONTRIBUTOR_HTTP_ROUTE_SCHEMA;
  readonly release_state: typeof CONTRIBUTOR_NODE_MANIFEST.release_state;
  readonly release_gate: typeof CONTRIBUTOR_NODE_MANIFEST.release_gate;
  readonly transport_state: 'closed' | 'configured';
  readonly controller_signing_configured: boolean;
  readonly transactional_store_configured: boolean;
  /** Supabase client exposes the operation-RPC surface; migration/function
   * existence is not claimed until a real operation call succeeds. */
  readonly atomic_rpc_capable: boolean;
  readonly atomic_controller_configured: boolean;
  readonly controller_auth_configured: boolean;
  readonly operator_revocation_configured: boolean;
  readonly rate_limiter_configured: boolean;
  readonly mutating_routes_enabled: false | true;
  readonly public_metadata_only: true;
}

interface ConfiguredController {
  readonly controller?: ContributorTransportController;
  readonly atomicController?: ContributorAtomicRouteController;
  readonly operatorConfigured: boolean;
  readonly transactionalStore: boolean;
  readonly atomicRpcCapable: boolean;
}

class ContributorHttpError extends Error {
  readonly status: 400 | 401 | 403 | 404 | 405 | 409 | 429 | 503;
  readonly code: string;
  readonly retryAfterSeconds: number | null;
  readonly transportCode: string | null;

  constructor(
    status: ContributorHttpError['status'],
    code: string,
    message: string,
    options: { readonly retryAfterSeconds?: number; readonly transportCode?: string } = {},
  ) {
    super(message);
    this.name = 'ContributorHttpError';
    this.status = status;
    this.code = code;
    this.retryAfterSeconds = options.retryAfterSeconds ?? null;
    this.transportCode = options.transportCode ?? null;
  }
}

function firstHeader(value: string | string[] | undefined): string | null {
  const candidate = Array.isArray(value) ? value[0] : value;
  return typeof candidate === 'string' && candidate.length > 0 ? candidate : null;
}

function invalidTransport(message = 'request body or transport headers are invalid'): ContributorHttpError {
  return new ContributorHttpError(400, 'invalid_transport', message);
}

function safeEndpointName(endpoint: ContributorEndpoint): string {
  return `apocrypha.contributor.${endpoint}`;
}

function secureHeaders(
  res: NextApiResponse,
  endpoint: ContributorEndpoint,
  correlationId: string,
): void {
  res.setHeader('Cache-Control', 'private, no-store, max-age=0');
  res.setHeader('Vary', 'Authorization, Content-Type, X-Forwarded-For');
  res.setHeader('X-Content-Type-Options', 'nosniff');
  res.setHeader('X-Apocrypha-Route', safeEndpointName(endpoint));
  res.setHeader('X-Apocrypha-Schema', CONTRIBUTOR_HTTP_ROUTE_SCHEMA);
  res.setHeader('X-Correlation-ID', correlationId);
}

function pathHasUnexpectedQuery(req: NextApiRequest): boolean {
  return Object.keys(req.query ?? {}).length > 0;
}

function ensureJsonContentType(req: NextApiRequest): void {
  const contentType = firstHeader(req.headers['content-type']);
  if (!contentType || !/^application\/json\s*(?:;|$)/i.test(contentType)) {
    throw invalidTransport('application/json content type required');
  }
  const contentEncoding = firstHeader(req.headers['content-encoding']);
  if (contentEncoding && contentEncoding.toLowerCase() !== 'identity') {
    throw invalidTransport('compressed request bodies are not accepted');
  }
}

function declaredLength(req: NextApiRequest): number | null {
  const value = firstHeader(req.headers['content-length']);
  if (value === null) return null;
  if (!/^(?:0|[1-9]\d*)$/.test(value)) throw invalidTransport('content-length is invalid');
  const length = Number(value);
  if (!Number.isSafeInteger(length) || length < 1 || length > CONTRIBUTOR_HTTP_MAX_BODY_BYTES) {
    throw invalidTransport('request body exceeds the transport limit');
  }
  return length;
}

function decodeJson(raw: Buffer): unknown {
  if (raw.length < 2 || raw.length > CONTRIBUTOR_HTTP_MAX_BODY_BYTES) throw invalidTransport('request body is empty or too large');
  let text: string;
  try {
    text = new TextDecoder('utf-8', { fatal: true }).decode(raw);
  } catch {
    throw invalidTransport('request body is not valid UTF-8');
  }
  let value: unknown;
  try {
    value = JSON.parse(text) as unknown;
  } catch {
    throw invalidTransport('request body is not valid JSON');
  }
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    throw invalidTransport('request body must be a JSON object');
  }
  return value;
}

async function readBoundedJsonBody(req: NextApiRequest): Promise<unknown> {
  ensureJsonContentType(req);
  const expectedLength = declaredLength(req);

  // Direct invocation tests and a few framework adapters expose a parsed body
  // even when the Next body parser is disabled.  Re-serialize only to enforce
  // the same byte bound; controller parsers still perform exact schema checks.
  if (req.body !== undefined) {
    let raw: Buffer;
    if (Buffer.isBuffer(req.body)) raw = req.body;
    else if (typeof req.body === 'string') raw = Buffer.from(req.body, 'utf8');
    else {
      try {
        raw = Buffer.from(JSON.stringify(req.body), 'utf8');
      } catch {
        throw invalidTransport('request body cannot be serialized');
      }
    }
    if (expectedLength !== null && expectedLength !== raw.length) throw invalidTransport('content-length does not match body');
    return decodeJson(raw);
  }

  const iterable = req as unknown as { [Symbol.asyncIterator]?: () => AsyncIterator<Buffer | Uint8Array | string> };
  if (typeof iterable[Symbol.asyncIterator] !== 'function') throw invalidTransport('request body unavailable');
  const chunks: Buffer[] = [];
  let bytes = 0;
  for await (const chunk of iterable as AsyncIterable<Buffer | Uint8Array | string>) {
    const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
    bytes += buffer.length;
    if (bytes > CONTRIBUTOR_HTTP_MAX_BODY_BYTES) throw invalidTransport('request body exceeds the transport limit');
    chunks.push(buffer);
  }
  if (expectedLength !== null && expectedLength !== bytes) throw invalidTransport('content-length does not match body');
  return decodeJson(Buffer.concat(chunks, bytes));
}

function requestKey(req: NextApiRequest): string {
  const forwarded = firstHeader(req.headers['x-forwarded-for']);
  if (forwarded) return forwarded.split(',')[0]?.trim().slice(0, 128) || 'forwarded-unknown';
  const remote = req.socket?.remoteAddress;
  return typeof remote === 'string' && remote.length > 0 ? remote.slice(0, 128) : 'unknown-client';
}

async function enforceRateLimit(
  endpoint: ContributorEndpoint,
  req: NextApiRequest,
  dependencies: ContributorRouteDependencies,
): Promise<void> {
  const limiter = dependencies.rateLimiter;
  if (!limiter) {
    throw new ContributorHttpError(
      503,
      'transport_unconfigured',
      'contributor transport abuse controls are not configured',
      { retryAfterSeconds: 60 },
    );
  }
  let decision: ContributorRateLimitDecision;
  try {
    decision = await limiter.check({ endpoint, key: requestKey(req), method: req.method ?? 'unknown' });
  } catch {
    throw new ContributorHttpError(
      503,
      'transport_unconfigured',
      'contributor transport abuse controls are unavailable',
      { retryAfterSeconds: 60 },
    );
  }
  if (!decision || typeof decision.allowed !== 'boolean') {
    throw new ContributorHttpError(
      503,
      'transport_unconfigured',
      'contributor transport abuse controls returned no decision',
      { retryAfterSeconds: 60 },
    );
  }
  if (!decision.allowed) {
    const retryAfter = Number.isSafeInteger(decision.retry_after_seconds)
      && Number(decision.retry_after_seconds) >= 1
      && Number(decision.retry_after_seconds) <= 3_600
      ? Number(decision.retry_after_seconds)
      : 60;
    throw new ContributorHttpError(429, 'rate_limited', 'contributor transport request rate limited', {
      retryAfterSeconds: retryAfter,
    });
  }
}

function readEd25519PrivateKey(name: string): KeyObject | null {
  const pem = process.env[name];
  if (!pem || pem.length > 16_384) return null;
  try {
    const key = createPrivateKey(pem);
    return key.type === 'private' && key.asymmetricKeyType === 'ed25519' ? key : null;
  } catch {
    return null;
  }
}

function readEd25519PublicKey(name: string): KeyObject | null {
  const pem = process.env[name];
  if (!pem || pem.length > 16_384) return null;
  try {
    const key = createPublicKey(pem);
    return key.type === 'public' && key.asymmetricKeyType === 'ed25519' ? key : null;
  } catch {
    return null;
  }
}

function envKeyId(name: string): string | null {
  const value = process.env[name];
  return value && /^[A-Za-z0-9][A-Za-z0-9._:-]{2,127}$/.test(value) ? value : null;
}

function controllerTokenDigest(dependencies: ContributorRouteDependencies): string | null {
  const digest = dependencies.controllerTokenSha256 ?? process.env.APOCRYPHA_CONTRIBUTOR_CONTROLLER_TOKEN_SHA256;
  return digest && /^[0-9a-f]{64}$/.test(digest) ? digest : null;
}

function configuredController(): ConfiguredController | null {
  const controllerKeyId = envKeyId('APOCRYPHA_CONTRIBUTOR_CONTROLLER_KEY_ID');
  const controllerPrivateKey = readEd25519PrivateKey('APOCRYPHA_CONTRIBUTOR_CONTROLLER_PRIVATE_KEY_PEM');
  if (!controllerKeyId || !controllerPrivateKey) return null;

  const storeAvailability: ContributorTransportStoreAvailability = createSupabaseContributorTransportStore();
  if (!storeAvailability.ok || !storeAvailability.store.transactional) return null;

  const operatorKeyId = envKeyId('APOCRYPHA_CONTRIBUTOR_OPERATOR_KEY_ID');
  const operatorPublicKey = readEd25519PublicKey('APOCRYPHA_CONTRIBUTOR_OPERATOR_PUBLIC_KEY_PEM');
  try {
    const controller = new ContributorTransportController({
      controllerKeyId,
      controllerPrivateKey,
      ...(operatorKeyId && operatorPublicKey ? { operatorKeyId, operatorPublicKey } : {}),
      store: storeAvailability.store,
    });
    return {
      controller,
      operatorConfigured: Boolean(operatorKeyId && operatorPublicKey),
      transactionalStore: true,
      atomicRpcCapable: false,
    };
  } catch {
    return null;
  }
}

function configuredControllerForRoute(
  endpoint: ContributorEndpoint,
  dependencies: ContributorRouteDependencies,
): ConfiguredController {
  if (dependencies.atomicController) {
    if (!dependencies.atomicController.controllerSigningConfigured) {
      throw new ContributorHttpError(
        503,
        'transport_unconfigured',
        'contributor atomic controller signing is not configured',
        { retryAfterSeconds: 60 },
      );
    }
    if (!dependencies.atomicController.atomicRpcCapable) {
      throw new ContributorHttpError(
        503,
        'transport_unconfigured',
        'contributor atomic RPC persistence is not configured',
        { retryAfterSeconds: 60 },
      );
    }
    if (endpoint === 'revoke' && !dependencies.atomicController.operatorConfigured) {
      throw new ContributorHttpError(
        503,
        'transport_unconfigured',
        'contributor operator revocation key is not configured',
        { retryAfterSeconds: 60 },
      );
    }
    return {
      atomicController: dependencies.atomicController,
      operatorConfigured: dependencies.atomicController.operatorConfigured,
      transactionalStore: dependencies.atomicController.genericTransactionCapable,
      atomicRpcCapable: true,
    };
  }
  if (dependencies.controller) {
    return {
      controller: dependencies.controller,
      operatorConfigured: true,
      transactionalStore: true,
      atomicRpcCapable: false,
    };
  }
  const resolved = configuredController();
  if (!resolved) {
    throw new ContributorHttpError(
      503,
      'transport_unconfigured',
      'contributor controller signing and transactional persistence are not configured',
      { retryAfterSeconds: 60 },
    );
  }
  if (endpoint === 'revoke' && !resolved.operatorConfigured) {
    throw new ContributorHttpError(
      503,
      'transport_unconfigured',
      'contributor operator revocation key is not configured',
      { retryAfterSeconds: 60 },
    );
  }
  return resolved;
}

function verifyControllerBearer(req: NextApiRequest, dependencies: ContributorRouteDependencies): void {
  const expectedDigest = controllerTokenDigest(dependencies);
  if (!expectedDigest) {
    throw new ContributorHttpError(
      503,
      'transport_unconfigured',
      'contributor controller authentication is not configured',
      { retryAfterSeconds: 60 },
    );
  }
  const authorization = firstHeader(req.headers.authorization);
  if (!authorization || !authorization.startsWith('Bearer ')) {
    throw new ContributorHttpError(401, 'controller_auth_required', 'controller bearer authentication required');
  }
  const token = authorization.slice('Bearer '.length).trim();
  if (!token || token.length > 512) {
    throw new ContributorHttpError(401, 'controller_auth_invalid', 'controller bearer authentication invalid');
  }
  const actual = createHash('sha256').update(token, 'utf8').digest();
  const expected = Buffer.from(expectedDigest, 'hex');
  if (actual.length !== expected.length || !timingSafeEqual(actual, expected)) {
    throw new ContributorHttpError(401, 'controller_auth_invalid', 'controller bearer authentication invalid');
  }
}

async function verifyAdmin(req: NextApiRequest, dependencies: ContributorRouteDependencies): Promise<void> {
  let result: AdminAuthorizationResult;
  try {
    result = await (dependencies.authorize ?? getAdminAuthorization)(req);
  } catch {
    throw new ContributorHttpError(503, 'transport_unconfigured', 'owner authorization service unavailable', {
      retryAfterSeconds: 60,
    });
  }
  if (!result.authConfigured && result.failureKind === 'unconfigured') {
    throw new ContributorHttpError(503, 'transport_unconfigured', 'owner authorization is not configured', {
      retryAfterSeconds: 60,
    });
  }
  if (!result.user) throw new ContributorHttpError(401, 'admin_required', 'verified owner session required');
  if (!result.authorized) throw new ContributorHttpError(403, 'admin_denied', 'verified owner session is not authorized');
}

function mapTransportError(error: ContributorTransportError): ContributorHttpError {
  const code = error.code;
  if (code === 'TRANSPORT_STORE_UNAVAILABLE' || code === 'TRANSPORT_REVOCATION_UNCONFIGURED') {
    return new ContributorHttpError(503, 'transport_unconfigured', 'contributor transport is not configured', {
      retryAfterSeconds: 60,
      transportCode: code,
    });
  }
  if (code === 'TRANSPORT_SIGNATURE_INVALID' || code === 'TRANSPORT_PUBLIC_KEY_INVALID' || code === 'TRANSPORT_NODE_KEY_MISMATCH') {
    return new ContributorHttpError(401, 'transport_auth_invalid', 'contributor transport signature is invalid', {
      transportCode: code,
    });
  }
  if (code === 'TRANSPORT_NODE_REVOKED') {
    return new ContributorHttpError(403, 'node_revoked', 'contributor node is revoked', { transportCode: code });
  }
  if (code === 'TRANSPORT_NODE_NOT_ENROLLED' || code === 'TRANSPORT_LEASE_UNKNOWN') {
    return new ContributorHttpError(404, 'transport_not_found', 'contributor transport resource not found', {
      transportCode: code,
    });
  }
  if (
    code === 'TRANSPORT_ENROLLMENT_REPLAY'
    || code === 'TRANSPORT_IDEMPOTENCY_CONFLICT'
    || code === 'TRANSPORT_RESULT_REPLAY'
  ) {
    return new ContributorHttpError(409, 'transport_replay', 'contributor transport replay or idempotency conflict', {
      transportCode: code,
    });
  }
  return new ContributorHttpError(400, 'transport_invalid', 'contributor transport payload is invalid', {
    transportCode: code,
  });
}

function respondError(
  res: NextApiResponse<ContributorErrorBody>,
  endpoint: ContributorEndpoint,
  correlationId: string,
  error: unknown,
): void {
  const mapped = error instanceof ContributorHttpError
    ? error
    : error instanceof ContributorTransportError
      ? mapTransportError(error)
      : new ContributorHttpError(503, 'transport_unavailable', 'contributor transport is unavailable', { retryAfterSeconds: 60 });
  secureHeaders(res, endpoint, correlationId);
  if (mapped.retryAfterSeconds !== null) res.setHeader('Retry-After', String(mapped.retryAfterSeconds));
  const body: ContributorErrorBody = {
    ok: false,
    code: mapped.code,
    error: mapped.message,
    ...(mapped.transportCode ? { transport_code: mapped.transportCode } : {}),
    correlation_id: correlationId,
    ...envelope(),
  };
  logHit(safeEndpointName(endpoint), { method: 'response', status: mapped.status, code: mapped.code });
  res.status(mapped.status).json(body);
}

interface ContributorStoreCapabilities {
  readonly genericTransactionCapable: boolean;
  readonly atomicRpcCapable: boolean;
}

/**
 * Capability observation is intentionally side-effect free.  Supabase's
 * client always exposes `rpc`; this reports only that client surface, not
 * that migration 0054 is applied or that any operation RPC has succeeded.
 */
function productionStoreCapabilities(): ContributorStoreCapabilities {
  const availability = createSupabaseContributorTransportStore();
  if (!availability.ok) {
    return { genericTransactionCapable: false, atomicRpcCapable: false };
  }
  return {
    genericTransactionCapable: availability.store.transactional,
    atomicRpcCapable: availability.store.atomicRpcCapable,
  };
}

function statusPayload(dependencies: ContributorRouteDependencies): ContributorStatusPayload {
  const storeCapabilities = productionStoreCapabilities();
  const atomicController = dependencies.atomicController;
  const controllerKeyConfigured = Boolean(
    envKeyId('APOCRYPHA_CONTRIBUTOR_CONTROLLER_KEY_ID')
    && readEd25519PrivateKey('APOCRYPHA_CONTRIBUTOR_CONTROLLER_PRIVATE_KEY_PEM'),
  ) || Boolean(dependencies.controller) || Boolean(atomicController?.controllerSigningConfigured);
  const operatorConfigured = Boolean(
    envKeyId('APOCRYPHA_CONTRIBUTOR_OPERATOR_KEY_ID')
    && readEd25519PublicKey('APOCRYPHA_CONTRIBUTOR_OPERATOR_PUBLIC_KEY_PEM'),
  ) || Boolean(dependencies.controller) || Boolean(atomicController?.operatorConfigured);
  const storeConfigured = Boolean(dependencies.controller) || storeCapabilities.genericTransactionCapable;
  const atomicRpcCapable = Boolean(atomicController?.atomicRpcCapable) || storeCapabilities.atomicRpcCapable;
  const atomicControllerConfigured = Boolean(
    atomicController?.atomicRpcCapable
    && atomicController.controllerSigningConfigured,
  );
  const tokenConfigured = Boolean(controllerTokenDigest(dependencies));
  const limiterConfigured = Boolean(dependencies.rateLimiter);
  // Keep the injected generic controller as a unit-test-only compatibility
  // seam.  Production mutating capability requires an atomic controller plus
  // the external limiter and internal controller bearer configuration.
  const legacyTestEnabled = Boolean(dependencies.controller && dependencies.rateLimiter);
  const atomicEnabled = Boolean(
    atomicControllerConfigured
    && dependencies.rateLimiter
    && tokenConfigured,
  );
  const enabled = legacyTestEnabled || atomicEnabled;
  return {
    schema_version: CONTRIBUTOR_HTTP_ROUTE_SCHEMA,
    release_state: CONTRIBUTOR_NODE_MANIFEST.release_state,
    release_gate: CONTRIBUTOR_NODE_MANIFEST.release_gate,
    transport_state: enabled ? 'configured' : 'closed',
    controller_signing_configured: controllerKeyConfigured,
    transactional_store_configured: storeConfigured,
    atomic_rpc_capable: atomicRpcCapable,
    atomic_controller_configured: atomicControllerConfigured,
    controller_auth_configured: tokenConfigured,
    operator_revocation_configured: operatorConfigured,
    rate_limiter_configured: limiterConfigured,
    mutating_routes_enabled: enabled,
    public_metadata_only: true,
  };
}

function respondStatus(
  req: NextApiRequest,
  res: NextApiResponse<ContributorSuccessBody<never> | ContributorErrorBody>,
  dependencies: ContributorRouteDependencies,
  correlationId: string,
): void {
  secureHeaders(res, 'status', correlationId);
  if (req.method !== 'GET') {
    res.setHeader('Allow', 'GET');
    throw new ContributorHttpError(405, 'method_not_allowed', 'GET only');
  }
  if (pathHasUnexpectedQuery(req)) throw invalidTransport('status endpoint does not accept query parameters');
  const body: ContributorSuccessBody<never> = {
    ok: true,
    status: statusPayload(dependencies),
    correlation_id: correlationId,
    ...envelope(),
  };
  res.status(200).json(body);
}

async function dispatchEndpoint(
  endpoint: Exclude<ContributorEndpoint, 'status'>,
  req: NextApiRequest,
  dependencies: ContributorRouteDependencies,
): Promise<EnrollmentReceipt | LeaseDispatch | ResultReceipt | RevokeReceipt> {
  if (pathHasUnexpectedQuery(req)) throw invalidTransport('contributor transport endpoint does not accept query parameters');
  if (endpoint === 'revoke') await verifyAdmin(req, dependencies);
  if (endpoint === 'lease') verifyControllerBearer(req, dependencies);
  const body = await readBoundedJsonBody(req);
  await enforceRateLimit(endpoint, req, dependencies);
  const configured = configuredControllerForRoute(endpoint, dependencies);
  if (configured.atomicController) {
    if (endpoint === 'enroll') {
      return configured.atomicController.operations.atomicEnroll(
        await configured.atomicController.prepare.enrollment(body),
      );
    }
    if (endpoint === 'lease') {
      return configured.atomicController.operations.atomicIssueLease(
        await configured.atomicController.prepare.lease(body),
      );
    }
    if (endpoint === 'result') {
      return configured.atomicController.operations.atomicAcceptResult(
        await configured.atomicController.prepare.result(body),
      );
    }
    return configured.atomicController.operations.atomicRevoke(
      await configured.atomicController.prepare.revoke(body),
    );
  }
  if (!configured.controller) {
    throw new ContributorHttpError(
      503,
      'transport_unconfigured',
      'contributor transport controller is not configured',
      { retryAfterSeconds: 60 },
    );
  }
  if (endpoint === 'enroll') return configured.controller.enroll(body);
  if (endpoint === 'lease') return configured.controller.issueLease(body);
  if (endpoint === 'result') return configured.controller.acceptResult(body);
  return configured.controller.revoke(body);
}

export function createContributorRouteHandler(
  endpoint: ContributorEndpoint,
  dependencies: ContributorRouteDependencies = {},
): (req: NextApiRequest, res: NextApiResponse) => Promise<void> {
  return async function contributorRouteHandler(req, res): Promise<void> {
    const correlationId = randomUUID();
    logHit(safeEndpointName(endpoint), { method: req.method ?? 'unknown' });
    try {
      if (endpoint === 'status') {
        respondStatus(req, res, dependencies, correlationId);
        return;
      }
      const expectedMethod = 'POST';
      if (req.method !== expectedMethod) {
        res.setHeader('Allow', expectedMethod);
        throw new ContributorHttpError(405, 'method_not_allowed', 'POST only');
      }
      const result = await dispatchEndpoint(endpoint, req, dependencies);
      secureHeaders(res, endpoint, correlationId);
      const body: ContributorSuccessBody<typeof result> = {
        ok: true,
        result,
        correlation_id: correlationId,
        ...envelope(),
      };
      res.status(200).json(body);
    } catch (error) {
      respondError(res, endpoint, correlationId, error);
    }
  };
}
