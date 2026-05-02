// cssl-edge · /api/content/publish/complete
// § T11-W12-UGC-PUBLISH — finalizes publish · verifies-Ed25519-sig ·
// creates Σ-Chain anchor · updates content_packages row · returns publish-id.
//
// POST /api/content/publish/complete
//   headers : x-loa-cap (int) · x-author-pubkey (must match)
//   body    : PublishCompleteRequest
//   → 200 { ok:true, package_id, sigma_chain_anchor, state:'published', ts }
//   → 403 cap-denied / pubkey-mismatch
//   → 422 sig-verify-failed / chunk-count-mismatch
//   → 200 { stub:true } when Supabase env-vars absent.
//
// Sovereignty :
//   ¬ unauthorized-publish · cap REQUIRED + author_pubkey-match
//   Σ-Chain anchor computed deterministically from sha256+pubkey+ts_ns
//   ¬ silent-revoke · revocations carry their own audit row in content_packages

import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, envelope, stubEnvelope } from '@/lib/response';
import { isSovereignFromIncoming } from '@/lib/sovereign';
import { CONTENT_CAP_PUBLISH, checkCap } from '@/lib/cap';
import { logEvent, auditEvent } from '@/lib/audit';
import {
  validateComplete,
  type PublishCompleteRequest,
  HEX64_RE,
  canonicalSignMessage,
  verifyEd25519,
  makeSigmaAnchor,
} from '@/lib/content-publish';

interface OkResp {
  ok: true;
  package_id: string;
  sigma_chain_anchor: string;
  state: 'published';
  finalized_at_ns: number;
  ts: string;
  served_by: string;
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  logHit('content.publish.complete', { method: req.method ?? 'POST' });

  if (req.method !== 'POST') {
    res.status(405).json({ ok: false, error: 'POST only', ...envelope() });
    return;
  }

  const body = (req.body ?? {}) as PublishCompleteRequest;
  const v = validateComplete(body);
  if (!v.ok) {
    logEvent(auditEvent('content.publish.complete', 0, false, 'denied', { reason: v.reason }));
    res.status(400).json({ ok: false, error: v.reason, ...envelope() });
    return;
  }

  // Cap-gate.
  const capRaw = req.headers['x-loa-cap'];
  const capInt = Number(Array.isArray(capRaw) ? capRaw[0] : (capRaw ?? '0'));
  const sovereign = isSovereignFromIncoming(req.headers, true);
  const decision = checkCap(Number.isFinite(capInt) ? capInt : 0, CONTENT_CAP_PUBLISH, sovereign);
  if (!decision.ok) {
    logEvent(auditEvent('content.publish.complete', capInt, sovereign, 'denied', { reason: decision.reason }));
    res.status(403).json({ ok: false, error: decision.reason ?? 'cap-denied', ...envelope() });
    return;
  }

  // Author-pubkey header for cross-check.
  const authorHeader = (() => {
    const x = req.headers['x-author-pubkey'];
    return typeof x === 'string' ? x : Array.isArray(x) ? x[0] ?? '' : '';
  })();
  if (!HEX64_RE.test(authorHeader)) {
    res.status(400).json({ ok: false, error: 'x-author-pubkey must be 64-hex', ...envelope() });
    return;
  }

  const supabaseUrl = process.env.NEXT_PUBLIC_SUPABASE_URL;
  const sbServiceKey = process.env.SUPABASE_SERVICE_ROLE_KEY;
  const tsNs = Date.now() * 1_000_000;

  if (!supabaseUrl || !sbServiceKey) {
    // Stub-mode : compute the anchor + verify shape, return success.
    const anchor = await makeSigmaAnchor(body.sha256, authorHeader, tsNs);
    res.status(200).json({
      ok: true,
      package_id: body.package_id,
      sigma_chain_anchor: anchor,
      state: 'published',
      finalized_at_ns: tsNs,
      ...stubEnvelope('wire SUPABASE_SERVICE_ROLE_KEY ; complete invokes content_publish_finalize RPC + emits Σ-Chain anchor + mycelium broadcast'),
    });
    return;
  }

