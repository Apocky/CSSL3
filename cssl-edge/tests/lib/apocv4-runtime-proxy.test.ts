import {
  RuntimeProxyError,
  RUNPOD_CODE_DEADLINE_MS,
  RUNPOD_SYNC_DEADLINE_MS,
  fetchRuntimeHealth,
  publicRuntimeError,
  submitRuntimeCode,
  submitRuntimeObjective,
  submitRuntimeRollback,
  validateRuntimeCodePaths,
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

function rejectsSync(run: () => unknown, code: string): void {
  try {
    run();
  } catch (error) {
    assert(error instanceof RuntimeProxyError, `${code} must be a RuntimeProxyError`);
    assert(error.code === code, `expected ${code}, got ${error.code}`);
    return;
  }
  throw new Error(`expected rejection: ${code}`);
}

function digest(character: string): string {
  return character.repeat(64);
}

function promotedCodeEnvelope(objective: string, privacyPartition: string): Record<string, unknown> {
  return {
    schema_version: 'apocv4.runtime-service.v1',
    result: {
      schema_version: 'apocv4.journaled-patch-runtime.v1',
      state: 'PROMOTED',
      frame_digest: digest('1'),
      authority_digest: digest('2'),
      proposal_digest: digest('3'),
      faculty_attempts: [{
        index: 0,
        faculty_identity_digest: digest('4'),
        status: 'ACCEPTED_FOR_ADMISSION',
        proposal_digest: digest('3'),
        error_class: null,
        error_digest: null,
      }],
      request_digest: digest('5'),
      approval_digest: digest('6'),
      admission: {
        allowed: true,
        reason_code: 'admitted',
        frame_digest: digest('1'),
        authority_digest: digest('2'),
        request_digest: digest('5'),
        approval_digest: digest('6'),
      },
      reservation_event_digest: digest('7'),
      isolated_outcome: {
        state: 'ACCEPTED_ISOLATED',
        proposal_digest: digest('3'),
        source_prestate_digest: digest('8'),
        worktree_root: '/private/runtime/worktree-must-not-be-projected',
        lease_digest: digest('9'),
        delta_digest: digest('a'),
        test_receipt: {
          command_digest: digest('b'),
          runner_contract_digest: digest('c'),
          exit_code: 0,
          timed_out: false,
          error_class: null,
          stdout_sha256: digest('d'),
          stdout_bytes: 21,
          stderr_sha256: digest('e'),
          stderr_bytes: 0,
          elapsed_ms: 12.5,
          passed: true,
          receipt_digest: digest('f'),
        },
        failure_class: null,
        failure_digest: null,
        outcome_digest: digest('0'),
      },
      promotion_prepared_event_digest: digest('a'),
      promotion_event_digest: digest('b'),
      terminal_event_digest: digest('b'),
      journal_tip_digest: digest('c'),
      requested_objective: objective,
      privacy_partition: privacyPartition,
      perception_frame_digest: digest('d'),
      vision_observation_digests: [],
      faculty_team_id: 'apocv4-test-team',
      raw_private_marker: 'must-not-be-projected',
    },
  };
}

function rollbackEnvelope(promotionEventDigest: string): Record<string, unknown> {
  return {
    schema_version: 'apocv4.runtime-service.v1',
    result: {
      schema_version: 'apocv4.journaled-patch-runtime.v1',
      state: 'ROLLED_BACK',
      promotion_event_digest: promotionEventDigest,
      rollback_event_digest: digest('d'),
      journal_tip_digest: digest('e'),
      operation_ref: digest('f'),
    },
  };
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
    assert(RUNPOD_CODE_DEADLINE_MS === 240_000, 'bounded coding deadline remains below the edge limit');
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

    const allowedPaths = ['src/apocv4/example.py', 'tests/test_example.py'];
    assert(
      JSON.stringify(validateRuntimeCodePaths(allowedPaths)) === JSON.stringify(allowedPaths),
      'canonical sorted code paths are accepted exactly',
    );
    for (const protectedPaths of [
      ['env.txt'],
      ['.env.production'],
      ['config/runpod-runtime-auth.json'],
      ['deploy/runpod/apocv4-apex-service.sh'],
      ['specs/wayfinder/APOCV4_GOAL_OBJECTIVE_2026-07-31.csl'],
      ['src/apocv4/runtime_service.py'],
      ['notes/private-key.pem'],
      ['z.py', 'a.py'],
      ['src/A.py', 'src/a.py'],
    ]) {
      rejectsSync(
        () => validateRuntimeCodePaths(protectedPaths),
        'code_request_invalid',
      );
    }

    const codeObjective = 'Implement the bounded fixture and keep the exact operator tests green.';
    const privacyPartition = 'owner:apocky';
    globalThis.fetch = async (input, init) => {
      capturedUrl = String(input);
      capturedInit = init;
      return new Response(JSON.stringify(promotedCodeEnvelope(codeObjective, privacyPartition)), {
        status: 200,
        headers: {
          'Content-Type': 'application/json',
          'X-Apocv4-Auth-Mode': 'STRICT_REGISTRY',
          'X-Apocv4-Auth-Registry-Ref': digest('0'),
          'X-Apocv4-Binding-Ref': digest('1'),
          'X-Apocv4-Principal-Ref': digest('2'),
          'X-Apocv4-Privacy-Partition-Ref': digest('3'),
          'X-Apocv4-Effect-Scope-Ref': digest('4'),
        },
      });
    };
    const code = await submitRuntimeCode({
      objective: codeObjective,
      allowedPaths,
      privacyPartition,
    });
    assert(String(capturedUrl) === `${validOrigin}/v1/code`, 'code uses fixed runtime route');
    const codeUpstream = JSON.parse(String(capturedInit?.body)) as Record<string, unknown>;
    assert(
      JSON.stringify(Object.keys(codeUpstream).sort())
        === JSON.stringify(['allowed_paths', 'objective', 'privacy_partition']),
      'code forwards exactly the runtime contract keys',
    );
    assert(codeUpstream.objective === codeObjective, 'code objective is forwarded exactly');
    assert(codeUpstream.privacy_partition === privacyPartition, 'server-selected code partition is forwarded exactly');
    assert(JSON.stringify(codeUpstream.allowed_paths) === JSON.stringify(allowedPaths), 'allowed paths are forwarded exactly');
    assert(code.observed.runtime.state === 'PROMOTED', 'promoted state is observed');
    assert(code.observed.test?.passed === true, 'isolated test receipt is projected as observed evidence');
    assert(code.observed.receipt.effect_scope_ref === digest('4'), 'effect scope receipt is preserved');
    assert(
      JSON.stringify(code.generated.requested_allowed_paths) === JSON.stringify(allowedPaths),
      'requested allowlist is labeled separately from generated artifacts',
    );
    assert(!Object.hasOwn(code.generated, 'target_paths'), 'proxy does not invent actual changed paths');
    const codeProjection = JSON.stringify(code);
    assert(!codeProjection.includes(privacyPartition), 'raw privacy partition is not projected');
    assert(!codeProjection.includes(codeObjective), 'raw coding objective is not projected');
    assert(!codeProjection.includes('/private/runtime/'), 'server worktree path is not projected');
    assert(!codeProjection.includes('must-not-be-projected'), 'unknown upstream fields are not projected');
    assert(!codeProjection.includes(token), 'runtime credential never crosses the proxy');

    globalThis.fetch = async () => new Response(
      JSON.stringify(promotedCodeEnvelope(codeObjective, privacyPartition)),
      {
        status: 200,
        headers: {
          'Content-Type': 'application/json',
          'X-Apocv4-Auth-Mode': 'STRICT_REGISTRY',
          'X-Apocv4-Binding-Ref': digest('1'),
          'X-Apocv4-Principal-Ref': digest('2'),
          'X-Apocv4-Privacy-Partition-Ref': digest('3'),
          'X-Apocv4-Effect-Scope-Ref': digest('4'),
        },
      },
    );
    await rejectsCode(
      () => submitRuntimeCode({ objective: codeObjective, allowedPaths, privacyPartition }),
      'runtime_effect_attestation_invalid',
    );

    const promotionEventDigest = digest('b');
    globalThis.fetch = async (input, init) => {
      capturedUrl = String(input);
      capturedInit = init;
      return new Response(JSON.stringify(rollbackEnvelope(promotionEventDigest)), {
        status: 200,
        headers: {
          'Content-Type': 'application/json',
          'X-Apocv4-Auth-Mode': 'STRICT_REGISTRY',
          'X-Apocv4-Auth-Registry-Ref': digest('0'),
          'X-Apocv4-Binding-Ref': digest('1'),
          'X-Apocv4-Principal-Ref': digest('2'),
          'X-Apocv4-Privacy-Partition-Ref': digest('3'),
          'X-Apocv4-Rollback-Lease-Ref': digest('4'),
        },
      });
    };
    const rollback = await submitRuntimeRollback(promotionEventDigest);
    assert(String(capturedUrl) === `${validOrigin}/v1/code/rollback`, 'rollback uses fixed runtime route');
    const rollbackUpstream = JSON.parse(String(capturedInit?.body)) as Record<string, unknown>;
    assert(
      JSON.stringify(Object.keys(rollbackUpstream)) === JSON.stringify(['promotion_event_digest']),
      'rollback forwards exactly one receipt key',
    );
    assert(rollback.observed.runtime.state === 'ROLLED_BACK', 'rollback state is observed');
    assert(
      rollback.observed.runtime.journal_tip_digest === digest('e'),
      'rollback receipt preserves journal tip',
    );
    assert(rollback.observed.runtime.operation_ref === digest('f'), 'rollback operation identity is preserved');

    globalThis.fetch = async () => {
      const invalid = rollbackEnvelope(promotionEventDigest);
      const result = invalid.result as Record<string, unknown>;
      result.unexpected = true;
      return new Response(JSON.stringify(invalid), {
        status: 200,
        headers: {
          'Content-Type': 'application/json',
          'X-Apocv4-Auth-Mode': 'STRICT_REGISTRY',
          'X-Apocv4-Auth-Registry-Ref': digest('0'),
          'X-Apocv4-Binding-Ref': digest('1'),
          'X-Apocv4-Principal-Ref': digest('2'),
          'X-Apocv4-Privacy-Partition-Ref': digest('3'),
          'X-Apocv4-Rollback-Lease-Ref': digest('4'),
        },
      });
    };
    await rejectsCode(
      () => submitRuntimeRollback(promotionEventDigest),
      'runtime_response_invalid',
    );

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
