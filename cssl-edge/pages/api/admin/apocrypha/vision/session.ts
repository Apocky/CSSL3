import type { NextApiRequest, NextApiResponse } from 'next';

import {
  fetchApocryphaV2,
  requireApocryphaOwner,
  setPrivateNoStore,
} from '@/lib/apocrypha/proxy';
import {
  isVisionConsentId,
  isVisionSessionRef,
  VISION_PURPOSE,
  VISION_SOURCE_REF,
  visionPayloadIsMetadataOnly,
} from '@/lib/apocrypha/vision';
import { hasSameOrigin } from '@/lib/auth-session';
import { envelope } from '@/lib/response';

const MAX_PURPOSE_LENGTH = 256;

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

  const body = (req.body ?? {}) as Record<string, unknown>;
  const sessionRef = body.session_ref;
  const consentId = body.consent_id;
  const purpose = typeof body.purpose === 'string' && body.purpose.trim().length <= MAX_PURPOSE_LENGTH
    ? body.purpose.trim()
    : VISION_PURPOSE;
  const durationSeconds = typeof body.duration_seconds === 'number'
    && Number.isInteger(body.duration_seconds)
    ? Math.max(1, Math.min(3600, body.duration_seconds))
    : 300;
  const maxFps = typeof body.max_fps === 'number' && Number.isInteger(body.max_fps)
    ? Math.max(1, Math.min(30, body.max_fps))
    : 5;
  if (!isVisionSessionRef(sessionRef) || !isVisionConsentId(consentId)) {
    res.status(400).json({ error: 'session_ref and consent_id must be opaque UUIDv4 values', ...envelope() });
    return;
  }

  const upstream = await fetchApocryphaV2({
    method: 'POST',
    upstreamPath: '/v2/vision/session',
    deadlineMs: 25_000,
    body: {
      session_ref: sessionRef,
      principal_ref: owner.principalRef,
      source_ref: VISION_SOURCE_REF,
      consent_ref: `consent:authenticated-vision:${owner.principalRef}:${consentId}`,
      purpose,
      privacy_class: 'restricted',
      duration_seconds: durationSeconds,
      max_fps: maxFps,
    },
  });
  if (!upstream.ok || !upstream.payload || !visionPayloadIsMetadataOnly(upstream.payload)) {
    res.status(upstream.status || 502).json({
      error: 'Vision session could not be established without raw-frame exposure.',
      upstream_status: upstream.status,
      ...envelope(),
    });
    return;
  }
  res.status(upstream.status).json({ upstream_status: upstream.status, data: upstream.payload, ...envelope() });
}
