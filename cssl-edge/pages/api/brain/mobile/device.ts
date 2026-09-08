import type { NextApiRequest, NextApiResponse } from 'next';

import { hasSameOrigin } from '@/lib/auth-session';
import {
  requireBrainOwner,
  respondBrainOwnerFailure,
  setBrainPrivateHeaders,
} from '@/lib/brain/owner';
import {
  issueMiniBrainDeviceCapability,
  MiniBrainRelayError,
} from '@/lib/brain/mobile-relay';
import { envelope } from '@/lib/response';

function exactBody(value: unknown): value is { device_id: string; public_key_jwk: JsonWebKey } {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const row = value as Record<string, unknown>;
  return Object.keys(row).sort().join(',') === 'device_id,public_key_jwk'
    && typeof row.device_id === 'string'
    && Boolean(row.public_key_jwk);
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  setBrainPrivateHeaders(res);
  res.setHeader('Allow', 'POST');
  if (req.method !== 'POST') {
    res.status(405).json({ error: 'Method not allowed', code: 'BRAIN_METHOD_NOT_ALLOWED', ...envelope() });
    return;
  }
  if (!hasSameOrigin(req)) {
    res.status(403).json({ error: 'Same-origin request required', code: 'BRAIN_ORIGIN_DENIED', ...envelope() });
    return;
  }
  const contentType = (Array.isArray(req.headers['content-type']) ? req.headers['content-type'][0] : req.headers['content-type'])
    ?.split(';', 1)[0]
    ?.trim()
    .toLowerCase();
  if (contentType !== 'application/json') {
    res.status(415).json({ error: 'Content-Type must be application/json', code: 'BRAIN_CONTENT_TYPE_REQUIRED', ...envelope() });
    return;
  }
  const owner = await requireBrainOwner(req);
  if (!owner.ok) {
    respondBrainOwnerFailure(res, owner);
    return;
  }
  if (!exactBody(req.body)) {
    res.status(400).json({ error: 'Device registration body is invalid.', code: 'BRAIN_DEVICE_REGISTRATION_INVALID', ...envelope() });
    return;
  }
  try {
    const capability = issueMiniBrainDeviceCapability({
      userId: owner.user.id,
      deviceId: req.body.device_id,
      publicKeyJwk: req.body.public_key_jwk,
    });
    res.status(200).json({
      schema_version: 'apocky.mini-brain.device-registration.v1',
      status: 'bound',
      ...capability,
      controls: {
        owner_session: 'verified',
        private_key: 'non_exportable_browser_key',
        token: 'server_signed_owner_and_public_key_binding',
      },
      ...envelope(),
    });
  } catch (error) {
    const code = error instanceof MiniBrainRelayError ? error.code : 'BRAIN_DEVICE_REGISTRATION_FAILED';
    const status = error instanceof MiniBrainRelayError ? error.publicStatus : 503;
    res.status(status).json({
      error: 'This browser could not be bound to the owner session. No private data was returned.',
      code,
      ...envelope(),
    });
  }
}
