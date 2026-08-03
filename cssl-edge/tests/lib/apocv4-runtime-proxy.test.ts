import {
  RuntimeProxyError,
  RUNPOD_SYNC_DEADLINE_MS,
  fetchRuntimeHealth,
  publicRuntimeError,
  submitRuntimeObjective,
  validateRuntimeUrl,
} from '@/lib/apocv4/runtime-proxy';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

async function rejectsCode(run: () => Promise<unknown>, code: string): Promise<void> {
  try {
    await run();
  } catch (error) {
    assert(error instanceof RuntimeProxyError, `${code} must be a RuntimeProxyError`);
    assert(error.code === code, `expected ${code}, got ${error.code}`);
    return;
  }
  throw new Error(`expected rejection: ${code}`);
}

const originalFetch = globalThis.fetch;
const originalRuntimeUrl = process.env.APOCV4_RUNTIME_URL;
const originalRuntimeToken = process.env.APOCV4_API_TOKEN;
const originalRuntimeTransport = process.env.APOCV4_RUNTIME_TRANSPORT;
const originalRuntimeIp = process.env.APOCV4_RUNTIME_DIRECT_IP;
const originalRuntimePort = process.env.APOCV4_RUNTIME_DIRECT_PORT;

async function main(): Promise<void> {
  try {
    const validOrigin = 'https://198.51.100.42:31234';
    const token = 'runtime-test-token-123';
    process.env.APOCV4_RUNTIME_URL = validOrigin;
    process.env.APOCV4_API_TOKEN = token;
    process.env.APOCV4_RUNTIME_TRANSPORT = 'test-fetch';
    process.env.APOCV4_RUNTIME_DIRECT_IP = '198.51.100.42';
    process.env.APOCV4_RUNTIME_DIRECT_PORT = '31234';
    assert(RUNPOD_SYNC_DEADLINE_MS === 95_000, 'bounded synchronous runtime deadline remains stable');
    const timeout = publicRuntimeError(
      new RuntimeProxyError('runtime_deadline_exceeded', 504, null, RUNPOD_SYNC_DEADLINE_MS),
    );
    assert(timeout.observed?.receipt.deadline_ms === 95_000, 'timeout carries observed deadline receipt');

    assert(validateRuntimeUrl(validOrigin) === validOrigin, 'canonical pinned direct origin accepted');
    for (const invalid of [
      'http://198.51.100.42:31234',
      'https://198.51.100.42:31234/',
      'https://198.51.100.42:31234/runtime',
      'https://198.51.100.42.evil.invalid:31234',
      'https://user:pass@198.51.100.42:31234',
      'https://198.51.100.43:31234',
      'https://198.51.100.42:31235',
      'https://198.51.100.42:31234?target=internal',
    ]) {
      let rejected = false;
      try {
        validateRuntimeUrl(invalid);
      } catch (error) {
        rejected = error instanceof RuntimeProxyError
          && error.code === 'runtime_configuration_invalid';
      }
      assert(rejected, `invalid origin rejected: ${invalid}`);
    }

    let capturedUrl = '';
    let capturedInit: RequestInit | undefined;
    globalThis.fetch = async (input, init) => {
      capturedUrl = String(input);
      capturedInit = init;
      return new Response(JSON.stringify({
        schema_version: 'apocv4.runtime-service.v1',
        status: 'READY',
        engine: { active_runs: 0, perpetual: true },
        vision: true,
      }), {
        status: 200,
        headers: {
          'Content-Type': 'application/json; charset=utf-8',
          'X-Apocv4-Auth-Mode': 'STRICT_REGISTRY',
          'X-Apocv4-Auth-Registry-Ref': 'a'.repeat(64),
        },
      });
    };
    const health = await fetchRuntimeHealth();
    assert(capturedUrl === `${validOrigin}/health`, 'health uses fixed allowlisted path');
    assert(capturedInit?.method === 'GET', 'health uses GET');
    assert(capturedInit?.redirect === 'error', 'redirects are forbidden');
    assert(capturedInit?.cache === 'no-store', 'runtime response is not cached');
    assert(new Headers(capturedInit?.headers).get('Accept-Encoding') === 'identity', 'runtime compression is disabled');
    assert(new Headers(capturedInit?.headers).get('Authorization') === `Bearer ${token}`, 'token is server transport only');
    assert(health.observed.runtime.status === 'READY', 'health is projected as observed runtime state');
    assert(health.model_reported.present === false, 'health does not invent model evidence');
    assert(!JSON.stringify(health).includes(token), 'health projection never returns credential');

    globalThis.fetch = async (input, init) => {
      capturedUrl = String(input);
      capturedInit = init;
      return new Response(JSON.stringify({
        schema_version: 'apocv4.runtime-service.v1',
        result: {
          schema_version: 'apocv4.agent-supervisor.v1',
          status: 'ACCEPTED',
          terminal_reason: 'observed_test_accepted',
          iterations_completed: 1,
          checkpoint_digest: 'b'.repeat(64),
          privacy_partition: 'must-not-be-projected',
          attempts: [{
            sequence: 1,
            cycle_status: 'ACCEPTED',
            test_passed: true,
            test_run_digest: 'c'.repeat(64),
            evidence_spine_digest: 'd'.repeat(64),
            active_model_id: 'fixture/model',
            candidate_digest: 'e'.repeat(64),
            council_decision: { evidence_lane: 'model_reported' },
          }],
        },
      }), {
        status: 200,
        headers: {
          'Content-Type': 'application/json',
          'X-Apocv4-Binding-Ref': 'f'.repeat(64),
          'X-Apocv4-Principal-Ref': '1'.repeat(64),
          'X-Apocv4-Privacy-Partition-Ref': '2'.repeat(64),
        },
      });
    };
    const objective = await submitRuntimeObjective('Build and verify one seam.');
    assert(String(capturedUrl) === `${validOrigin}/v1/objectives`, 'objective uses fixed runtime route');
    const upstreamBody = JSON.parse(String(capturedInit?.body)) as Record<string, unknown>;
    assert(Object.keys(upstreamBody).length === 2, 'proxy adds only the bounded iteration control');
    assert(upstreamBody.objective === 'Build and verify one seam.', 'objective forwarded exactly');
    assert(upstreamBody.max_iterations === 1, 'first owner objective is bounded to one council iteration');
    assert(!Object.hasOwn(upstreamBody, 'privacy_partition'), 'runtime binding owns privacy partition');
    assert(objective.observed.attempts[0]?.test_passed === true, 'test receipt is observed');
    assert(objective.model_reported.attempts[0]?.active_model_id === 'fixture/model', 'model identity is separately reported');
    assert(!JSON.stringify(objective).includes('must-not-be-projected'), 'partition name is not returned to browser');

    globalThis.fetch = async () => new Response('{}', {
      status: 200,
      headers: { 'Content-Type': 'application/json', 'Content-Length': String(300 * 1024) },
    });
    await rejectsCode(fetchRuntimeHealth, 'runtime_response_too_large');

    globalThis.fetch = async () => new Response(JSON.stringify({
      schema_version: 'apocv4.runtime-service.v1',
      status: 'READY',
      engine: { reflected: token },
      vision: false,
    }), { status: 200, headers: { 'Content-Type': 'application/json' } });
    await rejectsCode(fetchRuntimeHealth, 'runtime_reflected_credential');
  } finally {
    globalThis.fetch = originalFetch;
    if (originalRuntimeUrl === undefined) delete process.env.APOCV4_RUNTIME_URL;
    else process.env.APOCV4_RUNTIME_URL = originalRuntimeUrl;
    if (originalRuntimeToken === undefined) delete process.env.APOCV4_API_TOKEN;
    else process.env.APOCV4_API_TOKEN = originalRuntimeToken;
    if (originalRuntimeTransport === undefined) delete process.env.APOCV4_RUNTIME_TRANSPORT;
    else process.env.APOCV4_RUNTIME_TRANSPORT = originalRuntimeTransport;
    if (originalRuntimeIp === undefined) delete process.env.APOCV4_RUNTIME_DIRECT_IP;
    else process.env.APOCV4_RUNTIME_DIRECT_IP = originalRuntimeIp;
    if (originalRuntimePort === undefined) delete process.env.APOCV4_RUNTIME_DIRECT_PORT;
    else process.env.APOCV4_RUNTIME_DIRECT_PORT = originalRuntimePort;
  }
  console.log('apocv4-runtime-proxy.test : OK · strict RunPod transport and evidence split');
}

void main().catch((error: unknown) => {
  console.error(error);
  process.exitCode = 1;
});
