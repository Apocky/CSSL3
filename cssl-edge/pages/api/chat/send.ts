import type { NextApiRequest, NextApiResponse } from 'next';

import { getRequestUser } from '../../../lib/admin-auth';
import { getServiceClient, checkAndBumpRate, ensureSession, enqueueTurn } from '../../../lib/chat-relay';

// POST /api/chat/send — enqueue a turn for the caller's instanced sub-mind.
// Login-gated (any signed-in user) + rate-limited. The local bridge processes the queue
// out-of-band and streams the answer back via chat_chunk (the browser tails it over Realtime).
export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  if (req.method !== 'POST') {
    res.setHeader('Allow', 'POST');
    res.status(405).json({ ok: false, error: 'method_not_allowed' });
    return;
  }

  const session = await getRequestUser(req);
  if (!session.user) {
    res.status(401).json({ ok: false, error: 'unauthorized', reason: session.reason ?? 'Sign in to chat.' });
    return;
  }

  const body: Record<string, unknown> =
    req.body && typeof req.body === 'object' ? (req.body as Record<string, unknown>) : {};
  const prompt = typeof body['prompt'] === 'string' ? body['prompt'].trim() : '';
  const sessionId = typeof body['session_id'] === 'string' ? body['session_id'] : undefined;
  if (prompt.length < 1 || prompt.length > 8192) {
    res.status(400).json({ ok: false, error: 'bad_prompt' });
    return;
  }

  const sb = getServiceClient();
  if (!sb) {
    res.status(503).json({ ok: false, error: 'relay_unconfigured' });
    return;
  }

  const rate = await checkAndBumpRate(sb, session.user.id);
  if (!rate.ok) {
    res.status(429).json({ ok: false, error: 'rate_limited' });
    return;
  }

  const sid = await ensureSession(sb, session.user.id, sessionId);
  if (!sid) {
    res.status(500).json({ ok: false, error: 'session_failed' });
    return;
  }
  const turnId = await enqueueTurn(sb, session.user.id, sid, prompt);
  if (!turnId) {
    res.status(500).json({ ok: false, error: 'enqueue_failed' });
    return;
  }

  res.status(200).json({ ok: true, turn_id: turnId, session_id: sid, remaining: rate.remaining });
}
