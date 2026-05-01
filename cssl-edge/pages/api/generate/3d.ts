// cssl-edge · /api/generate/3d
// Neural-3D gateway. Real impl fans out to Stability / Meshy / Tripo, applies
// the asset-license filter to the *output* (the generated mesh's licensing
// terms), and caches the result via Supabase. Stage-0 returns an enriched
// stub envelope — license-filter now applied, request_id echoed, audit logged.
//
// Wave-4 enrichment :
//   - input validation : prompt required · ≤ 500 chars · sanitize control chars
//   - license filter   : `license=cc0,cc-by` query/body param (default both)
//   - request_id       : UUID echoed back so callers can correlate audit lines
//   - audit log        : structured event reading `x-loa-cap` header

import type { NextApiRequest, NextApiResponse } from 'next';
import { auditEvent, logEvent } from '@/lib/audit';
import { normalizeLicense, PERMITTED_FOR_ASSETS, type License } from '@/lib/license_filter';
import { logHit, stubEnvelope } from '@/lib/response';

interface GenerateRequest {
  prompt?: unknown;
  provider?: unknown;
  format?: unknown;
  license?: unknown;
}

type Provider = 'stability' | 'meshy' | 'tripo' | 'auto';

interface GenerateResponse {
  job_id: string;
  request_id: string;
  status: 'queued' | 'stub';
  provider: Provider;
  format: 'glb' | 'gltf' | 'obj';
  prompt: string;
  result_url: string | null;
  license_filter: License[];
  served_by: string;
  ts: string;
  stub: true;
  todo: string;
}

interface GenerateError {
  error: string;
  request_id: string;
  served_by: string;
  ts: string;
}

const PROMPT_MAX_CHARS = 500;

function isObject(b: unknown): b is Record<string, unknown> {
  return typeof b === 'object' && b !== null;
}

function pickProvider(raw: unknown): Provider {
  if (raw === 'stability' || raw === 'meshy' || raw === 'tripo') return raw;
  return 'auto';
}

function pickFormat(raw: unknown): 'glb' | 'gltf' | 'obj' {
  if (raw === 'gltf' || raw === 'obj') return raw;
  return 'glb';
}

// Strip control chars (NUL through 0x1F + DEL) — defense-in-depth for
// downstream prompt-injection. Keep printable + extended Unicode intact.
function sanitizePrompt(s: string): string {
  // eslint-disable-next-line no-control-regex
  return s.replace(/[\x00-\x1f\x7f]/g, ' ').trim();
}

// Deterministic stub-job-id so callers can poll without server state.
function stubJobId(prompt: string): string {
  let hash = 5381;
  for (let i = 0; i < prompt.length; i += 1) {
    hash = ((hash * 33) ^ prompt.charCodeAt(i)) >>> 0;
  }
  return `stub-${hash.toString(16).padStart(8, '0')}`;
}

