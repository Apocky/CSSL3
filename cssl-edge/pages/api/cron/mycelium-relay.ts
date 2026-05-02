// § T11-W14-K · /api/cron/mycelium-relay
// CADENCE : every 5 minutes
// PURPOSE : serve pull-requests from LOCAL mycelium-daemons (W14-J + W14-L) ·
//           cache hot-patterns · k-anon-aggregate ≥ 10 distinct sources only.
//
// Sovereignty :
//   - cross-user federation IS the substrate axiom · BUT k-anon ≥ 10 enforced
//   - patterns with < 10 distinct contributors marked private · not served
//   - never returns raw user-IDs · only aggregate cluster-signatures
//   - sigma_mask gate : only patterns with cap ≥ 2 (Aggregate-Relay) federated

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

interface MyceliumPattern {
  cluster_signature: string;
  pattern_kind: string;
  contributor_count: number;
  last_seen_at: string;
  cap_floor: number;
}

interface OkResp {
  ok: true;
  job: 'mycelium-relay';
  patterns_aggregated: number;
  patterns_served: number;
  k_anon_dropped: number;
  stub: boolean;
  hot_patterns: MyceliumPattern[]; // up to 50 · sorted by contributor_count DESC
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

const K_ANON_FLOOR = 10;
const MAX_PATTERNS_RETURNED = 50;

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<OkResp | ErrResp>
): Promise<void> {
  logHit('cron.mycelium-relay', { method: req.method ?? 'POST' });
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
      job: 'mycelium-relay',
      patterns_aggregated: 0,
      patterns_served: 0,
      k_anon_dropped: 0,
      stub: true,
      hot_patterns: [],
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
  let aggregated = 0;
  let served = 0;
  let dropped = 0;
  let hot: MyceliumPattern[] = [];
  let notes: string | null = null;

  if (sb === null) {
    notes = 'supabase-unconfigured-trace-only';
  } else {
    try {
      // Pull aggregated mycelium patterns from the aggregate-view.
      // Schema (see migration 0034) :
      //   mycelium_patterns_agg :
      //     cluster_signature (text · pk)
      //     pattern_kind      (text)
      //     contributor_count (int · COUNT DISTINCT cap_witness_hash)
      //     last_seen_at      (timestamptz)
      //     cap_floor         (smallint · MIN sigma_mask)
      const { data, error } = await sb
        .from('mycelium_patterns_agg')
        .select('*')
        .gte('cap_floor', 2) // require Aggregate-Relay or higher
        .order('contributor_count', { ascending: false })
        .limit(500);
      if (!error && Array.isArray(data)) {
        aggregated = data.length;
        const filtered = data.filter((p: MyceliumPattern) =>
          p.contributor_count >= K_ANON_FLOOR
        );
        dropped = aggregated - filtered.length;
        served = filtered.length;
        hot = filtered.slice(0, MAX_PATTERNS_RETURNED);
      } else if (error) {
        notes = `query-error:${error.code ?? 'unknown'}`;
      }
    } catch (e) {
      notes = e instanceof Error ? e.message.slice(0, 200) : 'exception';
    }
  }

  const { finished_at, duration_ms } = nowDurationMs(startMs);
  void emitCronAudit({
    job_name: 'mycelium-relay',
    started_at: new Date(startMs).toISOString(),
    finished_at,
    duration_ms,
    status: notes === null ? 'ok' : 'partial',
    rows_processed: served,
    retry_count: 0,
    via: auth.via,
    notes,
  });

  const env = envelope();
  res.status(200).json({
    ok: true,
    job: 'mycelium-relay',
    patterns_aggregated: aggregated,
    patterns_served: served,
    k_anon_dropped: dropped,
    stub: false,
    hot_patterns: hot,
    notes,
    served_by: env.served_by,
    ts: env.ts,
  });
}

// k-anon enforcement constant exported for tests.
export { K_ANON_FLOOR, MAX_PATTERNS_RETURNED };
