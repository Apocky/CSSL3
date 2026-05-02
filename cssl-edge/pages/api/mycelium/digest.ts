// cssl-edge · /api/mycelium/digest
// § T11-W14-MYCELIUM-HEARTBEAT — CLOUD-side fan-out endpoint.
//
// GET /api/mycelium/digest?since=<u32-ts-bucketed>&kind=<u8>&limit=<n>
//   →    { rows: [
//             { kind, payload_hash, cohort_size, mean_confidence_q8,
//               last_ts_bucketed, observation_count }
//           ],
//           cursor_next: <u32>, k_anon_floor: 10 }
//
// Returns ONLY rows whose distinct-emitter cohort has crossed the
// k-anon floor (k=10). Below-floor rows live in mycelium_federation_staged
// and are NEVER served from this endpoint (RLS denies anon-read on staged).
//
// PAGINATION
//   `since` is the ts_bucketed cursor (unix-minutes). The response's
//   `cursor_next` advances the caller's poll-loop one digest-cycle.
//   `limit` is bounded server-side (100 default, 500 max).
//
// SOVEREIGNTY
//   - public-read on `mycelium_federation_public` (k-anon promoted only).
//   - ¬ emitter_handle exposed in the digest (cohort_size only — the per-
//     emitter set is private to the staged-table).
//   - cron-secret optional ; the digest is k-anon safe so anonymous peers
//     can fetch it. Authenticated peers (with cron-secret) are rate-
//     limited less aggressively.

import type { NextApiRequest, NextApiResponse } from 'next';
import { envelope, logHit } from '@/lib/response';
import { isCronAuthorized } from '@/lib/cron-auth';

interface DigestRow {
  kind: number;
  payload_hash: string;
  cohort_size: number;
  mean_confidence_q8: number;
  last_ts_bucketed: number;
  observation_count: number;
}

const DEFAULT_LIMIT = 100;
const MAX_LIMIT = 500;
const K_ANON_FLOOR = 10;

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse,
): Promise<void> {
  logHit('mycelium.digest', { method: req.method ?? 'GET' });

  if (req.method !== 'GET') {
    res.setHeader('Allow', 'GET');
    res.status(405).json({ ok: false, error: 'GET only', ...envelope() });
    return;
  }

  // Auth optional ; record `via` for rate-limiting bookkeeping (downstream).
  const auth = isCronAuthorized(req);

  // ─── parse query ─────────────────────────────────────────────────────
  const sinceRaw = pickFirst(req.query.since);
  const kindRaw = pickFirst(req.query.kind);
  const limitRaw = pickFirst(req.query.limit);

  const since = clampU32(parseUint(sinceRaw, 0));
  const kind =
    kindRaw === undefined || kindRaw === '' ? null : clampU8(parseUint(kindRaw, 0));
  const limit = clampLimit(parseUint(limitRaw, DEFAULT_LIMIT));

  // ─── stub-mode (no supabase) ─────────────────────────────────────────
  const supabaseUrl = process.env.NEXT_PUBLIC_SUPABASE_URL;
  const sbAnonKey = process.env.NEXT_PUBLIC_SUPABASE_ANON_KEY;
  if (!supabaseUrl || !sbAnonKey) {
    res.status(200).json({
      ok: true,
      rows: [] as DigestRow[],
      cursor_next: since,
      k_anon_floor: K_ANON_FLOOR,
      stub: true,
      authed: auth.ok,
      ...envelope(),
    });
    return;
  }

  // ─── fetch from supabase (read-only public-table) ────────────────────
  // The mycelium_federation_public view is filtered by the SQL migration
  // to ONLY expose rows where cohort_size ≥ K_ANON_FLOOR. Schema CHECK
  // enforces this structurally ; even a bug here can't leak below-floor
  // rows because the table itself doesn't contain them.
  let url = `${supabaseUrl}/rest/v1/mycelium_federation_public?select=kind,payload_hash,cohort_size,mean_confidence_q8,last_ts_bucketed,observation_count`;
  url += `&last_ts_bucketed=gte.${since}`;
  if (kind !== null) {
    url += `&kind=eq.${kind}`;
  }
  url += `&order=last_ts_bucketed.asc&limit=${limit}`;

  try {
    const r = await fetch(url, {
      method: 'GET',
      headers: {
        apikey: sbAnonKey,
        authorization: `Bearer ${sbAnonKey}`,
      },
    });
    if (!r.ok) {
      res.status(502).json({
        ok: false,
        error: 'fetch failed',
        status: r.status,
        ...envelope(),
      });
      return;
    }
    const rows = ((await r.json()) as DigestRow[]) ?? [];

    // Compute next cursor : MAX(last_ts_bucketed) + 1, or `since` if empty.
    let cursor_next = since;
    for (const row of rows) {
      if (row.last_ts_bucketed >= cursor_next) {
        cursor_next = row.last_ts_bucketed + 1;
      }
    }

    res.status(200).json({
      ok: true,
      rows,
      cursor_next,
      k_anon_floor: K_ANON_FLOOR,
      authed: auth.ok,
      ...envelope(),
    });
  } catch (e: unknown) {
    res.status(502).json({
      ok: false,
      error: e instanceof Error ? e.message : 'fetch error',
      ...envelope(),
    });
  }
}

// ─── helpers ─────────────────────────────────────────────────────────────

function pickFirst(q: string | string[] | undefined): string | undefined {
  if (Array.isArray(q)) return q[0];
  return q;
}

function parseUint(s: string | undefined, dflt: number): number {
  if (s === undefined || s === '') return dflt;
  const n = Number.parseInt(s, 10);
  if (Number.isNaN(n) || n < 0) return dflt;
  return n;
}

function clampU8(n: number): number {
  return Math.min(255, Math.max(0, Math.floor(n)));
}

function clampU32(n: number): number {
  return Math.min(0xff_ff_ff_ff, Math.max(0, Math.floor(n)));
}

function clampLimit(n: number): number {
  return Math.min(MAX_LIMIT, Math.max(1, Math.floor(n)));
}
