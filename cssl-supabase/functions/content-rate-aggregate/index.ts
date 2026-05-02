// § T11-W14-K · Supabase edge-function · content-rate-aggregate
// PURPOSE : recompute trending content ranks from per-rating rows ·
//           materialize `content_trending_24h` (top-N by weighted score) ·
//           k-anon ≥ 10 distinct raters per package required for inclusion.
//
// Trigger : invoked hourly via /api/cron/playtest-cycle OR supabase-CLI.
// Auth    : Authorization Bearer CRON_SECRET.
//
// Sovereignty :
//   - aggregator counts DISTINCT rater_pubkey · ¬ surface raw IDs
//   - rating-rows themselves stay RLS-gated · only counts emerge
//   - score formula = (avg_rating × log10(1 + count)) so spam-bots can't
//     dominate by repeat-rating (UNIQUE(package_id, rater_pubkey))

// deno-lint-ignore-file no-explicit-any
import { createClient } from 'https://esm.sh/@supabase/supabase-js@2';

const CRON_SECRET = Deno.env.get('CRON_SECRET') ?? '';
const SB_URL = Deno.env.get('SUPABASE_URL') ?? '';
const SB_SVC_KEY = Deno.env.get('SUPABASE_SERVICE_ROLE_KEY') ?? '';

interface Body {
  window_hours?: number;
  k_anon_floor?: number;
  top_n?: number;
}

interface RespOk {
  ok: true;
  packages_scored: number;
  packages_dropped_below_k: number;
  window_hours: number;
  k_anon_floor: number;
  top_n: number;
  ts: string;
}

interface RespErr {
  ok: false;
  error: string;
  ts: string;
}

function respond(body: RespOk | RespErr, status: number): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

Deno.serve(async (req: Request): Promise<Response> => {
  const auth = req.headers.get('authorization') ?? '';
  if (CRON_SECRET.length === 0) {
    return respond({
      ok: false,
      error: 'CRON_SECRET not configured · stub-mode',
      ts: new Date().toISOString(),
    }, 200);
  }
  if (!auth.startsWith('Bearer ') || auth.slice(7) !== CRON_SECRET) {
    return respond({
      ok: false,
      error: 'unauthorized',
      ts: new Date().toISOString(),
    }, 401);
  }
  if (req.method !== 'POST' && req.method !== 'GET') {
    return respond({
      ok: false,
      error: 'POST or GET only',
      ts: new Date().toISOString(),
    }, 405);
  }

  let body: Body = {};
  if (req.method === 'POST') {
    try { body = await req.json(); } catch (_e) { body = {}; }
  }
  const windowHours = body.window_hours ?? 24;
  const kAnonFloor = body.k_anon_floor ?? 10;
  const topN = body.top_n ?? 100;

  if (SB_URL.length === 0 || SB_SVC_KEY.length === 0) {
    return respond({
      ok: false,
      error: 'SUPABASE_URL or SERVICE_ROLE_KEY missing',
      ts: new Date().toISOString(),
    }, 200);
  }

  const sb = createClient(SB_URL, SB_SVC_KEY, { auth: { persistSession: false } });

  try {
    const sinceIso = new Date(Date.now() - windowHours * 3600 * 1000).toISOString();

    // Pull ratings within window.
    const { data: rows, error: qErr } = await sb
      .from('content_ratings')
      .select('package_id, rater_pubkey, rating, rated_at')
      .gte('rated_at', sinceIso)
      .limit(50000);
    if (qErr) {
      return respond({
        ok: false,
        error: `query-failed: ${qErr.message}`,
        ts: new Date().toISOString(),
      }, 502);
    }

    // Aggregate.
    const agg = new Map<string, {
      package_id: string;
      raters: Set<string>;
      sum: number;
      count: number;
    }>();
    for (const r of rows ?? []) {
      const pid = String(r.package_id);
      const rater = String(r.rater_pubkey);
      const score = Number(r.rating ?? 0);
      const existing = agg.get(pid);
      if (existing === undefined) {
        agg.set(pid, {
          package_id: pid,
          raters: new Set([rater]),
          sum: score,
          count: 1,
        });
      } else {
        existing.raters.add(rater);
        existing.sum += score;
        existing.count += 1;
      }
    }

    let dropped = 0;
    const ranked: Array<{
      package_id: string;
      score: number;
      avg_rating: number;
      rater_count: number;
      window_hours: number;
      computed_at: string;
    }> = [];
    const computedAt = new Date().toISOString();
    for (const v of agg.values()) {
      const distinctRaters = v.raters.size;
      if (distinctRaters < kAnonFloor) {
        dropped += 1;
        continue;
      }
      const avg = v.sum / v.count;
      // weighted-score formula : avg × log10(1 + distinct-raters)
      const score = avg * Math.log10(1 + distinctRaters);
      ranked.push({
        package_id: v.package_id,
        score,
        avg_rating: avg,
        rater_count: distinctRaters,
        window_hours: windowHours,
        computed_at: computedAt,
      });
    }

    ranked.sort((a, b) => b.score - a.score);
    const top = ranked.slice(0, topN);

    if (top.length > 0) {
      // Truncate-and-replace via upsert. content_trending table created in
      // migration 0034 (or pre-existing).
      const { error: upErr } = await sb
        .from('content_trending')
        .upsert(top, { onConflict: 'package_id' });
      if (upErr && upErr.code !== '42P01' /* table-not-found · ok in stub */) {
        return respond({
          ok: false,
          error: `upsert-failed: ${upErr.message}`,
          ts: new Date().toISOString(),
        }, 502);
      }
    }

    return respond({
      ok: true,
      packages_scored: top.length,
      packages_dropped_below_k: dropped,
      window_hours: windowHours,
      k_anon_floor: kAnonFloor,
      top_n: topN,
      ts: computedAt,
    }, 200);
  } catch (e: unknown) {
    return respond({
      ok: false,
      error: e instanceof Error ? e.message : 'exception',
      ts: new Date().toISOString(),
    }, 500);
  }
});
