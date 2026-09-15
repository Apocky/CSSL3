import { sha256, stableJson } from './crypto';
import { synthesisSummary, synthesizeRecords, type SynthesizedRecord } from './synthesis';
import type {
  ClaimedJob,
  MemoryAdapterManifest,
  RetrievalAdapterResult,
  RetrievalBundle,
  RetrievalRecord,
  WorkerConfig,
} from './types';

type Fetch = typeof fetch;

const MAX_ADAPTER_ATTEMPTS = 2;
const ADAPTER_RETRY_DELAY_MS = 200;
const ADAPTER_RETRY_GRACE_MS = 1_000;
export const MEMORY_READINESS_QUERY =
  'current Apocrypha and Chaos Tarot Oracle production memory recall readiness';

interface AdapterAttemptOutcome {
  result: RetrievalAdapterResult;
  retryable: boolean;
}

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
  const legacyPrompt = [request.prompt, request.query, request.text, request.content, request.oracle_prompt]
    .find((value): value is string => typeof value === 'string' && Boolean(value.trim()))
    ?.trim() ?? '';
  const currentPrompt = (question || legacyPrompt).slice(0, 4_000);
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
  const taskQuery = [currentPrompt, reading, history, source, structured].filter(Boolean).join('\n');
  if (taskQuery) return taskQuery.slice(0, 4_000);

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

async function invokeAdapterAttempt(
  adapter: MemoryAdapterManifest,
  job: ClaimedJob,
  query: string,
  rawUrl: string,
  token: string | undefined,
  timeoutMs: number,
  fetchImpl: Fetch,
  limit = 8,
): Promise<AdapterAttemptOutcome> {
  const started = Date.now();
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(new Error('retrieval timeout')), timeoutMs);
  try {
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
        result: {
          name: adapter.name,
          state,
          durationMs: Date.now() - started,
          records: [],
          detail: upstream?.cause ?? code ?? `HTTP ${response.status}`,
        },
        retryable: state === 'timeout'
          || (state === 'error' && (response.status === 408 || response.status === 425
            || response.status === 429 || response.status >= 500)),
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
      result: {
        name: adapter.name,
        state: 'ok',
        durationMs: Date.now() - started,
        records: normalizeRecords(adapter.name, payload, adapter.maxChars ?? 7_000),
      },
      retryable: false,
    };
  } catch (error) {
    return {
      result: {
        name: adapter.name,
        state: controller.signal.aborted ? 'timeout' : 'error',
        durationMs: Date.now() - started,
        records: [],
        detail: error instanceof Error ? error.message.slice(0, 300) : 'retrieval failed',
      },
      retryable: true,
    };
  } finally {
    clearTimeout(timer);
  }
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
  const timeoutOverride = Number(env[`APOCRYPHA_${adapter.name.toUpperCase()}_READ_TIMEOUT_MS`]?.trim());
  const timeoutMs = Number.isInteger(timeoutOverride) && timeoutOverride >= 250 && timeoutOverride <= 60_000
    ? timeoutOverride
    : adapter.timeoutMs ?? 3_500;
  const token = adapter.tokenEnv ? env[adapter.tokenEnv]?.trim() : undefined;
  const deadlineAt = started + timeoutMs + ADAPTER_RETRY_GRACE_MS;
  let finalResult: RetrievalAdapterResult | null = null;
  for (let attempt = 1; attempt <= MAX_ADAPTER_ATTEMPTS; attempt += 1) {
    const remainingMs = deadlineAt - Date.now();
    if (remainingMs <= 0) break;
    const attemptTimeoutMs = Math.max(1, Math.min(timeoutMs, remainingMs));
    const outcome = await invokeAdapterAttempt(adapter, job, query, rawUrl, token, attemptTimeoutMs, fetchImpl, limit);
    finalResult = { ...outcome.result, durationMs: Date.now() - started };
    if (!outcome.retryable || attempt === MAX_ADAPTER_ATTEMPTS) return finalResult;
    const delayMs = Math.min(ADAPTER_RETRY_DELAY_MS, Math.max(0, deadlineAt - Date.now()));
    if (delayMs <= 0) return finalResult;
    await new Promise((resolve) => setTimeout(resolve, delayMs));
  }
  return finalResult ?? { name: adapter.name, state: 'error', durationMs: Date.now() - started, records: [], detail: 'retrieval failed' };
}

