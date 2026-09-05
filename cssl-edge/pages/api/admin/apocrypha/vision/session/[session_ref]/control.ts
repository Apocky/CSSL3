import type { NextApiRequest, NextApiResponse } from 'next';

import { proxyV2ToApocrypha, setPrivateNoStore } from '@/lib/apocrypha/proxy';
import { isVisionControlEvent, isVisionSessionRef } from '@/lib/apocrypha/vision';
import { hasSameOrigin } from '@/lib/auth-session';
import { envelope } from '@/lib/response';

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  setPrivateNoStore(res);
  res.setHeader('Allow', 'POST');
  if (req.method !== 'POST') {
    res.status(405).json({ error: 'Method not allowed', ...envelope() });
    return;
  }
  if (!hasSameOrigin(req)) {
    res.status(403).json({ error: 'Same-origin request required', ...envelope() });
    return;
  }
  const event = (req.body as { event?: unknown } | undefined)?.event;
  const sessionRef = req.query.session_ref;
  if (!isVisionSessionRef(sessionRef) || !isVisionControlEvent(event)) {
    res.status(400).json({ error: 'session_ref and event are invalid', ...envelope() });
    return;
  }
  await proxyV2ToApocrypha(req, res, {
    method: 'POST',
    upstreamPath: `/v2/vision/session/${sessionRef}/control`,
    query: { event },
  });
}
