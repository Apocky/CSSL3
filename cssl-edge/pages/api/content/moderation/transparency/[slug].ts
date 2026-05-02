// cssl-edge · /api/content/moderation/transparency/:slug
// § T11-W12-MODERATION — full Σ-mask-aggregate · transparent-to-author
//
// GET /api/content/moderation/transparency/:slug
//   200 : envelope({
//     content_id        : string,
//     aggregate         : { total_flags, distinct_flaggers, severity_weighted,
//                           per_kind_counts, needs_review, visible_to_author,
//                           last_flag_iso, sovereign_revoked_at_iso },
//     decisions         : [{ decision_id, kind, decided_at_iso,
//                            sigma_chain_anchor, rationale, cap_class }],
//     appeals           : [{ appeal_id, filed_at_iso, resolved_at_iso,
//                            resolution_kind, curator_quorum_reached }],
//     no_shadowban_attestation : string,
//   })
//   404 : slug not found
//   405 : non-GET method
//
// PRIME-DIRECTIVE invariants enforced :
//   ─ ALL flag-counts visible to author (T2 floor: total_flags ≥ 3)
//   ─ ALL curator-decisions surfaced with Σ-Chain anchor
//   ─ ALL appeals with resolution-status surfaced
//   ─ no_shadowban_attestation string present in EVERY response
//   ─ transparency-by-default : no time-decay · no algorithmic-suppression
import type { NextApiRequest, NextApiResponse } from 'next';
import { envelope, logHit } from '@/lib/response';

interface AggregateView {
  total_flags: number;
  distinct_flaggers: number;
  severity_weighted: number;
  per_kind_counts: number[];
  needs_review: boolean;
  visible_to_author: boolean;
  last_flag_iso: string | null;
  sovereign_revoked_at_iso: string | null;
}

interface DecisionView {
  decision_id: string;
  kind: number;
  decided_at_iso: string;
  sigma_chain_anchor: string;
  rationale: string;
  cap_class: number;
}

interface AppealView {
  appeal_id: string;
  filed_at_iso: string;
  resolved_at_iso: string | null;
  resolution_kind: number | null;
  curator_quorum_reached: boolean;
}

interface TransparencyOk {
  ok: true;
  content_id: string;
  aggregate: AggregateView;
  decisions: DecisionView[];
  appeals: AppealView[];
  no_shadowban_attestation: string;
  served_by: string;
  ts: string;
  source: 'supabase' | 'stub';
}

interface TransparencyErr {
  error: string;
  served_by: string;
  ts: string;
}

const ROUTE_PREFIX = '/api/content/moderation/transparency/';

const NO_SHADOWBAN_ATTESTATION =
  'cssl-content-moderation : NO-shadowban + NO-algo-suppression + ' +
  'sovereign-revoke-wins + Sigma-Chain-anchor + author-transparent + ' +
  'flagger-revocable + 30d-appeal-window + 7d-auto-restore';

function buildStubAggregate(): AggregateView {
  return {
    total_flags: 0,
    distinct_flaggers: 0,
    severity_weighted: 0,
    per_kind_counts: [0, 0, 0, 0, 0, 0, 0, 0],
    needs_review: false,
    visible_to_author: false,
    last_flag_iso: null,
    sovereign_revoked_at_iso: null,
  };
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<TransparencyOk | TransparencyErr>
): Promise<void> {
  const slug = (Array.isArray(req.query.slug) ? req.query.slug[0] : req.query.slug) ?? '';
  logHit(`${ROUTE_PREFIX}${slug}`, { method: req.method });

  if (req.method !== 'GET') {
    res.setHeader('Allow', 'GET');
    res.status(405).json({ error: 'method-not-allowed', ...envelope() });
    return;
  }
  if (!slug || slug.length > 256) {
    res.status(404).json({ error: 'slug-not-found', ...envelope() });
    return;
  }

  // Stage-0 stub : Supabase not wired ⟶ deterministic empty-state response
  // carrying the no-shadowban attestation. Real impl reads from
  // content_moderation_aggregates + content_curator_decisions + content_appeals.
  const aggregate = buildStubAggregate();
  const decisions: DecisionView[] = [];
  const appeals: AppealView[] = [];

  res.status(200).json({
    ok: true,
    content_id: slug,
    aggregate,
    decisions,
    appeals,
    no_shadowban_attestation: NO_SHADOWBAN_ATTESTATION,
    source: 'stub',
    ...envelope(),
  });
}
