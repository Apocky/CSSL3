// Server-only, admin-route transport for the authenticated Apocv4 runtime.

import { request as httpsRequest } from 'node:https';
import { isIP } from 'node:net';

export const APOCV4_PROXY_SCHEMA = 'apocky.apocv4-runtime-proxy.v1';

const RUNTIME_SCHEMA = 'apocv4.runtime-service.v1';
const TOKEN_RE = /^[A-Za-z0-9._~+/-]{1,8192}$/;
const SHA256_RE = /^[0-9a-f]{64}$/;
const TRACEPARENT_RE = /^00-([0-9a-f]{32})-([0-9a-f]{16})-01$/;
const PORT_RE = /^(?:[1-9][0-9]{0,3}|[1-5][0-9]{4}|6[0-4][0-9]{3}|65[0-4][0-9]{2}|655[0-2][0-9]|6553[0-5])$/;
const BASE64_RE = /^[A-Za-z0-9+/]+={0,2}$/;
const HEALTH_RESPONSE_LIMIT = 256 * 1024;
const CHAT_RESPONSE_LIMIT = 512 * 1024;
const OBJECTIVE_RESPONSE_LIMIT = 2 * 1024 * 1024;
const CODE_RESPONSE_LIMIT = 3 * 1024 * 1024;
const ROLLBACK_RESPONSE_LIMIT = 512 * 1024;
const HEALTH_DEADLINE_MS = 12_000;
const CHAT_DEADLINE_MS = 80_000;
export const RUNPOD_SYNC_DEADLINE_MS = 95_000;
export const RUNPOD_CODE_DEADLINE_MS = 240_000;
const ROLLBACK_DEADLINE_MS = 45_000;
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[45][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const CODE_PATH_RE = /^[A-Za-z0-9_.@+ -]+(?:\/[A-Za-z0-9_.@+ -]+)*$/;
const WINDOWS_RESERVED_PATH_STEMS = new Set([
  'CON', 'PRN', 'AUX', 'NUL',
  'COM1', 'COM2', 'COM3', 'COM4', 'COM5', 'COM6', 'COM7', 'COM8', 'COM9',
  'LPT1', 'LPT2', 'LPT3', 'LPT4', 'LPT5', 'LPT6', 'LPT7', 'LPT8', 'LPT9',
]);
const PROTECTED_CODE_PATHS = new Set([
  'AGENTS.md',
  'PRIME_DIRECTIVE.md',
  'env.txt',
  'src/apocv4/admission.py',
  'src/apocv4/authority_policy.py',
  'src/apocv4/effect_gateway.py',
  'src/apocv4/patch_effect_executor.py',
  'src/apocv4/patch_runtime.py',
  'src/apocv4/runtime_auth.py',
  'src/apocv4/runtime_service.py',
]);
const PROTECTED_CODE_PREFIXES = [
  '.staging/',
  'config/',
  'deploy/',
  'specs/wayfinder/',
];

type JsonObject = Record<string, unknown>;
export type RuntimeCredentialProfile = 'owner' | 'public';

export interface RuntimeChatAuthority {
  effect_authority: 'NONE';
  tool_authority: 'NONE' | 'READ_ONLY_CONTEXT';
  memory_scope: 'ephemeral' | 'owner_partitioned_retrieval' | 'public_safe_retrieval';
  conversation_history: 'not_retained' | 'session_bounded';
  training_consent: false;
}

export interface RuntimeChatIdentity {
  schema_version: 'apocv4.identity.v1';
  system_id: 'apocrypha';
  architecture: 'governed_hybrid_digital_intelligence';
  compiler_version: string;
  identity_digest: string;
  learned_model_role: 'replaceable_faculty_not_system_identity';
  lineage: string;
}

export interface RuntimeChatCapability {
  id: string;
  status: string;
  authority: string;
  evidence: string;
}

export interface RuntimeChatContext extends JsonObject {
  frame_id: string;
  frame_digest: string;
  provenance_spine_digest: string;
  retrieval: JsonObject & { status: string; count: number; refs: unknown[] };
  memory: JsonObject & {
    provider: string;
    status: string;
    records_used: number;
    receipt_digest: string | null;
    refs: unknown[];
  };
  capabilities: RuntimeChatCapability[];
}

export interface RuntimeReceipt {
  observed_at: string;
  latency_ms: number;
  upstream_status: number;
  auth_mode: string | null;
  auth_registry_ref: string | null;
  binding_ref: string | null;
  principal_ref: string | null;
  privacy_partition_ref: string | null;
  effect_scope_ref: string | null;
}

export interface RuntimeHealthProjection {
  schema_version: typeof APOCV4_PROXY_SCHEMA;
  kind: 'health';
  observed: {
    evidence_lane: 'observed_runtime_http';
    receipt: RuntimeReceipt;
    runtime: JsonObject;
  };
  model_reported: {
    evidence_lane: 'model_reported';
    present: false;
    note: string;
  };
}

export interface RuntimeObjectiveProjection {
  schema_version: typeof APOCV4_PROXY_SCHEMA;
  kind: 'objective';
  observed: {
    evidence_lane: 'observed_runtime_http_and_test_receipts';
    receipt: RuntimeReceipt;
    runtime: JsonObject;
    attempts: JsonObject[];
  };
  model_reported: {
    evidence_lane: 'model_reported_not_observed_fact';
    note: string;
    attempts: JsonObject[];
  };
}

export interface RuntimeChatProjection {
  schema_version: typeof APOCV4_PROXY_SCHEMA;
  kind: 'chat';
  observed: {
    evidence_lane: 'observed_runtime_http_and_transport';
    receipt: RuntimeReceipt;
    runtime: JsonObject;
  };
  model_reported: JsonObject & {
    evidence_lane: 'model_reported_not_observed_fact';
    text: string;
  };
  authority: RuntimeChatAuthority;
  identity: RuntimeChatIdentity | null;
  context: RuntimeChatContext | null;
}

export interface RuntimeChatInput {
  message: string;
  conversationId: string;
  requestId: string;
  privacyPartition: string;
  credentialProfile?: RuntimeCredentialProfile;
}

export interface RuntimeCodeInput {
  objective: string;
  allowedPaths: string[];
  privacyPartition: string;
}

export interface RuntimeCodeProjection {
  schema_version: typeof APOCV4_PROXY_SCHEMA;
  kind: 'code';
  observed: {
    evidence_lane: 'observed_runtime_http_effect_and_test_receipts';
    receipt: RuntimeReceipt;
    runtime: JsonObject;
    test: JsonObject | null;
  };
  generated: {
    evidence_lane: 'model_generated_artifact_identity_not_source_diff';
    proposal_digest: string;
    requested_allowed_paths: string[];
    faculty_attempts: JsonObject[];
  };
}

export interface RuntimeRollbackProjection {
  schema_version: typeof APOCV4_PROXY_SCHEMA;
  kind: 'rollback';
  observed: {
    evidence_lane: 'observed_runtime_http_rollback_receipt';
    receipt: RuntimeReceipt;
    runtime: JsonObject;
  };
  model_reported: {
    evidence_lane: 'model_reported';
    present: false;
    note: string;
  };
}

export class RuntimeProxyError extends Error {
  readonly code: string;
  readonly publicStatus: number;
  readonly upstreamStatus: number | null;
  readonly observedAt: string;
  readonly deadlineMs: number | null;

  constructor(
    code: string,
    publicStatus: number,
    upstreamStatus: number | null = null,
    deadlineMs: number | null = null,
  ) {
    super(code);
    this.name = 'RuntimeProxyError';
    this.code = code;
    this.publicStatus = publicStatus;
    this.upstreamStatus = upstreamStatus;
    this.observedAt = new Date().toISOString();
    this.deadlineMs = deadlineMs;
  }
}

interface RuntimeCall {
  data: JsonObject;
  receipt: RuntimeReceipt;
}

function isObject(value: unknown): value is JsonObject {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function exactKeys(value: JsonObject, expected: readonly string[]): boolean {
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  return actual.length === wanted.length && actual.every((key, index) => key === wanted[index]);
}

function boundedCanonicalString(value: unknown, maximumLength = 512): value is string {
  return typeof value === 'string'
    && value === value.trim()
    && value.length > 0
    && value.length <= maximumLength;
}

function boundedJsonValue(value: unknown, maximumBytes: number): boolean {
  try {
    const encoded = JSON.stringify(value);
    return typeof encoded === 'string' && Buffer.byteLength(encoded, 'utf8') <= maximumBytes;
  } catch {
    return false;
  }
}

function validateV2ChatIdentity(value: unknown): value is RuntimeChatIdentity {
  return isObject(value)
    && exactKeys(value, [
      'schema_version', 'system_id', 'architecture', 'compiler_version',
      'identity_digest', 'learned_model_role', 'lineage',
    ])
    && value.schema_version === 'apocv4.identity.v1'
    && value.system_id === 'apocrypha'
    && value.architecture === 'governed_hybrid_digital_intelligence'
    && boundedCanonicalString(value.compiler_version, 128)
    && typeof value.identity_digest === 'string'
    && SHA256_RE.test(value.identity_digest)
    && value.learned_model_role === 'replaceable_faculty_not_system_identity'
    && boundedCanonicalString(value.lineage, 128);
}

function validateV2ChatContext(value: unknown): value is RuntimeChatContext {
  if (!isObject(value) || !exactKeys(value, [
    'frame_id', 'frame_digest', 'provenance_spine_digest',
    'retrieval', 'memory', 'capabilities',
  ])) return false;
  const retrieval = value.retrieval;
  const memory = value.memory;
  const capabilities = value.capabilities;
  return boundedCanonicalString(value.frame_id, 256)
    && value.frame_id.startsWith('acf-')
    && typeof value.frame_digest === 'string'
    && SHA256_RE.test(value.frame_digest)
    && typeof value.provenance_spine_digest === 'string'
    && SHA256_RE.test(value.provenance_spine_digest)
    && isObject(retrieval)
    && exactKeys(retrieval, ['status', 'count', 'refs'])
    && boundedCanonicalString(retrieval.status, 128)
    && Number.isInteger(retrieval.count)
    && Number(retrieval.count) >= 0
    && Number(retrieval.count) <= 128
    && Array.isArray(retrieval.refs)
    && retrieval.refs.length <= 128
    && boundedJsonValue(retrieval.refs, 64 * 1024)
    && isObject(memory)
    && exactKeys(memory, ['provider', 'status', 'records_used', 'receipt_digest', 'refs'])
    && boundedCanonicalString(memory.provider, 128)
    && boundedCanonicalString(memory.status, 128)
    && Number.isInteger(memory.records_used)
    && Number(memory.records_used) >= 0
    && Number(memory.records_used) <= 128
    && (
      memory.receipt_digest === null
      || (typeof memory.receipt_digest === 'string' && SHA256_RE.test(memory.receipt_digest))
    )
    && Array.isArray(memory.refs)
    && memory.refs.length <= 128
    && boundedJsonValue(memory.refs, 64 * 1024)
    && Array.isArray(capabilities)
    && capabilities.length <= 64
    && capabilities.every((entry) => isObject(entry)
      && exactKeys(entry, ['id', 'status', 'authority', 'evidence'])
      && boundedCanonicalString(entry.id, 128)
      && boundedCanonicalString(entry.status, 128)
      && boundedCanonicalString(entry.authority, 128)
      && boundedCanonicalString(entry.evidence, 512));
}

function canonicalRuntimeOrigin(raw: string | undefined): string {
  if (!raw || raw !== raw.trim() || raw.endsWith('/')) {
    throw new RuntimeProxyError('runtime_configuration_invalid', 503);
  }
  let parsed: URL;
  try {
    parsed = new URL(raw);
  } catch {
    throw new RuntimeProxyError('runtime_configuration_invalid', 503);
  }
  const expectedIp = process.env.APOCV4_RUNTIME_DIRECT_IP?.trim();
  const expectedPort = process.env.APOCV4_RUNTIME_DIRECT_PORT?.trim();
  const transport = process.env.APOCV4_RUNTIME_TRANSPORT?.trim();
  const testTransport = transport === 'test-fetch' && process.env.NODE_ENV !== 'production';
  if (
    transport !== 'direct-tls'
    && !testTransport
  ) {
    throw new RuntimeProxyError('runtime_configuration_invalid', 503);
  }
  if (
    parsed.protocol !== 'https:'
    || parsed.username !== ''
    || parsed.password !== ''
    || !expectedIp
    || isIP(expectedIp) !== 4
    || parsed.hostname !== expectedIp
    || !expectedPort
    || !PORT_RE.test(expectedPort)
    || parsed.port !== expectedPort
    || parsed.pathname !== '/'
    || parsed.search !== ''
    || parsed.hash !== ''
    || parsed.origin !== raw
  ) {
    throw new RuntimeProxyError('runtime_configuration_invalid', 503);
  }
  return parsed.origin;
}

function runtimeCa(): Buffer {
  const encoded = process.env.APOCV4_RUNTIME_CA_B64?.trim();
  if (!encoded || encoded.length > 32_768 || !BASE64_RE.test(encoded)) {
    throw new RuntimeProxyError('runtime_configuration_invalid', 503);
  }
  const certificate = Buffer.from(encoded, 'base64');
  const pem = certificate.toString('utf8');
  if (
    certificate.length < 256
    || certificate.length > 24_576
    || !pem.startsWith('-----BEGIN CERTIFICATE-----\n')
    || !pem.endsWith('-----END CERTIFICATE-----\n')
  ) {
    throw new RuntimeProxyError('runtime_configuration_invalid', 503);
  }
  return certificate;
}

async function directTlsRequest(
  url: string,
  init: RequestInit,
  maximumBytes: number,
  deadlineMs: number,
): Promise<Response> {
  const parsed = new URL(url);
  const headers = Object.fromEntries(new Headers(init.headers).entries());
  const body = typeof init.body === 'string' ? Buffer.from(init.body, 'utf8') : null;
  return new Promise<Response>((resolve, reject) => {
    let settled = false;
    const fail = (error: Error): void => {
      if (settled) return;
      settled = true;
      reject(error);
    };
    const request = httpsRequest({
      protocol: 'https:',
      hostname: parsed.hostname,
      port: Number(parsed.port),
      path: `${parsed.pathname}${parsed.search}`,
      method: init.method,
      headers,
      ca: runtimeCa(),
      rejectUnauthorized: true,
      timeout: deadlineMs,
    }, (incoming) => {
      const chunks: Buffer[] = [];
      let total = 0;
      incoming.on('data', (chunk: Buffer | string) => {
        const bytes = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
        total += bytes.length;
        if (total > maximumBytes) {
          incoming.destroy();
          fail(new RuntimeProxyError('runtime_response_too_large', 502, incoming.statusCode ?? null));
          return;
        }
        chunks.push(bytes);
      });
      incoming.on('error', fail);
      incoming.on('end', () => {
        if (settled) return;
        const responseHeaders = new Headers();
        for (const [name, value] of Object.entries(incoming.headers)) {
          if (Array.isArray(value)) {
            for (const member of value) responseHeaders.append(name, member);
          } else if (value !== undefined) {
            responseHeaders.set(name, String(value));
          }
        }
        settled = true;
        resolve(new Response(Buffer.concat(chunks, total), {
          status: incoming.statusCode ?? 502,
          headers: responseHeaders,
        }));
      });
    });
    request.once('timeout', () => {
      request.destroy();
      fail(new RuntimeProxyError('runtime_deadline_exceeded', 504, null, deadlineMs));
    });
    request.once('error', (error) => {
      fail(error instanceof RuntimeProxyError
        ? error
        : new RuntimeProxyError('runtime_unreachable', 502));
    });
    if (init.signal) {
      const abort = (): void => {
        request.destroy();
        fail(new RuntimeProxyError('runtime_deadline_exceeded', 504, null, deadlineMs));
      };
      if (init.signal.aborted) abort();
      else init.signal.addEventListener('abort', abort, { once: true });
    }
    if (body) request.write(body);
    request.end();
  });
}

async function runtimeRequest(
  url: string,
  init: RequestInit,
  maximumBytes: number,
  deadlineMs: number,
): Promise<Response> {
  if (
    process.env.APOCV4_RUNTIME_TRANSPORT === 'test-fetch'
    && process.env.NODE_ENV !== 'production'
  ) {
    return fetch(url, init);
  }
  return directTlsRequest(url, init, maximumBytes, deadlineMs);
}

export function validateRuntimeUrl(raw: string): string {
  return canonicalRuntimeOrigin(raw);
}

function runtimeToken(profile: RuntimeCredentialProfile = 'owner'): string {
  const token = profile === 'public'
    ? process.env.APOCV4_PUBLIC_API_TOKEN
    : process.env.APOCV4_API_TOKEN;
  if (!token || !TOKEN_RE.test(token)) {
    throw new RuntimeProxyError('runtime_credential_unavailable', 503);
  }
  return token;
}

function boundedHeaderRef(headers: Headers, name: string): string | null {
  const value = headers.get(name);
  return value && SHA256_RE.test(value) ? value : null;
}

function boundedMode(headers: Headers): string | null {
  const value = headers.get('x-apocv4-auth-mode');
  return value && /^[A-Z_]{1,64}$/.test(value) ? value : null;
}

async function readBoundedJson(
  response: Response,
  maximumBytes: number,
  protectedValue: string,
): Promise<JsonObject> {
  const contentType = response.headers.get('content-type')?.split(';', 1)[0]?.trim().toLowerCase();
  if (contentType !== 'application/json') {
    throw new RuntimeProxyError('runtime_response_invalid', 502, response.status);
  }
  const contentEncoding = response.headers.get('content-encoding');
  if (contentEncoding && contentEncoding.toLowerCase() !== 'identity') {
    throw new RuntimeProxyError('runtime_response_invalid', 502, response.status);
  }
  const declared = response.headers.get('content-length');
  if (declared !== null) {
    if (!/^(0|[1-9][0-9]*)$/.test(declared) || Number(declared) > maximumBytes) {
      throw new RuntimeProxyError('runtime_response_too_large', 502, response.status);
    }
  }
  if (!response.body) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, response.status);
  }
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  try {
    while (true) {
      const chunk = await reader.read();
      if (chunk.done) break;
      total += chunk.value.byteLength;
      if (total > maximumBytes) {
        await reader.cancel();
        throw new RuntimeProxyError('runtime_response_too_large', 502, response.status);
      }
      chunks.push(chunk.value);
    }
  } finally {
    reader.releaseLock();
  }
  if (declared !== null && total !== Number(declared)) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, response.status);
  }
  const joined = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    joined.set(chunk, offset);
    offset += chunk.byteLength;
  }
  let text: string;
  try {
    text = new TextDecoder('utf-8', { fatal: true }).decode(joined);
  } catch {
    throw new RuntimeProxyError('runtime_response_invalid', 502, response.status);
  }
  if (text.includes(protectedValue)) {
    throw new RuntimeProxyError('runtime_reflected_credential', 502, response.status);
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    throw new RuntimeProxyError('runtime_response_invalid', 502, response.status);
  }
  if (!isObject(parsed) || parsed.schema_version !== RUNTIME_SCHEMA) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, response.status);
  }
  return parsed;
}

