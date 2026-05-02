// § T11-W14-K · /api/cron/kan-rollup
// CADENCE : every 1 hour
// PURPOSE : promote analytics_rollup_1min → 1hr → 1day rollups. Calls the
//           Postgres helper rollup_promote_minutes() which already exists in
//           migration 0024_analytics.sql. Idempotent · k-anon-preserved.
//
// Sovereignty :
//   - aggregates honor sigma_consent_cap ≥ 2 (AggregateRelay or Full)
//   - rows with cap < 2 stay in the 1min bucket · auto-dropped by 0024 RLS
//   - cron NEVER reads per-player · only triggers the promote helper

import type { NextApiRequest, NextApiResponse } from 'next';
import { envelope, logHit } from '@/lib/response';
import {
  isCronAuthorized,
  isCronStubMode,
  reject401,
  emitCronAudit,
  nowDurationMs,
  getServiceRoleClient,
} from '@/lib/cron-auth';

interface OkResp {
  ok: true;
  job: 'kan-rollup';
  promoted_1hr: number;
  promoted_1day: number;
  stub: boolean;
  notes: string | null;
  served_by: string;
  ts: string;
}

interface ErrResp {
  ok: false;
  error: string;
  served_by: string;
  ts: string;
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<OkResp | ErrResp>
): Promise<void> {
  logHit('cron.kan-rollup', { method: req.method ?? 'POST' });
  const startMs = Date.now();

  if (req.method !== 'POST' && req.method !== 'GET') {
    res.setHeader('Allow', 'POST, GET');
    const env = envelope();
    res.status(405).json({
      ok: false,
      error: 'POST or GET only',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  if (isCronStubMode()) {
    const env = envelope();
    res.status(200).json({
      ok: true,
      job: 'kan-rollup',
      promoted_1hr: 0,
      promoted_1day: 0,
      stub: true,
      notes: 'stub-mode · no CRON_SECRET',
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

  const sb = getServiceRoleClient();
  let promoted_1hr = 0;
  let promoted_1day = 0;
  let notes: string | null = null;

  if (sb === null) {
    notes = 'supabase-unconfigured-trace-only';
  } else {
    try {
      // Call the existing rollup_promote_minutes() helper (0024_analytics.sql).
      // Returns a row { promoted_to_1hr, promoted_to_1day } — those are the
      // counts of buckets-promoted, not events.
      const { data, error } = await sb.rpc('rollup_promote_minutes');
      if (!error && data) {
        const row = Array.isArray(data) ? data[0] : data;
        promoted_1hr = Number(row?.promoted_to_1hr ?? 0);
        promoted_1day = Number(row?.promoted_to_1day ?? 0);
      } else if (error) {
        notes = `rpc-error:${error.code ?? 'unknown'}`;
      }
    } catch (e) {
      notes = e instanceof Error ? e.message.slice(0, 200) : 'exception';
    }
  }

  const { finished_at, duration_ms } = nowDurationMs(startMs);
  void emitCronAudit({
    job_name: 'kan-rollup',
    started_at: new Date(startMs).toISOString(),
    finished_at,
    duration_ms,
    status: notes === null ? 'ok' : 'partial',
    rows_processed: promoted_1hr + promoted_1day,
    retry_count: 0,
    via: auth.via,
    notes,
  });

  const env = envelope();
  res.status(200).json({
    ok: true,
    job: 'kan-rollup',
    promoted_1hr,
    promoted_1day,
    stub: false,
    notes,
    served_by: env.served_by,
    ts: env.ts,
  });
}
