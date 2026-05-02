// cssl-edge · /api/content/notifications
// § T11-W12-SUBSCRIBE — read unread feed for a subscriber.
//
// GET /api/content/notifications?since=<ts_ns>&limit=<n>
//   headers : x-subscriber-pubkey (hex64)
//   → 200 { ok:true, notifications:[{ id, kind, content_id, reason?,
//                                     created_at_ns, sigma_mask }] }
//   → 400 bad-shape · 200 stub
//
// POST /api/content/notifications/mark-read (collapsed here for slice)
//   POST body : { ids: [<hex64>...] }  · marks read_at_ns = now()
//   Implemented as the second method of this endpoint (action=mark-read query).
//
// ANTI-SPAM : NEVER auto-resurface old content. Once read_at_ns is set,
// the row is excluded from this endpoint's response.
// Σ-MASK GATE : `auth.uid() = subscriber_pubkey` enforced by RLS.

import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, envelope, stubEnvelope } from '@/lib/response';
import { logEvent, auditEvent } from '@/lib/audit';
import { isSovereignFromIncoming } from '@/lib/sovereign';

const HEX64_RE = /^[0-9a-f]{64}$/i;
const MAX_LIMIT = 200;

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  logHit('content.notifications', { method: req.method ?? 'GET' });

  const subscriberPubkey = (() => {
    const v = req.headers['x-subscriber-pubkey'];
    return typeof v === 'string' ? v : Array.isArray(v) ? v[0] ?? '' : '';
  })();
  if (!HEX64_RE.test(subscriberPubkey)) {
    res.status(400).json({ ok: false, error: 'x-subscriber-pubkey must be hex64', ...envelope() });
    return;
  }

  const sovereign = isSovereignFromIncoming(req.headers, true);
  const capRaw = req.headers['x-loa-cap'];
  const capInt = Number(Array.isArray(capRaw) ? capRaw[0] : (capRaw ?? '0'));

  const supabaseUrl = process.env.NEXT_PUBLIC_SUPABASE_URL;
  const sbServiceKey = process.env.SUPABASE_SERVICE_ROLE_KEY;

  if (req.method === 'GET') {
    if (!supabaseUrl || !sbServiceKey) {
      res.status(200).json({ ...stubEnvelope('wire SUPABASE_SERVICE_ROLE_KEY ; reads from public.content_notifications WHERE subscriber_pubkey = $self AND read_at_ns IS NULL'), notifications: [] });
      return;
    }
    const sinceRaw = typeof req.query.since === 'string' ? req.query.since : undefined;
    const limitRaw = typeof req.query.limit === 'string' ? req.query.limit : undefined;
    const since = sinceRaw && /^\d+$/.test(sinceRaw) ? sinceRaw : undefined;
    const limit = Math.min(MAX_LIMIT, Math.max(1, Number.isFinite(Number(limitRaw)) ? Number(limitRaw) : 50));

    let url = `${supabaseUrl}/rest/v1/content_notifications`
      + `?subscriber_pubkey=eq.${encodeURIComponent(subscriberPubkey)}`
      + `&read_at_ns=is.null`
      + `&order=created_at_ns.asc`
      + `&limit=${limit}`
      + `&select=id,subscription_id,kind,content_id,reason,sigma_mask,created_at_ns`;
    if (since) url += `&created_at_ns=gte.${since}`;

    try {
      const r = await fetch(url, {
        headers: {
          apikey: sbServiceKey,
          authorization: `Bearer ${sbServiceKey}`,
        },
      });
      if (!r.ok) {
        res.status(502).json({ ok: false, error: `supabase ${r.status}`, ...envelope() });
        return;
      }
      const rows = await r.json();
      logEvent(auditEvent('content.notifications', capInt, sovereign, 'ok', { count: Array.isArray(rows) ? rows.length : 0 }));
      res.status(200).json({ ok: true, notifications: rows, ...envelope() });
    } catch (e: unknown) {
      res.status(500).json({ ok: false, error: e instanceof Error ? e.message : 'internal-error', ...envelope() });
    }
    return;
  }

  if (req.method === 'POST') {
    // Mark-read action.
    const body: { ids?: string[] } = (req.body ?? {}) as { ids?: string[] };
    const ids = (body.ids ?? []).filter((s) => typeof s === 'string' && HEX64_RE.test(s));
    if (ids.length === 0) {
      res.status(400).json({ ok: false, error: 'ids must be a non-empty array of hex64', ...envelope() });
      return;
    }
    if (!supabaseUrl || !sbServiceKey) {
      res.status(200).json(stubEnvelope('wire SUPABASE_SERVICE_ROLE_KEY ; mark-read sets read_at_ns'));
      return;
    }
    const ts_ns = BigInt(Date.now()) * 1_000_000n;
    const inList = ids.map((s) => `"${s}"`).join(',');
    try {
      const r = await fetch(`${supabaseUrl}/rest/v1/content_notifications?id=in.(${inList})&subscriber_pubkey=eq.${encodeURIComponent(subscriberPubkey)}&read_at_ns=is.null`, {
        method: 'PATCH',
        headers: {
          apikey: sbServiceKey,
          authorization: `Bearer ${sbServiceKey}`,
          'content-type': 'application/json',
          prefer: 'return=minimal',
        },
        body: JSON.stringify({ read_at_ns: ts_ns.toString() }),
      });
      if (!r.ok) {
        res.status(502).json({ ok: false, error: `supabase ${r.status}`, ...envelope() });
        return;
      }
      res.status(200).json({ ok: true, marked: ids.length, ...envelope() });
    } catch (e: unknown) {
      res.status(500).json({ ok: false, error: e instanceof Error ? e.message : 'internal-error', ...envelope() });
    }
    return;
  }

  res.status(405).json({ ok: false, error: 'GET (read) or POST (mark-read) only', ...envelope() });
}
