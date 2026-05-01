// cssl-edge · /api/intent
// POST text → scene-graph stub. Real impl will call Claude (Anthropic SDK) when
// CLAUDE_API_KEY is set. Stage-0 returns empty scene_graph + warning.

import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, resolveCap, stubEnvelope } from '@/lib/response';

interface IntentRequest {
  text?: unknown;
  cap?: unknown;
}

interface SceneGraph {
  nodes: unknown[];
  edges: unknown[];
}

interface IntentResponse {
  scene_graph: SceneGraph;
  warnings: string[];
  latency_ms: number;
  served_by: string;
  ts: string;
  stub: true;
  todo: string;
  cap: 'sovereign' | 'none';
}

interface IntentError {
  error: string;
  served_by: string;
  ts: string;
}

function isIntentRequest(b: unknown): b is IntentRequest {
  return typeof b === 'object' && b !== null;
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<IntentResponse | IntentError>
): Promise<void> {
  const start = Date.now();
  logHit('intent', { method: req.method ?? 'GET' });

  if (req.method !== 'POST') {
    const stub = stubEnvelope('POST only');
    res.setHeader('Allow', 'POST');
    res.status(405).json({
      error: 'Method Not Allowed — POST {text, cap}',
      served_by: stub.served_by,
      ts: stub.ts,
    });
    return;
  }

  const body: unknown = req.body;
  if (!isIntentRequest(body)) {
    const stub = stubEnvelope('body must be JSON object');
    res.status(400).json({
      error: 'Bad Request — body must be JSON {text, cap}',
      served_by: stub.served_by,
      ts: stub.ts,
    });
    return;
  }

  const text = typeof body.text === 'string' ? body.text : '';
  const cap = resolveCap(typeof body.cap === 'string' ? body.cap : undefined);

  const hasKey = Boolean(process.env.CLAUDE_API_KEY);
  const warnings: string[] = [];
  if (!hasKey) {
    warnings.push('LLM not configured · returning stub (set CLAUDE_API_KEY in Vercel env)');
  }
  if (text.length === 0) {
    warnings.push('empty text · returning empty scene_graph');
  }

  const stub = stubEnvelope('wire Anthropic SDK · text→scene-graph compiler');
  const response: IntentResponse = {
    scene_graph: { nodes: [], edges: [] },
    warnings,
    latency_ms: Date.now() - start,
    served_by: stub.served_by,
    ts: stub.ts,
    stub: true,
    todo: stub.todo,
    cap,
  };

  res.status(200).json(response);
}