async function callRuntime(
  path: '/health' | '/v1/chat' | '/v1/code' | '/v1/code/rollback' | '/v1/objectives',
  body: JsonObject | null,
  traceparent?: string,
  credentialProfile: RuntimeCredentialProfile = 'owner',
): Promise<RuntimeCall> {
  const origin = canonicalRuntimeOrigin(process.env.APOCV4_RUNTIME_URL);
  const token = runtimeToken(credentialProfile);
  const objective = path === '/v1/objectives';
  const chat = path === '/v1/chat';
  const code = path === '/v1/code';
  const rollback = path === '/v1/code/rollback';
  const deadlineMs = code
    ? RUNPOD_CODE_DEADLINE_MS
    : rollback
      ? ROLLBACK_DEADLINE_MS
      : objective
        ? RUNPOD_SYNC_DEADLINE_MS
        : chat
          ? CHAT_DEADLINE_MS
          : HEALTH_DEADLINE_MS;
  const responseLimit = code
    ? CODE_RESPONSE_LIMIT
    : rollback
      ? ROLLBACK_RESPONSE_LIMIT
      : objective
        ? OBJECTIVE_RESPONSE_LIMIT
        : chat
          ? CHAT_RESPONSE_LIMIT
          : HEALTH_RESPONSE_LIMIT;
  const controller = new AbortController();
  const deadline = setTimeout(() => controller.abort(), deadlineMs);
  const started = Date.now();
  const traceMatch = traceparent ? TRACEPARENT_RE.exec(traceparent) : null;
  try {
    const response = await runtimeRequest(`${origin}${path}`, {
      method: body === null ? 'GET' : 'POST',
      headers: {
        Accept: 'application/json',
        'Accept-Encoding': 'identity',
        Authorization: `Bearer ${token}`,
        ...(traceMatch ? {
          Traceparent: traceparent,
          'X-Apocky-Trace-Id': traceMatch[1],
        } : {}),
        ...(body === null ? {} : { 'Content-Type': 'application/json' }),
      },
      body: body === null ? undefined : JSON.stringify(body),
      cache: 'no-store',
      redirect: 'error',
      signal: controller.signal,
    }, responseLimit, deadlineMs);
    const data = await readBoundedJson(response, responseLimit, token);
    if (!response.ok) {
      throw new RuntimeProxyError('runtime_http_error', 502, response.status);
    }
    const expectedKeys = objective || chat || code || rollback
      ? ['schema_version', 'result']
      : ['schema_version', 'status', 'engine', 'vision'];
    if (!exactKeys(data, expectedKeys)) {
      throw new RuntimeProxyError('runtime_response_invalid', 502, response.status);
    }
    if (
      ((objective || chat || code || rollback) && !isObject(data.result))
      || (!objective && !chat && !code && !rollback
        && (data.status !== 'READY' || !isObject(data.engine) || typeof data.vision !== 'boolean'))
    ) {
      throw new RuntimeProxyError('runtime_response_invalid', 502, response.status);
    }
    return {
      data,
      receipt: {
        observed_at: new Date().toISOString(),
        latency_ms: Math.max(0, Date.now() - started),
        upstream_status: response.status,
        auth_mode: boundedMode(response.headers),
        auth_registry_ref: boundedHeaderRef(response.headers, 'x-apocv4-auth-registry-ref'),
        binding_ref: boundedHeaderRef(response.headers, 'x-apocv4-binding-ref'),
        principal_ref: boundedHeaderRef(response.headers, 'x-apocv4-principal-ref'),
        privacy_partition_ref: boundedHeaderRef(response.headers, 'x-apocv4-privacy-partition-ref'),
        effect_scope_ref: boundedHeaderRef(response.headers, 'x-apocv4-effect-scope-ref'),
      },
    };
  } catch (error) {
    if (error instanceof RuntimeProxyError) throw error;
    const timedOut = controller.signal.aborted;
    throw new RuntimeProxyError(
      timedOut ? 'runtime_deadline_exceeded' : 'runtime_unreachable',
      timedOut ? 504 : 502,
      null,
      timedOut ? deadlineMs : null,
    );
  } finally {
    clearTimeout(deadline);
  }
}

