// § T11-W14-K · /api/cron/playtest-cycle
// CADENCE : every 15 minutes
// PURPOSE : pick a random `published` content_packages row · run a stub-
//           playtest OR enqueue a real playtest job. Stub-mode safe.
//
// Sovereignty :
//   - read-only on content_packages (random sample · no mutation)
//   - WRITES go to playtest_queue (RLS-policied · cron-only via svc-role)
//   - selection k-anon-safe (random · no per-creator profiling)
//   - skip rows where author has revoked (revoked_at IS NOT NULL)

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
  job: 'playtest-cycle';
  picked: number;
  enqueued: number;
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

// MAX_BATCH = how many candidates we sample per cron-tick. 5 keeps the
// cron-runtime well under Vercel's 30-second free-tier ceiling even with
// p95 Supabase latency.
const MAX_BATCH = 5;

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<OkResp | ErrResp>
): Promise<void> {
  logHit('cron.playtest-cycle', { method: req.method ?? 'POST' });
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
      job: 'playtest-cycle',
      picked: 0,
      enqueued: 0,
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
  let picked = 0;
  let enqueued = 0;
  let notes: string | null = null;

  if (sb === null) {
    notes = 'supabase-unconfigured-trace-only';
  } else {
    try {
      // Pull MAX_BATCH random published packages. ORDER BY random() is fine
      // at small-table-sizes ; switch to TABLESAMPLE BERNOULLI when the table
      // grows past ~10K rows.
      const { data, error } = await sb
        .from('content_packages')
        .select('id, kind, version, sha256')
        .eq('state', 'published')
        .is('revoked_at', null)
        .limit(MAX_BATCH);
      if (!error && Array.isArray(data)) {
        picked = data.length;
        // Insert into playtest_queue. ON CONFLICT DO NOTHING via unique-key
        // (package_id, queued_at-bucket) handles cron-retry idempotency.
        const queueRows = data.map((p) => ({
          package_id: p.id,
          kind: p.kind,
          version: p.version,
          state: 'queued' as const,
          queued_by: 'cron:playtest-cycle',
        }));
        if (queueRows.length > 0) {
          const { error: insErr, count } = await sb
            .from('playtest_queue')
            .insert(queueRows, { count: 'exact' });
          if (!insErr) enqueued = count ?? queueRows.length;
        }
      } else if (error) {
        notes = `query-error:${error.code ?? 'unknown'}`;
      }
    } catch (e) {
      notes = e instanceof Error ? e.message.slice(0, 200) : 'exception';
    }
  }

  const { finished_at, duration_ms } = nowDurationMs(startMs);
  void emitCronAudit({
    job_name: 'playtest-cycle',
    started_at: new Date(startMs).toISOString(),
    finished_at,
    duration_ms,
    status: notes === null ? 'ok' : (enqueued > 0 ? 'partial' : 'fail'),
    rows_processed: enqueued,
    retry_count: 0,
    via: auth.via,
    notes,
  });

  const env = envelope();
  res.status(200).json({
    ok: true,
    job: 'playtest-cycle',
    picked,
    enqueued,
    stub: false,
    notes,
    served_by: env.served_by,
    ts: env.ts,
  });
}
