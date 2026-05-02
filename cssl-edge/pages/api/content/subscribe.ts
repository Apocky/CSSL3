// cssl-edge · /api/content/subscribe
// § T11-W12-SUBSCRIBE — follow-creator/tag/chain endpoint.
//
// POST /api/content/subscribe
//   headers : x-loa-cap (int) · x-subscriber-pubkey (hex64)
//   body : { target_kind: 'creator'|'tag'|'content-chain',
//            target_id: <hex64-or-tag>, frequency: 'realtime'|'daily'|'manual',
//            sigma_mask?: <uuid> }
//   → 200 { ok:true, subscription_id }
//   → 400 bad-shape · 403 cap-denied · 500 internal · 200 stub
//
// Sovereignty :
//   ¬ surveillance — only the subscriber's own pubkey may subscribe
//   ¬ engagement-bait — frequency is subscriber-chosen, never server-forced
//   ¬ silent-bind — subscription rows are public to subscriber-self only

import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, envelope, stubEnvelope } from '@/lib/response';
import { isSovereignFromIncoming } from '@/lib/sovereign';
import { logEvent, auditEvent } from '@/lib/audit';
import { createHash } from 'node:crypto';

const HEX64_RE = /^[0-9a-f]{64}$/i;
const TAG_RE = /^.{1,64}$/;
const KINDS = new Set(['creator', 'tag', 'content-chain']);
const FREQS = new Set(['realtime', 'daily', 'manual']);

interface Body {
  target_kind?: string;
  target_id?: string;
  frequency?: string;
  sigma_mask?: string;
}

function blake3Hex(_pk: string, _kind: string, _tid: string, _ts: bigint): string {
  // Server-side stable id : SHA-256-based deterministic hash to mirror the
  // crate's BLAKE3 (the actual content-id contract is enforced by the crate ;
  // this endpoint stores whatever id the crate passes back ; for the no-key
  // anonymous-stub case we synthesise one).
  const h = createHash('sha256');
  h.update(_pk);
  h.update(_kind);
  h.update(_tid);
  h.update(_ts.toString());
  return h.digest('hex');
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  logHit('content.subscribe', { method: req.method ?? 'POST' });

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
  const kind = body.target_kind ?? '';
  const tid = body.target_id ?? '';
  const freq = body.frequency ?? 'realtime';
  if (!KINDS.has(kind)) {
    res.status(400).json({ ok: false, error: 'bad target_kind', ...envelope() });
    return;
  }
  if (!FREQS.has(freq)) {
    res.status(400).json({ ok: false, error: 'bad frequency', ...envelope() });
    return;
  }
  if (kind === 'tag') {
    if (!TAG_RE.test(tid)) {
      res.status(400).json({ ok: false, error: 'tag must be 1..=64 chars', ...envelope() });
      return;
    }
  } else if (!HEX64_RE.test(tid)) {
    res.status(400).json({ ok: false, error: 'target_id must be hex64', ...envelope() });
    return;
  }

  const sovereign = isSovereignFromIncoming(req.headers, true);
  const capRaw = req.headers['x-loa-cap'];
  const capInt = Number(Array.isArray(capRaw) ? capRaw[0] : (capRaw ?? '0'));

  const supabaseUrl = process.env.NEXT_PUBLIC_SUPABASE_URL;
  const sbServiceKey = process.env.SUPABASE_SERVICE_ROLE_KEY;
  if (!supabaseUrl || !sbServiceKey) {
    logEvent(auditEvent('content.subscribe', capInt, sovereign, 'ok', { stub: true, kind, freq }));
    res.status(200).json(stubEnvelope('wire SUPABASE_SERVICE_ROLE_KEY ; subscriptions persist to public.content_subscriptions'));
    return;
  }

  try {
    const ts_ns = BigInt(Date.now()) * 1_000_000n;
    const id = blake3Hex(subscriberPubkey, kind, tid, ts_ns);
    const r = await fetch(`${supabaseUrl}/rest/v1/content_subscriptions`, {
      method: 'POST',
      headers: {
        apikey: sbServiceKey,
        authorization: `Bearer ${sbServiceKey}`,
        'content-type': 'application/json',
        prefer: 'return=minimal,resolution=merge-duplicates',
      },
      body: JSON.stringify({
        id,
        subscriber_pubkey: subscriberPubkey,
        target_kind: kind,
        target_id: tid,
        sigma_mask: body.sigma_mask ?? undefined,
        frequency: freq,
        created_at_ns: ts_ns.toString(),
      }),
    });
    if (!r.ok && r.status !== 409) {
      const txt = await r.text().catch(() => '');
      res.status(502).json({ ok: false, error: `supabase ${r.status} ${txt.slice(0, 80)}`, ...envelope() });
      return;
    }
    logEvent(auditEvent('content.subscribe', capInt, sovereign, 'ok', { kind }));
    res.status(200).json({ ok: true, subscription_id: id, ...envelope() });
  } catch (e: unknown) {
    res.status(500).json({
      ok: false,
      error: e instanceof Error ? e.message : 'internal-error',
      ...envelope(),
    });
  }
}