function selected(source: JsonObject, keys: readonly string[]): JsonObject {
  const result: JsonObject = {};
  for (const key of keys) {
    if (Object.hasOwn(source, key)) result[key] = source[key];
  }
  return result;
}

function requireStrictEffectReceipt(receipt: RuntimeReceipt, requireEffectScope: boolean): void {
  if (
    receipt.auth_mode !== 'STRICT_REGISTRY'
    || !receipt.auth_registry_ref
    || !receipt.binding_ref
    || !receipt.principal_ref
    || !receipt.privacy_partition_ref
    || (requireEffectScope && !receipt.effect_scope_ref)
  ) {
    throw new RuntimeProxyError('runtime_effect_attestation_invalid', 502, receipt.upstream_status);
  }
}

export async function fetchRuntimeHealth(traceparent?: string): Promise<RuntimeHealthProjection> {
  const call = await callRuntime('/health', null, traceparent);
  return {
    schema_version: APOCV4_PROXY_SCHEMA,
    kind: 'health',
    observed: {
      evidence_lane: 'observed_runtime_http',
      receipt: call.receipt,
      runtime: call.data,
    },
    model_reported: {
      evidence_lane: 'model_reported',
      present: false,
      note: 'Health contains runtime observations only; it is not a model reasoning claim.',
    },
  };
}

