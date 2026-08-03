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
const HEALTH_DEADLINE_MS = 12_000;
const CHAT_DEADLINE_MS = 25_000;
export const RUNPOD_SYNC_DEADLINE_MS = 95_000;
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[45][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

type JsonObject = Record<string, unknown>;

export interface RuntimeReceipt {
  observed_at: string;
  latency_ms: number;
  upstream_status: number;
  auth_mode: string | null;
  auth_registry_ref: string | null;
  binding_ref: string | null;
  principal_ref: string | null;
  privacy_partition_ref: string | null;
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
}

export interface RuntimeChatInput {
  message: string;
  conversationId: string;
  requestId: string;
  privacyPartition: string;
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

function runtimeToken(): string {
  const token = process.env.APOCV4_API_TOKEN;
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
  path: '/health' | '/v1/chat' | '/v1/objectives',
  body: JsonObject | null,
  traceparent?: string,
): Promise<RuntimeCall> {
  const origin = canonicalRuntimeOrigin(process.env.APOCV4_RUNTIME_URL);
  const token = runtimeToken();
  const objective = path === '/v1/objectives';
  const chat = path === '/v1/chat';
  const deadlineMs = objective ? RUNPOD_SYNC_DEADLINE_MS : chat ? CHAT_DEADLINE_MS : HEALTH_DEADLINE_MS;
  const responseLimit = objective ? OBJECTIVE_RESPONSE_LIMIT : chat ? CHAT_RESPONSE_LIMIT : HEALTH_RESPONSE_LIMIT;
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
    const expectedKeys = objective || chat
      ? ['schema_version', 'result']
      : ['schema_version', 'status', 'engine', 'vision'];
    if (!exactKeys(data, expectedKeys)) {
      throw new RuntimeProxyError('runtime_response_invalid', 502, response.status);
    }
    if (
      ((objective || chat) && !isObject(data.result))
      || (!objective && !chat && (data.status !== 'READY' || !isObject(data.engine) || typeof data.vision !== 'boolean'))
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
  const { message, conversationId, requestId, privacyPartition } = input;
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
  ) {
    throw new RuntimeProxyError('chat_request_invalid', 400);
  }
  const call = await callRuntime('/v1/chat', {
    message,
    conversation_id: conversationId,
    request_id: requestId,
    privacy_partition: privacyPartition,
  }, traceparent);
  const result = call.data.result;
  if (!isObject(result) || !exactKeys(result, [
    'schema_version', 'text', 'model_reported', 'observed', 'authority',
    'conversation_id', 'request_id', 'privacy_partition_ref', 'outcome',
    'learned_faculty_used', 'duplicate_effect_protection',
  ])) {
    throw new RuntimeProxyError('runtime_response_invalid', 502, call.receipt.upstream_status);
  }
  const model = result.model_reported;
  const observed = result.observed;
  const authority = result.authority;
  if (
    result.schema_version !== 'apocv4.chat-response.v1'
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
    || authority.tool_authority !== 'NONE'
    || authority.memory_scope !== 'ephemeral'
    || authority.conversation_history !== 'not_retained'
    || authority.training_consent !== false
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
        observed,
      },
    },
    model_reported: {
      ...model,
      evidence_lane: 'model_reported_not_observed_fact',
      text: result.text,
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
