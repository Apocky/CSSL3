import { createHash, createHmac } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { Readable } from 'node:stream';

import type { NextApiRequest, NextApiResponse } from 'next';

import {
  canonicalJson,
  TRINITY_OBSERVATION_ROUTE,
  verifyChaosObservationTransport,
} from '../../lib/apocrypha/chaos-observation';
import handler from '../../pages/api/internal/apocrypha/trinity-domain-boundary';

interface Output {
  status: number;
  body: Record<string, unknown> | null;
  headers: Record<string, string>;
}

const KEY_ID = 'chaos-observation-test-v1';
const KEY_BYTES = Buffer.from(Array.from({ length: 48 }, (_value, index) => index));
const KEY_BASE64URL = KEY_BYTES.toString('base64url');
const PRINCIPAL = `ct_${'p'.repeat(43)}`;
const TENANT = `ctt_${'t'.repeat(43)}`;
const IDEMPOTENCY_KEY = 'reading:test:000000000001';
const REQUEST_DIGEST = '1'.repeat(64);
const READING_DIGEST = '2'.repeat(64);

function assert(condition: boolean, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

function equal(actual: unknown, expected: unknown, message: string): void {
  if (actual !== expected) {
    throw new Error(`assert failed: ${message}; expected=${String(expected)} actual=${String(actual)}`);
  }
}

function sha256(value: string | Buffer): string {
  return createHash('sha256').update(value).digest('hex');
}

function validBoundary(): Record<string, unknown> {
  const unsigned = {
    schema: 'trinity.domain-boundary.v1',
    source_product: 'chaos-tarot',
    destination_entity: 'apocrypha',
    purpose: 'divination.interpretation',
    principal_ref: PRINCIPAL,
    tenant_ref: TENANT,
    consent: {
      grant_id: 'consent:test:1',
      scope: ['reading.synthesize'],
      issued_at: '2026-09-08T15:00:00.000Z',
      expires_at: '2026-09-08T16:00:00.000Z',
      revocation_ref: 'revocation:test:1',
    },
    authority: {
      capability_id: 'chaos.reading.synthesize',
      scope: ['reading.synthesize'],
      resource: ['reading:test:1'],
      budget: {
        max_jobs: 1,
        max_input_bytes: 65_536,
        max_output_tokens: 4_096,
      },
      idempotency_key: IDEMPOTENCY_KEY,
    },
    provenance: {
      request_digest: REQUEST_DIGEST,
      canonical_reading_digest: READING_DIGEST,
      schema_refs: ['chaos-tarot.canonical-reading.v1'],
      build_refs: ['chaos-build:test'],
    },
    privacy: {
      class: 'restricted',
      retention: 'session-bound',
      training_allowed: false,
    },
    isolation: {
      canonical_memory_write: false,
      weight_update: false,
      effect_execution: false,
    },
  };
  return { ...unsigned, boundary_digest: sha256(canonicalJson(unsigned)) };
}

function withRecomputedDigest(value: Record<string, unknown>): Record<string, unknown> {
  const { boundary_digest: _oldDigest, ...unsigned } = value;
  return { ...unsigned, boundary_digest: sha256(canonicalJson(unsigned)) };
}

function signedRequest(
  boundary: Record<string, unknown>,
  timestamp = Date.now(),
): { rawBody: Buffer; headers: Record<string, string> } {
  const rawBody = Buffer.from(canonicalJson(boundary), 'utf8');
  const bodyDigest = sha256(rawBody);
  const authority = boundary.authority as Record<string, unknown>;
  const provenance = boundary.provenance as Record<string, unknown>;
  const nonce = 'nonce:test:000000000001';
  const signatureInput = [
    'trinity.domain-boundary.signed-request.v1',
    String(timestamp),
    nonce,
    'POST',
    TRINITY_OBSERVATION_ROUTE,
    KEY_ID,
    'chaos-tarot',
    boundary.principal_ref,
    boundary.tenant_ref,
    authority.idempotency_key,
    provenance.request_digest,
    provenance.canonical_reading_digest ?? '-',
    boundary.boundary_digest,
    bodyDigest,
  ].join('\n');
  const signature = createHmac('sha256', KEY_BYTES).update(signatureInput, 'utf8').digest('hex');
  return {
    rawBody,
    headers: {
      'content-type': 'application/json',
      'content-length': String(rawBody.length),
      'x-trinity-key-id': KEY_ID,
      'x-trinity-timestamp': String(timestamp),
      'x-trinity-nonce': nonce,
      'x-trinity-content-sha256': bodyDigest,
      'x-trinity-boundary-sha256': String(boundary.boundary_digest),
      'x-trinity-principal': String(boundary.principal_ref),
      'x-trinity-tenant': String(boundary.tenant_ref),
      'idempotency-key': String(authority.idempotency_key),
      'x-trinity-request-sha256': String(provenance.request_digest),
      'x-trinity-canonical-reading-sha256': String(provenance.canonical_reading_digest ?? '-'),
      'x-trinity-signature': `v1=${signature}`,
    },
  };
}

function resignRawBody(rawBody: Buffer, priorHeaders: Record<string, string>): Record<string, string> {
  const bodyDigest = sha256(rawBody);
  const headers: Record<string, string> = {
    ...priorHeaders,
    'content-length': String(rawBody.length),
    'x-trinity-content-sha256': bodyDigest,
  };
  const signatureInput = [
    'trinity.domain-boundary.signed-request.v1',
    headers['x-trinity-timestamp'],
    headers['x-trinity-nonce'],
    'POST',
    TRINITY_OBSERVATION_ROUTE,
    headers['x-trinity-key-id'],
    'chaos-tarot',
    headers['x-trinity-principal'],
    headers['x-trinity-tenant'],
    headers['idempotency-key'],
    headers['x-trinity-request-sha256'],
    headers['x-trinity-canonical-reading-sha256'],
    headers['x-trinity-boundary-sha256'],
    bodyDigest,
  ].join('\n');
  headers['x-trinity-signature'] = `v1=${createHmac('sha256', KEY_BYTES)
    .update(signatureInput, 'utf8')
    .digest('hex')}`;
  return headers;
}

function reqRes(
  method: string,
  rawBody = Buffer.alloc(0),
  headers: Record<string, string> = {},
  url: string = TRINITY_OBSERVATION_ROUTE,
): { req: NextApiRequest; res: NextApiResponse; out: Output } {
  const req = Readable.from(rawBody.length > 0 ? [rawBody] : []) as unknown as NextApiRequest;
  Object.assign(req, { method, url, headers });
  const out: Output = { status: 0, body: null, headers: {} };
  const res = {
    setHeader(name: string, value: string | number | readonly string[]) {
      out.headers[name.toLowerCase()] = Array.isArray(value) ? value.join(', ') : String(value);
      return this;
    },
    status(code: number) {
      out.status = code;
      return this;
    },
    json(value: Record<string, unknown>) {
      out.body = value;
      return this;
    },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

async function invoke(
  method: string,
  rawBody = Buffer.alloc(0),
  headers: Record<string, string> = {},
  url: string = TRINITY_OBSERVATION_ROUTE,
): Promise<Output> {
  const harness = reqRes(method, rawBody, headers, url);
  await handler(harness.req, harness.res);
  return harness.out;
}

function assertBoundedRefusal(out: Output): Record<string, unknown> {
  assert(out.body !== null, 'response has a body');
  equal(out.body.schema, 'apocrypha.chaos.transport-refusal.v1', 'response schema is transport-only');
  equal(out.body.route, TRINITY_OBSERVATION_ROUTE, 'response names the exact receiver route');
  equal(out.body.source_product, 'chaos-tarot', 'response names the source product');
  equal(out.body.destination_entity, 'apocrypha', 'response names the destination entity');
  equal(out.body.admitted, false, 'transport scaffold never claims admission');
  equal(out.body.writes_committed, 0, 'transport scaffold reports zero committed writes');
  equal(out.body.effect_authority, 'NONE', 'transport scaffold grants no effects');
  equal(out.body.response_authentication, 'UNAVAILABLE', 'unsigned response is explicitly unavailable');
  equal(out.headers['cache-control'], 'private, no-store, max-age=0', 'response cannot be cached');
  equal(out.headers['x-content-type-options'], 'nosniff', 'response disables MIME sniffing');
  equal(out.headers['x-apocrypha-route'], TRINITY_OBSERVATION_ROUTE, 'route identity is in headers');
  assert(typeof out.headers['x-correlation-id'] === 'string', 'response includes a correlation id');
  assert(!JSON.stringify(out.body).includes(PRINCIPAL), 'principal never crosses back in a refusal');
  assert(!JSON.stringify(out.body).includes(KEY_BASE64URL), 'request key never crosses back in a refusal');
  return out.body;
}

const originalKeyId = process.env.CHAOS_TRINITY_OBSERVATION_KEY_ID;
const originalKey = process.env.CHAOS_TRINITY_OBSERVATION_HMAC_KEY;

async function main(): Promise<void> {
  try {
    const goldenKeyId = 'trinity-key-epoch-20260908';
    const goldenKey = createHash('sha256').update('test-only-chaos-observation-key').digest();
    const goldenNow = 1_788_868_800_000;
    const goldenNonce = 'nonce-20260908-0000000001';
    const goldenUnsigned = {
      schema: 'trinity.domain-boundary.v1',
      source_product: 'chaos-tarot',
      destination_entity: 'apocrypha',
      purpose: 'divination.interpretation',
      principal_ref: `ct_${'a'.repeat(43)}`,
      tenant_ref: `ctt_${'b'.repeat(43)}`,
      consent: {
        grant_id: 'grant:reading:20260908',
        scope: ['reading.synthesize'],
        issued_at: '2026-09-08T12:00:00.000Z',
        expires_at: '2026-09-08T12:10:00.000Z',
        revocation_ref: 'revocation:reading:20260908',
      },
      authority: {
        capability_id: 'apocrypha.reading.synthesize.v1',
        scope: ['reading.synthesize'],
        resource: ['reading:sealed:001'],
        budget: { max_jobs: 1, max_input_bytes: 65_536, max_output_tokens: 4_096 },
        idempotency_key: 'reading-idempotency-0001',
      },
      provenance: {
        request_digest: '1'.repeat(64),
        canonical_reading_digest: '2'.repeat(64),
        schema_refs: ['schema:chaos-reading:v1'],
        build_refs: ['build:apocrypha:20260908', 'build:chaos:20260908'],
      },
      privacy: { class: 'restricted', retention: 'ephemeral:10m', training_allowed: false },
      isolation: { canonical_memory_write: false, weight_update: false, effect_execution: false },
    };
    const goldenBoundary = {
      ...goldenUnsigned,
      boundary_digest: sha256(canonicalJson(goldenUnsigned)),
    };
    equal(
      goldenBoundary.boundary_digest,
      '2e6e5157a3c25fcda1137e5d0ecc2e5f1f30574f6dad37e7fcc01ce2d5832d02',
      'TypeScript boundary digest matches the committed Python membrane golden',
    );
    const goldenBody = Buffer.from(canonicalJson(goldenBoundary), 'utf8');
    const goldenBodyDigest = sha256(goldenBody);
    const goldenSignatureInput = [
      'trinity.domain-boundary.signed-request.v1',
      String(goldenNow),
      goldenNonce,
      'POST',
      TRINITY_OBSERVATION_ROUTE,
      goldenKeyId,
      'chaos-tarot',
      goldenBoundary.principal_ref,
      goldenBoundary.tenant_ref,
      goldenBoundary.authority.idempotency_key,
      goldenBoundary.provenance.request_digest,
      goldenBoundary.provenance.canonical_reading_digest,
      goldenBoundary.boundary_digest,
      goldenBodyDigest,
    ].join('\n');
    const goldenSignature = createHmac('sha256', goldenKey)
      .update(goldenSignatureInput, 'utf8')
      .digest('hex');
    equal(
      goldenSignature,
      '38bdd4e3a96b56817d66ab9862a10f93361b119e9b8a349593e97bc01a4dce07',
      'request signature matches the committed Python and Chaos TypeScript golden',
    );
    equal(goldenBody.length, 1_284, 'canonical body length matches the committed golden');
    const goldenHeaders = {
      'content-type': 'application/json',
      'content-length': String(goldenBody.length),
      'x-trinity-key-id': goldenKeyId,
      'x-trinity-timestamp': String(goldenNow),
      'x-trinity-nonce': goldenNonce,
      'x-trinity-content-sha256': goldenBodyDigest,
      'x-trinity-boundary-sha256': goldenBoundary.boundary_digest,
      'x-trinity-principal': goldenBoundary.principal_ref,
      'x-trinity-tenant': goldenBoundary.tenant_ref,
      'idempotency-key': goldenBoundary.authority.idempotency_key,
      'x-trinity-request-sha256': goldenBoundary.provenance.request_digest,
      'x-trinity-canonical-reading-sha256': goldenBoundary.provenance.canonical_reading_digest,
      'x-trinity-signature': `v1=${goldenSignature}`,
    };
    const goldenVerified = verifyChaosObservationTransport({
      rawBody: goldenBody,
      headers: goldenHeaders,
      method: 'POST',
      url: TRINITY_OBSERVATION_ROUTE,
      nowMs: goldenNow,
      env: {
        NODE_ENV: 'test',
        CHAOS_TRINITY_OBSERVATION_KEY_ID: goldenKeyId,
        CHAOS_TRINITY_OBSERVATION_HMAC_KEY: goldenKey.toString('base64url'),
      },
    });
    equal(
      goldenVerified.requestAuthDigest,
      'e72321231d91413bfd24821ad31610b501ae7e465eeed7cce28eb4492cc38038',
      'request authentication digest matches the committed Python membrane golden',
    );

    const base = signedRequest(validBoundary());

    const wrongMethod = await invoke('GET');
    equal(wrongMethod.status, 405, 'non-POST request is rejected');
    equal(wrongMethod.headers.allow, 'POST', 'only POST is advertised');
    assertBoundedRefusal(wrongMethod);

    delete process.env.CHAOS_TRINITY_OBSERVATION_KEY_ID;
    delete process.env.CHAOS_TRINITY_OBSERVATION_HMAC_KEY;
    const unconfigured = await invoke('POST', base.rawBody, base.headers);
    equal(unconfigured.status, 503, 'missing dedicated credential fails closed');
    equal(assertBoundedRefusal(unconfigured).code, 'receiver_unavailable', 'missing credential is typed');
    equal(unconfigured.headers['retry-after'], '60', 'unavailable receiver exposes bounded retry timing');

    process.env.CHAOS_TRINITY_OBSERVATION_KEY_ID = KEY_ID;
    process.env.CHAOS_TRINITY_OBSERVATION_HMAC_KEY = KEY_BASE64URL;

    const missingSignatureHeaders = { ...base.headers };
    delete missingSignatureHeaders['x-trinity-signature'];
    const missingSignature = await invoke('POST', base.rawBody, missingSignatureHeaders);
    equal(missingSignature.status, 401, 'missing request proof is rejected');
    equal(assertBoundedRefusal(missingSignature).code, 'authentication_failed', 'missing proof is typed');

    const oversizedDeclared = await invoke('POST', base.rawBody, {
      ...base.headers,
      'content-length': '65537',
    });
    equal(oversizedDeclared.status, 400, 'declared oversized body is rejected before verification');
    equal(assertBoundedRefusal(oversizedDeclared).code, 'invalid_transport', 'oversized declaration is typed');

    const tamperedHeaders = { ...base.headers, 'x-trinity-signature': `v1=${'0'.repeat(64)}` };
    const tampered = await invoke('POST', base.rawBody, tamperedHeaders);
    equal(tampered.status, 401, 'incorrect request proof is rejected');
    equal(assertBoundedRefusal(tampered).code, 'authentication_failed', 'incorrect proof is typed');

    const stale = signedRequest(validBoundary(), Date.now() - 600_001);
    const staleResult = await invoke('POST', stale.rawBody, stale.headers);
    equal(staleResult.status, 401, 'stale signed request is rejected');
    equal(assertBoundedRefusal(staleResult).code, 'request_stale', 'stale request is typed');

    const wrongPath = await invoke('POST', base.rawBody, base.headers, `${TRINITY_OBSERVATION_ROUTE}?shadow=1`);
    equal(wrongPath.status, 400, 'query-bearing route cannot reuse the canonical signature');
    equal(assertBoundedRefusal(wrongPath).code, 'invalid_transport', 'wrong route is typed');

    const invalidUtf8 = Buffer.from(base.rawBody);
    const sourceOffset = invalidUtf8.indexOf(Buffer.from('chaos-tarot', 'utf8'));
    assert(sourceOffset >= 0, 'fixture contains the source product');
    invalidUtf8[sourceOffset] = 0xff;
    const invalidUtf8Result = await invoke('POST', invalidUtf8, resignRawBody(invalidUtf8, base.headers));
    equal(invalidUtf8Result.status, 400, 'invalid UTF-8 is rejected after authentication');
    equal(assertBoundedRefusal(invalidUtf8Result).code, 'invalid_contract', 'invalid UTF-8 is typed');
    equal(invalidUtf8Result.body?.transport_authenticated, true, 'invalid UTF-8 transport was authenticated');

    const noncanonical = Buffer.from(JSON.stringify(validBoundary(), null, 2), 'utf8');
    const noncanonicalResult = await invoke('POST', noncanonical, resignRawBody(noncanonical, base.headers));
    equal(noncanonicalResult.status, 400, 'noncanonical JSON is rejected after authentication');
    equal(assertBoundedRefusal(noncanonicalResult).code, 'invalid_contract', 'noncanonical JSON is typed');

    const duplicate = Buffer.from(
      base.rawBody.toString('utf8').replace('{', '{"schema":"trinity.domain-boundary.v1",'),
      'utf8',
    );
    const duplicateResult = await invoke('POST', duplicate, resignRawBody(duplicate, base.headers));
    equal(duplicateResult.status, 400, 'duplicate JSON fields are rejected after authentication');
    equal(assertBoundedRefusal(duplicateResult).code, 'invalid_contract', 'duplicate JSON is typed');

    const wrongDirection = withRecomputedDigest({ ...validBoundary(), destination_entity: 'chaos-tarot' });
    const wrongDirectionRequest = signedRequest(wrongDirection);
    const wrongDirectionResult = await invoke('POST', wrongDirectionRequest.rawBody, wrongDirectionRequest.headers);
    equal(wrongDirectionResult.status, 400, 'wrong destination is rejected after authentication');
    const wrongDirectionBody = assertBoundedRefusal(wrongDirectionResult);
    equal(wrongDirectionBody.code, 'invalid_contract', 'wrong destination is typed');
    equal(wrongDirectionBody.transport_authenticated, true, 'verified invalid contract reports its transport state');

    const rawPrivate = withRecomputedDigest({ ...validBoundary(), question: 'must never enter this membrane' });
    const rawPrivateRequest = signedRequest(rawPrivate);
    const rawPrivateResult = await invoke('POST', rawPrivateRequest.rawBody, rawPrivateRequest.headers);
    equal(rawPrivateResult.status, 400, 'raw private payload is rejected');
    const rawPrivateBody = assertBoundedRefusal(rawPrivateResult);
    equal(rawPrivateBody.code, 'raw_private_payload', 'raw payload refusal is typed');
    equal(rawPrivateBody.transport_authenticated, true, 'verified raw payload reports its transport state');
    assert(!JSON.stringify(rawPrivateResult.body).includes('must never enter'), 'raw payload is not echoed');

    const forbiddenFields = [
      'StateRoot',
      'parent_root',
      'authority_ref',
      'effect_grant',
      'capability_profile',
      'model_policy',
      'memory_profile',
      'tool_registry',
    ];
    for (const field of forbiddenFields) {
      const body = withRecomputedDigest({ ...validBoundary(), [field]: `forbidden:${field}` });
      const signed = signedRequest(body);
      const result = await invoke('POST', signed.rawBody, signed.headers);
      equal(result.status, 403, `${field} is refused before admission`);
      equal(
        assertBoundedRefusal(result).code,
        'forbidden_authority_or_state_field',
        `${field} receives the authority/state refusal code`,
      );
      equal(result.body?.transport_authenticated, true, `${field} was authenticated before contract refusal`);
    }

    const nestedForbiddenBase = validBoundary();
    const nestedAuthority = nestedForbiddenBase.authority as Record<string, unknown>;
    const nestedForbidden = withRecomputedDigest({
      ...nestedForbiddenBase,
      authority: { ...nestedAuthority, model_policy: 'forbidden:nested' },
    });
    const nestedRequest = signedRequest(nestedForbidden);
    const nestedResult = await invoke('POST', nestedRequest.rawBody, nestedRequest.headers);
    equal(nestedResult.status, 403, 'nested policy authority is rejected');
    equal(assertBoundedRefusal(nestedResult).code, 'forbidden_authority_or_state_field', 'nested refusal is typed');

    const verified = await invoke('POST', base.rawBody, base.headers);
    equal(verified.status, 503, 'authenticated request remains closed without native admission and response signing');
    const verifiedBody = assertBoundedRefusal(verified);
    equal(verifiedBody.code, 'receiver_unavailable', 'verified-but-dormant receiver is typed');
    equal(verifiedBody.transport_authenticated, true, 'request HMAC verification is reported truthfully');
    equal(verified.headers['x-apocrypha-transport-authentication'], 'verified', 'verified transport is in headers');
    equal(verified.headers['retry-after'], '60', 'verified sender receives bounded retry timing');

    process.env.CHAOS_TRINITY_OBSERVATION_HMAC_KEY = Buffer.alloc(32, 7).toString('base64url');
    const weakCredential = await invoke('POST', base.rawBody, base.headers);
    equal(weakCredential.status, 503, 'weak repeating key material disables the receiver');
    equal(assertBoundedRefusal(weakCredential).code, 'receiver_unavailable', 'weak key failure is bounded');

    const routeSource = readFileSync(
      'pages/api/internal/apocrypha/trinity-domain-boundary.ts',
      'utf8',
    );
    const helperSource = readFileSync('lib/apocrypha/chaos-observation.ts', 'utf8');
    for (const forbiddenDependency of [
      '@supabase',
      'enqueueApocryphaJob',
      'getApocryphaServiceClient',
      'fetch(',
    ]) {
      assert(
        !routeSource.includes(forbiddenDependency) && !helperSource.includes(forbiddenDependency),
        `zero-write transport scaffold excludes ${forbiddenDependency}`,
      );
    }

    console.log('apocrypha-chaos-observation.test : OK · exact signed ingress is authenticated and fail-closed with zero writes');
  } finally {
    if (originalKeyId === undefined) delete process.env.CHAOS_TRINITY_OBSERVATION_KEY_ID;
    else process.env.CHAOS_TRINITY_OBSERVATION_KEY_ID = originalKeyId;
    if (originalKey === undefined) delete process.env.CHAOS_TRINITY_OBSERVATION_HMAC_KEY;
    else process.env.CHAOS_TRINITY_OBSERVATION_HMAC_KEY = originalKey;
  }
}

void main().catch((error: unknown) => {
  console.error(error);
  process.exitCode = 1;
});
