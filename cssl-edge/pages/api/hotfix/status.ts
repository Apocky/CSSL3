// cssl-edge · /api/hotfix/status
// § T11-W11-HOTFIX-INFRA — fleet-wide telemetry collector.
//
// POST /api/hotfix/status
//   body : { channel, version, status: 'applied'|'failed'|'rolled_back', ts_ns,
//            jwt_sub? (optional · null = anonymous-aggregate-only) }
//   → { ok: true, recorded: bool }
//   Aggregates into hotfix_apply_status (counts) + optionally
//   hotfix_user_status (per-user, Σ-mask-gated by jwt_sub presence).
import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, envelope, stubEnvelope } from '@/lib/response';

interface StatusBody {
  channel?: string;
  version?: string;
  status?: 'applied' | 'failed' | 'rolled_back';
  ts_ns?: number;
  jwt_sub?: string | null;
  error?: string;
}

const VALID_CHANNELS = new Set([
  'loa.binary', 'cssl.bundle', 'kan.weights', 'balance.config',
  'recipe.book', 'nemesis.bestiary', 'security.patch',
  'storylet.content', 'render.pipeline',
]);

const VALID_STATUSES = new Set(['applied', 'failed', 'rolled_back']);

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  logHit('hotfix.status', { method: req.method ?? 'POST' });

  if (req.method !== 'POST') {
    res.status(405).json({ ok: false, error: 'POST only', ...envelope() });
    return;
  }

  const body: StatusBody = (req.body ?? {}) as StatusBody;
  if (!body.channel || !VALID_CHANNELS.has(body.channel)) {
    res.status(400).json({ ok: false, error: 'bad channel', ...envelope() });
    return;
  }
  if (!body.version || !/^\d+\.\d+\.\d+$/.test(body.version)) {
    res.status(400).json({ ok: false, error: 'bad version', ...envelope() });
    return;
  }
  if (!body.status || !VALID_STATUSES.has(body.status)) {
    res.status(400).json({ ok: false, error: 'bad status', ...envelope() });
    return;
  }

  const supabaseUrl = process.env.NEXT_PUBLIC_SUPABASE_URL;
  const sbServiceKey = process.env.SUPABASE_SERVICE_ROLE_KEY;
  if (!supabaseUrl || !sbServiceKey) {
    res.status(200).json(stubEnvelope('wire SUPABASE_SERVICE_ROLE_KEY ; status persists to hotfix_apply_status + hotfix_user_status'));
    return;
  }

  try {
    // Aggregate counters (always recorded ; no PII).
    const incCol = body.status === 'applied'
      ? 'applied_count'
      : body.status === 'failed' ? 'failed_count' : 'rolled_back_count';
    // Use the rpc helper bump_hotfix_apply_status defined in 0024_hotfix.sql.
    await fetch(`${supabaseUrl}/rest/v1/rpc/bump_hotfix_apply_status`, {
      method: 'POST',
      headers: {
        apikey: sbServiceKey,
        authorization: `Bearer ${sbServiceKey}`,
        'content-type': 'application/json',
      },
      body: JSON.stringify({
        p_channel: body.channel,
        p_version: body.version,
        p_column: incCol,
      }),
    });

    // Per-user row : ONLY if jwt_sub present (Σ-mask gate ; anonymous skips).
    if (body.jwt_sub) {
      await fetch(`${supabaseUrl}/rest/v1/hotfix_user_status`, {
        method: 'POST',
        headers: {
          apikey: sbServiceKey,
          authorization: `Bearer ${sbServiceKey}`,
          'content-type': 'application/json',
          prefer: 'resolution=merge-duplicates',
        },
        body: JSON.stringify({
          jwt_sub: body.jwt_sub,
          channel: body.channel,
          version: body.version,
          status: body.status,
          ts_ns: body.ts_ns ?? Date.now() * 1_000_000,
          error_msg: body.error ?? null,
        }),
      });
    }

    res.status(200).json({ ok: true, recorded: true, ...envelope() });
  } catch (e: unknown) {
    res.status(500).json({
      ok: false,
      error: e instanceof Error ? e.message : 'internal-error',
      ...envelope(),
    });
  }
}
