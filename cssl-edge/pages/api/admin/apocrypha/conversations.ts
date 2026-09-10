// Native V2 history boundary.
//
// The current body has conversation continuity references but no governed
// owner-facing history projection. Never fill that gap with the predecessor's
// conversation database.

import type { NextApiRequest, NextApiResponse } from 'next';

import { requireApocryphaOwner, setPrivateNoStore } from '@/lib/apocrypha/proxy';
import { envelope } from '@/lib/response';

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  setPrivateNoStore(res);
  res.setHeader('Allow', 'GET');
  if (req.method !== 'GET') {
    res.status(405).json({ error: 'Method not allowed', ...envelope() });
    return;
  }
  if (!(await requireApocryphaOwner(req, res))) return;
  res.status(200).json({
    available: false,
    reason_code: 'native_v2_history_projection_absent',
    conversations: [],
    ...envelope(),
  });
}
