import { sha256, stableJson } from './crypto';
import type {
  ClaimedJob,
  MemoryAdapterManifest,
  RetrievalAdapterResult,
  RetrievalBundle,
  RetrievalRecord,
  WorkerConfig,
} from './types';

type Fetch = typeof fetch;

function record(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' ? value as Record<string, unknown> : {};
}

function safeText(value: unknown): string {
  if (typeof value === 'string') return value;
  if (value === null || value === undefined) return '';
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function isSafeAdapterUrl(raw: string): boolean {
  try {
    const url = new URL(raw);
    const loopback = ['127.0.0.1', 'localhost', '::1', '[::1]'].includes(url.hostname);
    return url.protocol === 'https:' || (url.protocol === 'http:' && loopback);
  } catch {
    return false;
  }
}

function sourceItems(payload: unknown): unknown[] {
  if (Array.isArray(payload)) return payload;
  const body = record(payload);
  for (const key of ['records', 'results', 'items', 'matches', 'memories', 'data']) {
    const value = body[key];
    if (Array.isArray(value)) return value;
  }
  if (typeof body.text === 'string' || typeof body.content === 'string') return [body];
  return [];
}

async function boundedResponseText(response: Response, limit = 1_000_000): Promise<string> {
  if (!response.body) return (await response.text()).slice(0, limit);
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let text = '';
  while (text.length < limit) {
    const { done, value } = await reader.read();
    if (done) break;
    text += decoder.decode(value, { stream: true });
    if (text.length >= limit) {
      await reader.cancel('retrieval response size limit reached').catch(() => undefined);
      break;
    }
  }
  text += decoder.decode();
  return text.slice(0, limit);
}

function normalizeRecords(name: string, payload: unknown, maxChars: number): RetrievalRecord[] {
  const records: RetrievalRecord[] = [];
  let remaining = maxChars;
  for (const [index, item] of sourceItems(payload).entries()) {
    if (remaining <= 0 || records.length >= 12) break;
    const body = record(item);
    const rawText = body.text ?? body.content ?? body.summary ?? body.value ?? item;
    const text = safeText(rawText).trim().slice(0, remaining);
    if (!text) continue;
    const idValue = body.provenance_id ?? body.source_id ?? body.id ?? body.uri ?? `${name}:${index}`;
    const provenanceId = safeText(idValue).slice(0, 256) || `${name}:${index}`;
    const metadata = record(body.metadata);
    records.push({ source: name, provenanceId, text, metadata });
    remaining -= text.length;
  }
  return records;
}

function boundedJson(value: unknown, maxChars: number): string {
  try {
    return JSON.stringify(value).slice(0, maxChars);
  } catch {
    return '';
  }
}

function publicAdapterError(payload: string): { code: string; cause: string | null } | null {
  try {
    const body = record(JSON.parse(payload) as unknown);
    const code = body.error;
    const cause = body.cause;
    return typeof code === 'string' && /^[A-Z][A-Z0-9_]{1,79}$/u.test(code)
      ? {
          code,
          cause: typeof cause === 'string' && /^[A-Z][A-Z0-9_]{1,79}$/u.test(cause) ? cause : null,
        }
      : null;
  } catch {
    return null;
  }
}

function canonicalReadingQuery(value: unknown): string {
  const reading = record(value);
  if (Object.keys(reading).length === 0) return '';
  const system = record(reading.system);
  const spread = record(reading.spread);
  const items = Array.isArray(reading.items)
    ? reading.items.slice(0, 24).flatMap((item): string[] => {
        const entry = record(item);
        const position = record(entry.position);
        const name = safeText(entry.name).trim();
        if (!name) return [];
        const positionName = safeText(position.name).trim();
        return [`${positionName ? `${positionName}: ` : ''}${name}${entry.is_reversed === true ? ' reversed' : ''}`];
      })
    : [];
  return [
    safeText(system.name ?? system.id).trim(),
    safeText(spread.name ?? spread.id).trim(),
    items.join(', '),
  ].filter(Boolean).join(' | ').slice(0, 1_500);
}

export function queryFromJob(job: ClaimedJob): string {
  const request = job.request;
  const explicit = request.retrieval_query;
  if (typeof explicit === 'string' && explicit.trim()) return explicit.trim().slice(0, 4_000);

  const question = safeText(request.question).trim();
  const reading = canonicalReadingQuery(request.canonical_reading);
  const source = safeText(request.source_text).trim().slice(0, 1_800);
  const structured = boundedJson(request.structured_context, 1_000);
  const history = Array.isArray(request.conversation_history)
    ? request.conversation_history
        .map((item) => record(item))
        .filter((item) => item.role === 'user')
        .map((item) => safeText(item.content).trim())
        .filter(Boolean)
        .slice(-2)
        .join('\n')
        .slice(0, 1_000)
    : '';
  const chaosQuery = [question, reading, history, source, structured].filter(Boolean).join('\n');
  if (chaosQuery) return chaosQuery.slice(0, 4_000);

  const legacy = request.query ?? request.prompt ?? request.text ?? request.content ?? request.oracle_prompt;
  if (typeof legacy === 'string' && legacy.trim()) return legacy.trim().slice(0, 4_000);
  const messages = request.messages;
  if (Array.isArray(messages)) {
    const joined = messages
      .map((item) => record(item))
      .filter((item) => item.role === 'user')
      .map((item) => safeText(item.content))
      .filter(Boolean)
      .join('\n');
    if (joined) return joined.slice(-4_000);
  }
  return `${job.kind} ${job.capability}`;
}

async function invokeAdapter(
  adapter: MemoryAdapterManifest,
  job: ClaimedJob,
  query: string,
  env: NodeJS.ProcessEnv,
  fetchImpl: Fetch,
  limit = 8,
): Promise<RetrievalAdapterResult> {
  const started = Date.now();
  const rawUrl = env[adapter.urlEnv]?.trim();
  if (!rawUrl) return { name: adapter.name, state: 'unconfigured', durationMs: 0, records: [] };
  if (!isSafeAdapterUrl(rawUrl)) {
    return { name: adapter.name, state: 'denied', durationMs: 0, records: [], detail: 'adapter URL must use HTTPS or loopback HTTP' };
  }
  if (adapter.requiredCapabilities?.length && !adapter.requiredCapabilities.includes(job.capability)) {
    return { name: adapter.name, state: 'denied', durationMs: 0, records: [], detail: 'capability not admitted' };
  }
  const controller = new AbortController();
  const timeoutOverride = Number(env[`APOCRYPHA_${adapter.name.toUpperCase()}_READ_TIMEOUT_MS`]?.trim());
  const timeoutMs = Number.isInteger(timeoutOverride) && timeoutOverride >= 250 && timeoutOverride <= 60_000
    ? timeoutOverride
    : adapter.timeoutMs ?? 3_500;
  const timer = setTimeout(() => controller.abort(new Error('retrieval timeout')), timeoutMs);
  try {
    const token = adapter.tokenEnv ? env[adapter.tokenEnv]?.trim() : undefined;
    const response = await fetchImpl(rawUrl, {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        accept: 'application/json',
        ...(token ? { authorization: `Bearer ${token}` } : {}),
      },
      body: JSON.stringify({
        operation: 'search',
        read_only: true,
        query,
        limit,
        tenant_id: job.tenantId,
        principal_id: job.ownerPrincipalId,
        capability: job.capability,
        memory_manifest_hash: job.memoryManifestHash,
      }),
      signal: controller.signal,
    });
    if (!response.ok) {
      const bounded = await boundedResponseText(response, 16_384);
      const upstream = publicAdapterError(bounded);
      const code = upstream?.code ?? null;
      const state = response.status === 401 || response.status === 403
        ? 'denied'
        : response.status === 504 || code === 'ADAPTER_TIMEOUT'
          ? 'timeout'
          : code === 'ADAPTER_UNCONFIGURED'
            ? 'unconfigured'
            : 'error';
      return {
        name: adapter.name,
        state,
        durationMs: Date.now() - started,
        records: [],
        detail: upstream?.cause ?? code ?? `HTTP ${response.status}`,
      };
    }
    const bounded = await boundedResponseText(response);
    let payload: unknown = bounded;
    try {
      payload = JSON.parse(bounded);
    } catch {
      // A bounded text response is still useful read-only context.
    }
    return {
      name: adapter.name,
      state: 'ok',
      durationMs: Date.now() - started,
      records: normalizeRecords(adapter.name, payload, adapter.maxChars ?? 7_000),
    };
  } catch (error) {
    return {
      name: adapter.name,
      state: controller.signal.aborted ? 'timeout' : 'error',
      durationMs: Date.now() - started,
      records: [],
      detail: error instanceof Error ? error.message.slice(0, 300) : 'retrieval failed',
    };
  } finally {
    clearTimeout(timer);
  }
}

