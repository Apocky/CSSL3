import assert from 'node:assert/strict';
import { createHash, generateKeyPairSync, type KeyObject } from 'node:crypto';

import type { NextApiRequest, NextApiResponse } from 'next';

import {
  CONTRIBUTOR_ENROLLMENT_SCHEMA,
  ContributorTransportController,
  MemoryContributorTransportStore,
  publicKeySpkiB64,
  signEnrollmentRequest,
  type EnrollmentRequest,
} from '@/lib/apocrypha/contributor-transport';
import {
  CONTRIBUTOR_HTTP_MAX_BODY_BYTES,
  createContributorRouteHandler,
  type ContributorRateLimiter,
} from '@/lib/apocrypha/contributor-http';

interface Output {
  statusCode: number;
  body: unknown;
  headers: Record<string, string>;
}

function mock(
  method: string,
  body: unknown = undefined,
  headers: Record<string, string> = {},
  query: Record<string, unknown> = {},
): { req: NextApiRequest; res: NextApiResponse; out: Output } {
  const out: Output = { statusCode: 0, body: null, headers: {} };
  const req = {
    method,
    headers,
    query,
    body,
    socket: { remoteAddress: '127.0.0.1' },
  } as unknown as NextApiRequest;
  const res = {
    status(code: number) { out.statusCode = code; return this; },
    json(value: unknown) { out.body = value; return this; },
    setHeader(name: string, value: string | number | readonly string[]) {
      out.headers[name.toLowerCase()] = Array.isArray(value) ? value.join(', ') : String(value);
      return this;
    },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

function bodyHeaders(body: unknown): Record<string, string> {
  return { 'content-type': 'application/json', 'content-length': String(Buffer.byteLength(JSON.stringify(body), 'utf8')) };
}

function fixedController(): { controller: ContributorTransportController; enrollment: EnrollmentRequest; node: KeyObject } {
  const now = 1_800_000_000_000;
  const controller = generateKeyPairSync('ed25519');
  const node = generateKeyPairSync('ed25519');
  const enrollment = signEnrollmentRequest({
    schema_version: CONTRIBUTOR_ENROLLMENT_SCHEMA,
    request_id: 'route-enroll-01',
    node_id: 'route-node-01',
    node_key_id: 'route-node-key-v1',
    node_public_key_spki_b64: publicKeySpkiB64(node.publicKey),
    platform: 'windows-x64',
    capabilities: ['vector_dot'],
    consent_revision: 'consent-v1',
    issued_at: now - 1_000,
    expires_at: now + 120_000,
  }, node.privateKey);
  return {
    enrollment,
    node: node.privateKey,
    controller: new ContributorTransportController({
      controllerKeyId: 'route-controller-v1',
      controllerPrivateKey: controller.privateKey,
      store: new MemoryContributorTransportStore(),
      now: () => now,
    }),
  };
}

const allowAll: ContributorRateLimiter = { check: () => ({ allowed: true }) };

function isolateContributorEnv(): () => void {
  const names = [
    'APOCRYPHA_CONTRIBUTOR_CONTROLLER_KEY_ID',
    'APOCRYPHA_CONTRIBUTOR_CONTROLLER_PRIVATE_KEY_PEM',
    'APOCRYPHA_CONTRIBUTOR_OPERATOR_KEY_ID',
    'APOCRYPHA_CONTRIBUTOR_OPERATOR_PUBLIC_KEY_PEM',
    'APOCRYPHA_CONTRIBUTOR_CONTROLLER_TOKEN_SHA256',
    'APOCKY_HUB_SUPABASE_URL',
    'NEXT_PUBLIC_SUPABASE_URL',
    'SUPABASE_SERVICE_ROLE_KEY',
  ] as const;
  const previous = new Map<string, string | undefined>();
  for (const name of names) {
    previous.set(name, process.env[name]);
    delete process.env[name];
  }
  return () => {
    for (const [name, value] of previous) {
      if (value === undefined) delete process.env[name];
      else process.env[name] = value;
    }
  };
}

export async function testMethodsAndStrictBody(): Promise<void> {
  const method = mock('GET');
  await createContributorRouteHandler('enroll')(method.req, method.res);
  assert.equal(method.out.statusCode, 405);
  assert.equal((method.out.body as Record<string, unknown>).code, 'method_not_allowed');
  assert.equal(method.out.headers.allow, 'POST');

  const contentType = mock('POST', '{}', { 'content-type': 'text/plain' });
  await createContributorRouteHandler('enroll', { rateLimiter: allowAll })(contentType.req, contentType.res);
  assert.equal(contentType.out.statusCode, 400);
  assert.equal((contentType.out.body as Record<string, unknown>).code, 'invalid_transport');

  const oversized = mock(
    'POST',
    'x'.repeat(CONTRIBUTOR_HTTP_MAX_BODY_BYTES + 1),
    { 'content-type': 'application/json' },
  );
  await createContributorRouteHandler('enroll', { rateLimiter: allowAll })(oversized.req, oversized.res);
  assert.equal(oversized.out.statusCode, 400);
  assert.equal((oversized.out.body as Record<string, unknown>).code, 'invalid_transport');
}

export async function testUnconfiguredAndUnsignedAreFailClosed(): Promise<void> {
  const restore = isolateContributorEnv();
  try {
    const fixture = fixedController();
    const unconfigured = mock('POST', fixture.enrollment, bodyHeaders(fixture.enrollment));
    await createContributorRouteHandler('enroll', { rateLimiter: allowAll })(unconfigured.req, unconfigured.res);
    assert.equal(unconfigured.out.statusCode, 503);
    assert.equal((unconfigured.out.body as Record<string, unknown>).code, 'transport_unconfigured');

    const unsigned = { ...fixture.enrollment, signature_b64: 'A'.repeat(86) };
    const rejected = mock('POST', unsigned, bodyHeaders(unsigned));
    await createContributorRouteHandler('enroll', {
      controller: fixture.controller,
      rateLimiter: allowAll,
    })(rejected.req, rejected.res);
    assert.equal(rejected.out.statusCode, 401);
    assert.equal((rejected.out.body as Record<string, unknown>).code, 'transport_auth_invalid');
    assert.equal((rejected.out.body as Record<string, unknown>).transport_code, 'TRANSPORT_SIGNATURE_INVALID');
  } finally {
    restore();
  }
}

export async function testInjectedControllerEnrolsAndStatusIsMetadataOnly(): Promise<void> {
  const fixture = fixedController();
  const enrolled = mock('POST', fixture.enrollment, bodyHeaders(fixture.enrollment));
  await createContributorRouteHandler('enroll', {
    controller: fixture.controller,
    rateLimiter: allowAll,
  })(enrolled.req, enrolled.res);
  assert.equal(enrolled.out.statusCode, 200);
  assert.equal((enrolled.out.body as Record<string, unknown>).ok, true);
  assert.equal(
    ((enrolled.out.body as { result: { node_id: string } }).result).node_id,
    fixture.enrollment.node_id,
  );

  const status = mock('GET');
  await createContributorRouteHandler('status', {
    controller: fixture.controller,
    rateLimiter: allowAll,
  })(status.req, status.res);
  assert.equal(status.out.statusCode, 200);
  const statusBody = (status.out.body as { status: Record<string, unknown> }).status;
  assert.equal(statusBody.public_metadata_only, true);
  assert.equal(statusBody.mutating_routes_enabled, true);
  assert.equal('private_key' in statusBody, false);

  const query = mock('GET', undefined, {}, { node_id: fixture.enrollment.node_id });
  await createContributorRouteHandler('status')(query.req, query.res);
  assert.equal(query.out.statusCode, 400);
  assert.equal((query.out.body as Record<string, unknown>).code, 'invalid_transport');
}

export async function testControllerBearerAndRateLimitHooks(): Promise<void> {
  const fixture = fixedController();
  const token = 'controller-route-test-token';
  const digest = createHash('sha256').update(token, 'utf8').digest('hex');
  const request = {
    schema_version: 'apocrypha.contributor.lease-request.v1',
    request_id: 'route-lease-01',
    idempotency_key: 'route-idempotency-01',
    node_id: fixture.enrollment.node_id,
    issued_at: 1_800_000_000_000 - 500,
    expires_at: 1_800_000_000_000 + 30_000,
    attempt: 1,
    task: { kind: 'vector_dot', left: [1], right: [2] },
  };
  await fixture.controller.enroll(fixture.enrollment);

  const missingAuth = mock('POST', request, bodyHeaders(request));
  await createContributorRouteHandler('lease', {
    controller: fixture.controller,
    controllerTokenSha256: digest,
    rateLimiter: allowAll,
  })(missingAuth.req, missingAuth.res);
  assert.equal(missingAuth.out.statusCode, 401);
  assert.equal((missingAuth.out.body as Record<string, unknown>).code, 'controller_auth_required');

  const limited = mock('POST', request, {
    ...bodyHeaders(request),
    authorization: `Bearer ${token}`,
  });
  await createContributorRouteHandler('lease', {
    controller: fixture.controller,
    controllerTokenSha256: digest,
    rateLimiter: { check: () => ({ allowed: false, retry_after_seconds: 17 }) },
  })(limited.req, limited.res);
  assert.equal(limited.out.statusCode, 429);
  assert.equal(limited.out.headers['retry-after'], '17');
}

async function runAll(): Promise<void> {
  await testMethodsAndStrictBody();
  await testUnconfiguredAndUnsignedAreFailClosed();
  await testInjectedControllerEnrolsAndStatusIsMetadataOnly();
  await testControllerBearerAndRateLimitHooks();
  console.log('contributor-node/routes.test : OK · 4 tests passed');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: unknown } | undefined;
if (typeof require !== 'undefined' && typeof module !== 'undefined' && require.main === module) {
  void runAll().catch((error: unknown) => {
    console.error(error);
    process.exitCode = 1;
  });
}

