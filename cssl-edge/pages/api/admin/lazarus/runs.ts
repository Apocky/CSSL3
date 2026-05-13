import type { NextApiRequest, NextApiResponse } from 'next';

import { envelope } from '@/lib/response';
import { requireAdmin, requireRunnerToken } from '@/lib/lazarus/auth';
import { finishRun, listRuns } from '@/lib/lazarus/store';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  try {
    if (req.method === 'GET') {
      if (!(await requireAdmin(req, res))) return;
      return res.status(200).json({ ...(await listRuns()), ...envelope() });
    }
    if (req.method === 'POST') {
      if (!requireRunnerToken(req, res)) return;
      const body = (req.body ?? {}) as { run_id?: unknown; status?: unknown; summary?: unknown };
      if (typeof body.run_id !== 'string') return res.status(400).json({ error: 'run_id required', ...envelope() });
      if (body.status !== 'completed' && body.status !== 'failed' && body.status !== 'cancelled') {
        return res.status(400).json({ error: 'status must be completed|failed|cancelled', ...envelope() });
      }
      const summary = typeof body.summary === 'string' ? body.summary : `run ${body.status}`;
      return res.status(200).json({ ...(await finishRun(body.run_id, body.status, summary)), ...envelope() });
    }
    res.setHeader('Allow', 'GET, POST');
    return res.status(405).json({ error: 'Method not allowed', ...envelope() });
  } catch (err) {
    return res.status(500).json({ error: err instanceof Error ? err.message : String(err), ...envelope() });
  }
}
