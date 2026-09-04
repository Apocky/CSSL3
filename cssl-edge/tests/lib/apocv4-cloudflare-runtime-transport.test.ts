import assert from 'node:assert/strict';

import {
  CLOUDFLARE_RUNTIME_ORIGIN,
  CloudflareRuntimeTransportError,
  cloudflareRuntimeProtectedValues,
  fetchCloudflareRuntime,
  validateCloudflareRuntimeOrigin,
} from '@/lib/apocv4/cloudflare-runtime-transport';

const TEST_CLIENT_ID = 'fixture-client-id.access';
const TEST_CLIENT_SECRET = 'fixture-client-secret';
const ENV_NAMES = [
  'APOCV4_RUNTIME_TRANSPORT',
  'APOCV4_RUNTIME_URL',
  'APOCRYPHA_TUNNEL_HOST',
  'CF_ACCESS_CLIENT_ID',
  'CF_ACCESS_CLIENT_SECRET',
] as const;

const originalFetch = globalThis.fetch;

function setValidEnvironment(): void {
  process.env.APOCV4_RUNTIME_TRANSPORT = 'cloudflare-access';
  process.env.APOCV4_RUNTIME_URL = CLOUDFLARE_RUNTIME_ORIGIN;
  process.env.APOCRYPHA_TUNNEL_HOST = 'apocrypha.apocky.com';
  process.env.CF_ACCESS_CLIENT_ID = TEST_CLIENT_ID;
  process.env.CF_ACCESS_CLIENT_SECRET = TEST_CLIENT_SECRET;
}

function clearTestEnvironment(): void {
  for (const name of ENV_NAMES) {
    delete process.env[name];
  }
}

function expectCode(run: () => unknown, code: string): void {
  assert.throws(run, (error: unknown) => (
    error instanceof CloudflareRuntimeTransportError && error.code === code
  ));
}

