// cssl-edge · /api/content/publish/init
// § T11-W12-UGC-PUBLISH — reserves a content-id, validates the publish
// request shape, returns the chunked-upload URL.
//
// POST /api/content/publish/init
//   headers : x-loa-cap (int) · x-loa-sovereign-cap (optional bypass)
//   body    : PublishInitRequest
//   → 200 { ok:true, package_id, upload_url_template, chunk_count, sigma_mask }
//   → 403  cap-denied
//   → 400  validation-failed
//   → 200 { stub:true, todo } when Supabase env-vars absent
//
// Cap-bit : CONTENT_CAP_PUBLISH (0x800) REQUIRED. Sovereign-bypass honoured.
//
// Sovereignty :
//   ¬ unauthorized-publish · cap REQUIRED
//   ¬ pay-for-publish      · gift-economy axiom DB-enforced
//   ¬ surveillance         · author_pubkey is sovereign-revocable identifier

import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, envelope, stubEnvelope } from '@/lib/response';
import { isSovereignFromIncoming } from '@/lib/sovereign';
import { CONTENT_CAP_PUBLISH, checkCap } from '@/lib/cap';
import { logEvent, auditEvent } from '@/lib/audit';
import {
  validateInit,
  type PublishInitRequest,
  CONTENT_KINDS,
  CONTENT_LICENSES,
} from '@/lib/content-publish';

interface OkResp {
  ok: true;
  package_id: string;
  upload_url_template: string;
  chunk_count: number;
  state: string;
  ts: string;
  served_by: string;
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  logHit('content.publish.init', { method: req.method ?? 'POST' });

  if (req.method !== 'POST') {
    res.status(405).json({ ok: false, error: 'POST only', ...envelope() });
    return;
  }

  const body = (req.body ?? {}) as PublishInitRequest;
  const v = validateInit(body);
  if (!v.ok) {
    logEvent(auditEvent('content.publish.init', 0, false, 'denied', { reason: v.reason }));
    res.status(400).json({ ok: false, error: v.reason, ...envelope() });
    return;
  }

  // Cap-gate.
  const capRaw = req.headers['x-loa-cap'];
  const capInt = Number(Array.isArray(capRaw) ? capRaw[0] : (capRaw ?? '0'));
  const sovereign = isSovereignFromIncoming(req.headers, true);
  const decision = checkCap(Number.isFinite(capInt) ? capInt : 0, CONTENT_CAP_PUBLISH, sovereign);
  if (!decision.ok) {
    logEvent(auditEvent('content.publish.init', capInt, sovereign, 'denied', { reason: decision.reason }));
    res.status(403).json({ ok: false, error: decision.reason ?? 'cap-denied', ...envelope() });
    return;
  }

  // Stub-mode : no Supabase wired → synth a stable package_id and return.
  const supabaseUrl = process.env.NEXT_PUBLIC_SUPABASE_URL;
  const sbServiceKey = process.env.SUPABASE_SERVICE_ROLE_KEY;
  if (!supabaseUrl || !sbServiceKey) {
    const stubId = randomUuid();
    res.status(200).json({
      ok: true,
      package_id: stubId,
      upload_url_template: `/api/content/publish/chunk?id=${stubId}&seq=<seq>`,
      chunk_count: body.chunk_count,
      state: 'init',
      ...stubEnvelope('wire NEXT_PUBLIC_SUPABASE_URL + SUPABASE_SERVICE_ROLE_KEY ; init writes a row to content_packages with state=init'),
    });
    return;
  }

