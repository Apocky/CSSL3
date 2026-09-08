import { createHash } from 'node:crypto';
import { pathToFileURL } from 'node:url';

export const CLIENT_DEADLINE_MS = 94_000;
export const PROXY_SCHEMA = 'apocky.apocv4-runtime-proxy.v1';

const DEFAULT_OBJECTIVE = [
  'Perform one bounded, reversible post-promotion acceptance cycle.',
  'Preserve every authority boundary, make no external changes, and return observed test receipts.',
].join(' ');
const MAX_RESPONSE_BYTES = 4 * 1024 * 1024;

export class AcceptanceFailure extends Error {
  constructor(step, code) {
    super(code);
    this.name = 'AcceptanceFailure';
    this.step = step;
    this.code = code;
  }
}

function requireCondition(condition, step, code) {
  if (!condition) throw new AcceptanceFailure(step, code);
}

function canonicalBaseUrl(raw) {
  requireCondition(typeof raw === 'string' && raw === raw.trim() && raw.length > 0, 'config', 'BASE_URL_REQUIRED');
  let parsed;
  try {
    parsed = new URL(raw);
  } catch {
    throw new AcceptanceFailure('config', 'BASE_URL_INVALID');
  }
  requireCondition(
    parsed.protocol === 'https:'
      && parsed.username === ''
      && parsed.password === ''
      && parsed.pathname === '/'
      && parsed.search === ''
      && parsed.hash === '',
    'config',
    'BASE_URL_INVALID',
  );
  return parsed.origin;
}

function protectedValues(bearerToken, cookie) {
  const values = [];
  if (bearerToken) values.push(bearerToken, `Bearer ${bearerToken}`);
  if (cookie) {
    values.push(cookie);
    for (const part of cookie.split(';')) {
      const index = part.indexOf('=');
      const value = index >= 0 ? part.slice(index + 1).trim() : '';
      if (value.length >= 8) values.push(value);
    }
  }
  return [...new Set(values.filter((value) => value.length >= 8))];
}

function credentialHeaders(bearerToken, cookie) {
  requireCondition(!(bearerToken && cookie), 'config', 'MULTIPLE_AUTH_INPUTS');
  if (bearerToken) return { Authorization: `Bearer ${bearerToken}` };
  if (cookie) return { Cookie: cookie };
  return null;
}

function sha256(text) {
  return createHash('sha256').update(text, 'utf8').digest('hex');
}

async function boundedText(response, step, secrets) {
  const declared = response.headers.get('content-length');
  if (declared !== null) {
    requireCondition(/^\d+$/.test(declared) && Number(declared) <= MAX_RESPONSE_BYTES, step, 'RESPONSE_TOO_LARGE');
  }
  const bytes = new Uint8Array(await response.arrayBuffer());
  requireCondition(bytes.byteLength <= MAX_RESPONSE_BYTES, step, 'RESPONSE_TOO_LARGE');
  const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  requireCondition(!secrets.some((secret) => text.includes(secret)), step, 'CREDENTIAL_RESPONSE_REFLECTION');
  return text;
}

async function request(fetchImpl, baseUrl, path, init, step, secrets) {
  const controller = new AbortController();
  const deadline = setTimeout(() => controller.abort(), CLIENT_DEADLINE_MS);
  const started = Date.now();
  try {
    const response = await fetchImpl(`${baseUrl}${path}`, {
      ...init,
      cache: 'no-store',
      redirect: 'error',
      signal: controller.signal,
    });
    const text = await boundedText(response, step, secrets);
    const durationMs = Date.now() - started;
    requireCondition(durationMs <= 95_000, step, 'CLIENT_BOUND_EXCEEDED');
    return { response, text, durationMs, bodySha256: sha256(text) };
  } catch (error) {
    if (error instanceof AcceptanceFailure) throw error;
    throw new AcceptanceFailure(step, controller.signal.aborted ? 'CLIENT_DEADLINE_EXCEEDED' : 'REQUEST_FAILED');
  } finally {
    clearTimeout(deadline);
  }
}

function jsonBody(result, step) {
  requireCondition(
    result.response.headers.get('content-type')?.toLowerCase().startsWith('application/json'),
    step,
    'CONTENT_TYPE_INVALID',
  );
  try {
    const parsed = JSON.parse(result.text);
    requireCondition(parsed !== null && typeof parsed === 'object' && !Array.isArray(parsed), step, 'JSON_ENVELOPE_INVALID');
    return parsed;
  } catch (error) {
    if (error instanceof AcceptanceFailure) throw error;
    throw new AcceptanceFailure(step, 'JSON_INVALID');
  }
}