async function main(): Promise<void> {
  try {
    setValidEnvironment();
    assert.equal(validateCloudflareRuntimeOrigin(), CLOUDFLARE_RUNTIME_ORIGIN);
    assert.equal(validateCloudflareRuntimeOrigin(CLOUDFLARE_RUNTIME_ORIGIN), CLOUDFLARE_RUNTIME_ORIGIN);
    const protectedValues = cloudflareRuntimeProtectedValues();
    assert.equal(protectedValues.length, 2, 'both Access credentials must be protected');
    assert.equal(Object.isFrozen(protectedValues), true, 'protected credential list must be immutable');
    assert.equal(
      protectedValues[0] === TEST_CLIENT_ID && protectedValues[1] === TEST_CLIENT_SECRET,
      true,
      'protected credential list did not preserve the configured pair',
    );

    for (const invalidOrigin of [
      'http://apocrypha.apocky.com',
      'https://apocrypha.apocky.com:444',
      'https://user@apocrypha.apocky.com',
      'https://apocrypha.apocky.com/path',
      'https://apocrypha.apocky.com?next=https://evil.example',
      'https://apocrypha.apocky.com#fragment',
      'https://apocrypha.apocky.com.evil.example',
      'https://APOCRYPHA.apocky.com',
      ' https://apocrypha.apocky.com',
    ]) {
      expectCode(() => validateCloudflareRuntimeOrigin(invalidOrigin), 'runtime_configuration_invalid');
    }

    process.env.APOCV4_RUNTIME_TRANSPORT = 'direct-tls';
    expectCode(() => validateCloudflareRuntimeOrigin(), 'runtime_configuration_invalid');
    setValidEnvironment();
    process.env.APOCRYPHA_TUNNEL_HOST = 'apocrypha.apocky.com.evil.example';
    expectCode(() => validateCloudflareRuntimeOrigin(), 'runtime_configuration_invalid');
    setValidEnvironment();

    for (const name of ['CF_ACCESS_CLIENT_ID', 'CF_ACCESS_CLIENT_SECRET'] as const) {
      const validValue = process.env[name];
      delete process.env[name];
      expectCode(() => cloudflareRuntimeProtectedValues(), 'runtime_credential_unavailable');
      process.env[name] = validValue;
    }
    process.env.CF_ACCESS_CLIENT_SECRET = `bad\nvalue`;
    expectCode(() => cloudflareRuntimeProtectedValues(), 'runtime_credential_unavailable');
    process.env.CF_ACCESS_CLIENT_SECRET = 'x'.repeat(4097);
    expectCode(() => cloudflareRuntimeProtectedValues(), 'runtime_credential_unavailable');
    setValidEnvironment();

    const callerController = new AbortController();
    let observedUrl = '';
    let observedInit: RequestInit | undefined;
    globalThis.fetch = async (input, init) => {
      observedUrl = String(input);
      observedInit = init;
      return new Response('{"status":"READY"}', {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    };

    const response = await fetchCloudflareRuntime(
      `${CLOUDFLARE_RUNTIME_ORIGIN}/health?probe=1`,
      {
        method: 'POST',
        headers: {
          Accept: 'application/json',
          'X-Caller-Header': 'preserved',
          'CF-Access-Client-Id': 'caller-cannot-override',
          'CF-Access-Client-Secret': 'caller-cannot-override',
        },
        body: '{}',
        cache: 'force-cache',
        redirect: 'follow',
        signal: callerController.signal,
      },
    );
    assert.equal(response.status, 200);
    assert.equal(observedUrl, `${CLOUDFLARE_RUNTIME_ORIGIN}/health?probe=1`);
    assert.ok(observedInit);
    assert.equal(observedInit.method, 'POST');
    assert.equal(observedInit.body, '{}');
    assert.equal(observedInit.cache, 'no-store');
    assert.equal(observedInit.redirect, 'error');
    assert.equal(observedInit.signal, callerController.signal);
    const observedHeaders = new Headers(observedInit.headers);
    assert.equal(observedHeaders.get('accept'), 'application/json');
    assert.equal(observedHeaders.get('x-caller-header'), 'preserved');
    assert.ok(
      observedHeaders.get('cf-access-client-id') === TEST_CLIENT_ID,
      'Cloudflare Access client ID was not injected',
    );
    assert.ok(
      observedHeaders.get('cf-access-client-secret') === TEST_CLIENT_SECRET,
      'Cloudflare Access client secret was not injected',
    );

    for (const invalidRequest of [
      'http://apocrypha.apocky.com/health',
      'https://apocrypha.apocky.com:444/health',
      'https://apocrypha.apocky.com.evil.example/health',
      'https://user@apocrypha.apocky.com/health',
      'https://apocrypha.apocky.com/health#fragment',
      'https://evil.example/?next=https://apocrypha.apocky.com/health',
      '/health',
    ]) {
      await assert.rejects(
        fetchCloudflareRuntime(invalidRequest),
        (error: unknown) => (
          error instanceof CloudflareRuntimeTransportError
          && error.code === 'runtime_request_invalid'
        ),
      );
    }

    let fetchReached = false;
    globalThis.fetch = async () => {
      fetchReached = true;
      throw new Error('fetch should not run for invalid configuration');
    };
    process.env.APOCV4_RUNTIME_URL = 'https://127.0.0.1';
    await assert.rejects(
      fetchCloudflareRuntime(`${CLOUDFLARE_RUNTIME_ORIGIN}/health`),
      (error: unknown) => (
        error instanceof CloudflareRuntimeTransportError
        && error.code === 'runtime_configuration_invalid'
      ),
    );
    assert.equal(fetchReached, false);
    setValidEnvironment();

    let projectedError: unknown;
    process.env.CF_ACCESS_CLIENT_SECRET = 'invalid secret with spaces';
    try {
      cloudflareRuntimeProtectedValues();
    } catch (error) {
      projectedError = error;
    }
    assert.ok(projectedError instanceof CloudflareRuntimeTransportError);
    const serializedError = JSON.stringify(projectedError);
    const credentialMaterial = /fixture-client-id\.access|fixture-client-secret|invalid secret with spaces/;
    assert.equal(credentialMaterial.test(serializedError), false, 'serialized error exposed credential material');
    assert.equal(credentialMaterial.test(String(projectedError)), false, 'string error exposed credential material');
  } finally {
    globalThis.fetch = originalFetch;
    clearTestEnvironment();
  }
}

main()
  .then(() => console.log('apocv4-cloudflare-runtime-transport.test : OK'))
  .catch(() => {
    console.error('apocv4-cloudflare-runtime-transport.test : FAIL');
    process.exitCode = 1;
  });
