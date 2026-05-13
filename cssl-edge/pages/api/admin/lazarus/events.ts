import type { NextApiRequest, NextApiResponse } from 'next';

import { envelope } from '@/lib/response';
import { requireAdmin, requireRunnerToken } from '@/lib/lazarus/auth';
import { listEvents, recordEvent } from '@/lib/lazarus/store';
import type { JsonRecord, LazarusEventLevel } from '@/lib/lazarus/types';

function level(raw: unknown): LazarusEventLevel {
  return raw === 'warn' || raw === 'error' || raw === 'debug' ? raw : 'info';
}

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  try {
    if (req.method === 'GET') {
      if (!(await requireAdmin(req, res))) return;
      const run_id = typeof req.query.run_id === 'string' ? req.query.run_id : undefined;
      return res.status(200).json({ ...(await listEvents(run_id)), ...envelope() });
    }
    if (req.method === 'POST') {
      if (!requireRunnerToken(req, res)) return;
      const body = (req.body ?? {}) as {
        run_id?: unknown;
        level?: unknown;
        kind?: unknown;
        message?: unknown;
        payload?: unknown;
      };
      if (typeof body.run_id !== 'string') return res.status(400).json({ error: 'run_id required', ...envelope() });
      if (typeof body.kind !== 'string') return res.status(400).json({ error: 'kind required', ...envelope() });
      if (typeof body.message !== 'string') return res.status(400).json({ error: 'message required', ...envelope() });
      const payload = body.payload && typeof body.payload === 'object' && !Array.isArray(body.payload) ? body.payload as JsonRecord : {};
      return res.status(201).json({
        ...(await recordEvent(body.run_id, level(body.level), body.kind, body.message, payload)),
        ...envelope(),
      });
    }
    res.setHeader('Allow', 'GET, POST');
    return res.status(405).json({ error: 'Method not allowed', ...envelope() });
  } catch (err) {
    return res.status(500).json({ error: err instanceof Error ? err.message : String(err), ...envelope() });
  }
}
