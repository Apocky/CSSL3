import type { NextApiRequest, NextApiResponse } from 'next';

import {
  fetchApocryphaV2,
  requireApocryphaOwner,
  setPrivateNoStore,
} from '@/lib/apocrypha/proxy';
import { isVisionSessionRef, visionPayloadIsMetadataOnly } from '@/lib/apocrypha/vision';
import { hasSameOrigin } from '@/lib/auth-session';
import { envelope } from '@/lib/response';

const MAX_FRAME_B64 = 5_600_000;
const MEDIA_TYPES = new Set(['image/jpeg', 'image/png', 'image/webp']);

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
  const owner = await requireApocryphaOwner(req, res);
  if (!owner) return;
  const sessionRef = req.query.session_ref;
  const body = (req.body ?? {}) as Record<string, unknown>;
  const contentB64 = body.content_b64;
  const sequence = body.sequence;
  const capturedAt = body.captured_at_unix_ns;
  const recordedAt = body.recorded_at_unix_ns;
  const mediaType = body.media_type;
  if (!isVisionSessionRef(sessionRef)
    || typeof contentB64 !== 'string'
    || contentB64.length < 1
    || contentB64.length > MAX_FRAME_B64
    || !Number.isSafeInteger(sequence)
    || (sequence as number) < 0
    || !Number.isSafeInteger(capturedAt)
    || !Number.isSafeInteger(recordedAt)
    || typeof mediaType !== 'string'
    || !MEDIA_TYPES.has(mediaType)) {
    res.status(400).json({ error: 'Vision frame is malformed or exceeds its bounded media contract', ...envelope() });
    return;
  }
  const upstream = await fetchApocryphaV2({
    method: 'POST',
    upstreamPath: `/v2/vision/session/${sessionRef}/frame`,
    deadlineMs: 25_000,
    body,
  });
  if (!upstream.ok || !upstream.payload || !visionPayloadIsMetadataOnly(upstream.payload)) {
    res.status(upstream.status || 502).json({
      error: 'Vision projection unavailable or contained forbidden raw-frame material.',
      upstream_status: upstream.status,
      ...envelope(),
    });
    return;
  }
  res.status(upstream.status).json({ upstream_status: upstream.status, data: upstream.payload, ...envelope() });
}