  try {
    // Pull the package row to cross-check author_pubkey + kind/version.
    const fetchRow = await fetch(
      `${supabaseUrl}/rest/v1/content_packages?id=eq.${encodeURIComponent(body.package_id)}&select=*`,
      {
        headers: {
          apikey: sbServiceKey,
          authorization: `Bearer ${sbServiceKey}`,
        },
      },
    );
    const rows: Array<{
      id: string;
      author_pubkey: string;
      kind: string;
      version: string;
      state: string;
    }> = fetchRow.ok ? await fetchRow.json() : [];
    if (rows.length === 0) {
      res.status(404).json({ ok: false, error: 'package not found', ...envelope() });
      return;
    }
    const pkg = rows[0];
    if (pkg === undefined) {
      res.status(404).json({ ok: false, error: 'package not found', ...envelope() });
      return;
    }
    if (pkg.author_pubkey !== authorHeader) {
      logEvent(auditEvent('content.publish.complete', capInt, sovereign, 'denied', { reason: 'pubkey-mismatch' }));
      res.status(403).json({ ok: false, error: 'author_pubkey mismatch', ...envelope() });
      return;
    }
    if (pkg.state === 'published' || pkg.state === 'revoked' || pkg.state === 'rejected') {
      res.status(409).json({ ok: false, error: `package state ${pkg.state} cannot be re-finalized`, ...envelope() });
      return;
    }

    // Verify chunk count present.
    const chunkCountRes = await fetch(
      `${supabaseUrl}/rest/v1/content_chunks_upload?package_id=eq.${encodeURIComponent(body.package_id)}&select=seq`,
      {
        headers: {
          apikey: sbServiceKey,
          authorization: `Bearer ${sbServiceKey}`,
          prefer: 'count=exact',
        },
      },
    );
    const chunks: Array<{ seq: number }> = chunkCountRes.ok ? await chunkCountRes.json() : [];
    if (chunks.length !== body.chunk_count) {
      logEvent(auditEvent('content.publish.complete', capInt, sovereign, 'denied', { reason: 'chunk-count-mismatch', expected: body.chunk_count, got: chunks.length }));
      res.status(422).json({
        ok: false,
        error: `chunk-count mismatch : claimed ${body.chunk_count}, db has ${chunks.length}`,
        ...envelope(),
      });
      return;
    }
    // Check seqs are 0..chunk_count-1 contiguous.
    const seen = new Set<number>(chunks.map((c) => c.seq));
    for (let i = 0; i < body.chunk_count; i++) {
      if (!seen.has(i)) {
        res.status(422).json({ ok: false, error: `missing seq ${i}`, ...envelope() });
        return;
      }
    }

    // Verify Ed25519 signature over the canonical metadata.
    const msg = canonicalSignMessage(
      pkg.author_pubkey,
      pkg.kind as Parameters<typeof canonicalSignMessage>[1],
      pkg.version,
      body.sha256,
      body.size_bytes,
      body.chunk_count,
    );
    const verified = await verifyEd25519(pkg.author_pubkey, msg, body.signature_ed25519);
    if (!verified) {
      logEvent(auditEvent('content.publish.complete', capInt, sovereign, 'denied', { reason: 'sig-verify-failed' }));
      res.status(422).json({ ok: false, error: 'Ed25519 signature verify failed', ...envelope() });
      return;
    }

    // Compute Σ-Chain anchor.
    const anchor = await makeSigmaAnchor(body.sha256, pkg.author_pubkey, tsNs);

    // Atomic finalize via RPC.
    const finRes = await fetch(`${supabaseUrl}/rest/v1/rpc/content_publish_finalize`, {
      method: 'POST',
      headers: {
        apikey: sbServiceKey,
        authorization: `Bearer ${sbServiceKey}`,
        'content-type': 'application/json',
      },
      body: JSON.stringify({
        p_id: body.package_id,
        p_sha256: body.sha256,
        p_signature: body.signature_ed25519,
        p_anchor: anchor,
        p_size_bytes: body.size_bytes,
        p_chunk_count: body.chunk_count,
      }),
    });
    if (!finRes.ok) {
      const txt = await finRes.text().catch(() => '');
      res.status(502).json({ ok: false, error: `supabase rpc ${finRes.status} ${txt}`, ...envelope() });
      return;
    }
    const finOk = await finRes.json();
    if (finOk !== true) {
      res.status(409).json({ ok: false, error: 'finalize-rejected (state-transition)', ...envelope() });
      return;
    }

    logEvent(auditEvent('content.publish.complete', capInt, sovereign, 'ok', {
      package_id: body.package_id,
      sigma_chain_anchor: anchor,
    }));

    const env = envelope();
    const okResp: OkResp = {
      ok: true,
      package_id: body.package_id,
      sigma_chain_anchor: anchor,
      state: 'published',
      finalized_at_ns: tsNs,
      ts: env.ts,
      served_by: env.served_by,
    };
    res.status(200).json(okResp);
  } catch (e: unknown) {
    logEvent(auditEvent('content.publish.complete', capInt, sovereign, 'error', { reason: e instanceof Error ? e.message : 'internal' }));
    res.status(500).json({
      ok: false,
      error: e instanceof Error ? e.message : 'internal-error',
      ...envelope(),
    });
  }
}
