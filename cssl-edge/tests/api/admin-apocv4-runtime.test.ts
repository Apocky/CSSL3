import type { NextApiRequest, NextApiResponse } from 'next';

import { _resetOperationalTelemetryForTests } from '@/lib/telemetry/server';
import codeHandler, { maxDuration as codeMaxDuration } from '@/pages/api/admin/apocv4/code';
import rollbackHandler, { maxDuration as rollbackMaxDuration } from '@/pages/api/admin/apocv4/code/rollback';
import healthHandler, { maxDuration as healthMaxDuration } from '@/pages/api/admin/apocv4/health';
import objectiveHandler, { maxDuration as objectiveMaxDuration } from '@/pages/api/admin/apocv4/objective';

type Body = Record<string, unknown>;

interface RequestOptions {
  admin?: boolean;
  contentType?: string | null;
  origin?: string | null;
  route?: string;
}

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

function digest(character: string): string {
  return character.repeat(64);
}

function responseHarness(): {
  response: NextApiResponse;
  read: () => { status: number; body: Body | null; headers: Record<string, string> };
} {
  let status = 200;
  let body: Body | null = null;
  const headers: Record<string, string> = {};
  const response = {
    statusCode: 200,
    setHeader(name: string, value: string | number | readonly string[]) {
      headers[name.toLowerCase()] = Array.isArray(value) ? value.join(', ') : String(value);
      return this;
    },
    status(this: { statusCode: number }, code: number) {
      status = code;
      this.statusCode = code;
      return this;
    },
    json(value: Body) {
      body = value;
      return this;
    },
  } as unknown as NextApiResponse;
  return { response, read: () => ({ status, body, headers }) };
}

function request(method: string, body?: unknown, options: RequestOptions = {}): NextApiRequest {
  const headers: Record<string, string> = {
    'x-forwarded-host': 'apocky.com',
    'x-forwarded-proto': 'https',
  };
  if (options.admin !== false) headers['x-apocky-test-admin-email'] = 'owner@example.com';
  if (options.origin !== null) headers.origin = options.origin ?? 'https://apocky.com';
  if (options.contentType !== null) headers['content-type'] = options.contentType ?? 'application/json';
  return {
    method,
    body,
    headers,
    url: options.route ?? '/api/admin/apocv4/test',
  } as unknown as NextApiRequest;
}

function promotedCodeEnvelope(objective: string, privacyPartition: string): Body {
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
        worktree_root: '/private/runtime/worktree-must-not-cross-api',
        lease_digest: digest('9'),
        delta_digest: digest('a'),
        test_receipt: {
          command_digest: digest('b'),
          runner_contract_digest: digest('c'),
          exit_code: 0,
          timed_out: false,
          error_class: null,
          stdout_sha256: digest('d'),
          stdout_bytes: 13,
          stderr_sha256: digest('e'),
          stderr_bytes: 0,
          elapsed_ms: 8.25,
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
    },
  };
}

function rollbackEnvelope(promotionEventDigest: string): Body {
  return {
    schema_version: 'apocv4.runtime-service.v1',
    result: {
      schema_version: 'apocv4.journaled-patch-runtime.v1',
      state: 'ROLLED_BACK',
      promotion_event_digest: promotionEventDigest,
      rollback_event_digest: digest('d'),
      journal_tip_digest: digest('e'),
    },
  };
}

const originalFetch = globalThis.fetch;
const originalEnv = {
  nodeEnv: process.env.NODE_ENV,
  bypass: process.env.LAZARUS_TEST_AUTH_BYPASS,
  admins: process.env.APOCKY_ADMIN_EMAILS,
  runtimeUrl: process.env.APOCV4_RUNTIME_URL,
  runtimeToken: process.env.APOCV4_API_TOKEN,
  runtimeTransport: process.env.APOCV4_RUNTIME_TRANSPORT,
  runtimeIp: process.env.APOCV4_RUNTIME_DIRECT_IP,
  runtimePort: process.env.APOCV4_RUNTIME_DIRECT_PORT,
  hubSupabaseUrl: process.env.APOCKY_HUB_SUPABASE_URL,
  hubSupabaseKey: process.env.APOCKY_HUB_SUPABASE_SERVICE_ROLE_KEY,
  publicSupabaseUrl: process.env.NEXT_PUBLIC_SUPABASE_URL,
  supabaseKey: process.env.SUPABASE_SERVICE_ROLE_KEY,
};

