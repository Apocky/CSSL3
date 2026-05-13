import type { NextApiRequest, NextApiResponse } from 'next';

import { envelope } from '@/lib/response';
import { requireAdmin } from '@/lib/lazarus/auth';
import { decideApproval, listApprovals, requestApproval } from '@/lib/lazarus/store';
import type { JsonRecord } from '@/lib/lazarus/types';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  try {
    if (req.method === 'GET') {
      if (!(await requireAdmin(req, res))) return;
      return res.status(200).json({ ...(await listApprovals()), ...envelope() });
    }
    if (req.method === 'POST') {
      if (!(await requireAdmin(req, res))) return;
      const body = (req.body ?? {}) as {
        action?: unknown;
        approval_id?: unknown;
        decision?: unknown;
        decided_by?: unknown;
        run_id?: unknown;
        gate?: unknown;
        reason?: unknown;
        payload?: unknown;
      };
      if (body.action === 'decide') {
        if (typeof body.approval_id !== 'string') return res.status(400).json({ error: 'approval_id required', ...envelope() });
        if (body.decision !== 'approved' && body.decision !== 'denied') {
          return res.status(400).json({ error: 'decision must be approved|denied', ...envelope() });
        }
        const decidedBy = typeof body.decided_by === 'string' ? body.decided_by : 'admin-console';
        return res.status(200).json({ ...(await decideApproval(body.approval_id, body.decision, decidedBy)), ...envelope() });
      }
      if (typeof body.run_id !== 'string') return res.status(400).json({ error: 'run_id required', ...envelope() });
      const reason = typeof body.reason === 'string' ? body.reason : 'approval requested';
      const payload = body.payload && typeof body.payload === 'object' && !Array.isArray(body.payload) ? body.payload as JsonRecord : {};
      return res.status(201).json({
        ...(await requestApproval(body.run_id, body.gate, reason, payload)),
        ...envelope(),
      });
    }
    res.setHeader('Allow', 'GET, POST');
    return res.status(405).json({ error: 'Method not allowed', ...envelope() });
  } catch (err) {
    return res.status(500).json({ error: err instanceof Error ? err.message : String(err), ...envelope() });
  }
}
