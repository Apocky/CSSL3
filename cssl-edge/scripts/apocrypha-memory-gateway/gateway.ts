import { createServer, type IncomingMessage, type Server, type ServerResponse } from 'node:http';
import { ADAPTER_NAMES, type AdapterName, type AdapterProbe, type GatewayConfig, type GatewayRuntime, type ReadOnlyAdapter } from './types';
import { GatewayError, isLoopbackAddress, validateSearchRequest, verifyBearer } from './security';
import { boundedRecordsEnvelope, normalizeRecords } from './normalize';

const PATHS = new Map<string, AdapterName>(ADAPTER_NAMES.map((name) => [`/v1/memory/${name}`, name]));

function json(response: ServerResponse, status: number, value: unknown): void {
  const bytes = Buffer.from(JSON.stringify(value), 'utf8');
  response.statusCode = status;
  response.setHeader('content-type', 'application/json; charset=utf-8');
  response.setHeader('content-length', String(bytes.length));
  response.setHeader('cache-control', 'no-store, max-age=0');
  response.setHeader('x-content-type-options', 'nosniff');
  response.end(bytes);
}

async function readJson(request: IncomingMessage, maximum: number): Promise<unknown> {
  const type = String(request.headers['content-type'] ?? '').toLowerCase();
  if (!type.startsWith('application/json')) throw new GatewayError(415, 'CONTENT_TYPE_REQUIRED');
  const declared = Number(request.headers['content-length'] ?? 0);
  if (Number.isFinite(declared) && declared > maximum) throw new GatewayError(413, 'BODY_TOO_LARGE');
  const chunks: Buffer[] = [];
  let size = 0;
  for await (const chunk of request) {
    size += Buffer.byteLength(chunk);
    if (size > maximum) throw new GatewayError(413, 'BODY_TOO_LARGE');
    chunks.push(Buffer.from(chunk));
  }
  try {
    return JSON.parse(Buffer.concat(chunks).toString('utf8') || '{}') as unknown;
  } catch {
    throw new GatewayError(400, 'JSON_INVALID');
  }
}

async function bounded<T>(timeoutMs: number, operation: (signal: AbortSignal) => Promise<T>): Promise<T> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try { return await operation(controller.signal); } finally { clearTimeout(timer); }
}

async function probes(
  adapters: ReadonlyMap<AdapterName, ReadOnlyAdapter>,
  timeoutMs: number,
): Promise<Record<AdapterName, AdapterProbe>> {
  const pairs = await Promise.all(ADAPTER_NAMES.map(async (name) => {
    const adapter = adapters.get(name);
    if (!adapter) return [name, { state: 'unconfigured', detail: 'adapter absent' }] as const;
    try {
      return [name, await bounded(adapter.timeoutMs ?? timeoutMs, (signal) => adapter.probe(signal))] as const;
    } catch {
      return [name, { state: 'unavailable', detail: 'probe failed or timed out' }] as const;
    }
  }));
  return Object.fromEntries(pairs) as Record<AdapterName, AdapterProbe>;
}

export function createGatewayServer(
  config: GatewayConfig,
  adapters: ReadonlyMap<AdapterName, ReadOnlyAdapter>,
  runtime: GatewayRuntime = { startedAt: new Date().toISOString(), requests: 0, rejected: 0, lastError: null },
): Server {
  const server = createServer(async (request, response) => {
    let adapterName: AdapterName | undefined;
    try {
      if (!isLoopbackAddress(request.socket.remoteAddress)) throw new GatewayError(403, 'LOOPBACK_REQUIRED');
      verifyBearer(request, config.token);
      const path = new URL(request.url ?? '/', 'http://loopback.invalid').pathname;
      if (path === '/health' || path === '/ready') {
        if (request.method !== 'GET') throw new GatewayError(405, 'METHOD_NOT_ALLOWED');
        const states = await probes(adapters, config.limits.timeoutMs);
        const ready = Object.values(states).some((state) => state.state === 'ready');
        const payload = {
          schema: 'apocrypha.memory.gateway.health.v1',
          status: ready ? 'ready' : 'degraded',
          authority: 'none',
          execution_authorized: false,
          read_only: true,
          started_at: runtime.startedAt,
          requests: runtime.requests,
          rejected: runtime.rejected,
          adapters: states,
          last_error: runtime.lastError,
        };
        return json(response, path === '/ready' && !ready ? 503 : 200, payload);
      }
      adapterName = PATHS.get(path);
      if (!adapterName) throw new GatewayError(404, 'NOT_FOUND');
      if (request.method !== 'POST') throw new GatewayError(405, 'METHOD_NOT_ALLOWED');
      const adapter = adapters.get(adapterName);
      if (!adapter) throw new GatewayError(503, 'ADAPTER_UNCONFIGURED');
      const search = validateSearchRequest(await readJson(request, config.limits.bodyBytes), config);
      runtime.requests += 1;
      const payload = await bounded(adapter.timeoutMs ?? config.limits.timeoutMs, (signal) => adapter.search(search, signal));
      const records = normalizeRecords(adapterName, payload, { ...config.limits, maxRecords: Math.min(search.limit, config.limits.maxRecords) });
      return json(response, 200, boundedRecordsEnvelope(adapterName, records, config.limits.responseBytes));
    } catch (error) {
      const known = error instanceof GatewayError ? error : null;
      const code = known?.code ?? (error instanceof Error && error.message === 'ADAPTER_UNCONFIGURED'
        ? 'ADAPTER_UNCONFIGURED' : error instanceof Error && /TIMEOUT|aborted/iu.test(error.message)
          ? 'ADAPTER_TIMEOUT' : 'ADAPTER_UNAVAILABLE');
      const status = known?.status ?? (code === 'ADAPTER_UNCONFIGURED' ? 503 : code === 'ADAPTER_TIMEOUT' ? 504 : 502);
      runtime.rejected += 1;
      runtime.lastError = { code, ...(adapterName ? { adapter: adapterName } : {}), at: new Date().toISOString() };
      if (!response.headersSent) json(response, status, { error: code, read_only: true }); else response.destroy();
    }
  });
  server.maxHeadersCount = 48;
  const maximumAdapterTimeout = Math.max(config.limits.timeoutMs,
    ...[...adapters.values()].map((adapter) => adapter.timeoutMs ?? config.limits.timeoutMs));
  server.requestTimeout = Math.max(5_000, maximumAdapterTimeout + 2_000);
  server.headersTimeout = 5_000;
  server.keepAliveTimeout = 5_000;
  return server;
}
