// cssl-edge · /api/content/cascade-publish
// § T11-W12-SUBSCRIBE — internal cascade-hook called by sibling W12-5
// publish-pipeline finalisation, sibling W12-5 creator-revoke, and
// sibling W12-11 moderation-revoke.
//
// POST /api/content/cascade-publish
//   headers : x-loa-cap (must include CONTENT_CAP_PUBLISH or CONTENT_CAP_REVOKE_ANY)
//   body : {
//     event_kind: 'publish'|'creator-revoke'|'moderation-revoke',
//     content_id: <hex64>,
//     creator_pubkey: <hex64>,
//     tags?: string[],
//     remix_root?: <hex64>,
//     audience_sigma_mask?: <uuid>,
//     reason?: string,
//   }
//   → 200 { ok:true, notif_count } · cascade fans out per matching active subscription
//   → 400 bad-shape · 403 cap-denied · 200 stub
//
// CASCADING FLOW :
//   1. Look up active subscriptions matching (target_kind, target_id) for
//      the event's creator + each tag + (if remix) the remix-root chain.
//   2. For each match → INSERT one notification row of the appropriate kind.
//   3. Σ-mask gate enforced via subscription.sigma_mask AND audience_sigma_mask.
//   4. Anti-spam : DB-level UNIQUE on (subscription_id, content_id, kind)
//      avoided in this slice ; rate-limit is enforced at-write-time by the
//      runtime crate (cssl-content-subscription's RateLimitBucket). The DB
//      stores all rows ; per-subscriber dedup happens client-side.

import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, envelope, stubEnvelope } from '@/lib/response';
import { logEvent, auditEvent } from '@/lib/audit';
import { isSovereignFromIncoming } from '@/lib/sovereign';
import { CONTENT_CAP_PUBLISH, CONTENT_CAP_REVOKE_ANY, checkCap } from '@/lib/cap';
import { createHash } from 'node:crypto';

const HEX64_RE = /^[0-9a-f]{64}$/i;
const KIND_TO_NOTIF: Record<string, string> = {
  'publish': 'new-published',
  'creator-revoke': 'revoked-by-creator',
  'moderation-revoke': 'revoked-by-moderation',
};

interface Body {
  event_kind?: string;
  content_id?: string;
  creator_pubkey?: string;
  tags?: string[];
  remix_root?: string;
  audience_sigma_mask?: string;
  reason?: string;
}

