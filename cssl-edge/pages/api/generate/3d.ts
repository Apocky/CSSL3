// cssl-edge · /api/generate/3d
// Neural-3D gateway stub. Real impl fans out to Stability / Meshy / Tripo
// then caches result via Supabase. Stage-0 : echoes prompt + returns stub URL.

import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, stubEnvelope } from '@/lib/response';

interface GenerateRequest {
  prompt?: unknown;
  provider?: unknown;
  format?: unknown;
}

type Provider = 'stability' | 'meshy' | 'tripo' | 'auto';

interface GenerateResponse {
  job_id: string;
  status: 'queued' | 'stub';
  provider: Provider;
  format: 'glb' | 'gltf' | 'obj';
  prompt: string;
  result_url: string | null;
  served_by: string;
  ts: string;
  stub: true;
  todo: string;
}

interface GenerateError {
  error: string;
  served_by: string;
  ts: string;
}

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

// Deterministic stub-job-id so callers can poll without server state.
function stubJobId(prompt: string): string {
  let hash = 5381;
  for (let i = 0; i < prompt.length; i += 1) {
    hash = ((hash * 33) ^ prompt.charCodeAt(i)) >>> 0;
  }
  return `stub-${hash.toString(16).padStart(8, '0')}`;
}

export default function handler(
  req: NextApiRequest,
  res: NextApiResponse<GenerateResponse | GenerateError>
): void {
  logHit('generate.3d', { method: req.method ?? 'GET' });

  if (req.method !== 'POST') {
    const stub = stubEnvelope('POST only');
    res.setHeader('Allow', 'POST');
    res.status(405).json({
      error: 'Method Not Allowed — POST {prompt, provider?, format?}',
      served_by: stub.served_by,
      ts: stub.ts,
    });
    return;
  }

  const body: unknown = req.body;
  if (!isObject(body)) {
    const stub = stubEnvelope('body must be JSON object');
    res.status(400).json({
      error: 'Bad Request — body must be JSON {prompt, provider?, format?}',
      served_by: stub.served_by,
      ts: stub.ts,
    });
    return;
  }

  const prompt = typeof body['prompt'] === 'string' ? (body['prompt'] as string) : '';
  if (prompt.length === 0) {
    const stub = stubEnvelope('require non-empty prompt');
    res.status(400).json({
      error: 'Bad Request — prompt is required',
      served_by: stub.served_by,
      ts: stub.ts,
    });
    return;
  }

  const provider = pickProvider(body['provider']);
  const format = pickFormat(body['format']);
  const stub = stubEnvelope('fan-out to neural-3D providers · cache · poll-status route');

  res.status(202).json({
    job_id: stubJobId(prompt),
    status: 'stub',
    provider,
    format,
    prompt,
    result_url: null,
    served_by: stub.served_by,
    ts: stub.ts,
    stub: true,
    todo: stub.todo,
  });
}
