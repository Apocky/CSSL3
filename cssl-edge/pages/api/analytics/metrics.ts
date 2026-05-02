// cssl-edge · /api/analytics/metrics
// ════════════════════════════════════════════════════════════════════════
// § T11-W11-ANALYTICS · GET handler returning bucketed-rollup aggregates.
//
// § Wire-format · GET /api/analytics/metrics?bucket=1min|1hr|1day
//   Returns JSON :
//     {
//       bucket: "1min",
//       attestation: { no_pii: true, ... },
//       kinds: [{ name, count, avg, min, max, fallback, err }, ...],
//       served_by: "...",
//       ts: "..."
//     }
//
// § Σ-mask discipline · only rows where sigma_consent_cap >= AggregateRelay
//   (= 2) are surfaced unless the caller is the row's own player. RLS
//   enforces this at the Supabase layer ; we re-filter here defensively.
//
// § Stub-mode-aware : if Supabase env-vars missing, return synthetic zero-
//   filled buckets so client smoke-tests pass.

import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, stubEnvelope, envelope } from '@/lib/response';
import {
  parseBucketTier,
  rollupTableForTier,
  EVENT_KIND_NAMES,
  sigmaMaskAttestation,
} from '@/lib/analytics';
import { getSupabase } from '@/lib/supabase';

interface KindRow {
  name: string;
  count: number;
  avg: number;
  min: number;
  max: number;
  fallback: number;
  err: number;
}

interface MetricsOk {
  bucket: '1min' | '1hr' | '1day';
  attestation: ReturnType<typeof sigmaMaskAttestation>;
  kinds: KindRow[];
  served_by: string;
  ts: string;
  stub?: true;
  todo?: string;
}

interface MetricsErr {
  error: string;
  served_by: string;
  ts: string;
}

interface RollupRow {
  kind_id: number;
  count: number;
  sum_payload32: number;
  min_payload32: number;
  max_payload32: number;
  fallback_count: number;
  error_count: number;
  sigma_consent_cap: number;
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<MetricsOk | MetricsErr>
): Promise<void> {
  logHit('analytics.metrics', { method: req.method ?? 'POST' });

  if (req.method !== 'GET') {
    const env = envelope();
    res.setHeader('Allow', 'GET');
    res.status(405).json({
      error: 'Method Not Allowed — GET only',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const bucket = parseBucketTier(req.query.bucket);
  const table = rollupTableForTier(bucket);

  const supabase = getSupabase();
  if (!supabase) {
    // Stub-mode : zero-filled buckets ; smoke-test friendly.
    const stub = stubEnvelope('Supabase env-vars missing · stub-mode metrics');
    const kinds: KindRow[] = EVENT_KIND_NAMES.map((name) => ({
      name,
      count: 0,
      avg: 0,
      min: 0,
      max: 0,
      fallback: 0,
      err: 0,
    }));
    res.status(200).json({
      bucket,
      attestation: sigmaMaskAttestation(),
      kinds,
      served_by: stub.served_by,
      ts: stub.ts,
      stub: true,
      todo: stub.todo,
    });
    return;
  }

  // § Real Supabase fetch. Aggregate on the JS side because Supabase-js
  // doesn't ship a server-side GROUP BY for our rollup table without
  // an explicit RPC. (RLS already filtered to caller-or-aggregate-cap rows.)
  try {
    const { data, error } = await supabase
      .from(table)
      .select(
        'kind_id, count, sum_payload32, min_payload32, max_payload32, fallback_count, error_count, sigma_consent_cap'
      )
      .gte('sigma_consent_cap', 2)
      .limit(10_000);
    if (error) {
      const env = envelope();
      res.status(502).json({
        error: `Supabase select failed — ${error.message}`,
        served_by: env.served_by,
        ts: env.ts,
      });
      return;
    }
    // Aggregate per kind_id.
    const totals = new Map<number, KindRow>();
    for (const name of EVENT_KIND_NAMES) {
      // pre-fill so empty kinds still appear (zeroed) for dashboard layout.
      totals.set(EVENT_KIND_NAMES.indexOf(name), {
        name,
        count: 0,
        avg: 0,
        min: 0,
        max: 0,
        fallback: 0,
        err: 0,
      });
    }
    let sumByKind = new Map<number, number>();
    for (const row of (data ?? []) as RollupRow[]) {
      const cur = totals.get(row.kind_id);
      if (!cur) continue;
      cur.count += row.count;
      sumByKind.set(
        row.kind_id,
        (sumByKind.get(row.kind_id) ?? 0) + row.sum_payload32
      );
      cur.min = cur.min === 0 ? row.min_payload32 : Math.min(cur.min, row.min_payload32);
      cur.max = Math.max(cur.max, row.max_payload32);
      cur.fallback += row.fallback_count;
      cur.err += row.error_count;
    }
    // Compute avg from accumulated sum/count.
    for (const [kindId, sum] of sumByKind.entries()) {
      const cur = totals.get(kindId);
      if (!cur || cur.count === 0) continue;
      cur.avg = Math.floor(sum / cur.count);
    }
    // Filter out zero-count kinds for the wire response.
    const kinds = [...totals.values()].filter((k) => k.count > 0);

    const env = envelope();
    res.status(200).json({
      bucket,
      attestation: sigmaMaskAttestation(),
      kinds,
      served_by: env.served_by,
      ts: env.ts,
    });
  } catch (e: unknown) {
    const env = envelope();
    const msg = e instanceof Error ? e.message : 'unknown';
    res.status(500).json({
      error: `Internal — ${msg}`,
      served_by: env.served_by,
      ts: env.ts,
    });
  }
}
