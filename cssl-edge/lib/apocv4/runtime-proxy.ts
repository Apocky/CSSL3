// Server-only, admin-route transport for the authenticated Apocv4 runtime.

export const APOCV4_PROXY_SCHEMA = 'apocky.apocv4-runtime-proxy.v1';

const RUNTIME_SCHEMA = 'apocv4.runtime-service.v1';
const RUNPOD_PROXY_HOST = /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?-\d+\.proxy\.runpod\.net$/;
const TOKEN_RE = /^[A-Za-z0-9._~+/-]{1,8192}$/;
const SHA256_RE = /^[0-9a-f]{64}$/;
const HEALTH_RESPONSE_LIMIT = 256 * 1024;
const OBJECTIVE_RESPONSE_LIMIT = 2 * 1024 * 1024;
const HEALTH_DEADLINE_MS = 12_000;
export const RUNPOD_SYNC_DEADLINE_MS = 95_000;

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
  if (
    parsed.protocol !== 'https:'
    || parsed.username !== ''
    || parsed.password !== ''
    || parsed.port !== ''
    || parsed.pathname !== '/'
    || parsed.search !== ''
    || parsed.hash !== ''
    || parsed.origin !== raw
    || !RUNPOD_PROXY_HOST.test(parsed.hostname)
  ) {
    throw new RuntimeProxyError('runtime_configuration_invalid', 503);
  }
  return parsed.origin;
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
  path: '/health' | '/v1/objectives',
  body: JsonObject | null,
): Promise<RuntimeCall> {
  const origin = canonicalRuntimeOrigin(process.env.APOCV4_RUNTIME_URL);
  const token = runtimeToken();
  const objective = path === '/v1/objectives';
  const deadlineMs = objective ? RUNPOD_SYNC_DEADLINE_MS : HEALTH_DEADLINE_MS;
  const responseLimit = objective ? OBJECTIVE_RESPONSE_LIMIT : HEALTH_RESPONSE_LIMIT;
  const controller = new AbortController();
  const deadline = setTimeout(() => controller.abort(), deadlineMs);
  const started = Date.now();
  try {
    const response = await fetch(`${origin}${path}`, {
      method: body === null ? 'GET' : 'POST',
      headers: {
        Accept: 'application/json',
        Authorization: `Bearer ${token}`,
        ...(body === null ? {} : { 'Content-Type': 'application/json' }),
      },
      body: body === null ? undefined : JSON.stringify(body),
      cache: 'no-store',
      redirect: 'error',
      signal: controller.signal,
    });
    const data = await readBoundedJson(response, responseLimit, token);
    if (!response.ok) {
      throw new RuntimeProxyError('runtime_http_error', 502, response.status);
    }
    const expectedKeys = objective
      ? ['schema_version', 'result']
      : ['schema_version', 'status', 'engine', 'vision'];
    if (!exactKeys(data, expectedKeys)) {
      throw new RuntimeProxyError('runtime_response_invalid', 502, response.status);
    }
    if (
      (objective && !isObject(data.result))
      || (!objective && (data.status !== 'READY' || !isObject(data.engine) || typeof data.vision !== 'boolean'))
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

export async function fetchRuntimeHealth(): Promise<RuntimeHealthProjection> {
  const call = await callRuntime('/health', null);
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

export async function submitRuntimeObjective(objective: string): Promise<RuntimeObjectiveProjection> {
  if (
    typeof objective !== 'string'
    || objective !== objective.trim()
    || objective.length < 1
    || objective.length > 16_384
  ) {
    throw new RuntimeProxyError('objective_invalid', 400);
  }
  const call = await callRuntime('/v1/objectives', { max_iterations: 1, objective });
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