export async function submitRuntimeObjective(objective: string, traceparent?: string): Promise<RuntimeObjectiveProjection> {
  if (
    typeof objective !== 'string'
    || objective !== objective.trim()
    || objective.length < 1
    || objective.length > 16_384
  ) {
    throw new RuntimeProxyError('objective_invalid', 400);
  }
  const call = await callRuntime('/v1/objectives', { max_iterations: 1, objective }, traceparent);
  const runtimeResult = call.data.result;
  if (!isObject(runtimeResult)) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  const rawAttempts = Array.isArray(runtimeResult.attempts)
    ? runtimeResult.attempts.filter(isObject).slice(0, 100)
    : [];
  const observedAttempts = rawAttempts.map((attempt) => selected(attempt, [
    'sequence',
    'kind',
    'cycle_status',
    'test_run_digest',
    'test_passed',
    'failed_oracles',
    'evidence_spine_digest',
    'error_class',
    'error_digest',
    'attempt_digest',
  ]));
  const modelAttempts = rawAttempts.map((attempt) => selected(attempt, [
    'sequence',
    'active_model_id',
    'candidate_digest',
    'model_route_digest',
    'model_attempts_digest',
    'model_transitions_digest',
    'council_decision',
    'council_decision_digest',
  ]));
  return {
    schema_version: APOCV4_PROXY_SCHEMA,
    kind: 'objective',
    observed: {
      evidence_lane: 'observed_runtime_http_and_test_receipts',
      receipt: call.receipt,
      runtime: selected(runtimeResult, [
        'schema_version',
        'status',
        'terminal_reason',
        'max_iterations',
        'iterations_completed',
        'accepted_candidate_digest',
        'last_test_run_digest',
        'checkpoint_digest',
        'perception_frame_digest',
        'vision_observation_digests',
        'faculty_team_id',
      ]),
      attempts: observedAttempts,
    },
    model_reported: {
      evidence_lane: 'model_reported_not_observed_fact',
      note: 'Faculty and council fields are model-reported outputs, not hidden chain-of-thought or independently observed facts.',
      attempts: modelAttempts,
    },
  };
}

