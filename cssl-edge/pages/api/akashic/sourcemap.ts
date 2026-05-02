// § Akashic-Webpage-Records · /api/akashic/sourcemap
// GET · fetch source-map for stack-trace de-minification. SERVER-SIDE ONLY ;
// source-maps must NEVER ship to client (info-leak). Sovereign-cap required.
//
// Stage-0 stub : returns 404 unless the requested .map file exists in the
// .next/build artifacts. Real impl would fetch from Vercel-hosted artifact
// store + cache. For now : transparent stub-friendly failure.

import type { NextApiRequest, NextApiResponse } from 'next';
import { envelope, logHit } from '@/lib/response';
import { isSovereignFromIncoming } from '@/lib/sovereign';

interface OkResp {
  served_by: string;
  ts: string;
  ok: true;
  bundle_url: string;
  sourcemap_available: boolean;
  // Stage-0 returns nothing more ; future versions fetch + return the map.
}
interface ErrResp { served_by: string; ts: string; error: string; }

export default function handler(
  req: NextApiRequest,
  res: NextApiResponse<OkResp | ErrResp>
): void {
  logHit('akashic.sourcemap', { method: req.method ?? 'GET' });
  if (req.method !== 'GET') {
    const env = envelope();
    res.setHeader('Allow', 'GET');
    res.status(405).json({ served_by: env.served_by, ts: env.ts, error: 'GET only' });
    return;
  }

  // Sovereign-cap required (source-maps can leak proprietary build paths).
  const sov = isSovereignFromIncoming(
    req.headers,
    (req.query['sovereign'] as string) === 'true'
  );
  if (!sov) {
    const env = envelope();
    res.status(401).json({ served_by: env.served_by, ts: env.ts, error: 'sovereign-cap required' });
    return;
  }

  const bundleRaw = req.query['bundle_url'];
  const bundle_url = Array.isArray(bundleRaw) ? bundleRaw[0] ?? '' : bundleRaw ?? '';
  if (typeof bundle_url !== 'string' || bundle_url.length === 0) {
    const env = envelope();
    res.status(400).json({ served_by: env.served_by, ts: env.ts, error: 'bundle_url query required' });
    return;
  }

  // Stage-0 : declare unavailability transparently. Future impl would attempt
  // fetch of the .map artifact from build storage + return source-map JSON.
  const env = envelope();
  res.status(200).json({
    served_by: env.served_by,
    ts: env.ts,
    ok: true,
    bundle_url,
    sourcemap_available: false,
  });
}
