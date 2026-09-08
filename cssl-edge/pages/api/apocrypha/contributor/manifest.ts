import type { NextApiRequest, NextApiResponse } from 'next';
import { envelope, logHit } from '@/lib/response';
import {
  CONTRIBUTOR_NODE_MANIFEST,
  parseContributorNodeManifest,
  type ContributorNodeManifest,
} from '@/lib/apocrypha/contributor-node';

interface ManifestResponse {
  readonly ok: true;
  readonly manifest: ContributorNodeManifest;
  readonly served_by: string;
  readonly ts: string;
}
interface ErrorResponse {
  readonly ok: false;
  readonly error: 'GET only' | 'manifest_invalid';
  readonly served_by: string;
  readonly ts: string;
}

export default function handler(
  req: NextApiRequest,
  res: NextApiResponse<ManifestResponse | ErrorResponse>,
): void {
  logHit('apocrypha.contributor.manifest', { method: req.method ?? 'unknown' });
  res.setHeader('cache-control', 'no-store');
  res.setHeader('x-content-type-options', 'nosniff');

  if (req.method !== 'GET') {
    res.setHeader('allow', 'GET');
    res.status(405).json({ ok: false, error: 'GET only', ...envelope() });
    return;
  }

  const manifest = parseContributorNodeManifest(CONTRIBUTOR_NODE_MANIFEST);
  if (!manifest) {
    res.status(503).json({ ok: false, error: 'manifest_invalid', ...envelope() });
    return;
  }
  res.status(200).json({ ok: true, manifest, ...envelope() });
}
