import type { NextApiRequest, NextApiResponse } from 'next';

import { externalJobSnapshot, readApocryphaJob } from '@/lib/apocrypha/job-control';
import { methodNotAllowed, noStore, requireChaosIdentity, respondJobError } from '@/lib/apocrypha/job-http';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  noStore(res);
  if (req.method !== 'GET') return methodNotAllowed(res, ['GET']);
  try {
    const identity = await requireChaosIdentity(req);
    const jobId = Array.isArray(req.query.id) ? req.query.id[0] : req.query.id;
    if (!jobId) return res.status(400).json({ ok: false, code: 'JOB_ID_REQUIRED' });
    const snapshot = await readApocryphaJob(jobId, identity);
    return snapshot ? res.status(200).json({ ok: true, ...externalJobSnapshot(snapshot) }) : res.status(404).json({ ok: false, code: 'JOB_NOT_FOUND' });
  } catch (error) {
    return respondJobError(res, error);
  }
}
