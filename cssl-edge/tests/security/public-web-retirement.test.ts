import assert from 'node:assert/strict';

import { NextRequest } from 'next/server';

import {
  APOCV4_WEB_RUNTIME_STATE,
  RuntimeProxyError,
  fetchRuntimeHealth,
} from '@/lib/apocv4/runtime-proxy';
import {
  isRetiredWebRuntimeRequest,
  isUnbrokeredPrivateRuntimeRequest,
  middleware,
} from '@/middleware';

const retiredUrls = [
  'https://www.apocky.com/apoc',
  'https://www.apocky.com/apocrypha/thread/example',
  'https://www.apocky.com/apocrypha-manifest.json',
  'https://www.apocky.com/apx',
  'https://www.apocky.com/chat',
  'https://www.apocky.com/admin/apex',
  'https://www.apocky.com/admin/apocrypha',
  'https://www.apocky.com/admin/chat',
  'https://www.apocky.com/api/apocrypha/presence',
  'https://www.apocky.com/api/admin/apocrypha/chat',
  'https://www.apocky.com/api/admin/apocv4/health',
  'https://www.apocky.com/api/cron/apocrypha-sms',
  'https://apocrypha.apocky.com/',
  'https://apocrypha.apocky.com/clearing',
];

const unbrokeredPrivateUrls = [
  'https://www.apocky.com/api/mneme/scratch/health',
  'https://www.apocky.com/api/mneme/scratch/list',
  'https://www.apocky.com/api/mneme/scratch/remember',
  'https://www.apocky.com/api/mneme/scratch/forget',
  'https://www.apocky.com/api/mneme/scratch/export',
  'https://www.apocky.com/api/mneme/me/ingest',
  'https://www.apocky.com/api/mneme/me/smoke',
];

const brokeredMemberUrls = [
  'https://www.apocky.com/api/mneme/me/health',
  'https://www.apocky.com/api/mneme/me/list',
  'https://www.apocky.com/api/mneme/me/remember',
  'https://www.apocky.com/api/mneme/me/recall',
  'https://www.apocky.com/api/mneme/me/forget',
  'https://www.apocky.com/api/mneme/me/export',
];

async function testRetiredRoutes(): Promise<void> {
  for (const url of retiredUrls) {
    const request = new NextRequest(url, { method: url.includes('/api/') ? 'POST' : 'GET' });
    assert.equal(isRetiredWebRuntimeRequest(request), true, `${url} must match the retirement boundary`);
    const response = middleware(request);
    assert.equal(response.status, 404, `${url} must fail closed`);
    assert.equal(await response.text(), '', `${url} must not disclose a branded tombstone`);
    assert.match(response.headers.get('cache-control') ?? '', /no-store/);
    assert.match(response.headers.get('x-robots-tag') ?? '', /noindex/);
  }

  for (const url of unbrokeredPrivateUrls) {
    const request = new NextRequest(url, { method: url.endsWith('/health') || url.endsWith('/list') || url.endsWith('/export') ? 'GET' : 'POST' });
    assert.equal(isRetiredWebRuntimeRequest(request), false, `${url} is private-unbrokered, not retired`);
    assert.equal(isUnbrokeredPrivateRuntimeRequest(request), true, `${url} must match the private memory boundary`);
    const response = middleware(request);
    assert.equal(response.status, 404, `${url} must fail closed before a profile handler runs`);
    assert.equal(await response.text(), '', `${url} must not reveal whether a memory profile exists`);
    assert.match(response.headers.get('cache-control') ?? '', /no-store/);
    assert.match(response.headers.get('x-robots-tag') ?? '', /noindex/);
  }

  for (const url of brokeredMemberUrls) {
    const request = new NextRequest(url, { method: url.endsWith('/health') || url.endsWith('/list') || url.endsWith('/export') ? 'GET' : 'POST' });
    assert.equal(isRetiredWebRuntimeRequest(request), false, `${url} is not retired`);
    assert.equal(isUnbrokeredPrivateRuntimeRequest(request), false, `${url} must reach the authenticated member handler`);
    assert.equal(middleware(request).status, 200, `${url} must pass middleware so its handler can verify the session`);
  }

  for (const url of [
    'https://www.apocky.com/apocrypha',
    'https://www.apocky.com/brain',
    'https://www.apocky.com/api/brain/snapshot',
    'https://www.apocky.com/api/brain/runtime/status',
  ]) {
    assert.equal(isRetiredWebRuntimeRequest(new NextRequest(url)), false, `${url} must remain outside the retirement boundary`);
    const response = middleware(new NextRequest(url));
    assert.equal(response.status, 200, `${url} must reach its owner authorization handler`);
    assert.match(response.headers.get('cache-control') ?? '', /no-store/, `${url} must never be publicly cached`);
    assert.match(response.headers.get('x-robots-tag') ?? '', /noindex/, `${url} must never be indexed`);
    assert.match(response.headers.get('content-security-policy') ?? '', /connect-src 'self'/, `${url} browser connections remain same-origin`);
  }

  for (const url of [
    'https://www.apocky.com/',
    'https://www.apocky.com/clearing',
    'https://www.apocky.com/atlas',
    'https://www.apocky.com/api/health',
  ]) {
    const request = new NextRequest(url);
    assert.equal(isRetiredWebRuntimeRequest(request), false, `${url} must remain outside the retirement boundary`);
    assert.equal(middleware(request).status, 200, `${url} must pass through`);
  }
}

async function testProductionRuntimeGuard(): Promise<void> {
  const mutableEnv = process.env as unknown as Record<string, string | undefined>;
  const originalNodeEnv = process.env.NODE_ENV;
  const originalTransport = process.env.APOCV4_RUNTIME_TRANSPORT;
  const originalFetch = globalThis.fetch;
  let fetchCalls = 0;
  try {
    Object.assign(process.env, {
      NODE_ENV: 'production',
      APOCV4_RUNTIME_TRANSPORT: 'test-fetch',
    });
    globalThis.fetch = async () => {
      fetchCalls += 1;
      throw new Error('retired web runtime attempted an outbound request');
    };

    assert.equal(APOCV4_WEB_RUNTIME_STATE, 'RETIRED');
    await assert.rejects(
      fetchRuntimeHealth(),
      (error: unknown) => error instanceof RuntimeProxyError
        && error.code === 'web_runtime_retired'
        && error.publicStatus === 404,
    );
    assert.equal(fetchCalls, 0, 'production retirement guard must run before transport');
  } finally {
    globalThis.fetch = originalFetch;
    if (originalNodeEnv === undefined) delete mutableEnv.NODE_ENV;
    else mutableEnv.NODE_ENV = originalNodeEnv;
    if (originalTransport === undefined) delete process.env.APOCV4_RUNTIME_TRANSPORT;
    else process.env.APOCV4_RUNTIME_TRANSPORT = originalTransport;
  }
}

async function main(): Promise<void> {
  await testRetiredRoutes();
  await testProductionRuntimeGuard();
  console.log('public web retirement : OK · neutral 404 boundary + private Mneme fail-closed + zero production runtime transport');
}

void main().catch((error: unknown) => {
  console.error(error);
  process.exitCode = 1;
});
