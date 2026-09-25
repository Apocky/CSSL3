// /api/cron/apocrypha-runner · every minute: answer any queued job the flagship runner missed.
import type { NextApiRequest, NextApiResponse } from 'next';
import { isCronAuthorized, isCronStubMode, reject401 } from '@/lib/cron-auth';
import { runQueuedJobs } from '@/lib/apocrypha/vercel-runner';
import { oidcFromRequest } from '@/lib/room/turn';

// Vercel hands each invocation its OIDC token in a header; the env var alone may be absent at runtime.
function runnerEnv(req: NextApiRequest): NodeJS.ProcessEnv {
  return { ...process.env, VERCEL_OIDC_TOKEN: process.env.VERCEL_OIDC_TOKEN || oidcFromRequest(req) };
}

export const config = { maxDuration: 60 };

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  res.setHeader('Cache-Control', 'no-store');
  if (isCronStubMode()) { res.status(200).json({ ok: true, stub: true }); return; }
  const auth = isCronAuthorized(req);
  if (!auth.ok) { reject401(res, auth.reason ?? 'unauthorized'); return; }
  try {
    res.status(200).json({ ok: true, ...(await runQueuedJobs({ env: runnerEnv(req), budgetMs: 50_000, maxJobs: 2 })) });
  } catch (error) {
    res.status(503).json({ ok: false, code: (error instanceof Error ? error.message : String(error)).split(':')[0] });
  }
}
