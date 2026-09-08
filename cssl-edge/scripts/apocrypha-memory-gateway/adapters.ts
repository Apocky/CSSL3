import { createHash } from 'node:crypto';
import { stat } from 'node:fs/promises';
import type {
  AdapterName, AdapterProbe, GatewayConfig, ReadOnlyAdapter, SearchRequest,
} from './types';
import { runBoundedJsonl, utf8Prefix } from './process';

async function regularFiles(paths: Array<string | undefined>): Promise<boolean> {
  if (paths.some((path) => !path)) return false;
  try {
    const states = await Promise.all(paths.map((path) => stat(path as string)));
    return states.every((state) => state.isFile());
  } catch {
    return false;
  }
}

async function boundedJson(response: Response, maximumBytes: number): Promise<unknown> {
  if (!response.body) return JSON.parse(await response.text()) as unknown;
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    size += value.length;
    if (size > maximumBytes) {
      await reader.cancel('response limit').catch(() => undefined);
      throw new Error('UPSTREAM_RESPONSE_LIMIT');
    }
    chunks.push(value);
  }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  return JSON.parse(new TextDecoder().decode(bytes)) as unknown;
}

class UnconfiguredAdapter implements ReadOnlyAdapter {
  constructor(readonly name: AdapterName) {}
  async search(): Promise<unknown> { throw new Error('ADAPTER_UNCONFIGURED'); }
  async probe(): Promise<AdapterProbe> { return { state: 'unconfigured', detail: 'no read-only source configured' }; }
}

class UpstreamAdapter implements ReadOnlyAdapter {
  constructor(
    readonly name: AdapterName,
    private readonly upstream: { url: string; token?: string; healthUrl?: string },
    private readonly outputBytes: number,
  ) {}

  async search(request: SearchRequest, signal: AbortSignal): Promise<unknown> {
    const response = await fetch(this.upstream.url, {
      method: 'POST',
      headers: {
        accept: 'application/json',
        'content-type': 'application/json',
        ...(this.upstream.token ? { authorization: `Bearer ${this.upstream.token}` } : {}),
      },
      body: JSON.stringify(request),
      signal,
    });
    if (!response.ok) throw new Error(response.status === 401 || response.status === 403 ? 'UPSTREAM_DENIED' : 'UPSTREAM_UNAVAILABLE');
    const type = response.headers.get('content-type') ?? '';
    if (!type.toLowerCase().includes('application/json')) throw new Error('UPSTREAM_RESPONSE_TYPE');
    return boundedJson(response, this.outputBytes);
  }

  async probe(signal: AbortSignal): Promise<AdapterProbe> {
    if (!this.upstream.healthUrl) return { state: 'configured', detail: 'loopback upstream configured; health URL absent' };
    try {
      const response = await fetch(this.upstream.healthUrl, {
        method: 'GET',
        headers: this.upstream.token ? { authorization: `Bearer ${this.upstream.token}` } : undefined,
        signal,
      });
      return response.ok
        ? { state: 'ready', detail: 'loopback upstream health verified' }
        : { state: 'unavailable', detail: `health HTTP ${response.status}` };
    } catch {
      return { state: 'unavailable', detail: 'loopback upstream health failed' };
    }
  }
}

class NativeMemPalaceAdapter implements ReadOnlyAdapter {
  readonly name = 'mempalace' as const;
  constructor(private readonly config: GatewayConfig) {}
  async search(request: SearchRequest, signal: AbortSignal): Promise<unknown> {
    const native = this.config.native;
    const frames = await runBoundedJsonl(native.federatorExecutable as string, ['framed'], [{
      op: 'query',
      request_id: `gateway-${Date.now().toString(36)}`,
      db_path: native.mempalaceDb,
      privacy_partition: native.privacyPartition,
      query: request.query,
      limit: request.limit,
      deadline_ms: this.config.limits.timeoutMs,
      include_sample_digest: false,
    }], signal, this.config.limits.responseBytes * 4);
    const payload = frames[0] as Record<string, unknown> | undefined;
    if (!payload || String(payload.status ?? '').toLowerCase() !== 'ok') throw new Error('NATIVE_MEMPALACE_UNAVAILABLE');
    return payload;
  }
  async probe(signal: AbortSignal): Promise<AdapterProbe> {
    if (!await regularFiles([this.config.native.federatorExecutable, this.config.native.mempalaceDb])
      || !this.config.native.privacyPartition) {
      return { state: 'unavailable', detail: 'native reader, database, or privacy partition absent' };
    }
    try {
      const payload = await this.search(probeRequest(this.config), signal) as Record<string, unknown>;
      return String(payload.status ?? '').toLowerCase() === 'ok'
        ? { state: 'ready', detail: 'immutable native query verified' }
        : { state: 'unavailable', detail: 'immutable native query returned degraded state' };
    } catch {
      return { state: 'unavailable', detail: 'immutable native query failed' };
    }
  }
}