  try {
    // Insert content_packages row in state=init.
    const insertBody: Record<string, unknown> = {
      author_pubkey: body.author_pubkey,
      kind: body.kind,
      version: body.version,
      license: body.license,
      gift_economy_only: true,  // axiom-enforced (DB CHECK also enforces)
      state: 'init',
    };
    if (typeof body.title === 'string') insertBody['title'] = body.title;
    if (typeof body.description === 'string') insertBody['description'] = body.description;

    const r = await fetch(`${supabaseUrl}/rest/v1/content_packages`, {
      method: 'POST',
      headers: {
        apikey: sbServiceKey,
        authorization: `Bearer ${sbServiceKey}`,
        'content-type': 'application/json',
        prefer: 'return=representation',
      },
      body: JSON.stringify(insertBody),
    });
    if (!r.ok) {
      const txt = await r.text().catch(() => '');
      logEvent(auditEvent('content.publish.init', capInt, sovereign, 'error', { reason: `supabase ${r.status}` }));
      res.status(502).json({ ok: false, error: `supabase ${r.status} ${txt}`, ...envelope() });
      return;
    }
    const rows: Array<{ id: string }> = await r.json();
    if (rows.length === 0 || typeof rows[0]?.id !== 'string') {
      res.status(502).json({ ok: false, error: 'supabase returned empty', ...envelope() });
      return;
    }
    const pkgId = rows[0].id;

    // Insert deps (cycle-checked via helper).
    if (body.dependencies !== undefined) {
      for (const d of body.dependencies) {
        // cycle-check
        const cyc = await fetch(`${supabaseUrl}/rest/v1/rpc/content_dep_cycle_check`, {
          method: 'POST',
          headers: {
            apikey: sbServiceKey,
            authorization: `Bearer ${sbServiceKey}`,
            'content-type': 'application/json',
          },
          body: JSON.stringify({ p_id: pkgId, p_depends_on_id: d.id }),
        });
        const cycOk = cyc.ok ? await cyc.json() : false;
        if (cycOk !== true) {
          logEvent(auditEvent('content.publish.init', capInt, sovereign, 'denied', { reason: 'dep-cycle' }));
          res.status(409).json({ ok: false, error: `dep ${d.id} forms cycle`, ...envelope() });
          return;
        }
        await fetch(`${supabaseUrl}/rest/v1/content_dependencies`, {
          method: 'POST',
          headers: {
            apikey: sbServiceKey,
            authorization: `Bearer ${sbServiceKey}`,
            'content-type': 'application/json',
            prefer: 'return=minimal',
          },
          body: JSON.stringify({
            package_id: pkgId,
            depends_on_id: d.id,
            depends_on_version: d.version,
          }),
        });
      }
    }

    // Insert remix-of edge if specified (cycle-checked).
    if (typeof body.remix_of === 'string') {
      const cyc = await fetch(`${supabaseUrl}/rest/v1/rpc/content_remix_cycle_check`, {
        method: 'POST',
        headers: {
          apikey: sbServiceKey,
          authorization: `Bearer ${sbServiceKey}`,
          'content-type': 'application/json',
        },
        body: JSON.stringify({ p_id: pkgId, p_remix_of_id: body.remix_of }),
      });
      const cycOk = cyc.ok ? await cyc.json() : false;
      if (cycOk !== true) {
        logEvent(auditEvent('content.publish.init', capInt, sovereign, 'denied', { reason: 'remix-cycle' }));
        res.status(409).json({ ok: false, error: 'remix-of forms cycle', ...envelope() });
        return;
      }
      await fetch(`${supabaseUrl}/rest/v1/content_remix_chain`, {
        method: 'POST',
        headers: {
          apikey: sbServiceKey,
          authorization: `Bearer ${sbServiceKey}`,
          'content-type': 'application/json',
          prefer: 'return=minimal',
        },
        body: JSON.stringify({
          package_id: pkgId,
          remix_of_id: body.remix_of,
          attribution_immutable: true,
        }),
      });
    }

    logEvent(auditEvent('content.publish.init', capInt, sovereign, 'ok', { package_id: pkgId }));
    const env = envelope();
    const okResp: OkResp = {
      ok: true,
      package_id: pkgId,
      upload_url_template: `/api/content/publish/chunk?id=${pkgId}&seq=<seq>`,
      chunk_count: body.chunk_count,
      state: 'init',
      ts: env.ts,
      served_by: env.served_by,
    };
    res.status(200).json(okResp);
  } catch (e: unknown) {
    logEvent(auditEvent('content.publish.init', capInt, sovereign, 'error', { reason: e instanceof Error ? e.message : 'internal' }));
    res.status(500).json({
      ok: false,
      error: e instanceof Error ? e.message : 'internal-error',
      ...envelope(),
    });
  }
}

// Tiny UUIDv4 generator that does not need crypto.randomUUID (older runtimes).
function randomUuid(): string {
  const b = new Uint8Array(16);
  if (typeof globalThis.crypto?.getRandomValues === 'function') {
    globalThis.crypto.getRandomValues(b);
  } else {
    for (let i = 0; i < 16; i++) b[i] = Math.floor(Math.random() * 256);
  }
  // RFC 4122 v4
  const b6 = b[6] ?? 0;
  b[6] = (b6 & 0x0f) | 0x40;
  const b8 = b[8] ?? 0;
  b[8] = (b8 & 0x3f) | 0x80;
  const h = (n: number) => n.toString(16).padStart(2, '0');
  return `${h(b[0]??0)}${h(b[1]??0)}${h(b[2]??0)}${h(b[3]??0)}-${h(b[4]??0)}${h(b[5]??0)}-${h(b[6]??0)}${h(b[7]??0)}-${h(b[8]??0)}${h(b[9]??0)}-${h(b[10]??0)}${h(b[11]??0)}${h(b[12]??0)}${h(b[13]??0)}${h(b[14]??0)}${h(b[15]??0)}`;
}

// Re-export the validation helpers as a compile-time use to satisfy
// import-tracker (some bundlers warn on unused imports). The actual
// guarantees come from validateInit + the cap-table constants.
const _used: ReadonlyArray<unknown> = [CONTENT_KINDS, CONTENT_LICENSES];
void _used;
