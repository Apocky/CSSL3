// Owner-only, metadata-only V2 telemetry projection.

import type { NextApiRequest, NextApiResponse } from 'next';

import { proxyV2ToApocrypha, setPrivateNoStore } from '@/lib/apocrypha/proxy';
import { envelope } from '@/lib/response';

const MAX_EVENT_SEQ = Number.MAX_SAFE_INTEGER;

function parseAfterEventSeq(value: string | string[] | undefined): number | null {
  if (value === undefined) return 0;
  if (typeof value !== 'string' || !/^(?:0|[1-9][0-9]{0,15})$/.test(value)) return null;
  const cursor = Number(value);
  return Number.isSafeInteger(cursor) && cursor >= 0 && cursor <= MAX_EVENT_SEQ ? cursor : null;
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  setPrivateNoStore(res);
  res.setHeader('Allow', 'GET');
  if (req.method !== 'GET') {
    res.status(405).json({ error: 'Method not allowed', ...envelope() });
    return;
  }
  const rawLimit = typeof req.query.limit === 'string' ? Number(req.query.limit) : 100;
  const limit = Number.isInteger(rawLimit) ? Math.max(1, Math.min(500, rawLimit)) : 100;
  const afterEventSeq = parseAfterEventSeq(req.query.after_event_seq);
  if (afterEventSeq === null) {
    res.status(400).json({
      error: `after_event_seq must be a decimal integer between 0 and ${MAX_EVENT_SEQ}`,
      ...envelope(),
    });
    return;
  }
  await proxyV2ToApocrypha(req, res, {
    upstreamPath: '/v2/telemetry',
    query: { after_event_seq: afterEventSeq, limit },
  });
}