class NativeGraphAdapter implements ReadOnlyAdapter {
  readonly name = 'graphify' as const;
  constructor(private readonly config: GatewayConfig) {}
  private args(): string[] {
    const native = this.config.native;
    return ['serve', '--graph', native.graphPath as string, '--csl', native.graphCsl as string,
      '--nil', native.graphNil as string, '--cssl', native.graphCssl as string];
  }
  async search(request: SearchRequest, signal: AbortSignal): Promise<unknown> {
    const frames = await runBoundedJsonl(this.config.native.graphExecutable as string, this.args(), [{
      schema_version: 'apocrypha.graph-organ.service-request.v1',
      request_id: `gateway-${Date.now().toString(36)}`,
      method: 'query',
      query: request.query,
      deadline_ms: this.config.limits.timeoutMs,
      max_depth: 2,
      max_results: request.limit,
    }], signal, this.config.limits.responseBytes * 4);
    const payload = frames[0] as Record<string, unknown> | undefined;
    if (!payload || payload.ok !== true) throw new Error('NATIVE_GRAPH_UNAVAILABLE');
    return payload;
  }
  async probe(signal: AbortSignal): Promise<AdapterProbe> {
    const native = this.config.native;
    if (!await regularFiles([native.graphExecutable, native.graphPath, native.graphCsl, native.graphNil, native.graphCssl])) {
      return { state: 'unavailable', detail: 'native graph executable, projection, or contracts absent' };
    }
    try {
      const result = await runBoundedJsonl(native.graphExecutable as string, this.args(), [{
        schema_version: 'apocrypha.graph-organ.service-request.v1',
        request_id: 'gateway-health',
        method: 'health',
      }], signal, 32_768);
      return result.length ? { state: 'ready', detail: 'native read-only graph health verified' }
        : { state: 'unavailable', detail: 'native graph health returned no frame' };
    } catch {
      return { state: 'unavailable', detail: 'native graph health failed' };
    }
  }
}

class NativeObserveAdapter implements ReadOnlyAdapter {
  constructor(readonly name: 'mneme' | 'metaharness', private readonly config: GatewayConfig) {}
  async search(request: SearchRequest, signal: AbortSignal): Promise<unknown> {
    const native = this.config.native;
    const query = utf8Prefix(request.query.replace(/\s+/gu, ' ').trim(), 768);
    const owner = native.ownerId as string;
    const partition = native.privacyPartition as string;
    const frames = await runBoundedJsonl(native.federatorExecutable as string,
      ['observe', '--config', native.federatorConfig as string], [{
        schema_version: 'apocrypha.memory.remote-observe-request.v1',
        request_id: `gateway-${Date.now().toString(36)}`,
        query,
        regions: [this.name === 'mneme' ? 'three_mneme' : 'metaharness'],
        limit: request.limit,
        deadline_ms: Math.min(this.config.limits.timeoutMs, 30_000),
        expected_owner_sha256: createHash('sha256').update(owner).digest('hex'),
        expected_privacy_partition_sha256: createHash('sha256').update(partition).digest('hex'),
      }], signal, this.config.limits.responseBytes * 4);
    const payload = frames[0] as Record<string, unknown> | undefined;
    const region = payload && Array.isArray(payload.regions) ? payload.regions[0] as Record<string, unknown> | undefined : undefined;
    if (!payload || !region || !['ok', 'ready'].includes(String(region.status ?? '').toLowerCase())) {
      throw new Error('NATIVE_OBSERVER_UNAVAILABLE');
    }
    return payload;
  }
  async probe(signal: AbortSignal): Promise<AdapterProbe> {
    const native = this.config.native;
    if (!await regularFiles([native.federatorExecutable, native.federatorConfig]) || !native.ownerId || !native.privacyPartition) {
      return { state: 'unavailable', detail: 'native observer, config, owner, or partition absent' };
    }
    try {
      await this.search(probeRequest(this.config), signal);
      return { state: 'ready', detail: 'zero-state native observation verified' };
    } catch {
      return { state: 'unavailable', detail: 'zero-state native observation failed' };
    }
  }
}

function probeRequest(config: GatewayConfig): SearchRequest {
  return {
    operation: 'search', read_only: true, query: 'gateway health', limit: 1,
    tenant_id: config.allowedTenants.values().next().value ?? '',
    principal_id: config.allowedPrincipals.values().next().value ?? '',
    capability: config.allowedCapabilities.values().next().value ?? '',
  };
}

export function createAdapters(config: GatewayConfig): Map<AdapterName, ReadOnlyAdapter> {
  const result = new Map<AdapterName, ReadOnlyAdapter>();
  for (const name of ['brainmonsoon', 'anamnesis'] as const) {
    result.set(name, config.upstreams[name]
      ? new UpstreamAdapter(name, config.upstreams[name] as NonNullable<GatewayConfig['upstreams'][AdapterName]>, config.limits.responseBytes * 4)
      : new UnconfiguredAdapter(name));
  }
  result.set('mempalace', config.upstreams.mempalace
    ? new UpstreamAdapter('mempalace', config.upstreams.mempalace, config.limits.responseBytes * 4)
    : config.native.federatorExecutable && config.native.mempalaceDb && config.native.privacyPartition
      ? new NativeMemPalaceAdapter(config) : new UnconfiguredAdapter('mempalace'));
  result.set('graphify', config.upstreams.graphify
    ? new UpstreamAdapter('graphify', config.upstreams.graphify, config.limits.responseBytes * 4)
    : config.native.graphExecutable && config.native.graphPath && config.native.graphCsl && config.native.graphNil && config.native.graphCssl
      ? new NativeGraphAdapter(config) : new UnconfiguredAdapter('graphify'));
  for (const name of ['mneme', 'metaharness'] as const) {
    result.set(name, config.upstreams[name]
      ? new UpstreamAdapter(name, config.upstreams[name] as NonNullable<GatewayConfig['upstreams'][AdapterName]>, config.limits.responseBytes * 4)
      : config.native.federatorExecutable && config.native.federatorConfig && config.native.ownerId && config.native.privacyPartition
        ? new NativeObserveAdapter(name, config) : new UnconfiguredAdapter(name));
  }
  return result;
}