async function invokeAdaptersBounded(
  config: WorkerConfig,
  operation: (adapter: MemoryAdapterManifest) => Promise<RetrievalAdapterResult>,
): Promise<Array<PromiseSettledResult<RetrievalAdapterResult>>> {
  const adapters = config.manifest.memory.adapters;
  const settled = new Array<PromiseSettledResult<RetrievalAdapterResult>>(adapters.length);
  let cursor = 0;
  const runner = async (): Promise<void> => {
    while (cursor < adapters.length) {
      const index = cursor;
      cursor += 1;
      const adapter = adapters[index] as MemoryAdapterManifest;
      try {
        settled[index] = { status: 'fulfilled', value: await operation(adapter) };
      } catch (reason) {
        settled[index] = { status: 'rejected', reason };
      }
    }
  };
  await Promise.all(Array.from(
    { length: Math.min(config.memoryReadConcurrency, Math.max(1, adapters.length)) },
    () => runner(),
  ));
  return settled;
}

function completedProbeAt(
  config: WorkerConfig,
  env: NodeJS.ProcessEnv,
  results: RetrievalAdapterResult[],
): string | null {
  const allConfigured = config.manifest.memory.adapters.every((adapter) => {
    const url = env[adapter.urlEnv]?.trim();
    return Boolean(url && isSafeAdapterUrl(url));
  });
  return allConfigured && results.length === config.manifest.memory.adapters.length
    && results.every((result) => result.state === 'ok')
    ? new Date().toISOString()
    : null;
}