async function main(): Promise<void> {
  let fetchCount = 0;
  let capturedUrl = '';
  let capturedInit: RequestInit | undefined;
  try {
    (process.env as Record<string, string | undefined>).NODE_ENV = 'test';
    process.env.LAZARUS_TEST_AUTH_BYPASS = '1';
    process.env.APOCKY_ADMIN_EMAILS = 'owner@example.com';
    process.env.APOCV4_RUNTIME_URL = 'https://198.51.100.42:31234';
    process.env.APOCV4_API_TOKEN = 'server-runtime-token-123';
    process.env.APOCV4_RUNTIME_TRANSPORT = 'test-fetch';
    process.env.APOCV4_RUNTIME_DIRECT_IP = '198.51.100.42';
    process.env.APOCV4_RUNTIME_DIRECT_PORT = '31234';
    delete process.env.APOCKY_HUB_SUPABASE_URL;
    delete process.env.APOCKY_HUB_SUPABASE_SERVICE_ROLE_KEY;
    delete process.env.NEXT_PUBLIC_SUPABASE_URL;
    delete process.env.SUPABASE_SERVICE_ROLE_KEY;
    _resetOperationalTelemetryForTests();

    assert(healthMaxDuration === 20, 'health route has short function duration');
    assert(objectiveMaxDuration === 300, 'objective route has 300-second Vercel duration');
    assert(codeMaxDuration === 300, 'code route has full bounded coding duration');
    assert(rollbackMaxDuration === 60, 'rollback route has bounded recovery duration');

    globalThis.fetch = async (input, init) => {
      fetchCount += 1;
      capturedUrl = String(input);
      capturedInit = init;
      const url = String(input);
      if (url.endsWith('/health')) {
        return new Response(JSON.stringify({
          schema_version: 'apocv4.runtime-service.v1',
          status: 'READY',
          engine: { perpetual: true },
          vision: false,
        }), { status: 200, headers: { 'Content-Type': 'application/json' } });
      }
      const upstream = JSON.parse(String(init?.body)) as Record<string, unknown>;
      if (url.endsWith('/v1/objectives')) {
        assert(!Object.hasOwn(upstream, 'privacy_partition'), 'objective API never forwards a client partition');
        return new Response(JSON.stringify({
          schema_version: 'apocv4.runtime-service.v1',
          result: { status: 'BUDGET_EXHAUSTED', iterations_completed: 1, attempts: [] },
        }), { status: 200, headers: { 'Content-Type': 'application/json' } });
      }
      if (url.endsWith('/v1/code')) {
        assert(
          JSON.stringify(Object.keys(upstream).sort())
            === JSON.stringify(['allowed_paths', 'objective', 'privacy_partition']),
          'code API forwards exactly the runtime contract',
        );
        assert(upstream.privacy_partition === 'owner:apocky', 'code API pins owner privacy partition server-side');
        assert(
          JSON.stringify(upstream.allowed_paths) === JSON.stringify(['src/apocv4/example.py', 'tests/test_example.py']),
          'code API forwards exact sorted allowlist',
        );
        return new Response(JSON.stringify(promotedCodeEnvelope(
          String(upstream.objective),
          String(upstream.privacy_partition),
        )), {
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
      }
      if (url.endsWith('/v1/code/rollback')) {
        assert(
          JSON.stringify(Object.keys(upstream)) === JSON.stringify(['promotion_event_digest']),
          'rollback API forwards exactly the promotion receipt',
        );
        return new Response(JSON.stringify(rollbackEnvelope(String(upstream.promotion_event_digest))), {
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
      }
      throw new Error(`unexpected runtime route: ${url}`);
    };

    {
      const harness = responseHarness();
      await healthHandler(request('GET', undefined, { admin: false }), harness.response);
      const result = harness.read();
      assert(result.status === 401, 'non-admin health request denied');
      assert(fetchCount === 0, 'unauthorized health never reaches runtime');
    }
    {
      const harness = responseHarness();
      await healthHandler(request('GET'), harness.response);
      const result = harness.read();
      assert(result.status === 200, 'admin health succeeds');
      assert(result.body?.kind === 'health', 'health projection returned');
      assert(result.headers['cache-control'] === 'no-store, max-age=0', 'health is non-cacheable');
    }
    {
      const before: number = fetchCount;
      const harness = responseHarness();
      await objectiveHandler(
        request('POST', { objective: 'Must not reach runtime.' }, { admin: false }),
        harness.response,
      );
      const result = harness.read();
      assert(result.status === 401, 'non-admin objective request denied');
      assert(fetchCount === before, 'unauthorized objective never reaches runtime');
    }
    {
      const before: number = fetchCount;
      const harness = responseHarness();
      await objectiveHandler(request('POST', {
        objective: 'Attempt cross partition.',
        privacy_partition: 'attacker-choice',
      }), harness.response);
      const result = harness.read();
      assert(result.status === 400, 'extra objective partition field rejected');
      assert(result.body?.error === 'objective_body_invalid', 'exact objective body error returned');
      assert(fetchCount === before, 'invalid objective body never reaches runtime');
    }
    {
      const harness = responseHarness();
      await objectiveHandler(request('POST', { objective: 'Run one bounded objective.' }), harness.response);
      const result = harness.read();
      assert(result.status === 200, 'admin objective succeeds');
      assert(result.body?.kind === 'objective', 'objective evidence projection returned');
      assert(!JSON.stringify(result.body).includes('server-runtime-token-123'), 'token never crosses objective API');
      assert(result.headers['cache-control'] === 'no-store, max-age=0', 'objective is non-cacheable');
    }

    const validCodeBody = {
      objective: 'Implement one bounded fixture and preserve the tests.',
      allowed_paths: ['src/apocv4/example.py', 'tests/test_example.py'],
      confirm_apply: true,
    };
    {
      const before: number = fetchCount;
      const harness = responseHarness();
      await codeHandler(request('POST', validCodeBody, { admin: false }), harness.response);
      const result = harness.read();
      assert(result.status === 401, 'non-admin code effect denied');
      assert(fetchCount === before, 'unauthorized code effect never reaches runtime');
    }
    for (const origin of [null, 'https://attacker.invalid']) {
      const before: number = fetchCount;
      const harness = responseHarness();
      await codeHandler(request('POST', validCodeBody, { origin }), harness.response);
      const result = harness.read();
      assert(result.status === 403, 'code effect requires exact same origin');
      assert(fetchCount === before, 'cross-origin code effect never reaches runtime');
    }
    {
      const before: number = fetchCount;
      const harness = responseHarness();
      await codeHandler(request('POST', validCodeBody, { contentType: 'application/json; charset=utf-8' }), harness.response);
      const result = harness.read();
      assert(result.status === 415, 'code effect requires exact JSON media type');
      assert(fetchCount === before, 'invalid code content type never reaches runtime');
    }
    for (const invalidBody of [
      { objective: validCodeBody.objective, allowed_paths: validCodeBody.allowed_paths },
      { ...validCodeBody, confirm_apply: false },
      { ...validCodeBody, privacy_partition: 'attacker-choice' },
      { ...validCodeBody, allowed_paths: ['env.txt'] },
      { ...validCodeBody, allowed_paths: ['tests/test_example.py', 'src/apocv4/example.py'] },
    ]) {
      const before: number = fetchCount;
      const harness = responseHarness();
      await codeHandler(request('POST', invalidBody), harness.response);
      const result = harness.read();
      assert(result.status === 400, 'code exact body and path boundary reject malformed input');
      assert(result.body?.error === 'code_body_invalid', 'code exact-body error is typed');
      assert(fetchCount === before, 'invalid code body never reaches runtime');
    }
    {
      const harness = responseHarness();
      await codeHandler(request('POST', validCodeBody), harness.response);
      const result = harness.read();
      assert(capturedUrl.endsWith('/v1/code'), 'code API dispatches fixed runtime route');
      assert(new Headers(capturedInit?.headers).get('Authorization') === 'Bearer server-runtime-token-123', 'runtime token stays server-side');
      assert(result.status === 200, 'owner-confirmed code effect succeeds');
      assert(result.body?.kind === 'code', 'safe code projection returned');
      assert(
        result.headers['cache-control'] === 'private, no-store, no-cache, must-revalidate, max-age=0',
        'code response is private and non-cacheable',
      );
      const encoded = JSON.stringify(result.body);
      assert(!encoded.includes('owner:apocky'), 'raw owner partition never crosses code API');
      assert(!encoded.includes('/private/runtime/'), 'runtime worktree path never crosses code API');
      assert(!encoded.includes('server-runtime-token-123'), 'runtime token never crosses code API');
      const generated = result.body?.generated as Body | undefined;
      assert(Array.isArray(generated?.requested_allowed_paths), 'requested scope is explicitly projected');
      assert(!generated || !Object.hasOwn(generated, 'target_paths'), 'API does not invent actual changed paths');
    }
    {
      const harness = responseHarness();
      await codeHandler(request('GET'), harness.response);
      const result = harness.read();
      assert(result.status === 405, 'code route permits POST only');
      assert(result.headers.allow === 'POST', 'code Allow header is exact');
    }

    const promotionEventDigest = digest('b');
    const validRollbackBody = {
      promotion_event_digest: promotionEventDigest,
      confirm_rollback: true,
    };
    {
      const before: number = fetchCount;
      const harness = responseHarness();
      await rollbackHandler(request('POST', validRollbackBody, { admin: false }), harness.response);
      const result = harness.read();
      assert(result.status === 401, 'non-admin rollback denied');
      assert(fetchCount === before, 'unauthorized rollback never reaches runtime');
    }
    {
      const before: number = fetchCount;
      const harness = responseHarness();
      await rollbackHandler(request('POST', validRollbackBody, { origin: 'https://attacker.invalid' }), harness.response);
      const result = harness.read();
      assert(result.status === 403, 'rollback requires exact same origin');
      assert(fetchCount === before, 'cross-origin rollback never reaches runtime');
    }
    {
      const before: number = fetchCount;
      const harness = responseHarness();
      await rollbackHandler(request('POST', validRollbackBody, { contentType: 'text/plain' }), harness.response);
      const result = harness.read();
      assert(result.status === 415, 'rollback requires exact JSON media type');
      assert(fetchCount === before, 'invalid rollback content type never reaches runtime');
    }
    for (const invalidBody of [
      { promotion_event_digest: promotionEventDigest },
      { ...validRollbackBody, confirm_rollback: false },
      { ...validRollbackBody, extra: true },
      { ...validRollbackBody, promotion_event_digest: 'not-a-digest' },
    ]) {
      const before: number = fetchCount;
      const harness = responseHarness();
      await rollbackHandler(request('POST', invalidBody), harness.response);
      const result = harness.read();
      assert(result.status === 400, 'rollback exact body rejects malformed input');
      assert(result.body?.error === 'rollback_body_invalid', 'rollback exact-body error is typed');
      assert(fetchCount === before, 'invalid rollback body never reaches runtime');
    }
    {
      const harness = responseHarness();
      await rollbackHandler(request('POST', validRollbackBody), harness.response);
      const result = harness.read();
      assert(capturedUrl.endsWith('/v1/code/rollback'), 'rollback API dispatches fixed runtime route');
      assert(result.status === 200, 'owner-confirmed rollback succeeds');
      assert(result.body?.kind === 'rollback', 'safe rollback projection returned');
      const observed = result.body?.observed as Body | undefined;
      const runtime = observed?.runtime as Body | undefined;
      assert(runtime?.schema_version === 'apocv4.journaled-patch-runtime.v1', 'rollback schema is preserved');
      assert(runtime?.state === 'ROLLED_BACK', 'rollback effect state is preserved');
      assert(runtime?.journal_tip_digest === digest('e'), 'five-key rollback journal receipt is preserved');
      assert(!JSON.stringify(result.body).includes('server-runtime-token-123'), 'runtime token never crosses rollback API');
      assert(
        result.headers['cache-control'] === 'private, no-store, no-cache, must-revalidate, max-age=0',
        'rollback response is private and non-cacheable',
      );
    }
    {
      const harness = responseHarness();
      await rollbackHandler(request('PUT'), harness.response);
      const result = harness.read();
      assert(result.status === 405, 'rollback route permits POST only');
      assert(result.headers.allow === 'POST', 'rollback Allow header is exact');
    }
  } finally {
    globalThis.fetch = originalFetch;
    const restore: Record<keyof typeof originalEnv, string> = {
      nodeEnv: 'NODE_ENV',
      bypass: 'LAZARUS_TEST_AUTH_BYPASS',
      admins: 'APOCKY_ADMIN_EMAILS',
      runtimeUrl: 'APOCV4_RUNTIME_URL',
      runtimeToken: 'APOCV4_API_TOKEN',
      runtimeTransport: 'APOCV4_RUNTIME_TRANSPORT',
      runtimeIp: 'APOCV4_RUNTIME_DIRECT_IP',
      runtimePort: 'APOCV4_RUNTIME_DIRECT_PORT',
      hubSupabaseUrl: 'APOCKY_HUB_SUPABASE_URL',
      hubSupabaseKey: 'APOCKY_HUB_SUPABASE_SERVICE_ROLE_KEY',
      publicSupabaseUrl: 'NEXT_PUBLIC_SUPABASE_URL',
      supabaseKey: 'SUPABASE_SERVICE_ROLE_KEY',
    };
    for (const key of Object.keys(originalEnv) as Array<keyof typeof originalEnv>) {
      const envName = restore[key];
      const value = originalEnv[key];
      if (value === undefined) delete process.env[envName];
      else process.env[envName] = value;
    }
    _resetOperationalTelemetryForTests();
  }
  console.log('admin-apocv4-runtime.test : OK · auth + origin + exact effect contracts');
}

void main().catch((error: unknown) => {
  console.error(error);
  process.exitCode = 1;
});