export async function submitRuntimeChat(
  input: RuntimeChatInput,
  traceparent?: string,
): Promise<RuntimeChatProjection> {
  const {
    message, conversationId, requestId, privacyPartition,
    credentialProfile = 'owner',
  } = input;
  if (
    typeof message !== 'string'
    || message !== message.trim()
    || Buffer.byteLength(message, 'utf8') < 1
    || Buffer.byteLength(message, 'utf8') > 16_384
    || !UUID_RE.test(conversationId)
    || !UUID_RE.test(requestId)
    || typeof privacyPartition !== 'string'
    || privacyPartition !== privacyPartition.trim()
    || privacyPartition.length < 1
    || privacyPartition.length > 256
    || (credentialProfile !== 'owner' && credentialProfile !== 'public')
  ) {
    throw new RuntimeProxyError('chat_request_invalid', 400);
  }
  if (
    (credentialProfile === 'owner' && privacyPartition !== 'owner:apocky')
    || (credentialProfile === 'public' && privacyPartition !== 'public:apocrypha')
  ) {
    throw new RuntimeProxyError('chat_request_invalid', 400);
  }
  const call = await callRuntime('/v1/chat', {
    message,
    conversation_id: conversationId,
    request_id: requestId,
    privacy_partition: privacyPartition,
  }, traceparent, credentialProfile);
  const result = call.data.result;
  if (!isObject(result)) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  const responseV2 = result.schema_version === 'apocv4.chat-response.v2';
  const expectedKeys = responseV2
    ? [
      'schema_version', 'text', 'model_reported', 'observed', 'authority',
      'identity', 'context', 'conversation_id', 'request_id', 'privacy_partition_ref',
      'outcome', 'learned_faculty_used', 'duplicate_effect_protection',
    ]
    : [
      'schema_version', 'text', 'model_reported', 'observed', 'authority',
      'conversation_id', 'request_id', 'privacy_partition_ref', 'outcome',
      'learned_faculty_used', 'duplicate_effect_protection',
    ];
  if (!exactKeys(result, expectedKeys)) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  const model = result.model_reported;
  const observed = result.observed;
  const authority = result.authority;
  if (
    (result.schema_version !== 'apocv4.chat-response.v1' && !responseV2)
    || result.conversation_id !== conversationId
    || result.request_id !== requestId
    || typeof result.privacy_partition_ref !== 'string'
    || !SHA256_RE.test(result.privacy_partition_ref)
    || result.outcome !== 'completed'
    || result.learned_faculty_used !== true
    || result.duplicate_effect_protection !== 'not_applicable_no_effect_authority'
    || !isObject(model)
    || model.evidence_lane !== 'model_reported_not_observed_fact'
    || typeof result.text !== 'string'
    || result.text !== result.text.trim()
    || Buffer.byteLength(result.text, 'utf8') < 1
    || Buffer.byteLength(result.text, 'utf8') > 128 * 1024
    || !isObject(observed)
    || observed.evidence_lane !== 'observed_runtime_transport'
    || !isObject(authority)
    || !exactKeys(authority, [
      'effect_authority', 'tool_authority', 'memory_scope',
      'conversation_history', 'training_consent',
    ])
    || authority.effect_authority !== 'NONE'
    || authority.training_consent !== false
  ) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  if (responseV2) {
    const expectedMemoryScope = credentialProfile === 'owner'
      ? 'owner_partitioned_retrieval'
      : 'public_safe_retrieval';
    if (
      authority.tool_authority !== 'READ_ONLY_CONTEXT'
      || authority.memory_scope !== expectedMemoryScope
      || authority.conversation_history !== 'session_bounded'
      || !validateV2ChatIdentity(result.identity)
      || !validateV2ChatContext(result.context)
    ) {
      throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
    }
  } else if (
    authority.tool_authority !== 'NONE'
    || authority.memory_scope !== 'ephemeral'
    || authority.conversation_history !== 'not_retained'
  ) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  for (const key of ['model_id', 'model_revision', 'model_family', 'response_id'] as const) {
    if (typeof model[key] !== 'string' || model[key] !== model[key].trim() || model[key].length < 1) {
      throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
    }
  }
  for (const key of ['serving_profile_digest', 'prompt_digest', 'response_digest'] as const) {
    if (typeof model[key] !== 'string' || !SHA256_RE.test(model[key])) {
      throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
    }
  }
  return {
    schema_version: APOCV4_PROXY_SCHEMA,
    kind: 'chat',
    observed: {
      evidence_lane: 'observed_runtime_http_and_transport',
      receipt: call.receipt,
      runtime: {
        schema_version: result.schema_version,
        conversation_id: result.conversation_id,
        request_id: result.request_id,
        privacy_partition_ref: result.privacy_partition_ref,
        outcome: result.outcome,
        learned_faculty_used: result.learned_faculty_used,
        duplicate_effect_protection: result.duplicate_effect_protection,
        authority,
        ...(responseV2 ? { identity: result.identity, context: result.context } : {}),
        observed,
      },
    },
    model_reported: {
      ...model,
      evidence_lane: 'model_reported_not_observed_fact',
      text: result.text,
    },
    authority: authority as unknown as RuntimeChatAuthority,
    identity: responseV2 ? result.identity as RuntimeChatIdentity : null,
    context: responseV2 ? result.context as RuntimeChatContext : null,
  };
}

