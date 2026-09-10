import type { NextApiRequest, NextApiResponse } from 'next';

import { cancelApocryphaJob, readApocryphaJob } from '@/lib/apocrypha/job-control';
import { methodNotAllowed, noStore, requireOwnerIdentity, respondJobError } from '@/lib/apocrypha/job-http';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  noStore(res);
  if (req.method !== 'GET' && req.method !== 'DELETE') return methodNotAllowed(res, ['GET', 'DELETE']);
  try {
    const identity = await requireOwnerIdentity(req, res);
    if (!identity) return;
    const jobId = Array.isArray(req.query.id) ? req.query.id[0] : req.query.id;
    if (!jobId) return res.status(400).json({ ok: false, code: 'JOB_ID_REQUIRED' });
    if (req.method === 'DELETE') {
      const job = await cancelApocryphaJob(jobId, identity, 'owner_cancelled');
      return job ? res.status(202).json({ ok: true, job }) : res.status(404).json({ ok: false, code: 'JOB_NOT_FOUND' });
    }
    const snapshot = await readApocryphaJob(jobId, identity);
    return snapshot ? res.status(200).json({ ok: true, ...snapshot }) : res.status(404).json({ ok: false, code: 'JOB_NOT_FOUND' });
  } catch (error) {
    return respondJobError(res, error);
  }
}
