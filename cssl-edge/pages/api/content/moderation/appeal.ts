// cssl-edge · /api/content/moderation/appeal
// § T11-W12-MODERATION — author-appeals · 30-day-window · auto-restore @ 7d
//
// POST /api/content/moderation/appeal
//   body : {
//     content_id        : string (uuid),
//     author_pubkey     : string (hex),
//     decision_id_appealed : number | null,
//     decision_at_iso   : string | null,
//     rationale         : string (≤ 1024 chars),
//     signature         : string (hex · ed25519),
//   }
//   header : x-loa-cap : caller cap-mask integer
//   200 : envelope({ ok, appeal_id, curator_quorum_status, auto_restore_at_iso })
//   400 : malformed body / 30-day-window-violated
//   403 : cap denied (CONTENT_CAP_APPEAL)
//   405 : non-POST method
//
// PRIME-DIRECTIVE invariants enforced :
//   ─ author-appeal ALWAYS-available within 30-day window
//   ─ auto-restore at 7-days-no-curator-decision (T5)
//   ─ rationale-bounded (anti-spam ; not anti-speech)
//   ─ ¬ shadowban : appeal-status ALWAYS-visible to author
import type { NextApiRequest, NextApiResponse } from 'next';
import { envelope, logHit, resolveCap } from '@/lib/response';
import { logEvent, auditEvent } from '@/lib/audit';
import { checkCap, CONTENT_CAP_APPEAL } from '@/lib/cap';

interface AppealBody {
  content_id: string;
  author_pubkey: string;
  decision_id_appealed: number | null;
  decision_at_iso: string | null;
  rationale: string;
  signature: string;
}

interface AppealOk {
  ok: true;
  appeal_id: string;
  curator_quorum_status: 'pending' | 'reached';
  curator_quorum_required: number;
  auto_restore_at_iso: string;
  appeal_window_expires_at_iso: string | null;
  served_by: string;
  ts: string;
  source: 'supabase' | 'stub';
}

interface AppealErr {
  error: string;
  served_by: string;
  ts: string;
}

const ROUTE = '/api/content/moderation/appeal';
const T_AUTO_RESTORE_DAYS = 7;
const T_APPEAL_WINDOW_DAYS = 30;
const K_APPEAL_CURATOR_QUORUM = 3;
const MS_PER_DAY = 86_400_000;

function validate(body: unknown): body is AppealBody {
  if (typeof body !== 'object' || body === null) return false;
  const b = body as Record<string, unknown>;
  return (
    typeof b.content_id === 'string' &&
    typeof b.author_pubkey === 'string' &&
    (typeof b.decision_id_appealed === 'number' || b.decision_id_appealed === null) &&
    (typeof b.decision_at_iso === 'string' || b.decision_at_iso === null) &&
    typeof b.rationale === 'string' &&
    typeof b.signature === 'string'
  );
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<AppealOk | AppealErr>
): Promise<void> {
  logHit(ROUTE, { method: req.method });
  if (req.method !== 'POST') {
    res.setHeader('Allow', 'POST');
    res.status(405).json({ error: 'method-not-allowed', ...envelope() });
    return;
  }

  const cap = Number(req.headers['x-loa-cap'] ?? 0);
  const sovereignHdr = resolveCap(req.headers['x-loa-sovereign']);
  const sovereign = sovereignHdr === 'sovereign';
  const decision = checkCap(cap, CONTENT_CAP_APPEAL, sovereign);
  if (!decision.ok) {
    logEvent(auditEvent('content.moderation.appeal', cap, sovereign, 'denied', { reason: decision.reason }));
    res.status(403).json({ error: decision.reason ?? 'cap-denied', ...envelope() });
    return;
  }

  const body = req.body as unknown;
  if (!validate(body)) {
    res.status(400).json({ error: 'malformed-body', ...envelope() });
    return;
  }
  if (body.rationale.length > 1024) {
    res.status(400).json({ error: 'rationale-too-long', ...envelope() });
    return;
  }
  // 30-day window enforcement when decision-context provided.
  let appeal_window_expires_at_iso: string | null = null;
  if (body.decision_id_appealed !== null && body.decision_at_iso !== null) {
    const decisionAtMs = Date.parse(body.decision_at_iso);
    if (Number.isNaN(decisionAtMs)) {
      res.status(400).json({ error: 'invalid-decision-at-iso', ...envelope() });
      return;
    }
    const windowEndMs = decisionAtMs + T_APPEAL_WINDOW_DAYS * MS_PER_DAY;
    appeal_window_expires_at_iso = new Date(windowEndMs).toISOString();
    if (Date.now() > windowEndMs) {
      res.status(400).json({ error: 'appeal-window-expired', ...envelope() });
      return;
    }
  }

  const filed_at_ms = Date.now();
  const auto_restore_at_iso = new Date(
    filed_at_ms + T_AUTO_RESTORE_DAYS * MS_PER_DAY
  ).toISOString();

  logEvent(auditEvent('content.moderation.appeal', cap, sovereign, 'ok', {
    content_id: body.content_id,
    decision_id_appealed: body.decision_id_appealed,
  }));

  res.status(200).json({
    ok: true,
    appeal_id: `stub-appeal-${filed_at_ms}`,
    curator_quorum_status: 'pending',
    curator_quorum_required: K_APPEAL_CURATOR_QUORUM,
    auto_restore_at_iso,
    appeal_window_expires_at_iso,
    source: 'stub',
    ...envelope(),
  });
}