function canonicalCodePaths(value: unknown): string[] {
  if (!Array.isArray(value) || value.length < 1 || value.length > 32) {
    throw new RuntimeProxyError('code_request_invalid', 400);
  }
  const paths: string[] = [];
  for (const member of value) {
    if (
      typeof member !== 'string'
      || member !== member.trim()
      || member.length < 1
      || member.length > 4_096
      || !CODE_PATH_RE.test(member)
    ) {
      throw new RuntimeProxyError('code_request_invalid', 400);
    }
    const parts = member.split('/');
    const firstPart = parts[0] ?? '';
    const folded = member.toLowerCase();
    const leaf = (parts.at(-1) ?? '').toLowerCase();
    if (
      parts.some((part) => !part || part === '.' || part === '..')
      || firstPart.toLowerCase() === '.git'
      || parts.some((part) => (
        part.trimEnd() !== part
        || part.endsWith('.')
        || part.includes(':')
        || WINDOWS_RESERVED_PATH_STEMS.has((part.split('.', 1)[0] ?? '').toUpperCase())
      ))
      || PROTECTED_CODE_PATHS.has(member)
      || PROTECTED_CODE_PREFIXES.some((prefix) => member.startsWith(prefix))
      || leaf === 'env.txt'
      || leaf === '.env'
      || leaf.startsWith('.env.')
      || /\.(?:pem|key|p12|pfx|crt|cer)$/.test(leaf)
      || /(?:^|[._-])(?:secret|credential|private[-_]?key)(?:[._-]|$)/.test(leaf)
      || folded.includes('/.git/')
    ) {
      throw new RuntimeProxyError('code_request_invalid', 400);
    }
    paths.push(member);
  }
  const sorted = [...paths].sort();
  if (
    new Set(paths).size !== paths.length
    || new Set(paths.map((path) => path.toLowerCase())).size !== paths.length
    || paths.some((path, index) => path !== sorted[index])
  ) {
    throw new RuntimeProxyError('code_request_invalid', 400);
  }
  return paths;
}

export function validateRuntimeCodePaths(value: unknown): string[] {
  return canonicalCodePaths(value);
}

function digestValue(value: unknown): string | null {
  return typeof value === 'string' && SHA256_RE.test(value) ? value : null;
}

function boundedOptionalString(value: unknown, maximum = 256): string | null {
  return typeof value === 'string'
    && value === value.trim()
    && value.length > 0
    && value.length <= maximum
    ? value
    : null;
}

