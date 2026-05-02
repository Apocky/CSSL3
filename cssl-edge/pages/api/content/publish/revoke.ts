// cssl-edge · /api/content/publish/revoke
// § T11-W12-UGC-PUBLISH — creator OR moderator (with cap) marks revoked ·
// cascades-to-subscribers via mycelium-broadcast.
//
// POST /api/content/publish/revoke
//   headers : x-loa-cap (int)  · x-author-pubkey (requester pubkey)
//   body    : PublishRevokeRequest { package_id, reason, who_pubkey, is_moderator? }
//   → 200 { ok:true, package_id, mycelium_broadcast: {...}, ts }
//   → 403 cap-denied (need CONTENT_CAP_PUBLISH for self-revoke
//                      OR CONTENT_CAP_REVOKE_ANY for moderator-revoke)
//   → 404 package not found
//   → 200 { stub:true } when Supabase env-vars absent.
//
// Cap-rule :
//   - SELF revoke (who_pubkey === author_pubkey) → CONTENT_CAP_PUBLISH bit
//   - ANY revoke  (who_pubkey !== author_pubkey) → CONTENT_CAP_REVOKE_ANY bit
//
// Sovereignty :
//   ¬ silent-revoke   · audit-row written + mycelium-broadcast emitted
//   creator-revoke    · always honoured (sovereign)
//   moderator-revoke  · cap-gated · audit-trail with moderator pubkey
//   sovereign-cap-bypass · honoured for both flows (Apocky operator)

import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, envelope, stubEnvelope } from '@/lib/response';
import { isSovereignFromIncoming } from '@/lib/sovereign';
import { CONTENT_CAP_PUBLISH, CONTENT_CAP_REVOKE_ANY, checkCap } from '@/lib/cap';
import { logEvent, auditEvent } from '@/lib/audit';
import {
  validateRevoke,
  type PublishRevokeRequest,
  buildRevokeBroadcast,
  type MyceliumRevokeBroadcast,
  HEX64_RE,
} from '@/lib/content-publish';

interface OkResp {
  ok: true;
  package_id: string;
  cascade_kind: 'self' | 'moderator';
  mycelium_broadcast: MyceliumRevokeBroadcast;
  ts: string;
  served_by: string;
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  logHit('content.publish.revoke', { method: req.method ?? 'POST' });

  if (req.method !== 'POST') {
    res.status(405).json({ ok: false, error: 'POST only', ...envelope() });
    return;
  }

  const body = (req.body ?? {}) as PublishRevokeRequest;
  const v = validateRevoke(body);
  if (!v.ok) {
    logEvent(auditEvent('content.publish.revoke', 0, false, 'denied', { reason: v.reason }));
    res.status(400).json({ ok: false, error: v.reason, ...envelope() });
    return;
  }

  // Pull the requester header pubkey to cross-check who_pubkey.
  const headerPubkey = (() => {
    const x = req.headers['x-author-pubkey'];
    return typeof x === 'string' ? x : Array.isArray(x) ? x[0] ?? '' : '';
  })();
  if (!HEX64_RE.test(headerPubkey)) {
    res.status(400).json({ ok: false, error: 'x-author-pubkey must be 64-hex', ...envelope() });
    return;
  }
  if (headerPubkey !== body.who_pubkey) {
    logEvent(auditEvent('content.publish.revoke', 0, false, 'denied', { reason: 'header-body-pubkey-mismatch' }));
    res.status(403).json({ ok: false, error: 'header pubkey != body.who_pubkey', ...envelope() });
    return;
  }

  // Capabilities.
  const capRaw = req.headers['x-loa-cap'];
  const capInt = Number(Array.isArray(capRaw) ? capRaw[0] : (capRaw ?? '0'));
  const sovereign = isSovereignFromIncoming(req.headers, true);

  const supabaseUrl = process.env.NEXT_PUBLIC_SUPABASE_URL;
  const sbServiceKey = process.env.SUPABASE_SERVICE_ROLE_KEY;

