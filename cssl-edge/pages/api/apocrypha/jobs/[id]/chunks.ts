import type { NextApiRequest, NextApiResponse } from 'next';

import { readExternalJobChunksPage } from '@/lib/apocrypha/job-control';
import { methodNotAllowed, noStore, requireChaosIdentity, respondJobError } from '@/lib/apocrypha/job-http';

function afterCursor(req: NextApiRequest): number {
  const value = Array.isArray(req.query.after) ? req.query.after[0] : req.query.after;
  const parsed = Number(value ?? 0);
  return Number.isSafeInteger(parsed) && parsed >= 0 ? parsed : 0;
}

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  noStore(res);
  if (req.method !== 'GET') return methodNotAllowed(res, ['GET']);
  try {
    const identity = await requireChaosIdentity(req);
    const jobId = Array.isArray(req.query.id) ? req.query.id[0] : req.query.id;
    if (!jobId) return res.status(400).json({ ok: false, code: 'JOB_ID_REQUIRED' });
    const page = await readExternalJobChunksPage(jobId, identity, afterCursor(req));
    return page
      ? res.status(200).json({ ok: true, ...page })
      : res.status(404).json({ ok: false, code: 'JOB_NOT_FOUND' });
  } catch (error) {
    return respondJobError(res, error);
  }
}
