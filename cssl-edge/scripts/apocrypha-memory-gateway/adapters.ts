import { createHash } from 'node:crypto';
import { stat } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import type {
  AdapterName, AdapterProbe, GatewayConfig, ReadOnlyAdapter, SearchRequest,
} from './types';
import { runBoundedJsonl, utf8Prefix } from './process';

function scopedRequestId(prefix: string, request: SearchRequest): string {
  const scope = [request.tenant_id, request.principal_id, request.capability].join('\0');
  return `${prefix}-${Date.now().toString(36)}-${createHash('sha256').update(scope).digest('hex').slice(0, 20)}`;
}

function object(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : null;
}

export function nativeErrorCode(payload: Record<string, unknown> | undefined, fallback: string): string {
  const nested = object(payload?.error)?.code;
  const direct = payload?.code;
  const code = typeof nested === 'string' ? nested : direct;
  return typeof code === 'string' && /^[A-Z][A-Z0-9_]{1,79}$/u.test(code) ? code : fallback;
}

function boundedScalar(value: unknown, maximum = 512): string {
  return typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean'
    ? String(value).replace(/\s+/gu, ' ').trim().slice(0, maximum) : '';
}

export function canonicalGraphQuery(value: string): string {
  return utf8Prefix(value.replace(/\s+/gu, ' ').trim(), 4_000);
}

function innerDeadline(outerTimeoutMs: number): number {
  return outerTimeoutMs - Math.min(2_000, Math.max(1, Math.floor(outerTimeoutMs / 5)));
}