function projectFacultyAttempts(value: unknown): JsonObject[] {
  if (!Array.isArray(value) || value.length > 16) {
    throw new RuntimeProxyError('runtime_response_invalid', 502);
  }
  return value.map((member) => {
    if (!isObject(member)) throw new RuntimeProxyError('runtime_response_invalid', 502);
    const index = member.index;
    const identity = digestValue(member.faculty_identity_digest);
    const status = member.status;
    const proposalDigest = member.proposal_digest === null
      ? null
      : digestValue(member.proposal_digest);
    const errorClass = member.error_class === null
      ? null
      : boundedOptionalString(member.error_class);
    const errorDigest = member.error_digest === null
      ? null
      : digestValue(member.error_digest);
    if (
      !Number.isSafeInteger(index)
      || Number(index) < 0
      || !identity
      || !['ACCEPTED_FOR_ADMISSION', 'FAILED'].includes(String(status))
      || (member.proposal_digest !== null && !proposalDigest)
      || (member.error_class !== null && !errorClass)
      || (member.error_digest !== null && !errorDigest)
    ) {
      throw new RuntimeProxyError('runtime_response_invalid', 502);
    }
    return {
      index: Number(index),
      faculty_identity_digest: identity,
      status,
      proposal_digest: proposalDigest,
      error_class: errorClass,
      error_digest: errorDigest,
    };
  });
}

function projectTestReceipt(value: unknown): JsonObject | null {
  if (value === null || value === undefined) return null;
  if (!isObject(value)) throw new RuntimeProxyError('runtime_response_invalid', 502);
  const commandDigest = digestValue(value.command_digest);
  const contractDigest = digestValue(value.runner_contract_digest);
  const stdoutDigest = digestValue(value.stdout_sha256);
  const stderrDigest = digestValue(value.stderr_sha256);
  const receiptDigest = digestValue(value.receipt_digest);
  if (
    !commandDigest
    || !contractDigest
    || !stdoutDigest
    || !stderrDigest
    || !receiptDigest
    || !Number.isSafeInteger(value.exit_code)
    || typeof value.timed_out !== 'boolean'
    || typeof value.passed !== 'boolean'
    || !Number.isSafeInteger(value.stdout_bytes)
    || Number(value.stdout_bytes) < 0
    || !Number.isSafeInteger(value.stderr_bytes)
    || Number(value.stderr_bytes) < 0
    || typeof value.elapsed_ms !== 'number'
    || !Number.isFinite(value.elapsed_ms)
    || Number(value.elapsed_ms) < 0
  ) {
    throw new RuntimeProxyError('runtime_response_invalid', 502);
  }
  const errorClass = value.error_class === null
    ? null
    : boundedOptionalString(value.error_class);
  if (value.error_class !== null && !errorClass) {
    throw new RuntimeProxyError('runtime_response_invalid', 502);
  }
  return {
    command_digest: commandDigest,
    runner_contract_digest: contractDigest,
    exit_code: Number(value.exit_code),
    timed_out: value.timed_out,
    error_class: errorClass,
    stdout_sha256: stdoutDigest,
    stdout_bytes: Number(value.stdout_bytes),
    stderr_sha256: stderrDigest,
    stderr_bytes: Number(value.stderr_bytes),
    elapsed_ms: Number(value.elapsed_ms),
    passed: value.passed,
    receipt_digest: receiptDigest,
  };
}

