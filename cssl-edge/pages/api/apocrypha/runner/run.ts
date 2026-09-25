// POST /api/apocrypha/runner/run · the flagship answers queued jobs on Vercel.
// Kicked by the chat surfaces right after they enqueue a turn, and swept by /api/cron/apocrypha-runner.
// A signed-in session is required: the runner spends gateway money.
import type { NextApiRequest, NextApiResponse } from 'next';
import { getRequestUser } from '@/lib/admin-auth';
import { isCronAuthorized } from '@/lib/cron-auth';
import { runQueuedJobs } from '@/lib/apocrypha/vercel-runner';

export const config = { maxDuration: 300 };

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  res.setHeader('Cache-Control', 'no-store');
  if (req.method !== 'POST') { res.setHeader('Allow', 'POST'); res.status(405).json({ ok: false, code: 'METHOD_NOT_ALLOWED' }); return; }
  const cron = isCronAuthorized(req);
  if (!cron.ok) {
    const session = await getRequestUser(req);
    if (!session.user) { res.status(401).json({ ok: false, code: 'SESSION_REQUIRED' }); return; }
  }
  try {
    const outcome = await runQueuedJobs({ budgetMs: 240_000 });
    res.status(200).json({ ok: true, ...outcome });
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    console.error(JSON.stringify({ at: new Date().toISOString(), level: 'error', event: 'apocrypha.runner.failed', detail: detail.slice(0, 300) }));
    res.status(503).json({ ok: false, code: detail.split(':')[0] ?? 'RUNNER_ERROR' });
  }
}