function readinessState(value: unknown): RetrievalAdapterResult['state'] {
  if (value === 'ready') return 'ok';
  if (value === 'unconfigured') return 'unconfigured';
  if (value === 'unavailable') return 'timeout';
  return 'error';
}

async function probeResidentGateway(
  config: WorkerConfig,
  fetchImpl: Fetch,
): Promise<RetrievalBundle | null> {
  const url = config.memoryReadinessUrl;
  const token = config.memoryReadinessToken;
  if (!url || !token) return null;
  const started = Date.now();
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(new Error('resident gateway readiness timeout')), 15_000);
  try {
    const response = await fetchImpl(url, {
      method: 'GET',
      headers: { accept: 'application/json', authorization: `Bearer ${token}` },
      signal: controller.signal,
    });
    const text = await boundedResponseText(response, 128_000);
    let body: Record<string, unknown> = {};
    try { body = record(JSON.parse(text)); } catch { /* preserve bounded transport failure below */ }
    const adapters = record(body.adapters);
    const results = config.manifest.memory.adapters.map((adapter): RetrievalAdapterResult => {
      const entry = record(adapters[adapter.name]);
      const state = readinessState(entry.state);
      return {
        name: adapter.name,
        state,
        durationMs: Date.now() - started,
        records: [],
        ...(typeof entry.detail === 'string' ? { detail: entry.detail.slice(0, 300) } : {}),
      };
    });
    const ready = response.ok && body.status === 'ready'
      && results.length === config.manifest.memory.adapters.length
      && results.every((result) => result.state === 'ok');
    const probedAt = ready ? new Date().toISOString() : null;
    const capabilityProbes: NonNullable<RetrievalBundle['capabilityProbes']> = {};
    for (const scope of [
      { capability: config.memoryProbeCapability },
      ...(config.memoryAdditionalProbeScopes ?? []),
    ]) {
      capabilityProbes[scope.capability] = {
        results: results.map((result) => ({ ...result })),
        probedAt,
      };
    }
    return {
      query: MEMORY_READINESS_QUERY,
      results,
      records: [],
      digest: sha256(stableJson([])),
      probedAt,
      capabilityProbes,
    };
  } catch (error) {
    const detail = error instanceof Error ? error.message.slice(0, 300) : 'resident gateway readiness failed';
    const results = config.manifest.memory.adapters.map((adapter): RetrievalAdapterResult => ({
      name: adapter.name, state: 'timeout', durationMs: Date.now() - started, records: [], detail,
    }));
    return {
      query: MEMORY_READINESS_QUERY,
      results,
      records: [],
      digest: sha256(stableJson([])),
      probedAt: null,
      capabilityProbes: Object.fromEntries(config.manifest.capabilities.map((capability) => [capability, {
        results: results.map((result) => ({ ...result })), probedAt: null,
      }])),
    };
  } finally {
    clearTimeout(timer);
  }
}

