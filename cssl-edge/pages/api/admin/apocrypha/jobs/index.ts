import { randomUUID } from 'node:crypto';

import type { NextApiRequest, NextApiResponse } from 'next';

import { enqueueApocryphaJob, type ApocryphaJobKind } from '@/lib/apocrypha/job-control';
import { methodNotAllowed, noStore, objectField, requireOwnerIdentity, respondJobError, stringField } from '@/lib/apocrypha/job-http';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  noStore(res);
  if (req.method !== 'POST') return methodNotAllowed(res, ['POST']);
  try {
    const identity = await requireOwnerIdentity(req, res);
    if (!identity) return;
    const body = objectField(req.body, 'body');
    const prompt = stringField(body.prompt, 'prompt', 32_000);
    const idempotencyKey = typeof body.idempotency_key === 'string' && body.idempotency_key.trim()
      ? stringField(body.idempotency_key, 'idempotency_key', 160)
      : randomUUID();
    const request = {
      prompt,
      conversation_id: typeof body.conversation_id === 'string' ? body.conversation_id : null,
      output_budget: Math.min(4096, Math.max(128, Number(body.output_budget) || 1536)),
      response_mode: body.response_mode === 'deep' ? 'deep' : 'standard',
      source: 'apocky.com',
      privacy_class: 'restricted',
      memory_scope: 'owner-authorized',
    };
    const job = await enqueueApocryphaJob({
      identity,
      kind: 'apocky_chat' as ApocryphaJobKind,
      capability: 'apocky_owner_chat',
      request,
      idempotencyKey,
      idempotencyScope: typeof body.conversation_id === 'string' ? body.conversation_id : 'new-conversation',
      priority: 20,
    });
    return res.status(202).json({ ok: true, accepted: true, job });
  } catch (error) {
    return respondJobError(res, error);
  }
}