export function brainmonsoonRecords(payload: Record<string, unknown>): Array<Record<string, unknown>> {
  const native = object(payload.native_response);
  const result = object(native?.result);
  const batch = object(result?.batch);
  const input = object(batch?.input);
  if (!batch || !input) return [];
  const records: Array<Record<string, unknown>> = [];
  const push = (id: string, text: string, kind: string) => {
    if (text && records.length < 12) records.push({
      id: id.slice(0, 256), text: text.slice(0, 7_000), kind,
      authority: 'read_only_analysis', effect_authority: false,
    });
  };
  for (const [index, value] of (Array.isArray(input.claims) ? input.claims : []).entries()) {
    const claim = object(value);
    if (!claim) continue;
    const actor = boundedScalar(claim.actor);
    const kind = boundedScalar(claim.kind, 80) || 'claim';
    const confidence = boundedScalar(claim.confidence, 32);
    push(boundedScalar(claim.claim_ref) || `brain-claim-${index}`,
      `${kind}: ${actor}${confidence ? ` (confidence ${confidence})` : ''}`, 'claim');
  }
  for (const [index, value] of (Array.isArray(input.relations) ? input.relations : []).entries()) {
    const relation = object(value);
    if (!relation) continue;
    const subject = boundedScalar(relation.subject);
    const predicate = boundedScalar(relation.predicate);
    const target = boundedScalar(relation.object);
    push(`brain-relation-${index}`, [subject, predicate, target].filter(Boolean).join(' '), 'relation');
  }
  for (const [index, value] of (Array.isArray(input.prior_events) ? input.prior_events : []).entries()) {
    const event = object(value);
    if (!event) continue;
    const kind = boundedScalar(event.kind, 160);
    const validAt = boundedScalar(event.valid_time, 80);
    push(boundedScalar(event.event_ref) || `brain-event-${index}`,
      `Prior event: ${kind}${validAt ? ` at ${validAt}` : ''}`, 'event');
  }
  return records;
}

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
      request_id: scopedRequestId('gateway-mem', request),
      db_path: native.mempalaceDb,
      privacy_partition: native.privacyPartition,
      query: request.query,
      limit: request.limit,
      deadline_ms: innerDeadline(this.config.limits.timeoutMs),
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
      request_id: scopedRequestId('gateway-graph', request),
      method: 'query',
      query: canonicalGraphQuery(request.query),
      deadline_ms: innerDeadline(this.config.limits.timeoutMs),
      max_depth: 2,
      max_results: request.limit,
    }], signal, this.config.limits.responseBytes * 4);
    const payload = frames[0] as Record<string, unknown> | undefined;
    if (!payload || payload.ok !== true) {
      throw new Error(nativeErrorCode(payload, 'NATIVE_GRAPH_UNAVAILABLE'));
    }
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
        request_id: scopedRequestId(`gateway-${this.name}`, request),
        query,
        regions: [this.name === 'mneme' ? 'three_mneme' : 'metaharness'],
        limit: Math.min(request.limit, 2),
        deadline_ms: innerDeadline(Math.min(this.config.limits.timeoutMs, 12_000)),
        expected_owner_sha256: createHash('sha256').update(owner).digest('hex'),
        expected_privacy_partition_sha256: createHash('sha256').update(partition).digest('hex'),
      }], signal, this.config.limits.responseBytes * 4);
    const payload = frames[0] as Record<string, unknown> | undefined;
    const region = payload && Array.isArray(payload.regions) ? payload.regions[0] as Record<string, unknown> | undefined : undefined;
    const status = String(region?.status ?? '').toLowerCase();
    const code = String(region?.code ?? '');
    const admitted = ['ok', 'ready'].includes(status) || (status === 'empty' && code === 'FED_OBSERVE_OK');
    if (!payload || !region || !admitted) {
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

class NativeAnamnesisAdapter implements ReadOnlyAdapter {
  readonly name = 'anamnesis' as const;
  private readonly reader = fileURLToPath(new URL('./anamnesis-reader.mjs', import.meta.url));
  constructor(private readonly config: GatewayConfig) {}
  async search(request: SearchRequest, signal: AbortSignal): Promise<unknown> {
    const frames = await runBoundedJsonl(process.execPath, [this.reader], [{
      schema: 'apocrypha.anamnesis.read-request.v1',
      db_path: this.config.native.anamnesisDb,
      query: utf8Prefix(request.query.replace(/\s+/gu, ' ').trim(), 4_000),
      limit: request.limit,
      deadline_ms: innerDeadline(this.config.limits.timeoutMs),
    }], signal, this.config.limits.responseBytes * 4);
    const payload = frames[0] as Record<string, unknown> | undefined;
    if (!payload || payload.ok !== true || payload.read_only !== true || payload.authority !== 'none') {
      throw new Error('NATIVE_ANAMNESIS_UNAVAILABLE');
    }
    return payload;
  }
  async probe(signal: AbortSignal): Promise<AdapterProbe> {
    if (!await regularFiles([this.reader, this.config.native.anamnesisDb])) {
      return { state: 'unavailable', detail: 'read-only Anamnesis reader or database absent' };
    }
    try {
      await this.search(probeRequest(this.config), signal);
      return { state: 'ready', detail: 'SQLite mode=ro query_only recall verified' };
    } catch {
      return { state: 'unavailable', detail: 'read-only Anamnesis recall failed' };
    }
  }
}

class NativeBrainmonsoonAdapter implements ReadOnlyAdapter {
  readonly name = 'brainmonsoon' as const;
  readonly timeoutMs = 30_000;
  private readonly reader = fileURLToPath(new URL('./brainmonsoon-reader.mjs', import.meta.url));
  constructor(private readonly config: GatewayConfig) {}
  private async invoke(method: 'health' | 'recall', query: string, limit: number, signal: AbortSignal): Promise<Record<string, unknown>> {
    const native = this.config.native;
    const frames = await runBoundedJsonl(process.execPath, [this.reader], [{
      schema_version: 'apocrypha.memory-gateway.brainmonsoon-reader-request.v1',
      request_id: `gateway-brain-${Date.now().toString(36)}`,
      method,
      executable_path: native.brainmonsoonExecutable,
      executable_sha256: native.brainmonsoonExecutableSha256,
      registry_path: native.brainmonsoonRegistry,
      registry_sha256: native.brainmonsoonRegistrySha256,
      csl_path: native.brainmonsoonCsl,
      csl_sha256: native.brainmonsoonCslSha256,
      nil_path: native.brainmonsoonNil,
      nil_sha256: native.brainmonsoonNilSha256,
      cssl_path: native.brainmonsoonCssl,
      cssl_sha256: native.brainmonsoonCsslSha256,
      state_root: native.brainmonsoonStateRoot,
      query,
      limit: Math.min(limit, 8),
      deadline_ms: innerDeadline(this.timeoutMs),
    }], signal, Math.max(524_288, this.config.limits.responseBytes * 4));
    const payload = frames[0] as Record<string, unknown> | undefined;
    if (!payload || payload.ok !== true || payload.read_only !== true || payload.authority !== 'read_only_analysis') {
      throw new Error('NATIVE_BRAINMONSOON_UNAVAILABLE');
    }
    return payload;
  }
  async search(request: SearchRequest, signal: AbortSignal): Promise<unknown> {
    const payload = await this.invoke('recall', this.config.native.brainmonsoonLineageSha256 as string, request.limit, signal);
    const records = brainmonsoonRecords(payload);
    if (records.length === 0) throw new Error('BRAINMONSOON_CORPUS_UNAVAILABLE');
    return { records };
  }
  async probe(signal: AbortSignal): Promise<AdapterProbe> {
    const native = this.config.native;
    if (!await regularFiles([
      this.reader, native.brainmonsoonExecutable, native.brainmonsoonRegistry,
      native.brainmonsoonCsl, native.brainmonsoonNil, native.brainmonsoonCssl,
    ]) || !native.brainmonsoonStateRoot) {
      return { state: 'unavailable', detail: 'pinned Brainmonsoon reader, package, or state root absent' };
    }
    try {
      const payload = await this.invoke('recall', native.brainmonsoonLineageSha256 as string, 1, signal);
      if (brainmonsoonRecords(payload).length === 0) throw new Error('BRAINMONSOON_CORPUS_UNAVAILABLE');
      return { state: 'ready', detail: 'managed stdio protected lineage recall verified' };
    } catch {
      return { state: 'unavailable', detail: 'managed stdio health failed' };
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
  const brain = config.native;
  const brainConfigured = Boolean(brain.brainmonsoonExecutable && brain.brainmonsoonExecutableSha256
    && brain.brainmonsoonRegistry && brain.brainmonsoonRegistrySha256
    && brain.brainmonsoonCsl && brain.brainmonsoonCslSha256
    && brain.brainmonsoonNil && brain.brainmonsoonNilSha256
    && brain.brainmonsoonCssl && brain.brainmonsoonCsslSha256 && brain.brainmonsoonStateRoot
    && brain.brainmonsoonLineageSha256);
  result.set('brainmonsoon', config.upstreams.brainmonsoon
    ? new UpstreamAdapter('brainmonsoon', config.upstreams.brainmonsoon, config.limits.responseBytes * 4)
    : brainConfigured ? new NativeBrainmonsoonAdapter(config) : new UnconfiguredAdapter('brainmonsoon'));
  result.set('anamnesis', config.upstreams.anamnesis
    ? new UpstreamAdapter('anamnesis', config.upstreams.anamnesis, config.limits.responseBytes * 4)
    : config.native.anamnesisDb
      ? new NativeAnamnesisAdapter(config) : new UnconfiguredAdapter('anamnesis'));
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
