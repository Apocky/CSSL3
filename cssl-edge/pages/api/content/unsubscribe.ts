// cssl-edge · /api/content/unsubscribe
// § T11-W12-SUBSCRIBE — sovereign-revoke + purge-feed.
//
// POST /api/content/unsubscribe
//   headers : x-subscriber-pubkey (hex64)
//   body : { subscription_id: <hex64> }
//   → 200 { ok:true, purged:<n> } · sovereign-revoke ALWAYS available
//   → 400 bad-shape · 403 cross-pubkey-attempt · 404 not-found · 200 stub
//
// SOVEREIGNTY-INVARIANT : the row's subscriber_pubkey MUST match the
// x-subscriber-pubkey header ; cross-pubkey unsubscribe is rejected.

import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, envelope, stubEnvelope } from '@/lib/response';
import { logEvent, auditEvent } from '@/lib/audit';
import { isSovereignFromIncoming } from '@/lib/sovereign';

const HEX64_RE = /^[0-9a-f]{64}$/i;

interface Body {
  subscription_id?: string;
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  logHit('content.unsubscribe', { method: req.method ?? 'POST' });

  if (req.method !== 'POST') {
    res.status(405).json({ ok: false, error: 'POST only', ...envelope() });
    return;
  }

  const subscriberPubkey = (() => {
    const v = req.headers['x-subscriber-pubkey'];
    return typeof v === 'string' ? v : Array.isArray(v) ? v[0] ?? '' : '';
  })();
  if (!HEX64_RE.test(subscriberPubkey)) {
    res.status(400).json({ ok: false, error: 'x-subscriber-pubkey must be hex64', ...envelope() });
    return;
  }

  const body: Body = (req.body ?? {}) as Body;
  const sid = body.subscription_id ?? '';
  if (!HEX64_RE.test(sid)) {
    res.status(400).json({ ok: false, error: 'subscription_id must be hex64', ...envelope() });
    return;
  }

  const sovereign = isSovereignFromIncoming(req.headers, true);
  const capRaw = req.headers['x-loa-cap'];
  const capInt = Number(Array.isArray(capRaw) ? capRaw[0] : (capRaw ?? '0'));

  const supabaseUrl = process.env.NEXT_PUBLIC_SUPABASE_URL;
  const sbServiceKey = process.env.SUPABASE_SERVICE_ROLE_KEY;
  if (!supabaseUrl || !sbServiceKey) {
    logEvent(auditEvent('content.unsubscribe', capInt, sovereign, 'ok', { stub: true, sid }));
    res.status(200).json(stubEnvelope('wire SUPABASE_SERVICE_ROLE_KEY ; revoke sets revoked_at_ns + cascades feed-delete'));
    return;
  }

  try {
    // Cross-pubkey defense : query the row and verify subscriber_pubkey first.
    const lookupUrl = `${supabaseUrl}/rest/v1/content_subscriptions?id=eq.${encodeURIComponent(sid)}&select=subscriber_pubkey,revoked_at_ns`;
    const lookup = await fetch(lookupUrl, {
      headers: {
        apikey: sbServiceKey,
        authorization: `Bearer ${sbServiceKey}`,
      },
    });
    if (!lookup.ok) {
      res.status(502).json({ ok: false, error: `supabase ${lookup.status}`, ...envelope() });
      return;
    }
    const rows: Array<{ subscriber_pubkey: string; revoked_at_ns: string | null }> = await lookup.json();
    if (rows.length === 0) {
      res.status(404).json({ ok: false, error: 'subscription not found', ...envelope() });
      return;
    }
    const first = rows[0];
    if (!first || first.subscriber_pubkey !== subscriberPubkey) {
      logEvent(auditEvent('content.unsubscribe', capInt, sovereign, 'denied', { reason: 'cross-pubkey' }));
      res.status(403).json({ ok: false, error: 'cross-pubkey unsubscribe denied', ...envelope() });
      return;
    }

    const ts_ns = BigInt(Date.now()) * 1_000_000n;
    // Set revoked_at_ns + delete feed-rows for this subscription.
    const upd = await fetch(`${supabaseUrl}/rest/v1/content_subscriptions?id=eq.${encodeURIComponent(sid)}`, {
      method: 'PATCH',
      headers: {
        apikey: sbServiceKey,
        authorization: `Bearer ${sbServiceKey}`,
        'content-type': 'application/json',
        prefer: 'return=minimal',
      },
      body: JSON.stringify({ revoked_at_ns: ts_ns.toString() }),
    });
    if (!upd.ok) {
      res.status(502).json({ ok: false, error: `supabase upd ${upd.status}`, ...envelope() });
      return;
    }

    // Delete feed-rows tied to this subscription (cascade enforced via FK ON DELETE
    // CASCADE in 0029 ; for the revoked-but-not-deleted case we explicitly DELETE).
    const del = await fetch(`${supabaseUrl}/rest/v1/content_notifications?subscription_id=eq.${encodeURIComponent(sid)}`, {
      method: 'DELETE',
      headers: {
        apikey: sbServiceKey,
        authorization: `Bearer ${sbServiceKey}`,
        prefer: 'return=representation,count=exact',
      },
    });
    let purged = 0;
    if (del.ok) {
      const cr = del.headers.get('content-range');
      // content-range looks like "0-9/10" → final 10
      if (cr) {
        const m = cr.match(/\/(\d+)$/);
        if (m) purged = Number(m[1]);
      }
    }

    logEvent(auditEvent('content.unsubscribe', capInt, sovereign, 'ok', { purged }));
    res.status(200).json({ ok: true, purged, ...envelope() });
  } catch (e: unknown) {
    res.status(500).json({
      ok: false,
      error: e instanceof Error ? e.message : 'internal-error',
      ...envelope(),
    });
  }
}
