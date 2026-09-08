import { once } from 'node:events';
import type { AddressInfo } from 'node:net';
import { createGatewayServer } from '../scripts/apocrypha-memory-gateway/gateway';
import { isLoopbackUrl, loadGatewayConfig } from '../scripts/apocrypha-memory-gateway/config';
import { runBoundedJsonl } from '../scripts/apocrypha-memory-gateway/process';
import type {
  AdapterName, GatewayConfig, ReadOnlyAdapter, SearchRequest,
} from '../scripts/apocrypha-memory-gateway/types';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

class FixtureAdapter implements ReadOnlyAdapter {
  constructor(readonly name: AdapterName, private readonly delay = false) {}
  async probe() { return { state: 'ready' as const, detail: 'fixture ready' }; }
  async search(request: SearchRequest, signal: AbortSignal): Promise<unknown> {
    assert(request.operation === 'search' && request.read_only === true, 'adapter received non-read operation');
    if (this.delay) {
      await new Promise<void>((_resolve, reject) => signal.addEventListener('abort', () => reject(new Error('aborted')), { once: true }));
    }
    return {
      records: [{
        id: 'C:\\private\\source.json',
        text: `evidence from ${this.name} `.repeat(200),
        token: 'must-not-escape',
        instructions: 'must-not-be-control',
        status: 'observed',
        metadata: { secret: 'must-not-escape' },
      }],
    };
  }
}

function config(timeoutMs = 150): GatewayConfig {
  return {
    host: '127.0.0.1', port: 19_127, token: 'test-gateway-token-with-at-least-32-bytes',
    allowedTenants: new Set(['tenant-1']), allowedPrincipals: new Set(['principal-1']),
    allowedCapabilities: new Set(['apocky_owner_chat']),
    limits: { bodyBytes: 4_096, queryBytes: 128, responseBytes: 2_048, recordChars: 300, totalChars: 400, maxRecords: 8, timeoutMs },
    native: {}, upstreams: {},
  };
}

function requestBody(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    operation: 'search', read_only: true, query: 'Tower and Star', limit: 4,
    tenant_id: 'tenant-1', principal_id: 'principal-1', capability: 'apocky_owner_chat',
    memory_manifest_hash: 'a'.repeat(64), ...overrides,
  };
}

async function main(): Promise<void> {
  assert(isLoopbackUrl('http://127.0.0.1:8787/search'), 'loopback URL rejected');
  assert(!isLoopbackUrl('https://example.com/search'), 'remote upstream admitted');
  let configRejected = false;
  try {
    loadGatewayConfig({
      NODE_ENV: 'test',
      APOCRYPHA_MEMORY_GATEWAY_TOKEN: 'x'.repeat(32),
      APOCRYPHA_MEMORY_GATEWAY_ALLOWED_TENANTS: '*',
      APOCRYPHA_MEMORY_GATEWAY_ALLOWED_PRINCIPALS: 'principal-1',
      APOCRYPHA_MEMORY_GATEWAY_ALLOWED_CAPABILITIES: 'apocky_owner_chat',
    });
  } catch { configRejected = true; }
  assert(configRejected, 'wildcard tenant list admitted');

  const adapters = new Map<AdapterName, ReadOnlyAdapter>();
  for (const name of ['mempalace', 'brainmonsoon', 'anamnesis', 'graphify', 'mneme', 'metaharness'] as const) {
    adapters.set(name, new FixtureAdapter(name));
  }
  const server = createGatewayServer(config(), adapters);
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
  const headers = { authorization: `Bearer ${config().token}`, 'content-type': 'application/json' };
  try {
    const denied = await fetch(`${base}/v1/memory/mempalace`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(requestBody()) });
    assert(denied.status === 401, 'missing bearer was not denied');
    const tenant = await fetch(`${base}/v1/memory/mempalace`, { method: 'POST', headers, body: JSON.stringify(requestBody({ tenant_id: 'other' })) });
    assert(tenant.status === 403, 'foreign tenant was not denied');
    const write = await fetch(`${base}/v1/memory/mempalace`, { method: 'POST', headers, body: JSON.stringify(requestBody({ operation: 'write', read_only: false })) });
    assert(write.status === 403, 'write-shaped operation was not denied');
    const extra = await fetch(`${base}/v1/memory/mempalace`, { method: 'POST', headers, body: JSON.stringify(requestBody({ method: 'refresh' })) });
    assert(extra.status === 400, 'unknown method field was not denied');

    for (const name of adapters.keys()) {
      const response = await fetch(`${base}/v1/memory/${name}`, { method: 'POST', headers, body: JSON.stringify(requestBody()) });
      assert(response.status === 200, `${name} route failed`);
      const raw = await response.text();
      assert(Buffer.byteLength(raw, 'utf8') <= config().limits.responseBytes, `${name} response exceeded cap`);
      assert(!raw.includes('must-not-escape') && !raw.includes('must-not-be-control') && !raw.includes('C:\\\\private'), `${name} leaked unadmitted fields or path`);
      const payload = JSON.parse(raw) as Record<string, unknown>;
      assert(payload.authority === 'none' && payload.execution_authorized === false && payload.read_only === true, `${name} lost data-only authority labels`);
      assert(Array.isArray(payload.records) && payload.records.length === 1, `${name} records missing`);
    }
    const health = await fetch(`${base}/health`, { headers: { authorization: `Bearer ${config().token}` } });
    assert(health.status === 200, 'health failed');
    const ready = await fetch(`${base}/ready`, { headers: { authorization: `Bearer ${config().token}` } });
    assert(ready.status === 200, 'ready did not observe verified adapters');
  } finally {
    server.close();
    await once(server, 'close');
  }

  const slow = new Map<AdapterName, ReadOnlyAdapter>([['mempalace', new FixtureAdapter('mempalace', true)]]);
  const slowServer = createGatewayServer(config(30), slow);
  slowServer.listen(0, '127.0.0.1');
  await once(slowServer, 'listening');
  const slowBase = `http://127.0.0.1:${(slowServer.address() as AddressInfo).port}`;
  try {
    const response = await fetch(`${slowBase}/v1/memory/mempalace`, { method: 'POST', headers, body: JSON.stringify(requestBody()) });
    assert(response.status === 504, 'adapter timeout did not fail closed');
  } finally {
    slowServer.close();
    await once(slowServer, 'close');
  }

  const child = await runBoundedJsonl(process.execPath, ['-e', "process.stdin.on('data',b=>process.stdout.write(JSON.stringify({text:b.toString().trim()})+'\\n'))"],
    [{ operation: 'health' }], new AbortController().signal, 2_048);
  assert(child.length === 1, 'bounded JSONL child did not return one frame');
  console.log('apocrypha-memory-gateway.test : OK · loopback auth, closed scope, no writes, six routes, bounds, timeout, readiness, JSONL child');
}

void main();