  if (!supabaseUrl || !sbServiceKey) {
    // Stub : we cannot resolve self-vs-moderator without DB → require either cap.
    const decision = checkCap(
      Number.isFinite(capInt) ? capInt : 0,
      CONTENT_CAP_PUBLISH | CONTENT_CAP_REVOKE_ANY,
      sovereign,
    );
    if (!decision.ok) {
      // Try one of the two bits individually (stub-mode permissive).
      const hasEither = ((capInt & CONTENT_CAP_PUBLISH) === CONTENT_CAP_PUBLISH)
                     || ((capInt & CONTENT_CAP_REVOKE_ANY) === CONTENT_CAP_REVOKE_ANY)
                     || sovereign;
      if (!hasEither) {
        logEvent(auditEvent('content.publish.revoke', capInt, sovereign, 'denied', { reason: 'no-cap' }));
        res.status(403).json({ ok: false, error: 'no revoke cap', ...envelope() });
        return;
      }
    }
    const broadcast = buildRevokeBroadcast(body.package_id, body.reason, body.who_pubkey);
    logEvent(auditEvent('content.publish.revoke', capInt, sovereign, 'ok', { package_id: body.package_id, stub: true }));
    res.status(200).json({
      ok: true,
      package_id: body.package_id,
      cascade_kind: 'self',
      mycelium_broadcast: broadcast,
      ...stubEnvelope('wire SUPABASE_SERVICE_ROLE_KEY ; revoke calls content_revoke_cascade RPC + emits mycelium broadcast'),
    });
    return;
  }

  try {
    // Look up package to determine self-vs-moderator + state.
    const fetchRow = await fetch(
      `${supabaseUrl}/rest/v1/content_packages?id=eq.${encodeURIComponent(body.package_id)}&select=author_pubkey,state,revoked_at`,
      {
        headers: {
          apikey: sbServiceKey,
          authorization: `Bearer ${sbServiceKey}`,
        },
      },
    );
    const rows: Array<{ author_pubkey: string; state: string; revoked_at: string | null }> = fetchRow.ok ? await fetchRow.json() : [];
    if (rows.length === 0) {
      res.status(404).json({ ok: false, error: 'package not found', ...envelope() });
      return;
    }
    const pkg = rows[0];
    if (pkg === undefined) {
      res.status(404).json({ ok: false, error: 'package not found', ...envelope() });
      return;
    }
    if (pkg.revoked_at !== null && pkg.revoked_at !== undefined) {
      // Idempotent : already revoked → return success but with `already=true`.
      const broadcast = buildRevokeBroadcast(body.package_id, body.reason, body.who_pubkey);
      res.status(200).json({
        ok: true,
        package_id: body.package_id,
        cascade_kind: pkg.author_pubkey === body.who_pubkey ? 'self' : 'moderator',
        mycelium_broadcast: broadcast,
        already_revoked: true,
        ...envelope(),
      });
      return;
    }
    const isSelf = pkg.author_pubkey === body.who_pubkey;
    const requiredCap = isSelf ? CONTENT_CAP_PUBLISH : CONTENT_CAP_REVOKE_ANY;
    const decision = checkCap(Number.isFinite(capInt) ? capInt : 0, requiredCap, sovereign);
    if (!decision.ok) {
      logEvent(auditEvent('content.publish.revoke', capInt, sovereign, 'denied', { reason: decision.reason, isSelf }));
      res.status(403).json({ ok: false, error: decision.reason ?? 'cap-denied', ...envelope() });
      return;
    }

    // Invoke the cascade RPC (atomic state + audit trail).
    const rpcRes = await fetch(`${supabaseUrl}/rest/v1/rpc/content_revoke_cascade`, {
      method: 'POST',
      headers: {
        apikey: sbServiceKey,
        authorization: `Bearer ${sbServiceKey}`,
        'content-type': 'application/json',
      },
      body: JSON.stringify({
        p_id: body.package_id,
        p_who_pubkey: body.who_pubkey,
        p_reason: body.reason,
      }),
    });
    if (!rpcRes.ok) {
      const txt = await rpcRes.text().catch(() => '');
      res.status(502).json({ ok: false, error: `supabase rpc ${rpcRes.status} ${txt}`, ...envelope() });
      return;
    }

    // Build mycelium broadcast envelope. The downstream chat-sync federation
    // ingests this AS a content.revoke pattern — subscribers re-check anchor.
    const broadcast = buildRevokeBroadcast(body.package_id, body.reason, body.who_pubkey);

    logEvent(auditEvent('content.publish.revoke', capInt, sovereign, 'ok', {
      package_id: body.package_id,
      cascade_kind: isSelf ? 'self' : 'moderator',
    }));

    const env = envelope();
    const okResp: OkResp = {
      ok: true,
      package_id: body.package_id,
      cascade_kind: isSelf ? 'self' : 'moderator',
      mycelium_broadcast: broadcast,
      ts: env.ts,
      served_by: env.served_by,
    };
    res.status(200).json(okResp);
  } catch (e: unknown) {
    logEvent(auditEvent('content.publish.revoke', capInt, sovereign, 'error', { reason: e instanceof Error ? e.message : 'internal' }));
    res.status(500).json({
      ok: false,
      error: e instanceof Error ? e.message : 'internal-error',
      ...envelope(),
    });
  }
}