async function invokeAdaptersBounded(
  config: WorkerConfig,
  operation: (adapter: MemoryAdapterManifest) => Promise<RetrievalAdapterResult>,
  concurrency = config.memoryReadConcurrency,
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
    { length: Math.min(Math.max(1, concurrency), Math.max(1, adapters.length)) },
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

function aggregateProbeResults(
  config: WorkerConfig,
  scopedResults: RetrievalAdapterResult[][],
): RetrievalAdapterResult[] {
  return config.manifest.memory.adapters.map((adapter, index): RetrievalAdapterResult => {
    const scoped = scopedResults.map((items) => items[index]).filter((item): item is RetrievalAdapterResult => Boolean(item));
    const failure = scoped.find((item) => item.state !== 'ok');
    return {
      name: adapter.name,
      state: failure?.state ?? 'ok',
      durationMs: scoped.reduce((total, item) => total + item.durationMs, 0),
      records: scoped.flatMap((item) => item.records).slice(0, 2),
      ...(failure?.detail ? { detail: failure.detail } : {}),
    };
  });
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
  // Synthesis, not concatenation: rank by relevance to this turn's query, fold
  // cross-faculty duplicates, and interleave so manifest order stops deciding
  // which faculty the model actually gets to read.
  const records = synthesizeRecords(results, query, 40);
  return {
    query, results, records, digest: sha256(stableJson(records)),
    // A job exercises only its own tenant/principal scope. It cannot certify
    // every configured readiness scope for the resident node.
    probedAt: null,
  };
}

export async function probeMemoryAdapters(
  config: WorkerConfig,
  env: NodeJS.ProcessEnv = process.env,
  fetchImpl: Fetch = fetch,
): Promise<RetrievalBundle | null> {
  // The resident gateway already owns bounded, per-faculty liveness probes.
  // Calling its readiness route avoids turning a multi-gigabyte recall query
  // into an operational health check while preserving full retrieval below.
  const resident = await probeResidentGateway(config, fetchImpl);
  if (resident) return resident;
  const scopes = [
    ...(config.memoryProbeTenantId ? [{
      tenantId: config.memoryProbeTenantId,
      principalId: config.memoryProbePrincipalId,
      capability: config.memoryProbeCapability,
    }] : []),
    ...(config.memoryAdditionalProbeScopes ?? []),
  ];
  if (scopes.length === 0) return null;
  const scopedResults: Array<{
    scope: { tenantId: string; principalId: string; capability: string };
    results: RetrievalAdapterResult[];
  }> = [];
  for (const [scopeIndex, scope] of scopes.entries()) {
    const job: ClaimedJob = {
      jobId: '00000000-0000-4000-8000-000000000000',
      attemptId: '00000000-0000-4000-8000-000000000000',
      attemptNo: 0,
      leaseEpoch: 0,
      leaseToken: '',
      leaseExpiresAt: new Date(0).toISOString(),
      tenantId: scope.tenantId,
      ownerPrincipalId: scope.principalId,
      kind: 'operational_probe',
      capability: scope.capability,
      request: { retrieval_query: MEMORY_READINESS_QUERY },
      modelAlias: config.modelAlias,
      profileHash: config.profileHash,
      toolRegistryVersion: config.toolRegistryVersion,
      memoryManifestHash: config.memoryManifestHash,
    };
    const query = queryFromJob(job);
    const settled = await invokeAdaptersBounded(
      config,
      (adapter) => invokeAdapter(adapter, job, query, env, fetchImpl, 1),
      config.memoryReadConcurrency,
    );
    scopedResults.push({
      scope,
      results: settled.map((result, index): RetrievalAdapterResult => result.status === 'fulfilled' ? result.value : ({
        name: config.manifest.memory.adapters[index]?.name ?? `adapter-${index}`,
        state: 'error', durationMs: 0, records: [],
        detail: `probe scope ${scopeIndex + 1}: ${result.reason instanceof Error ? result.reason.message : 'adapter probe failed'}`,
      })),
    });
  }
  const results = aggregateProbeResults(config, scopedResults.map((item) => item.results));
  const capabilityProbes: NonNullable<RetrievalBundle['capabilityProbes']> = {};
  for (const capability of new Set(scopes.map((scope) => scope.capability))) {
    const capabilityResults = aggregateProbeResults(
      config,
      scopedResults.filter((item) => item.scope.capability === capability).map((item) => item.results),
    );
    capabilityProbes[capability] = {
      results: capabilityResults,
      probedAt: completedProbeAt(config, env, capabilityResults),
    };
  }
  const query = MEMORY_READINESS_QUERY;
  const records = results.flatMap((result) => result.records).slice(0, 40);
  return {
    query, results, records, digest: sha256(stableJson(records)),
    probedAt: completedProbeAt(config, env, results),
    capabilityProbes,
  };
}

const MINIMUM_RECORD_CHARS = 480;

function scoreOf(record: RetrievalRecord): number | undefined {
  const value = (record as Partial<SynthesizedRecord>).score;
  return typeof value === 'number' ? value : undefined;
}

/**
 * Render the synthesized records under a character budget.
 *
 * Records are laid out by rank with a guaranteed minimum slice each, and the
 * space a short record does not use is handed back to the others. The previous
 * first-come loop let record #1 consume the entire budget, which is how a
 * single truncated drawer became the model's whole memory of a turn.
 */
export function renderMemoryContext(
  bundle: RetrievalBundle,
  maxChars = 28_000,
  results: readonly RetrievalAdapterResult[] = bundle.results,
): string {
  if (bundle.records.length === 0) {
    const degraded = results.filter((result) => result.state !== 'ok')
      .map((result) => `${result.name}:${result.state}`).join(', ');
    return `No admitted memory records were available for this turn.${degraded ? ` Degraded: ${degraded}.` : ''}`;
  }
  const summary = `Synthesis: ${synthesisSummary(bundle.records as SynthesizedRecord[], results)}`;
  let remaining = Math.max(0, maxChars - summary.length - 2);
  if (remaining <= 0) return summary;

  const headerFor = (record: RetrievalRecord): string => {
    const view = record as Partial<SynthesizedRecord>;
    const score = scoreOf(record);
    const merged = view.mergedFrom;
    // A raw echo count would read as independent confirmation; XL-10 says it is
    // not, so the header carries the discounted witness count beside it.
    const witnesses = view.independentWitnesses;
    return `[${view.lane === 'ambient' ? 'ambient · ' : ''}${record.source} · ${record.provenanceId}`
      + `${score === undefined ? '' : ` · relevance ${score.toFixed(2)}`}`
      + `${merged?.length ? ` · echoed by ${merged.length} (${witnesses ?? 1} independent)` : ''}]`;
  };

  // Header newline, the blank line between blocks, and a possible ellipsis.
  const blockOverhead = (record: RetrievalRecord): number => headerFor(record).length + 5;

  // How many records can be shown without any of them becoming a stub.
  const candidates: RetrievalRecord[] = [];
  let reserved = 0;
  for (const record of bundle.records) {
    const cost = Math.min(record.text.length, MINIMUM_RECORD_CHARS) + blockOverhead(record);
    if (reserved + cost > remaining) break;
    reserved += cost;
    candidates.push(record);
  }
  if (candidates.length === 0) candidates.push(bundle.records[0] as RetrievalRecord);

  const overheads = candidates.map(blockOverhead);
  let content = Math.max(0, remaining - overheads.reduce((sum, value) => sum + value, 0));
  const weights = candidates.map((record) => 0.4 + 0.6 * (scoreOf(record) ?? 0.5));
  const totalWeight = weights.reduce((sum, value) => sum + value, 0) || 1;
  const budgets = candidates.map((record, index) => Math.min(
    record.text.length,
    Math.max(Math.min(record.text.length, MINIMUM_RECORD_CHARS), Math.floor(content * (weights[index] as number) / totalWeight)),
  ));
  // Hand back whatever the short records did not need.
  let slack = content - budgets.reduce((sum, value) => sum + value, 0);
  while (slack > 0) {
    const hungry = candidates
      .map((record, index) => index)
      .filter((index) => (budgets[index] as number) < (candidates[index] as RetrievalRecord).text.length);
    if (hungry.length === 0) break;
    let given = 0;
    for (const index of hungry) {
      const want = (candidates[index] as RetrievalRecord).text.length - (budgets[index] as number);
      const grant = Math.min(want, Math.max(1, Math.floor(slack / hungry.length)), slack - given);
      budgets[index] = (budgets[index] as number) + grant;
      given += grant;
      if (given >= slack) break;
    }
    if (given === 0) break;
    slack -= given;
  }

  const blocks = candidates.map((record, index) => {
    const body = record.text.slice(0, budgets[index] as number);
    return `${headerFor(record)}\n${body}${body.length < record.text.length ? ' …' : ''}`;
  });
  // Allocation is integer-exact, but a future header change must not be able
  // to overrun a caller's hard budget.
  return [summary, ...blocks].join('\n\n').slice(0, maxChars);
}
