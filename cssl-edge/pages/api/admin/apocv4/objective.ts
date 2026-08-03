import type { NextApiRequest, NextApiResponse } from 'next';

import {
  APOCV4_PROXY_SCHEMA,
  RuntimeProxyError,
  publicRuntimeError,
  submitRuntimeObjective,
} from '@/lib/apocv4/runtime-proxy';
import { requireAdmin } from '@/lib/require-admin';
import { envelope } from '@/lib/response';

export const maxDuration = 300;

export const config = {
  api: {
    bodyParser: { sizeLimit: '24kb' },
    responseLimit: '3mb',
  },
};

function exactObjectiveBody(value: unknown): string | null {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return null;
  const body = value as Record<string, unknown>;
  if (Object.keys(body).length !== 1 || !Object.hasOwn(body, 'objective')) return null;
  const objective = body.objective;
  if (
    typeof objective !== 'string'
    || objective !== objective.trim()
    || objective.length < 1
    || objective.length > 16_384
  ) return null;
  return objective;
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  res.setHeader('Cache-Control', 'no-store, max-age=0');
  if (req.method !== 'POST') {
    res.setHeader('Allow', 'POST');
    res.status(405).json({ error: 'Method not allowed', ...envelope() });
    return;
  }
  if (!(await requireAdmin(req, res))) return;
  const objective = exactObjectiveBody(req.body);
  if (objective === null) {
    res.status(400).json({
      schema_version: APOCV4_PROXY_SCHEMA,
      error: 'objective_body_invalid',
      ...envelope(),
    });
    return;
  }
  try {
    const result = await submitRuntimeObjective(objective);
    res.status(200).json({ ...result, ...envelope() });
  } catch (error) {
    const status = error instanceof RuntimeProxyError ? error.publicStatus : 502;
    res.status(status).json({
      schema_version: APOCV4_PROXY_SCHEMA,
      ...publicRuntimeError(error),
      ...envelope(),
    });
  }
}
