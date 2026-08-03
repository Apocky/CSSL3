import { createHmac, randomUUID } from 'node:crypto';

import type { NextApiRequest, NextApiResponse } from 'next';

import { publicMemberPrincipalRef } from '@/lib/apocv4/session-principal';
import codeHandler from '@/pages/api/admin/apocv4/code';
import rollbackHandler from '@/pages/api/admin/apocv4/code/rollback';

interface Output {
  statusCode: number;
  body: Record<string, unknown>;
  headers: Record<string, string>;
}

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

function equal(actual: unknown, expected: unknown, message: string): void {
  if (actual !== expected) {
    throw new Error(`assert failed: ${message}; expected=${String(expected)} actual=${String(actual)}`);
  }
}

function reqRes(body: unknown): { req: NextApiRequest; res: NextApiResponse; out: Output } {
  const out: Output = { statusCode: 0, body: {}, headers: {} };
  const req = {
    method: 'POST',
    body,
    query: {},
    headers: {
      host: 'www.apocky.com',
      origin: 'https://www.apocky.com',
      'x-forwarded-proto': 'https',
      'content-type': 'application/json',
      'x-apocky-test-admin-email': 'owner@example.test',
    },
  } as unknown as NextApiRequest;
  const res = {
    status(code: number) { out.statusCode = code; return this; },
    json(value: unknown) { out.body = value as Record<string, unknown>; return this; },
    setHeader(name: string, value: string | number | readonly string[]) {
      out.headers[name.toLowerCase()] = Array.isArray(value) ? value.join(', ') : String(value);
      return this;
    },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

function digest(character: string): string {
  return character.repeat(64);
}

function canonicalJson(value: unknown): string {
  if (value === null || typeof value === 'string' || typeof value === 'boolean' || typeof value === 'number') {
    return JSON.stringify(value);
  }
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`;
  const record = value as Record<string, unknown>;
  return `{${Object.keys(record).sort().map((key) => (
    `${JSON.stringify(key)}:${canonicalJson(record[key])}`
  )).join(',')}}`;
}

function expectedMac(path: string, body: Record<string, unknown>): string {
  const unsigned = { ...body };
  delete unsigned.session_binding_mac;
  return createHmac('sha256', 's'.repeat(64))
    .update(canonicalJson({
      schema_version: 'apocv4.session-binding.v1',
      path,
      body: unsigned,
    }), 'utf8')
    .digest('hex');
}

function codeResult(
  body: Record<string, unknown>,
  options: { wrongSession?: boolean; globalTip?: boolean } = {},
): Record<string, unknown> {
  const proposal = digest('3');
  const promotion = digest('b');
  return {
    schema_version: 'apocv4.journaled-patch-runtime.v1',
    state: 'PROMOTED',
    frame_digest: digest('1'),
    authority_digest: digest('2'),
    proposal_digest: proposal,
    faculty_attempts: [],
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
      proposal_digest: proposal,
      source_prestate_digest: digest('8'),
      lease_digest: digest('9'),
      delta_digest: digest('a'),
      test_receipt: {
        command_digest: digest('b'),
        runner_contract_digest: digest('c'),
        exit_code: 0,
        timed_out: false,
        error_class: null,
        stdout_sha256: digest('d'),
        stdout_bytes: 1,
        stderr_sha256: digest('e'),
        stderr_bytes: 0,
        elapsed_ms: 1,
        passed: true,
        receipt_digest: digest('f'),
      },
      failure_class: null,
      failure_digest: null,
      outcome_digest: digest('0'),
    },
    promotion_prepared_event_digest: digest('a'),
    promotion_event_digest: promotion,
    terminal_event_digest: promotion,
    journal_tip_digest: digest('c'),
    requested_objective: body.objective,
    privacy_partition: body.privacy_partition,
    session_id: options.wrongSession ? randomUUID() : body.session_id,
    request_id: body.request_id,
    session_event_digests: {
      code_request: digest('1'),
      code_proposal: digest('2'),
      code_effect: digest('3'),
      rollback: null,
    },
    session_tip_digest: digest('3'),
    durable_replay: false,
    ...(options.globalTip ? { ledger_tip_digest: digest('9') } : {}),
  };
}

function rollbackResult(body: Record<string, unknown>): Record<string, unknown> {
  return {
    schema_version: 'apocv4.journaled-patch-runtime.v1',
    state: 'ROLLED_BACK',
    promotion_event_digest: body.promotion_event_digest,
    rollback_event_digest: digest('d'),
    journal_tip_digest: digest('e'),
    operation_ref: digest('f'),
    session_id: body.session_id,
    request_id: body.request_id,
    session_event_digests: { rollback: digest('4') },
    session_tip_digest: digest('4'),
    durable_replay: false,
  };
}

const STRICT_HEADERS = {
  'Content-Type': 'application/json',
  'X-Apocv4-Auth-Mode': 'STRICT_REGISTRY',
  'X-Apocv4-Auth-Registry-Ref': digest('0'),
  'X-Apocv4-Binding-Ref': digest('1'),
  'X-Apocv4-Principal-Ref': digest('2'),
  'X-Apocv4-Privacy-Partition-Ref': digest('3'),
  'X-Apocv4-Effect-Scope-Ref': digest('4'),
  'X-Apocv4-Rollback-Lease-Ref': digest('5'),
  'X-Apocv4-Session-Binding': 'VERIFIED',
};

async function main(): Promise<void> {
  const originalEnv = { ...process.env };
  const originalFetch = globalThis.fetch;
  try {
    Object.assign(process.env, {
      NODE_ENV: 'test',
      LAZARUS_TEST_AUTH_BYPASS: '1',
      APOCKY_ADMIN_EMAILS: 'owner@example.test',
      APOCV4_RUNTIME_URL: 'https://203.0.113.10:9443',
      APOCV4_RUNTIME_DIRECT_IP: '203.0.113.10',
      APOCV4_RUNTIME_DIRECT_PORT: '9443',
      APOCV4_RUNTIME_TRANSPORT: 'test-fetch',
      APOCV4_API_TOKEN: 'owner-token',
      APOCV4_SESSION_BINDING_SECRET: 's'.repeat(64),
    });

    const calls: Array<{ url: string; body: Record<string, unknown> }> = [];
    let wrongSession = false;
    let globalTip = false;
    let verifiedHeader = true;
    globalThis.fetch = async (input, init) => {
      const url = String(input);
      const body = JSON.parse(String(init?.body ?? '{}')) as Record<string, unknown>;
      calls.push({ url, body });
      const result = url.endsWith('/v1/code/rollback')
        ? rollbackResult(body)
        : codeResult(body, { wrongSession, globalTip });
      return new Response(JSON.stringify({
        schema_version: 'apocv4.runtime-service.v1',
        result,
      }), {
        status: 200,
        headers: {
          ...STRICT_HEADERS,
          ...(verifiedHeader ? {} : { 'X-Apocv4-Session-Binding': 'MISSING' }),
        },
      });
    };

    const sessionId = randomUUID();
    const requestId = randomUUID();
    const objective = 'Implement this bounded, durable owner change.';
    const codeBody = {
      objective,
      allowed_paths: ['src/apocv4/example.py'],
      confirm_apply: true,
      session_id: sessionId,
      request_id: requestId,
    };
    const code = reqRes(codeBody);
    await codeHandler(code.req, code.res);
    equal(code.out.statusCode, 200, 'durable owner code effect succeeds');
    const runtimeCall = calls.at(-1);
    assert(runtimeCall?.url.endsWith('/v1/code'), 'code uses the durable runtime route');
    equal(runtimeCall?.body.session_id, sessionId, 'runtime code is bound to the client session');
    equal(
      runtimeCall?.body.session_principal,
      publicMemberPrincipalRef('test-admin'),
      'owner effect uses the same member principal as chat and session reads',
    );
    assert(runtimeCall?.body.request_id !== requestId, 'browser request ID is principal scoped upstream');
    equal(
      runtimeCall?.body.session_binding_mac,
      expectedMac('/v1/code', runtimeCall!.body),
      'code HMAC binds the exact durable request body',
    );
    const mutated = { ...runtimeCall!.body, session_id: randomUUID() };
    assert(
      runtimeCall?.body.session_binding_mac !== expectedMac('/v1/code', mutated),
      'the detached MAC cannot be replayed for a different session',
    );
    const codeObserved = code.out.body.observed as Record<string, unknown>;
    const codeRuntime = codeObserved.runtime as Record<string, unknown>;
    equal(codeRuntime.session_id, sessionId, 'browser receipt echoes the durable client session');
    equal(codeRuntime.request_id, requestId, 'browser receipt reprojects only the client request ID');
    assert(
      (codeRuntime.session_event_digests as Record<string, unknown>).code_effect === digest('3'),
      'browser receives the durable code effect event digest',
    );
    assert(!JSON.stringify(code.out.body).includes(String(runtimeCall?.body.request_id)), 'scoped request ID is not exposed');
    assert(!JSON.stringify(code.out.body).includes('principal:apocky-member:'), 'member principal is not exposed');

    const missingIds = reqRes({ objective, allowed_paths: ['src/apocv4/example.py'], confirm_apply: true });
    const beforeMissing = calls.length;
    await codeHandler(missingIds.req, missingIds.res);
    equal(missingIds.out.statusCode, 400, 'code requires durable session and request IDs');
    equal(calls.length, beforeMissing, 'invalid code body never reaches the runtime');

    const forgedPrincipal = reqRes({ ...codeBody, session_principal: publicMemberPrincipalRef('attacker') });
    await codeHandler(forgedPrincipal.req, forgedPrincipal.res);
    equal(forgedPrincipal.out.statusCode, 400, 'client-asserted principal fails the exact body contract');

    wrongSession = true;
    const wrongEcho = reqRes({ ...codeBody, request_id: randomUUID() });
    await codeHandler(wrongEcho.req, wrongEcho.res);
    equal(wrongEcho.out.statusCode, 502, 'foreign durable session echo fails closed');
    wrongSession = false;

    globalTip = true;
    const unsafeTip = reqRes({ ...codeBody, request_id: randomUUID() });
    await codeHandler(unsafeTip.req, unsafeTip.res);
    equal(unsafeTip.out.statusCode, 502, 'global session-ledger tip is rejected');
    assert(!JSON.stringify(unsafeTip.out.body).includes(digest('9')), 'global tip does not escape');
    globalTip = false;

    verifiedHeader = false;
    const unverified = reqRes({ ...codeBody, request_id: randomUUID() });
    await codeHandler(unverified.req, unverified.res);
    equal(unverified.out.statusCode, 502, 'missing binding attestation fails closed');
    verifiedHeader = true;

    const rollbackRequestId = randomUUID();
    const rollback = reqRes({
      promotion_event_digest: digest('b'),
      confirm_rollback: true,
      session_id: sessionId,
      request_id: rollbackRequestId,
    });
    await rollbackHandler(rollback.req, rollback.res);
    equal(rollback.out.statusCode, 200, 'rollback persists into the same durable session');
    const rollbackCall = calls.at(-1);
    equal(rollbackCall?.body.session_id, sessionId, 'rollback runtime request keeps the session binding');
    equal(
      rollbackCall?.body.session_binding_mac,
      expectedMac('/v1/code/rollback', rollbackCall!.body),
      'rollback HMAC binds its exact durable request',
    );
    const rollbackObserved = rollback.out.body.observed as Record<string, unknown>;
    const rollbackRuntime = rollbackObserved.runtime as Record<string, unknown>;
    equal(rollbackRuntime.request_id, rollbackRequestId, 'rollback response reprojects the client request ID');
    equal(rollbackRuntime.operation_ref, digest('f'), 'rollback response preserves its operation identity');
    equal(
      (rollbackRuntime.session_event_digests as Record<string, unknown>).rollback,
      digest('4'),
      'rollback response carries its durable session event digest',
    );

    console.log('apocv4-durable-code-session.test : OK');
  } finally {
    globalThis.fetch = originalFetch;
    for (const key of Object.keys(process.env)) {
      if (!(key in originalEnv)) delete process.env[key];
    }
    Object.assign(process.env, originalEnv);
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