function noStore(result, step) {
  const cacheControl = result.response.headers.get('cache-control')?.toLowerCase() ?? '';
  requireCondition(cacheControl.includes('no-store'), step, 'CACHE_POLICY_INVALID');
}

function receipt(step, result, extra = {}) {
  return {
    step,
    status: 'PASSED',
    http_status: result.response.status,
    duration_ms: result.durationMs,
    response_sha256: result.bodySha256,
    ...extra,
  };
}

function skipped(step) {
  return {
    step,
    status: 'PRODUCTION_ENV_ONLY',
    reason: 'authenticated_runtime_evidence_requires_post_promotion_admin_session_and_production_runtime_binding',
  };
}

export async function runOwnerAcceptance(options = {}) {
  const env = options.env ?? process.env;
  const fetchImpl = options.fetchImpl ?? globalThis.fetch;
  const log = options.log ?? ((line) => process.stdout.write(`${line}\n`));
  const mode = options.mode ?? env.APOCV4_SMOKE_MODE ?? 'post-promotion';
  requireCondition(mode === 'preview' || mode === 'post-promotion', 'config', 'MODE_INVALID');
  requireCondition(typeof fetchImpl === 'function', 'config', 'FETCH_UNAVAILABLE');

  const baseUrl = canonicalBaseUrl(options.baseUrl ?? env.APOCV4_SMOKE_BASE_URL);
  const bearerToken = options.bearerToken ?? env.APOCV4_SMOKE_BEARER_TOKEN ?? '';
  const cookie = options.cookie ?? env.APOCV4_SMOKE_COOKIE ?? '';
  const authHeaders = credentialHeaders(bearerToken, cookie);
  const secrets = protectedValues(bearerToken, cookie);
  const objective = options.objective ?? env.APOCV4_SMOKE_OBJECTIVE ?? DEFAULT_OBJECTIVE;
  requireCondition(
    typeof objective === 'string'
      && objective === objective.trim()
      && objective.length >= 1
      && objective.length <= 16_384,
    'config',
    'OBJECTIVE_INVALID',
  );
  if (mode === 'post-promotion') {
    requireCondition(authHeaders !== null, 'config', 'ADMIN_SESSION_REQUIRED');
  }

  const results = [];
  const page = await request(fetchImpl, baseUrl, '/admin/apex', {
    method: 'GET',
    headers: { Accept: 'text/html' },
  }, 'owner_page', secrets);
  requireCondition(page.response.status === 200, 'owner_page', 'OWNER_PAGE_NOT_200');
  requireCondition(
    page.response.headers.get('content-type')?.toLowerCase().startsWith('text/html'),
    'owner_page',
    'OWNER_PAGE_CONTENT_TYPE_INVALID',
  );
  requireCondition(page.text.includes('Relay · Apocrypha · Apocky'), 'owner_page', 'OWNER_PAGE_MARKER_MISSING');
  results.push(receipt('owner_page', page));

  const anonymousHealth = await request(fetchImpl, baseUrl, '/api/admin/apocv4/health', {
    method: 'GET',
    headers: { Accept: 'application/json' },
  }, 'anonymous_health', secrets);
  const anonymousHealthBody = jsonBody(anonymousHealth, 'anonymous_health');
  requireCondition(anonymousHealth.response.status === 401, 'anonymous_health', 'ANONYMOUS_HEALTH_NOT_401');
  requireCondition(anonymousHealthBody.authorized === false, 'anonymous_health', 'ANONYMOUS_HEALTH_ENVELOPE_INVALID');
  noStore(anonymousHealth, 'anonymous_health');
  results.push(receipt('anonymous_health', anonymousHealth));

  const anonymousObjective = await request(fetchImpl, baseUrl, '/api/admin/apocv4/objective', {
    method: 'POST',
    headers: { Accept: 'application/json', 'Content-Type': 'application/json' },
    body: JSON.stringify({ objective }),
  }, 'anonymous_objective', secrets);
  const anonymousObjectiveBody = jsonBody(anonymousObjective, 'anonymous_objective');
  requireCondition(anonymousObjective.response.status === 401, 'anonymous_objective', 'ANONYMOUS_OBJECTIVE_NOT_401');
  requireCondition(anonymousObjectiveBody.authorized === false, 'anonymous_objective', 'ANONYMOUS_OBJECTIVE_ENVELOPE_INVALID');
  noStore(anonymousObjective, 'anonymous_objective');
  results.push(receipt('anonymous_objective', anonymousObjective));

  if (authHeaders === null) {
    results.push(skipped('authenticated_health'), skipped('authenticated_objective'));
  } else {
    const authenticatedHealth = await request(fetchImpl, baseUrl, '/api/admin/apocv4/health', {
      method: 'GET',
      headers: { Accept: 'application/json', ...authHeaders },
    }, 'authenticated_health', secrets);
    const health = jsonBody(authenticatedHealth, 'authenticated_health');
    requireCondition(authenticatedHealth.response.status === 200, 'authenticated_health', 'AUTHENTICATED_HEALTH_NOT_200');
    requireCondition(health.schema_version === PROXY_SCHEMA && health.kind === 'health', 'authenticated_health', 'HEALTH_SCHEMA_INVALID');
    requireCondition(health.observed?.evidence_lane === 'observed_runtime_http', 'authenticated_health', 'HEALTH_EVIDENCE_LANE_INVALID');
    requireCondition(health.observed?.receipt?.upstream_status === 200, 'authenticated_health', 'HEALTH_RECEIPT_INVALID');
    requireCondition(health.observed?.runtime?.status === 'READY', 'authenticated_health', 'RUNTIME_NOT_READY');
    requireCondition(health.model_reported?.present === false, 'authenticated_health', 'HEALTH_MODEL_LANE_INVALID');
    noStore(authenticatedHealth, 'authenticated_health');
    results.push(receipt('authenticated_health', authenticatedHealth, {
      runtime_status: health.observed.runtime.status,
    }));

    const authenticatedObjective = await request(fetchImpl, baseUrl, '/api/admin/apocv4/objective', {
      method: 'POST',
      headers: { Accept: 'application/json', 'Content-Type': 'application/json', ...authHeaders },
      body: JSON.stringify({ objective }),
    }, 'authenticated_objective', secrets);
    const objectiveBody = jsonBody(authenticatedObjective, 'authenticated_objective');
    requireCondition(authenticatedObjective.response.status === 200, 'authenticated_objective', 'AUTHENTICATED_OBJECTIVE_NOT_200');
    requireCondition(objectiveBody.schema_version === PROXY_SCHEMA && objectiveBody.kind === 'objective', 'authenticated_objective', 'OBJECTIVE_SCHEMA_INVALID');
    requireCondition(
      objectiveBody.observed?.evidence_lane === 'observed_runtime_http_and_test_receipts',
      'authenticated_objective',
      'OBJECTIVE_EVIDENCE_LANE_INVALID',
    );
    requireCondition(objectiveBody.observed?.receipt?.upstream_status === 200, 'authenticated_objective', 'OBJECTIVE_RECEIPT_INVALID');
    requireCondition(objectiveBody.observed?.runtime?.max_iterations === 1, 'authenticated_objective', 'OBJECTIVE_NOT_BOUNDED');
    requireCondition(objectiveBody.observed?.runtime?.iterations_completed === 1, 'authenticated_objective', 'OBJECTIVE_ITERATION_INVALID');
    requireCondition(objectiveBody.observed?.runtime?.status === 'ACCEPTED', 'authenticated_objective', 'OBJECTIVE_NOT_ACCEPTED');
    requireCondition(Array.isArray(objectiveBody.observed?.attempts), 'authenticated_objective', 'OBSERVED_ATTEMPTS_INVALID');
    requireCondition(
      objectiveBody.model_reported?.evidence_lane === 'model_reported_not_observed_fact'
        && Array.isArray(objectiveBody.model_reported?.attempts),
      'authenticated_objective',
      'MODEL_REPORTED_LANE_INVALID',
    );
    noStore(authenticatedObjective, 'authenticated_objective');
    results.push(receipt('authenticated_objective', authenticatedObjective, {
      runtime_status: objectiveBody.observed.runtime.status,
      iterations_completed: objectiveBody.observed.runtime.iterations_completed,
    }));
  }

  const report = {
    schema_version: 'apocky.apocv4-owner-acceptance.v1',
    status: 'PASSED',
    mode,
    client_deadline_ms: CLIENT_DEADLINE_MS,
    credential_kind: bearerToken ? 'bearer' : cookie ? 'cookie' : 'none',
    results,
  };
  const serialized = JSON.stringify(report);
  requireCondition(!secrets.some((secret) => serialized.includes(secret)), 'report', 'CREDENTIAL_REPORT_REFLECTION');
  log(serialized);
  return report;
}

function failureReport(error) {
  if (error instanceof AcceptanceFailure) {
    return {
      schema_version: 'apocky.apocv4-owner-acceptance.v1',
      status: 'FAILED',
      step: error.step,
      code: error.code,
    };
  }
  return {
    schema_version: 'apocky.apocv4-owner-acceptance.v1',
    status: 'FAILED',
    step: 'unclassified',
    code: 'UNEXPECTED_FAILURE',
  };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  runOwnerAcceptance().catch((error) => {
    process.stderr.write(`${JSON.stringify(failureReport(error))}\n`);
    process.exitCode = 1;
  });
}