// RFC4122-ish v4 UUID. Crypto.randomUUID is preferred when available; fall back
// to a hex-mash for older Node — both forms are unique-enough for request_id.
function genRequestId(): string {
  const g = globalThis as unknown as { crypto?: { randomUUID?: () => string } };
  if (g.crypto && typeof g.crypto.randomUUID === 'function') {
    return g.crypto.randomUUID();
  }
  // Fallback : 32 hex digits + dashes.
  const hex = (n: number): string => n.toString(16).padStart(2, '0');
  const bytes = new Array<number>(16);
  for (let i = 0; i < 16; i += 1) bytes[i] = Math.floor(Math.random() * 256);
  // Set version (4) + variant (10xx) per RFC4122.
  bytes[6] = ((bytes[6] ?? 0) & 0x0f) | 0x40;
  bytes[8] = ((bytes[8] ?? 0) & 0x3f) | 0x80;
  const h = bytes.map((b) => hex(b)).join('');
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20, 32)}`;
}

// Parse a license-filter query/body value into a list of permitted licenses.
// Accepts comma-separated forms : "cc0", "cc0,cc-by", etc. Rejects unknown.
// Default (no value supplied) = full PERMITTED_FOR_ASSETS set.
function parseLicenseFilter(raw: unknown): License[] {
  if (typeof raw !== 'string' || raw.length === 0) {
    return Array.from(PERMITTED_FOR_ASSETS);
  }
  const parts = raw.split(',').map((s) => normalizeLicense(s.trim()));
  const filtered = parts.filter((l): l is License => PERMITTED_FOR_ASSETS.has(l));
  // Empty after filtering → fall back to full permit-list rather than zero.
  return filtered.length === 0 ? Array.from(PERMITTED_FOR_ASSETS) : filtered;
}

// Read cap-bit from x-loa-cap header (hex or decimal). Defaults to 0.
function readCapHeader(hdrs: NextApiRequest['headers']): number {
  const raw = hdrs['x-loa-cap'];
  const v = Array.isArray(raw) ? raw[0] : raw;
  if (typeof v !== 'string' || v.length === 0) return 0;
  const trimmed = v.trim();
  if (trimmed.startsWith('0x') || trimmed.startsWith('0X')) {
    const n = parseInt(trimmed.slice(2), 16);
    return Number.isFinite(n) ? n : 0;
  }
  const n = parseInt(trimmed, 10);
  return Number.isFinite(n) ? n : 0;
}

export default function handler(
  req: NextApiRequest,
  res: NextApiResponse<GenerateResponse | GenerateError>
): void {
  logHit('generate.3d', { method: req.method ?? 'GET' });
  const requestId = genRequestId();
  const capUsed = readCapHeader(req.headers);

  if (req.method !== 'POST') {
    const stub = stubEnvelope('POST only');
    res.setHeader('Allow', 'POST');
    res.status(405).json({
      error: 'Method Not Allowed — POST {prompt, provider?, format?, license?}',
      request_id: requestId,
      served_by: stub.served_by,
      ts: stub.ts,
    });
    return;
  }

  const body: unknown = req.body;
  if (!isObject(body)) {
    const stub = stubEnvelope('body must be JSON object');
    logEvent(
      auditEvent('generate.3d', capUsed, false, 'error', {
        request_id: requestId,
        reason: 'body-not-object',
      })
    );
    res.status(400).json({
      error: 'Bad Request — body must be JSON {prompt, provider?, format?, license?}',
      request_id: requestId,
      served_by: stub.served_by,
      ts: stub.ts,
    });
    return;
  }

  const reqBody = body as GenerateRequest;
  const promptRaw = typeof reqBody.prompt === 'string' ? reqBody.prompt : '';
  if (promptRaw.length === 0) {
    const stub = stubEnvelope('require non-empty prompt');
    logEvent(
      auditEvent('generate.3d', capUsed, false, 'error', {
        request_id: requestId,
        reason: 'prompt-empty',
      })
    );
    res.status(400).json({
      error: 'Bad Request — prompt is required',
      request_id: requestId,
      served_by: stub.served_by,
      ts: stub.ts,
    });
    return;
  }
  if (promptRaw.length > PROMPT_MAX_CHARS) {
    const stub = stubEnvelope('prompt size cap exceeded');
    logEvent(
      auditEvent('generate.3d', capUsed, false, 'error', {
        request_id: requestId,
        reason: 'prompt-too-long',
        len: promptRaw.length,
      })
    );
    res.status(400).json({
      error: `Bad Request — prompt exceeds ${PROMPT_MAX_CHARS} chars (got ${promptRaw.length})`,
      request_id: requestId,
      served_by: stub.served_by,
      ts: stub.ts,
    });
    return;
  }

  const prompt = sanitizePrompt(promptRaw);
  const provider = pickProvider(reqBody.provider);
  const format = pickFormat(reqBody.format);
  const licenseFilter = parseLicenseFilter(reqBody.license);

  // Audit log — successful enqueue.
  logEvent(
    auditEvent('generate.3d', capUsed, false, 'ok', {
      request_id: requestId,
      provider,
      format,
      license_filter: licenseFilter,
      prompt_len: prompt.length,
    })
  );

  const stub = stubEnvelope('fan-out to neural-3D providers · cache · poll-status route');
  res.status(202).json({
    job_id: stubJobId(prompt),
    request_id: requestId,
    status: 'stub',
    provider,
    format,
    prompt,
    result_url: null,
    license_filter: licenseFilter,
    served_by: stub.served_by,
    ts: stub.ts,
    stub: true,
    todo: stub.todo,
  });
}