export async function submitRuntimeCode(
  input: RuntimeCodeInput,
  traceparent?: string,
): Promise<RuntimeCodeProjection> {
  const objective = input.objective;
  const privacyPartition = input.privacyPartition;
  if (
    typeof objective !== 'string'
    || objective !== objective.trim()
    || objective.length < 1
    || Buffer.byteLength(objective, 'utf8') > 32_768
    || typeof privacyPartition !== 'string'
    || privacyPartition !== privacyPartition.trim()
    || privacyPartition.length < 1
    || privacyPartition.length > 256
  ) {
    throw new RuntimeProxyError('code_request_invalid', 400);
  }
  const allowedPaths = canonicalCodePaths(input.allowedPaths);
  const call = await callRuntime('/v1/code', {
    objective,
    privacy_partition: privacyPartition,
    allowed_paths: allowedPaths,
  }, traceparent);
  const result = call.data.result;
  if (!isObject(result)) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  const state = result.state;
  const proposalDigest = digestValue(result.proposal_digest);
  const frameDigest = digestValue(result.frame_digest);
  const terminalDigest = digestValue(result.terminal_event_digest);
  const journalDigest = digestValue(result.journal_tip_digest);
  if (
    result.schema_version !== 'apocv4.journaled-patch-runtime.v1'
    || !['PROMOTED', 'PROMOTION_ABORTED', 'EXECUTION_ROLLED_BACK', 'SOURCE_DRIFT_BEFORE_ADMISSION', 'ADMISSION_REFUSED'].includes(String(state))
    || result.requested_objective !== objective
    || result.privacy_partition !== privacyPartition
    || !proposalDigest
    || !frameDigest
    || !terminalDigest
    || !journalDigest
  ) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  requireStrictEffectReceipt(call.receipt, state === 'PROMOTED');
  const attempts = projectFacultyAttempts(result.faculty_attempts);
  const outcome = result.isolated_outcome;
  let projectedOutcome: JsonObject | null = null;
  let test: JsonObject | null = null;
  if (outcome !== undefined) {
    if (!isObject(outcome)) {
      throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
    }
    test = projectTestReceipt(outcome.test_receipt);
    const outcomeDigest = digestValue(outcome.outcome_digest);
    const outcomeProposalDigest = digestValue(outcome.proposal_digest);
    const sourceDigest = digestValue(outcome.source_prestate_digest);
    const leaseDigest = digestValue(outcome.lease_digest);
    const deltaDigest = outcome.delta_digest === null ? null : digestValue(outcome.delta_digest);
    const failureDigest = outcome.failure_digest === null ? null : digestValue(outcome.failure_digest);
    const failureClass = outcome.failure_class === null ? null : boundedOptionalString(outcome.failure_class);
    if (
      !outcomeDigest
      || !outcomeProposalDigest
      || outcomeProposalDigest !== proposalDigest
      || !sourceDigest
      || !leaseDigest
      || (outcome.delta_digest !== null && !deltaDigest)
      || (outcome.failure_digest !== null && !failureDigest)
      || (outcome.failure_class !== null && !failureClass)
      || !boundedOptionalString(outcome.state)
    ) {
      throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
    }
    projectedOutcome = {
      state: outcome.state,
      source_prestate_digest: sourceDigest,
      lease_digest: leaseDigest,
      delta_digest: deltaDigest,
      failure_class: failureClass,
      failure_digest: failureDigest,
      outcome_digest: outcomeDigest,
    };
  }
  const promotionDigest = result.promotion_event_digest === null || result.promotion_event_digest === undefined
    ? null
    : digestValue(result.promotion_event_digest);
  if ((state === 'PROMOTED' && !promotionDigest) || (state !== 'PROMOTED' && promotionDigest)) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  const authorityDigest = digestValue(result.authority_digest);
  const requestDigest = digestValue(result.request_digest);
  const approvalDigest = digestValue(result.approval_digest);
  const reservationDigest = digestValue(result.reservation_event_digest);
  const preparedDigest = digestValue(result.promotion_prepared_event_digest);
  const admission = isObject(result.admission) ? result.admission : null;
  if (
    state === 'PROMOTED'
    && (
      !projectedOutcome
      || projectedOutcome.state !== 'ACCEPTED_ISOLATED'
      || !projectedOutcome.delta_digest
      || !test
      || test.passed !== true
      || !authorityDigest
      || !requestDigest
      || !approvalDigest
      || !reservationDigest
      || !preparedDigest
      || promotionDigest !== terminalDigest
      || !admission
      || admission.allowed !== true
      || admission.reason_code !== 'admitted'
      || admission.frame_digest !== frameDigest
      || admission.authority_digest !== authorityDigest
      || admission.request_digest !== requestDigest
      || admission.approval_digest !== approvalDigest
    )
  ) {
    throw new RuntimeProxyError('runtime_effect_attestation_invalid', 502, call.receipt.upstream_status);
  }
  return {
    schema_version: APOCV4_PROXY_SCHEMA,
    kind: 'code',
    observed: {
      evidence_lane: 'observed_runtime_http_effect_and_test_receipts',
      receipt: call.receipt,
      runtime: {
        schema_version: result.schema_version,
        state,
        frame_digest: frameDigest,
        authority_digest: authorityDigest,
        request_digest: requestDigest,
        approval_digest: approvalDigest,
        reservation_event_digest: reservationDigest,
        promotion_prepared_event_digest: result.promotion_prepared_event_digest === null
          ? null
          : preparedDigest,
        promotion_event_digest: promotionDigest,
        terminal_event_digest: terminalDigest,
        journal_tip_digest: journalDigest,
        isolated_outcome: projectedOutcome,
        perception_frame_digest: digestValue(result.perception_frame_digest),
        faculty_team_id: boundedOptionalString(result.faculty_team_id),
      },
      test,
    },
    generated: {
      evidence_lane: 'model_generated_artifact_identity_not_source_diff',
      proposal_digest: proposalDigest,
      requested_allowed_paths: allowedPaths,
      faculty_attempts: attempts,
    },
  };
}

export async function submitRuntimeRollback(
  promotionEventDigest: string,
  traceparent?: string,
): Promise<RuntimeRollbackProjection> {
  if (!SHA256_RE.test(promotionEventDigest)) {
    throw new RuntimeProxyError('rollback_request_invalid', 400);
  }
  const call = await callRuntime('/v1/code/rollback', {
    promotion_event_digest: promotionEventDigest,
  }, traceparent);
  requireStrictEffectReceipt(call.receipt, true);
  const result = call.data.result;
  if (
    !isObject(result)
    || !exactKeys(result, [
      'schema_version', 'state', 'promotion_event_digest', 'rollback_event_digest', 'journal_tip_digest',
    ])
    || result.schema_version !== 'apocv4.journaled-patch-runtime.v1'
    || result.state !== 'ROLLED_BACK'
    || result.promotion_event_digest !== promotionEventDigest
    || !digestValue(result.rollback_event_digest)
    || !digestValue(result.journal_tip_digest)
  ) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  return {
    schema_version: APOCV4_PROXY_SCHEMA,
    kind: 'rollback',
    observed: {
      evidence_lane: 'observed_runtime_http_rollback_receipt',
      receipt: call.receipt,
      runtime: {
        schema_version: result.schema_version,
        state: result.state,
        promotion_event_digest: result.promotion_event_digest,
        rollback_event_digest: result.rollback_event_digest,
        journal_tip_digest: result.journal_tip_digest,
      },
    },
    model_reported: {
      evidence_lane: 'model_reported',
      present: false,
      note: 'Rollback is a runtime effect receipt, not a model reasoning claim.',
    },
  };
}

export function publicRuntimeError(error: unknown): {
  error: string;
  upstream_status?: number;
  observed?: {
    evidence_lane: 'observed_runtime_transport_failure';
    receipt: {
      observed_at: string;
      outcome: string;
      upstream_status: number | null;
      deadline_ms: number | null;
    };
  };
} {
  if (!(error instanceof RuntimeProxyError)) return { error: 'runtime_proxy_failure' };
  return {
    error: error.code,
    ...(error.upstreamStatus === null ? {} : { upstream_status: error.upstreamStatus }),
    observed: {
      evidence_lane: 'observed_runtime_transport_failure',
      receipt: {
        observed_at: error.observedAt,
        outcome: error.code,
        upstream_status: error.upstreamStatus,
        deadline_ms: error.deadlineMs,
      },
    },
  };
}
