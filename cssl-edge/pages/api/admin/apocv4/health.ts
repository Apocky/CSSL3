import type { NextApiRequest, NextApiResponse } from 'next';

import {
  APOCV4_PROXY_SCHEMA,
  RuntimeProxyError,
  fetchRuntimeHealth,
  publicRuntimeError,
} from '@/lib/apocv4/runtime-proxy';
import { requireAdmin } from '@/lib/require-admin';
import { envelope } from '@/lib/response';

export const maxDuration = 20;

export const config = {
  api: {
    responseLimit: '512kb',
  },
};

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  res.setHeader('Cache-Control', 'no-store, max-age=0');
  if (req.method !== 'GET') {
    res.setHeader('Allow', 'GET');
    res.status(405).json({ error: 'Method not allowed', ...envelope() });
    return;
  }
  if (!(await requireAdmin(req, res))) return;
  try {
    const result = await fetchRuntimeHealth();
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
