// cssl-edge · /api/content/publish/status/:id
// § T11-W12-UGC-PUBLISH — reports upload-progress + verification-state ·
// Σ-mask-gated.
//
// GET /api/content/publish/status/:id
//   headers : x-loa-cap (int) · x-author-pubkey (must match unless sovereign)
//   → 200 { ok:true, package_id, state, chunks_uploaded, chunk_count_expected,
//           sigma_chain_anchor?, finalized_at?, revoked_at? }
//   → 403 cap-denied / pubkey-mismatch
//   → 404 package not found
//   → 200 { stub:true } when Supabase env-vars absent.
//
// Σ-mask gate :
//   - If `state == 'published'` AND `revoked_at IS NULL` → publicly visible.
//   - Otherwise → ONLY author OR sovereign can read (CONTENT_CAP_PUBLISH cap).
//
// Sovereignty :
//   ¬ surveillance · zero PII in response or logs

import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, envelope, stubEnvelope } from '@/lib/response';
import { isSovereignFromIncoming } from '@/lib/sovereign';
import { CONTENT_CAP_PUBLISH, checkCap } from '@/lib/cap';
import { logEvent, auditEvent } from '@/lib/audit';
import { UUID_RE, HEX64_RE } from '@/lib/content-publish';

interface PackageRow {
  id: string;
  author_pubkey: string;
  kind: string;
  version: string;
  state: string;
  size_bytes: number;
  chunk_count: number;
  sha256: string | null;
  sigma_chain_anchor: string | null;
  finalized_at: string | null;
  revoked_at: string | null;
  revoked_reason: string | null;
  license: string;
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  logHit('content.publish.status', { method: req.method ?? 'GET' });

  if (req.method !== 'GET') {
    res.status(405).json({ ok: false, error: 'GET only', ...envelope() });
    return;
  }

  const id = typeof req.query.id === 'string' ? req.query.id : '';
  if (!UUID_RE.test(id)) {
    res.status(400).json({ ok: false, error: 'id must be uuid', ...envelope() });
    return;
  }

  const capRaw = req.headers['x-loa-cap'];
  const capInt = Number(Array.isArray(capRaw) ? capRaw[0] : (capRaw ?? '0'));
  const sovereign = isSovereignFromIncoming(req.headers, true);
  const headerPubkey = (() => {
    const x = req.headers['x-author-pubkey'];
    return typeof x === 'string' ? x : Array.isArray(x) ? x[0] ?? '' : '';
  })();

  const supabaseUrl = process.env.NEXT_PUBLIC_SUPABASE_URL;
  const sbServiceKey = process.env.SUPABASE_SERVICE_ROLE_KEY;
  if (!supabaseUrl || !sbServiceKey) {
    res.status(200).json({
      ok: true,
      package_id: id,
      state: 'init',
      chunks_uploaded: 0,
      chunk_count_expected: 0,
      ...stubEnvelope('wire SUPABASE_SERVICE_ROLE_KEY ; status reads content_packages + counts content_chunks_upload'),
    });
    return;
  }

  try {
    const r = await fetch(
      `${supabaseUrl}/rest/v1/content_packages?id=eq.${encodeURIComponent(id)}&select=*`,
      {
        headers: {
          apikey: sbServiceKey,
          authorization: `Bearer ${sbServiceKey}`,
        },
      },
    );
    const rows: PackageRow[] = r.ok ? await r.json() : [];
    if (rows.length === 0) {
      res.status(404).json({ ok: false, error: 'package not found', ...envelope() });
      return;
    }
    const pkg = rows[0];
    if (pkg === undefined) {
      res.status(404).json({ ok: false, error: 'package not found', ...envelope() });
      return;
    }

    // Σ-mask gate : public-published is visible to all; private-state requires
    // author-match or sovereign.
    const isPublic = pkg.state === 'published' && pkg.revoked_at === null;
    if (!isPublic) {
      const decision = checkCap(Number.isFinite(capInt) ? capInt : 0, CONTENT_CAP_PUBLISH, sovereign);
      const isAuthor = HEX64_RE.test(headerPubkey) && headerPubkey === pkg.author_pubkey;
      if (!decision.ok && !isAuthor) {
        logEvent(auditEvent('content.publish.status', capInt, sovereign, 'denied', { reason: 'private-state' }));
        res.status(403).json({ ok: false, error: 'private-state requires cap or author-match', ...envelope() });
        return;
      }
    }

    // Count uploaded chunks.
    let chunksUploaded = 0;
    if (pkg.state === 'init' || pkg.state === 'uploading' || pkg.state === 'verifying') {
      const cr = await fetch(
        `${supabaseUrl}/rest/v1/content_chunks_upload?package_id=eq.${encodeURIComponent(id)}&select=seq`,
        {
          headers: {
            apikey: sbServiceKey,
            authorization: `Bearer ${sbServiceKey}`,
          },
        },
      );
      const cs: Array<{ seq: number }> = cr.ok ? await cr.json() : [];
      chunksUploaded = cs.length;
    } else {
      chunksUploaded = pkg.chunk_count;
    }

    logEvent(auditEvent('content.publish.status', capInt, sovereign, 'ok', { package_id: id, state: pkg.state }));

    const env = envelope();
    res.status(200).json({
      ok: true,
      package_id: pkg.id,
      author_pubkey: pkg.author_pubkey,
      kind: pkg.kind,
      version: pkg.version,
      state: pkg.state,
      chunks_uploaded: chunksUploaded,
      chunk_count_expected: pkg.chunk_count,
      sigma_chain_anchor: pkg.sigma_chain_anchor,
      sha256: pkg.sha256,
      finalized_at: pkg.finalized_at,
      revoked_at: pkg.revoked_at,
      revoked_reason: pkg.revoked_reason,
      license: pkg.license,
      ts: env.ts,
      served_by: env.served_by,
    });
  } catch (e: unknown) {
    logEvent(auditEvent('content.publish.status', capInt, sovereign, 'error', { reason: e instanceof Error ? e.message : 'internal' }));
    res.status(500).json({
      ok: false,
      error: e instanceof Error ? e.message : 'internal-error',
      ...envelope(),
    });
  }
}