export async function retrieveMemory(
  config: WorkerConfig,
  job: ClaimedJob,
  env: NodeJS.ProcessEnv = process.env,
  fetchImpl: Fetch = fetch,
): Promise<RetrievalBundle> {
  const query = queryFromJob(job);
  const settled = await invokeAdaptersBounded(config,
    (adapter) => invokeAdapter(adapter, job, query, env, fetchImpl));
  const results = settled.map((result, index): RetrievalAdapterResult => {
    if (result.status === 'fulfilled') return result.value;
    return {
      name: config.manifest.memory.adapters[index]?.name ?? `adapter-${index}`,
      state: 'error',
      durationMs: 0,
      records: [],
      detail: result.reason instanceof Error ? result.reason.message : 'adapter failed',
    };
  });
  const records = results.flatMap((result) => result.records).slice(0, 40);
  return {
    query, results, records, digest: sha256(stableJson(records)),
    probedAt: completedProbeAt(config, env, results),
  };
}

export async function probeMemoryAdapters(
  config: WorkerConfig,
  env: NodeJS.ProcessEnv = process.env,
  fetchImpl: Fetch = fetch,
): Promise<RetrievalBundle | null> {
  if (!config.memoryProbeTenantId) return null;
  const job: ClaimedJob = {
    jobId: '00000000-0000-4000-8000-000000000000',
    attemptId: '00000000-0000-4000-8000-000000000000',
    attemptNo: 0,
    leaseEpoch: 0,
    leaseToken: '',
    leaseExpiresAt: new Date(0).toISOString(),
    tenantId: config.memoryProbeTenantId,
    ownerPrincipalId: config.memoryProbePrincipalId,
    kind: 'operational_probe',
    capability: config.memoryProbeCapability,
    request: { retrieval_query: 'resident adapter operational health' },
    modelAlias: config.modelAlias,
    profileHash: config.profileHash,
    toolRegistryVersion: config.toolRegistryVersion,
    memoryManifestHash: config.memoryManifestHash,
  };
  const query = queryFromJob(job);
  const settled = await invokeAdaptersBounded(config,
    (adapter) => invokeAdapter(adapter, job, query, env, fetchImpl, 1));
  const results = settled.map((result, index): RetrievalAdapterResult => result.status === 'fulfilled' ? result.value : ({
    name: config.manifest.memory.adapters[index]?.name ?? `adapter-${index}`,
    state: 'error', durationMs: 0, records: [],
    detail: result.reason instanceof Error ? result.reason.message : 'adapter probe failed',
  }));
  const records = results.flatMap((result) => result.records).slice(0, 40);
  return {
    query, results, records, digest: sha256(stableJson(records)),
    probedAt: completedProbeAt(config, env, results),
  };
}

export function renderMemoryContext(bundle: RetrievalBundle, maxChars = 28_000): string {
  if (bundle.records.length === 0) return 'No admitted memory records were available for this turn.';
  let remaining = maxChars;
  const blocks: string[] = [];
  for (const item of bundle.records) {
    const header = `[${item.source} · ${item.provenanceId}]`;
    const budget = Math.max(0, remaining - header.length - 2);
    if (budget <= 0) break;
    const body = item.text.slice(0, budget);
    blocks.push(`${header}\n${body}`);
    remaining -= header.length + body.length + 2;
  }
  return blocks.join('\n\n');
}
