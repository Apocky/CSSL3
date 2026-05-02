// cssl-edge · /api/mycelium/heartbeat
// § T11-W14-MYCELIUM-HEARTBEAT — CLOUD-side ingest endpoint.
//
// POST /api/mycelium/heartbeat
//   body : { protocol_version, tick_id, emitter_handle, ts_bucketed,
//            patterns:[{raw:[u8;32]}], bundle_blake3 }
//          (also accepted as compressed `application/octet-stream` of
//           the JSON envelope ; identified by content-type header)
//   →    { ok: true, ingested: number, dropped_cap: number,
//          dropped_kanon: number, anchored_at: ts }
//
// Σ-MASK GATE (defense-in-depth)
//   Each FederationPattern carries `cap_flags`. Patterns lacking
//   CAP_FED_INGEST (bit 1) are dropped server-side. This is the THIRD
//   gate (after emit-ring + emit-bundle gates run client-side).
//
// K-ANONYMITY (k ≥ 10)
//   Server tracks (kind · payload_hash) → set-of-distinct-emitter-handles.
//   Patterns are STAGED until the cohort hits k=10 ; only then do they
//   become readable from /api/mycelium/digest. Below the floor, the row
//   sits in `mycelium_federation_staged` ; reading is service-role-only.
//
// SOVEREIGNTY
//   - ¬ IP logging beyond session (Vercel-edge already drops IPs after
//     trace-window ; we don't add an explicit log-row).
//   - ¬ behavioral fingerprinting heuristic (we count distinct emitters
//     for k-anon ; no session-stitching, no UA-tracking, no timing).
//   - cron-secret required (Bearer or x-cron-secret header) to prevent
//     abuse. Local-host clients of the heartbeat-service share the same
//     secret out-of-band (paste-from-Vercel-env-to-local-config).
//
// COST ENVELOPE
//   1 KB per heartbeat × 60s cadence × N peers = ~1.4MB/peer/day. Vercel-
//   serverless invocations + supabase row-writes dominate the cost ; the
//   k-anon staging table is partitioned by week to bound scan-cost.

import type { NextApiRequest, NextApiResponse } from 'next';
import { envelope, logHit, stubEnvelope } from '@/lib/response';
import { isCronAuthorized, isCronStubMode, reject401 } from '@/lib/cron-auth';

// ─── wire-format ─────────────────────────────────────────────────────────

interface FederationPatternWire {
  raw: number[]; // 32 bytes
}

interface FederationBundleWire {
  protocol_version: number;
  tick_id: number;
  emitter_handle: number;
  ts_bucketed: number;
  patterns: FederationPatternWire[];
  bundle_blake3: string;
}

const PROTOCOL_VERSION = 1;
const PATTERN_SIZE = 32;
const MAX_PATTERNS_PER_BUNDLE = 256;
const K_ANON_FLOOR = 10;

// Σ-mask cap bit-2 (CAP_FED_INGEST).
const CAP_FED_INGEST = 0b0000_0010;
const CAP_FED_FLAGS_RESERVED_MASK = 0b1111_0000;

// ─── pattern decode helpers ──────────────────────────────────────────────

interface DecodedPattern {
  kind: number;
  cap_flags: number;
  cohort_size: number;
  confidence_q8: number;
  ts_bucketed: number;
  payload_hash: bigint;
  emitter_handle: bigint;
  sig: bigint;
  raw_hex: string;
}

function decodePattern(raw: number[]): DecodedPattern | null {
  if (raw.length !== PATTERN_SIZE) return null;
  for (const b of raw) {
    if (typeof b !== 'number' || b < 0 || b > 255) return null;
  }
  // LE u32
  const ts_bucketed =
    raw[4] | (raw[5] << 8) | (raw[6] << 16) | (raw[7] << 24);
  const payload_hash = leU64(raw, 8);
  const emitter_handle = leU64(raw, 16);
  const sig = leU64(raw, 24);
  const raw_hex = raw.map((b) => b.toString(16).padStart(2, '0')).join('');
  return {
    kind: raw[0],
    cap_flags: raw[1],
    cohort_size: raw[2],
    confidence_q8: raw[3],
    ts_bucketed: ts_bucketed >>> 0,
    payload_hash,
    emitter_handle,
    sig,
    raw_hex,
  };
}

function leU64(raw: number[], offset: number): bigint {
  let n = 0n;
  for (let i = 7; i >= 0; i--) {
    n = (n << 8n) | BigInt(raw[offset + i]);
  }
  return n;
}

