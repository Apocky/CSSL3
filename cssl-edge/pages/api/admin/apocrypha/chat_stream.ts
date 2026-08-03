// Retired compatibility surface.
//
// V2 currently returns one final JSON expression. Converting that one payload
// into an SSE event would only make a REST response look streamed, so this
// former route fails explicitly and points clients at the truthful REST route.

import type { NextApiRequest, NextApiResponse } from 'next';

import { requireApocryphaOwner, setPrivateNoStore } from '@/lib/apocrypha/proxy';
import { envelope } from '@/lib/response';

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  setPrivateNoStore(res);
  res.setHeader('Allow', 'POST');
  if (req.method !== 'POST') {
    res.status(405).json({ error: 'Method not allowed', ...envelope() });
    return;
  }
  if (!(await requireApocryphaOwner(req, res))) return;
  res.status(410).json({
    error: 'Synthetic streaming is retired. Use the one-final V2 REST turn route.',
    replacement: '/api/admin/apocrypha/chat',
    response_mode: 'one_final_json',
    ...envelope(),
  });
}
