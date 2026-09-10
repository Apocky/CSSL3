import type { NextApiRequest, NextApiResponse } from 'next';

import { enqueueApocryphaJob, externalJobSnapshot, readApocryphaJob, type ApocryphaJobKind } from '@/lib/apocrypha/job-control';
import { methodNotAllowed, noStore, objectField, requireChaosIdentity, respondJobError, stringField } from '@/lib/apocrypha/job-http';

const ALLOWED_KINDS = new Set<ApocryphaJobKind>([
  'interpretation', 'followup', 'summary', 'astrology_natal', 'astrology_synastry', 'astrology_transit', 'continuation',
]);

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  noStore(res);
  if (req.method !== 'POST') return methodNotAllowed(res, ['POST']);
  try {
    const identity = await requireChaosIdentity(req);
    const body = objectField(req.body, 'body');
    const headerPrincipal = Array.isArray(req.headers['x-apocrypha-principal'])
      ? req.headers['x-apocrypha-principal'][0]
      : req.headers['x-apocrypha-principal'];
    if (body.principal_subject !== headerPrincipal) return res.status(400).json({ ok: false, code: 'PRINCIPAL_MISMATCH' });
    const kind = stringField(body.kind, 'kind', 64) as ApocryphaJobKind;
    if (!ALLOWED_KINDS.has(kind)) return res.status(400).json({ ok: false, code: 'INVALID_JOB_KIND' });
    const request = objectField(body.request, 'request');
    const encoded = JSON.stringify(request);
    if (encoded.length > 96_000) return res.status(413).json({ ok: false, code: 'REQUEST_TOO_LARGE' });
    const job = await enqueueApocryphaJob({
      identity,
      kind,
      capability: 'chaos_tarot_reading',
      request: { ...request, source: 'chaos-tarot.com', privacy_class: 'user-scoped' },
      idempotencyKey: stringField(body.idempotency_key, 'idempotency_key', 160),
      idempotencyScope: typeof body.idempotency_scope === 'string' ? body.idempotency_scope : 'reading',
      priority: 10,
    });
    const snapshot = await readApocryphaJob(job.id, identity);
    if (!snapshot) throw new Error('JOB_ENQUEUE_EMPTY');
    return res.status(202).json({ ok: true, accepted: true, ...externalJobSnapshot(snapshot) });
  } catch (error) {
    return respondJobError(res, error);
  }
}
