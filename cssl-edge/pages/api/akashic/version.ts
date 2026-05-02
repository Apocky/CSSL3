// § Akashic-Webpage-Records · /api/akashic/version
// GET · returns server-side dpl_id + commit_sha + build_time. Client polls
// every 60s · diff with bundle-baked dpl_id ⇒ deploy.detected canary.
//
// This is the canary for stuck-deploy detection (Vercel-stuck-deploy issue).
// Exotic-but-simple : zero state · zero db · just env-var reflection.

import type { NextApiRequest, NextApiResponse } from 'next';
import { envelope, logHit, commitSha } from '@/lib/response';

interface VersionResp {
  served_by: string;
  ts: string;
  dpl_id: string;
  commit_sha: string;
  build_time: string;
  vercel_env: string;
  vercel_url: string;
}

export default function handler(req: NextApiRequest, res: NextApiResponse<VersionResp>): void {
  logHit('akashic.version', { method: req.method ?? 'GET' });
  const env = envelope();
  // Vercel injects these at build/runtime ; fall back to local-dev sentinels.
  const dpl_id =
    process.env['VERCEL_DEPLOYMENT_ID'] ??
    process.env['NEXT_PUBLIC_VERCEL_DEPLOYMENT_ID'] ??
    'local-dev';
  const sha = commitSha();
  const build_time =
    process.env['BUILD_TIME'] ??
    process.env['NEXT_PUBLIC_BUILD_TIME'] ??
    'unknown';
  const vercel_env = process.env['VERCEL_ENV'] ?? 'local';
  const vercel_url = process.env['VERCEL_URL'] ?? 'localhost';

  // Cache-control : short s-maxage so stuck-deploy detection has fresh data,
  // but enough to absorb spikes from /api/akashic/version polling.
  res.setHeader('Cache-Control', 's-maxage=10, stale-while-revalidate=20');
  res.status(200).json({
    served_by: env.served_by,
    ts: env.ts,
    dpl_id,
    commit_sha: sha,
    build_time,
    vercel_env,
    vercel_url,
  });
}
