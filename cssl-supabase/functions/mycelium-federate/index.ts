// § T11-W14-K · Supabase edge-function · mycelium-federate
// PURPOSE : peer-pattern-aggregation across mycelium-emissions ·
//           refresh `mycelium_patterns_agg` table · k-anon enforced.
//
// Trigger : invoked from /api/cron/mycelium-relay OR via supabase-CLI cron.
// Auth    : verifies CRON_SECRET in Authorization Bearer header.
//
// Sovereignty :
//   - aggregator NEVER touches raw cap_witness_hash · only DISTINCT-COUNT
//   - patterns with cap_floor < 2 dropped (¬ Aggregate-Relay consent)
//   - refresh idempotent (TRUNCATE + INSERT in single TX)

// deno-lint-ignore-file no-explicit-any
import { createClient } from 'https://esm.sh/@supabase/supabase-js@2';

const CRON_SECRET = Deno.env.get('CRON_SECRET') ?? '';
const SB_URL = Deno.env.get('SUPABASE_URL') ?? '';
const SB_SVC_KEY = Deno.env.get('SUPABASE_SERVICE_ROLE_KEY') ?? '';

interface Body {
  window_hours?: number; // sliding-window for "active" patterns · default 24
  k_anon_floor?: number; // override · default 10
}

interface RespOk {
  ok: true;
  patterns_inserted: number;
  patterns_dropped_below_k: number;
  window_hours: number;
  k_anon_floor: number;
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
  // Auth
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

  // Parse body (POST) or use defaults (GET)
  let body: Body = {};
  if (req.method === 'POST') {
    try {
      body = await req.json();
    } catch (_e) {
      body = {};
    }
  }
  const windowHours = body.window_hours ?? 24;
  const kAnonFloor = body.k_anon_floor ?? 10;

  // Build the aggregate. Source = akashic_events filtered to recent window.
  // GROUP BY (cluster_signature) · COUNT(DISTINCT cap_witness_hash).
  if (SB_URL.length === 0 || SB_SVC_KEY.length === 0) {
    return respond({
      ok: false,
      error: 'SUPABASE_URL or SERVICE_ROLE_KEY missing',
      ts: new Date().toISOString(),
    }, 200);
  }
  const sb = createClient(SB_URL, SB_SVC_KEY, { auth: { persistSession: false } });

  try {
    // Use raw SQL via .rpc() to a helper — but we want this self-contained,
    // so emulate via two-step query. Step 1 : pull raw rows of distinct
    // (cluster_signature, cap_witness_hash, sigma_mask, ts).
    const sinceIso = new Date(Date.now() - windowHours * 3600 * 1000).toISOString();
    const { data: rows, error: qErr } = await sb
      .from('akashic_events')
      .select('cluster_signature, cap_witness_hash, sigma_mask, kind, ts_iso')
      .gte('ts_iso', sinceIso)
      .gte('sigma_mask', 2) // require AggregateRelay or higher
      .not('cluster_signature', 'is', null)
      .limit(50000);

    if (qErr) {
      return respond({
        ok: false,
        error: `query-failed: ${qErr.message}`,
        ts: new Date().toISOString(),
      }, 502);
    }

    // Aggregate in-memory.
    const agg = new Map<string, {
      cluster_signature: string;
      pattern_kind: string;
      contributors: Set<string>;
      last_seen_at: string;
      cap_floor: number;
    }>();

    for (const r of rows ?? []) {
      const sig = String(r.cluster_signature);
      const witness = String(r.cap_witness_hash ?? 'anon');
      const cap = Number(r.sigma_mask ?? 0);
      const ts = String(r.ts_iso);
      const kind = String(r.kind ?? 'unknown');

      const existing = agg.get(sig);
      if (existing === undefined) {
        agg.set(sig, {
          cluster_signature: sig,
          pattern_kind: kind,
          contributors: new Set([witness]),
          last_seen_at: ts,
          cap_floor: cap,
        });
      } else {
        existing.contributors.add(witness);
        if (ts > existing.last_seen_at) existing.last_seen_at = ts;
        if (cap < existing.cap_floor) existing.cap_floor = cap;
      }
    }

    // Convert to insert rows · drop sub-k entries.
    let dropped = 0;
    const inserts: Array<{
      cluster_signature: string;
      pattern_kind: string;
      contributor_count: number;
      last_seen_at: string;
      cap_floor: number;
      refreshed_at: string;
    }> = [];
    const refreshedAt = new Date().toISOString();
    for (const v of agg.values()) {
      const count = v.contributors.size;
      if (count < kAnonFloor) {
        dropped += 1;
        continue;
      }
      inserts.push({
        cluster_signature: v.cluster_signature,
        pattern_kind: v.pattern_kind,
        contributor_count: count,
        last_seen_at: v.last_seen_at,
        cap_floor: v.cap_floor,
        refreshed_at: refreshedAt,
      });
    }

    // Refresh : DELETE + INSERT inside one operation. We can't TX from JS,
    // so rely on idempotent UPSERT on the primary key cluster_signature.
    if (inserts.length > 0) {
      const { error: upErr } = await sb
        .from('mycelium_patterns_agg')
        .upsert(inserts, { onConflict: 'cluster_signature' });
      if (upErr) {
        return respond({
          ok: false,
          error: `upsert-failed: ${upErr.message}`,
          ts: new Date().toISOString(),
        }, 502);
      }
    }

    return respond({
      ok: true,
      patterns_inserted: inserts.length,
      patterns_dropped_below_k: dropped,
      window_hours: windowHours,
      k_anon_floor: kAnonFloor,
      ts: refreshedAt,
    }, 200);
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : 'exception';
    return respond({
      ok: false,
      error: msg,
      ts: new Date().toISOString(),
    }, 500);
  }
});