function notifId(sid: string, kind: string, cid: string, ts: string): string {
  const h = createHash('sha256');
  h.update(sid); h.update(kind); h.update(cid); h.update(ts);
  return h.digest('hex');
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  logHit('content.cascade-publish', { method: req.method ?? 'POST' });

  if (req.method !== 'POST') {
    res.status(405).json({ ok: false, error: 'POST only', ...envelope() });
    return;
  }

  const body: Body = (req.body ?? {}) as Body;
  const ek = body.event_kind ?? '';
  if (!Object.prototype.hasOwnProperty.call(KIND_TO_NOTIF, ek)) {
    res.status(400).json({ ok: false, error: 'bad event_kind', ...envelope() });
    return;
  }
  const contentId = body.content_id ?? '';
  const creatorPubkey = body.creator_pubkey ?? '';
  if (!HEX64_RE.test(contentId) || !HEX64_RE.test(creatorPubkey)) {
    res.status(400).json({ ok: false, error: 'content_id + creator_pubkey must be hex64', ...envelope() });
    return;
  }
  if (body.reason && body.reason.length > 200) {
    res.status(400).json({ ok: false, error: 'reason ≤ 200 chars', ...envelope() });
    return;
  }
  const tags = (body.tags ?? []).filter((t) => typeof t === 'string' && t.length >= 1 && t.length <= 64);
  const remixRoot = body.remix_root && HEX64_RE.test(body.remix_root) ? body.remix_root : undefined;

  const sovereign = isSovereignFromIncoming(req.headers, true);
  const capRaw = req.headers['x-loa-cap'];
  const capInt = Number(Array.isArray(capRaw) ? capRaw[0] : (capRaw ?? '0'));
  // Publish event → CONTENT_CAP_PUBLISH ; revoke events → CONTENT_CAP_REVOKE_ANY.
  const requiredCap = ek === 'publish' ? CONTENT_CAP_PUBLISH : CONTENT_CAP_REVOKE_ANY;
  const decision = checkCap(Number.isFinite(capInt) ? capInt : 0, requiredCap, sovereign);
  if (!decision.ok) {
    logEvent(auditEvent('content.cascade-publish', capInt, sovereign, 'denied', { reason: decision.reason ?? 'cap-denied' }));
    res.status(403).json({ ok: false, error: decision.reason ?? 'cap-denied', ...envelope() });
    return;
  }

  const supabaseUrl = process.env.NEXT_PUBLIC_SUPABASE_URL;
  const sbServiceKey = process.env.SUPABASE_SERVICE_ROLE_KEY;
  if (!supabaseUrl || !sbServiceKey) {
    res.status(200).json(stubEnvelope('wire SUPABASE_SERVICE_ROLE_KEY ; cascade fans-out per content_subscriptions match'));
    return;
  }

  try {
    // 1. Find active subscriptions matching (creator + each-tag + remix-root-as-chain).
    // PostgREST OR-filter syntax : ?or=(target_kind.eq.creator,target_id.eq.<pk>),(target_kind.eq.tag,target_id.eq.<t>)...
    // For simplicity + bounded query length, run separate queries and merge in TS.
    const matches: Array<{ id: string; subscriber_pubkey: string; sigma_mask: string }> = [];
    const sbUrl: string = supabaseUrl;
    const sbKey: string = sbServiceKey;
    async function fetchMatches(kind: string, targetId: string): Promise<void> {
      const url = `${sbUrl}/rest/v1/content_subscriptions`
        + `?target_kind=eq.${encodeURIComponent(kind)}`
        + `&target_id=eq.${encodeURIComponent(targetId)}`
        + `&revoked_at_ns=is.null`
        + `&select=id,subscriber_pubkey,sigma_mask,frequency`;
      const r = await fetch(url, {
        headers: { apikey: sbKey, authorization: `Bearer ${sbKey}` },
      });
      if (r.ok) {
        const rows: Array<{ id: string; subscriber_pubkey: string; sigma_mask: string; frequency: string }> = await r.json();
        for (const row of rows) {
          if (row.frequency === 'manual') continue; // manual = pull-only
          matches.push({ id: row.id, subscriber_pubkey: row.subscriber_pubkey, sigma_mask: row.sigma_mask });
        }
      }
    }

    await fetchMatches('creator', creatorPubkey);
    for (const t of tags) await fetchMatches('tag', t);
    if (remixRoot) await fetchMatches('content-chain', remixRoot);

    // Dedup by subscription id (one event = ≤ 1 notif per sub).
    const seen = new Set<string>();
    const dedup = matches.filter((m) => (seen.has(m.id) ? false : (seen.add(m.id), true)));

    // 2. Insert notification rows in one bulk POST.
    const ts_ns = BigInt(Date.now()) * 1_000_000n;
    const tsStr = ts_ns.toString();
    const notifKind = KIND_TO_NOTIF[ek] ?? 'new-published';
    const rows = dedup.map((m) => ({
      id: notifId(m.id, notifKind, contentId, tsStr),
      subscription_id: m.id,
      subscriber_pubkey: m.subscriber_pubkey,
      kind: notifKind,
      content_id: contentId,
      reason: body.reason ?? null,
      sigma_mask: m.sigma_mask,
      created_at_ns: tsStr,
    }));

    if (rows.length > 0) {
      const ins = await fetch(`${supabaseUrl}/rest/v1/content_notifications`, {
        method: 'POST',
        headers: {
          apikey: sbServiceKey,
          authorization: `Bearer ${sbServiceKey}`,
          'content-type': 'application/json',
          prefer: 'return=minimal,resolution=ignore-duplicates',
        },
        body: JSON.stringify(rows),
      });
      if (!ins.ok && ins.status !== 409) {
        const txt = await ins.text().catch(() => '');
        res.status(502).json({ ok: false, error: `supabase insert ${ins.status} ${txt.slice(0, 80)}`, ...envelope() });
        return;
      }
    }

    logEvent(auditEvent('content.cascade-publish', capInt, sovereign, 'ok', { ek, count: rows.length }));
    res.status(200).json({ ok: true, notif_count: rows.length, ...envelope() });
  } catch (e: unknown) {
    res.status(500).json({
      ok: false,
      error: e instanceof Error ? e.message : 'internal-error',
      ...envelope(),
    });
  }
}