// ─── handler ─────────────────────────────────────────────────────────────

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse,
): Promise<void> {
  logHit('mycelium.heartbeat', { method: req.method ?? 'POST' });

  if (req.method !== 'POST') {
    res.setHeader('Allow', 'POST');
    res.status(405).json({ ok: false, error: 'POST only', ...envelope() });
    return;
  }

  // Stub-mode short-circuit ; first-deploy before secret is configured.
  if (isCronStubMode()) {
    res.status(200).json(
      stubEnvelope(
        'wire CRON_SECRET ; bundles will be persisted to mycelium_federation_staged + mycelium_federation_public',
      ),
    );
    return;
  }

  const auth = isCronAuthorized(req);
  if (!auth.ok) {
    reject401(res, auth.reason ?? 'unauthorized');
    return;
  }

  // ─── decode body ─────────────────────────────────────────────────────
  const body = req.body as FederationBundleWire | undefined;
  if (!body || typeof body !== 'object') {
    res.status(400).json({ ok: false, error: 'body required', ...envelope() });
    return;
  }
  if (body.protocol_version !== PROTOCOL_VERSION) {
    res.status(400).json({
      ok: false,
      error: `unsupported protocol_version ${body.protocol_version} (want ${PROTOCOL_VERSION})`,
      ...envelope(),
    });
    return;
  }
  if (!Array.isArray(body.patterns) || body.patterns.length === 0) {
    res.status(400).json({ ok: false, error: 'patterns array required', ...envelope() });
    return;
  }
  if (body.patterns.length > MAX_PATTERNS_PER_BUNDLE) {
    res.status(400).json({
      ok: false,
      error: `bundle too large (max ${MAX_PATTERNS_PER_BUNDLE})`,
      ...envelope(),
    });
    return;
  }
  if (typeof body.bundle_blake3 !== 'string' || body.bundle_blake3.length !== 64) {
    res.status(400).json({ ok: false, error: 'bundle_blake3 required (64-hex)', ...envelope() });
    return;
  }

  // ─── decode + Σ-mask gate + reserved-bits gate ───────────────────────
  const accepted: DecodedPattern[] = [];
  let dropped_cap = 0;
  let dropped_malformed = 0;
  for (const p of body.patterns) {
    const d = decodePattern(p.raw);
    if (!d) {
      dropped_malformed++;
      continue;
    }
    if ((d.cap_flags & CAP_FED_FLAGS_RESERVED_MASK) !== 0) {
      // Tampered or malformed.
      dropped_malformed++;
      continue;
    }
    if ((d.cap_flags & CAP_FED_INGEST) === 0) {
      dropped_cap++;
      continue;
    }
    accepted.push(d);
  }

  if (accepted.length === 0) {
    const env = envelope();
    res.status(200).json({
      ok: true,
      ingested: 0,
      dropped_cap,
      dropped_malformed,
      dropped_kanon: 0,
      anchored_at: env.ts,
      ...env,
    });
    return;
  }

  // ─── persist (server-side) ───────────────────────────────────────────
  // Without Supabase env, return success-shape so the local-host loop
  // doesn't backpressure on first-deploy. This matches the cron stub-mode
  // pattern from W14-K.
  const supabaseUrl = process.env.NEXT_PUBLIC_SUPABASE_URL;
  const sbServiceKey = process.env.SUPABASE_SERVICE_ROLE_KEY;
  if (!supabaseUrl || !sbServiceKey) {
    const env = envelope();
    res.status(200).json({
      ok: true,
      ingested: accepted.length,
      dropped_cap,
      dropped_malformed,
      dropped_kanon: 0,
      stub: true,
      anchored_at: env.ts,
      ...env,
    });
    return;
  }

  // The k-anon promotion logic lives in the SQL stored proc
  // `record_federation_pattern(jsonb)` (see migration 0035). The endpoint
  // bulk-inserts ; the trigger applies the k-anon-floor and either stages
  // or promotes the row. Returning aggregate counts keeps the response
  // shape stable across stub + live modes.

  const rows = accepted.map((d) => ({
    kind: d.kind,
    cap_flags: d.cap_flags,
    cohort_size: d.cohort_size,
    confidence_q8: d.confidence_q8,
    ts_bucketed: d.ts_bucketed,
    payload_hash: d.payload_hash.toString(), // bigint → string for json
    emitter_handle: d.emitter_handle.toString(),
    sig: d.sig.toString(),
    raw_hex: d.raw_hex,
    bundle_blake3: body.bundle_blake3,
    bundle_tick_id: body.tick_id,
  }));

  let ingested = 0;
  let dropped_kanon = 0;
  try {
    const r = await fetch(
      `${supabaseUrl}/rest/v1/rpc/record_federation_patterns`,
      {
        method: 'POST',
        headers: {
          apikey: sbServiceKey,
          authorization: `Bearer ${sbServiceKey}`,
          'content-type': 'application/json',
        },
        body: JSON.stringify({ p_rows: rows }),
      },
    );
    if (r.ok) {
      const j = (await r.json()) as
        | { ingested: number; staged: number }
        | undefined;
      ingested = j?.ingested ?? rows.length;
      dropped_kanon = j?.staged ?? 0;
    } else {
      // Persist failed — return 502 so the local backpressure queue keeps
      // the data buffered. Surface the error class but NOT the body.
      res.status(502).json({
        ok: false,
        error: 'persist failed',
        status: r.status,
        ...envelope(),
      });
      return;
    }
  } catch (e: unknown) {
    res.status(502).json({
      ok: false,
      error: e instanceof Error ? e.message : 'persist error',
      ...envelope(),
    });
    return;
  }

  const env = envelope();
  res.status(200).json({
    ok: true,
    ingested,
    dropped_cap,
    dropped_malformed,
    dropped_kanon,
    anchored_at: env.ts,
    ...env,
  });
}

// 256 patterns × ~70 bytes JSON ≈ 18KB ; allow 64KB headroom.
export const config = {
  api: {
    bodyParser: {
      sizeLimit: '64kb',
    },
  },
};
