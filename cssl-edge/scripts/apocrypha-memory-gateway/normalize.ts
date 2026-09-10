import { createHash } from 'node:crypto';
import type { AdapterName, GatewayLimits, GatewayRecord } from './types';

const COLLECTION_KEYS = new Set([
  'records', 'results', 'items', 'matches', 'memories', 'data', 'evidence', 'regions', 'result', 'snapshot', 'surfaces',
  'ranked_nodes', 'node', 'claims', 'lenses',
]);
const TEXT_KEYS = ['text', 'content', 'document', 'summary', 'snippet', 'description', 'title', 'name', 'label', 'value'] as const;
const ID_KEYS = ['provenance_id', 'source_id', 'id', 'drawer_id', 'uri', 'locator', 'content_sha256', 'evidence_sha256'] as const;
const META_KEYS = new Set([
  'region', 'source_system', 'epistemic_status', 'kind', 'type', 'label', 'score', 'rank', 'authored_at', 'created_at',
  'status', 'code', 'authority', 'execution_authorized', 'effect_authority',
]);

function object(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : null;
}

function idFor(value: unknown, fallback: string): string {
  const raw = typeof value === 'string' || typeof value === 'number' ? String(value) : fallback;
  const bounded = raw.slice(0, 256);
  if (/^[a-z]:[\\/]|^[/\\]{1,2}/iu.test(bounded)) {
    return `opaque:${createHash('sha256').update(bounded).digest('hex')}`;
  }
  return bounded || fallback;
}

function scalar(value: unknown): string | number | boolean | null | undefined {
  if (value === null || typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') return value;
  return undefined;
}

function collect(value: unknown, output: unknown[], depth = 0): void {
  if (depth > 6 || output.length >= 160) return;
  if (Array.isArray(value)) {
    for (const item of value) collect(item, output, depth + 1);
    return;
  }
  const body = object(value);
  if (!body) return;
  if (TEXT_KEYS.some((key) => typeof body[key] === 'string' && String(body[key]).trim())) output.push(body);
  for (const [key, item] of Object.entries(body)) {
    if (COLLECTION_KEYS.has(key)) collect(item, output, depth + 1);
  }
}

export function normalizeRecords(name: AdapterName, payload: unknown, limits: GatewayLimits): GatewayRecord[] {
  const candidates: unknown[] = [];
  collect(payload, candidates);
  const records: GatewayRecord[] = [];
  let remaining = limits.totalChars;
  for (const [index, item] of candidates.entries()) {
    if (records.length >= limits.maxRecords || remaining <= 0) break;
    const body = object(item);
    if (!body) continue;
    const textValue = TEXT_KEYS.map((key) => body[key]).find((value) => typeof value === 'string' && value.trim());
    const text = typeof textValue === 'string' ? textValue.trim().slice(0, Math.min(limits.recordChars, remaining)) : '';
    if (!text) continue;
    const idValue = ID_KEYS.map((key) => body[key]).find((value) => typeof value === 'string' || typeof value === 'number');
    const metadata: GatewayRecord['metadata'] = { source: name, authority: 'none', read_only: true };
    for (const [key, value] of Object.entries(body)) {
      if (!META_KEYS.has(key)) continue;
      const admitted = scalar(value);
      if (admitted !== undefined && String(admitted).length <= 256) metadata[key] = admitted;
    }
    records.push({ id: idFor(idValue, `${name}:${index}`), text, metadata });
    remaining -= text.length;
  }
  return records;
}

export function boundedRecordsEnvelope(
  name: AdapterName,
  records: GatewayRecord[],
  responseBytes: number,
): Record<string, unknown> {
  const admitted = [...records];
  const envelope = () => ({
    schema: 'apocrypha.memory.gateway.records.v1',
    adapter: name,
    authority: 'none',
    execution_authorized: false,
    read_only: true,
    records: admitted,
  });
  while (admitted.length && Buffer.byteLength(JSON.stringify(envelope()), 'utf8') > responseBytes) admitted.pop();
  const result = envelope();
  if (Buffer.byteLength(JSON.stringify(result), 'utf8') > responseBytes) throw new Error('RESPONSE_LIMIT_TOO_SMALL');
  return result;
}
