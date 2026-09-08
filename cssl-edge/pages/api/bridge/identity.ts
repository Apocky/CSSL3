import type { NextApiRequest, NextApiResponse } from 'next';
import { requireBrainOwner, respondBrainOwnerFailure, setBrainPrivateHeaders } from '@/lib/brain/owner';
export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  setBrainPrivateHeaders(res); res.setHeader('Allow', 'GET');
  if (req.method !== 'GET') { res.status(405).json({ code: 'BRIDGE_METHOD_NOT_ALLOWED' }); return; }
  const owner = await requireBrainOwner(req);
  if (!owner.ok) { respondBrainOwnerFailure(res, owner); return; }
  if (Object.keys(req.query).length) { res.status(400).json({ code: 'BRIDGE_REQUEST_INVALID' }); return; }
  res.status(200).json({ schema_version: 'apocky.bridge.identity.v1', subject: owner.user.id });
}
