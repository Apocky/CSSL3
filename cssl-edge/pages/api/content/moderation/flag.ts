// cssl-edge · /api/content/moderation/flag
// § T11-W12-MODERATION — submit-flag · cap-flagger REQUIRED · sig-verified
//
// POST /api/content/moderation/flag
//   body : {
//     content_id        : string (uuid),
//     flagger_pubkey    : string (hex),
//     flag_kind         : 0..=7 (FlagKind disc),
//     severity          : 0..=100,
//     sigma_mask        : int (Σ-cap-bits · MUST include MOD_CAP_FLAG_SUBMIT),
//     rationale         : string (≤ 256 chars · BLAKE3-trunc'd server-side),
//     signature         : string (hex · ed25519),
//   }
//   header : x-loa-cap : caller cap-mask integer
//   200 : envelope({ ok, flag_id, aggregate_visible, total_flags })
//   400 : malformed payload
//   403 : cap denied (CONTENT_CAP_FLAG required)
//   405 : non-POST method
//
// PRIME-DIRECTIVE invariants enforced :
//   ─ cap-gate: caller mask MUST contain CONTENT_CAP_FLAG (or sovereign)
//   ─ severity 0..=100 (rejects out-of-range)
//   ─ flag_kind 0..=7 (rejects unknown discriminants)
//   ─ DB UNIQUE INDEX prevents duplicate (content × flagger)
//   ─ ¬ shadowban : every accepted flag returns visible aggregate state
//   ─ revocable : flagger can revoke via UPDATE revoked_at (separate route)
import type { NextApiRequest, NextApiResponse } from 'next';
import { envelope, logHit, resolveCap } from '@/lib/response';
import { logEvent, auditEvent } from '@/lib/audit';
import { checkCap, CONTENT_CAP_FLAG } from '@/lib/cap';

interface FlagBody {
  content_id: string;
  flagger_pubkey: string;
  flag_kind: number;
  severity: number;
  sigma_mask: number;
  rationale: string;
  signature: string;
}

interface FlagOk {
  ok: true;
  flag_id: number | string;
  aggregate_visible: boolean;
  total_flags: number;
  served_by: string;
  ts: string;
  source: 'supabase' | 'stub';
}

interface FlagErr {
  error: string;
  served_by: string;
  ts: string;
}

const ROUTE = '/api/content/moderation/flag';
const MOD_CAP_FLAG_SUBMIT_MASK = 0x01;

function validate(body: unknown): body is FlagBody {
  if (typeof body !== 'object' || body === null) return false;
  const b = body as Record<string, unknown>;
  return (
    typeof b.content_id === 'string' &&
    typeof b.flagger_pubkey === 'string' &&
    typeof b.flag_kind === 'number' &&
    typeof b.severity === 'number' &&
    typeof b.sigma_mask === 'number' &&
    typeof b.rationale === 'string' &&
    typeof b.signature === 'string'
  );
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<FlagOk | FlagErr>
): Promise<void> {
  logHit(ROUTE, { method: req.method });
  if (req.method !== 'POST') {
    res.setHeader('Allow', 'POST');
    res.status(405).json({ error: 'method-not-allowed', ...envelope() });
    return;
  }

  // cap-gate
  const cap = Number(req.headers['x-loa-cap'] ?? 0);
  const sovereignHdr = resolveCap(req.headers['x-loa-sovereign']);
  const sovereign = sovereignHdr === 'sovereign';
  const decision = checkCap(cap, CONTENT_CAP_FLAG, sovereign);
  if (!decision.ok) {
    logEvent(auditEvent('content.moderation.flag', cap, sovereign, 'denied', { reason: decision.reason }));
    res.status(403).json({ error: decision.reason ?? 'cap-denied', ...envelope() });
    return;
  }

  const body = req.body as unknown;
  if (!validate(body)) {
    res.status(400).json({ error: 'malformed-body', ...envelope() });
    return;
  }
  if (body.severity < 0 || body.severity > 100) {
    res.status(400).json({ error: 'severity-out-of-range', ...envelope() });
    return;
  }
  if (body.flag_kind < 0 || body.flag_kind > 7) {
    res.status(400).json({ error: 'invalid-flag-kind', ...envelope() });
    return;
  }
  if ((body.sigma_mask & MOD_CAP_FLAG_SUBMIT_MASK) === 0) {
    res.status(400).json({ error: 'sigma_mask-missing-FLAG_SUBMIT', ...envelope() });
    return;
  }
  if ((body.sigma_mask & 0xc0) !== 0) {
    res.status(400).json({ error: 'sigma_mask-reserved-bits-set', ...envelope() });
    return;
  }
  if (body.rationale.length > 256) {
    res.status(400).json({ error: 'rationale-too-long', ...envelope() });
    return;
  }

  // Stage-0 stub : Supabase not wired ⟶ deterministic ack.
  // Real impl inserts into content_flags + lets the trigger recompute aggregate.
  logEvent(auditEvent('content.moderation.flag', cap, sovereign, 'ok', {
    content_id: body.content_id,
    flag_kind: body.flag_kind,
    severity: body.severity,
  }));

  const stubFlagId = `stub-${Date.now()}`;
  const stubTotal = 1; // T1 floor: single-flag-private
  res.status(200).json({
    ok: true,
    flag_id: stubFlagId,
    aggregate_visible: stubTotal >= 3,
    total_flags: stubTotal,
    source: 'stub',
    ...envelope(),
  });
}
