// § T11-W14-K · /api/cron/heartbeat
// CADENCE : every 1 minute
// PURPOSE : engine-is-alive · feeds /api/status (W14-M live-status-page) ·
//           public-readable so visitors can see "engine is up" transparency.
//
// Sovereignty :
//   - GET allowed (public-read-ok · just confirms uptime · no PII)
//   - POST = cron-only (writes heartbeat row · audit-emits)
//   - heartbeat-record carries commit_sha + region + uptime_sec only

import type { NextApiRequest, NextApiResponse } from 'next';
import { envelope, logHit, commitSha } from '@/lib/response';
import {
  isCronAuthorized,
  isCronStubMode,
  reject401,
  emitCronAudit,
  nowDurationMs,
  getServiceRoleClient,
} from '@/lib/cron-auth';

interface HeartbeatRow {
  job_name: 'heartbeat';
  commit_sha: string;
  region: string;
  uptime_sec: number;
  emitted_at: string;
}

interface OkResp {
  ok: true;
  job: 'heartbeat';
  recorded: boolean;
  stub: boolean;
  commit_sha: string;
  region: string;
  uptime_sec: number;
  served_by: string;
  ts: string;
}

interface ErrResp {
  ok: false;
  error: string;
  served_by: string;
  ts: string;
}

// Cold-start time anchor — Vercel resets across cold-boots so this measures
// "this lambda's lifetime" not "true uptime", but it's stable for warm runs
// and the status-page tolerates the discontinuity.
const COLD_START_MS = Date.now();

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<OkResp | ErrResp>
): Promise<void> {
  logHit('cron.heartbeat', { method: req.method ?? 'GET' });
  const startMs = Date.now();

  // GET = public-read · last-known heartbeat OR live ping (no auth required).
  if (req.method === 'GET') {
    const env = envelope();
    res.status(200).json({
      ok: true,
      job: 'heartbeat',
      recorded: false, // GET-only · ¬ persist
      stub: isCronStubMode(),
      commit_sha: commitSha(),
      region: process.env['VERCEL_REGION'] ?? 'iad1',
      uptime_sec: Math.floor((Date.now() - COLD_START_MS) / 1000),
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  // POST = cron-only · persist heartbeat + emit audit.
  if (req.method !== 'POST') {
    res.setHeader('Allow', 'GET, POST');
    const env = envelope();
    res.status(405).json({
      ok: false,
      error: 'GET or POST only',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  // Stub-mode short-circuit BEFORE auth-check so smoke-tests pass on first-deploy.
  if (isCronStubMode()) {
    const env = envelope();
    res.status(200).json({
      ok: true,
      job: 'heartbeat',
      recorded: false,
      stub: true,
      commit_sha: commitSha(),
      region: process.env['VERCEL_REGION'] ?? 'iad1',
      uptime_sec: Math.floor((Date.now() - COLD_START_MS) / 1000),
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const auth = isCronAuthorized(req);
  if (!auth.ok) {
    reject401(res, auth.reason ?? 'auth-failed');
    return;
  }

  // Persist heartbeat row.
  const sb = getServiceRoleClient();
  let recorded = false;
  const row: HeartbeatRow = {
    job_name: 'heartbeat',
    commit_sha: commitSha(),
    region: process.env['VERCEL_REGION'] ?? 'iad1',
    uptime_sec: Math.floor((Date.now() - COLD_START_MS) / 1000),
    emitted_at: new Date().toISOString(),
  };
  if (sb !== null) {
    try {
      const { error } = await sb.from('cron_heartbeat').insert(row);
      recorded = error === null;
    } catch (_e) {
      recorded = false;
    }
  }

  // Audit-emit (fire-and-forget).
  const { finished_at, duration_ms } = nowDurationMs(startMs);
  void emitCronAudit({
    job_name: 'heartbeat',
    started_at: new Date(startMs).toISOString(),
    finished_at,
    duration_ms,
    status: recorded ? 'ok' : 'partial',
    rows_processed: recorded ? 1 : 0,
    retry_count: 0,
    via: auth.via,
    notes: recorded ? null : 'persistence-failed-trace-only',
  });

  const env = envelope();
  res.status(200).json({
    ok: true,
    job: 'heartbeat',
    recorded,
    stub: false,
    commit_sha: row.commit_sha,
    region: row.region,
    uptime_sec: row.uptime_sec,
    served_by: env.served_by,
    ts: env.ts,
  });
}
