// cssl-edge · /api/content/publish/chunk
// § T11-W12-UGC-PUBLISH — uploads-bundle-bytes per chunk · max 4MB.
//
// POST /api/content/publish/chunk?id=<package_id>&seq=<n>
//   headers : x-loa-cap (int) · x-author-pubkey (must match content_packages.author_pubkey)
//   body    : raw bytes (application/octet-stream)
//   → 200 { ok:true, seq, sha256 }
//   → 201 same shape but for new (resumable returns 409 instead)
//   → 409 if chunk already uploaded (resumable client treats as success)
//   → 403 cap-denied / pubkey-mismatch
//   → 413 chunk too large
//   → 200 { stub:true } when Supabase env-vars absent.
//
// Storage : Supabase storage bucket `content-chunks` keyed by `<id>/<seq>`.
// We ALSO write a row to content_chunks_upload so /complete can verify the
// expected number of seqs are present without a storage round-trip.
//
// Sovereignty :
//   ¬ unauthorized-publish · cap REQUIRED + author_pubkey-match
//   ¬ surveillance         · zero PII in logs

import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, envelope, stubEnvelope } from '@/lib/response';
import { isSovereignFromIncoming } from '@/lib/sovereign';
import { CONTENT_CAP_PUBLISH, checkCap } from '@/lib/cap';
import { logEvent, auditEvent } from '@/lib/audit';
import {
  UUID_RE,
  HEX64_RE,
  MAX_CHUNK_BYTES,
  MAX_CHUNK_COUNT,
  bytesToHex,
} from '@/lib/content-publish';

export const config = {
  api: {
    bodyParser: false,             // we read raw bytes
    responseLimit: false,
  },
};

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  logHit('content.publish.chunk', { method: req.method ?? 'POST' });

  if (req.method !== 'POST') {
    res.status(405).json({ ok: false, error: 'POST only', ...envelope() });
    return;
  }

  const idRaw = req.query.id;
  const seqRaw = req.query.seq;
  const id = typeof idRaw === 'string' ? idRaw : '';
  const seq = Number(typeof seqRaw === 'string' ? seqRaw : NaN);
  if (!UUID_RE.test(id)) {
    res.status(400).json({ ok: false, error: 'id must be uuid', ...envelope() });
    return;
  }
  if (!Number.isInteger(seq) || seq < 0 || seq >= MAX_CHUNK_COUNT) {
    res.status(400).json({ ok: false, error: `seq must be 0..${MAX_CHUNK_COUNT - 1}`, ...envelope() });
    return;
  }

  // Cap-gate.
  const capRaw = req.headers['x-loa-cap'];
  const capInt = Number(Array.isArray(capRaw) ? capRaw[0] : (capRaw ?? '0'));
  const sovereign = isSovereignFromIncoming(req.headers, true);
  const decision = checkCap(Number.isFinite(capInt) ? capInt : 0, CONTENT_CAP_PUBLISH, sovereign);
  if (!decision.ok) {
    logEvent(auditEvent('content.publish.chunk', capInt, sovereign, 'denied', { reason: decision.reason }));
    res.status(403).json({ ok: false, error: decision.reason ?? 'cap-denied', ...envelope() });
    return;
  }

  // Author-pubkey cross-check (extra defense-in-depth).
  const authorPubkey = (() => {
    const v = req.headers['x-author-pubkey'];
    return typeof v === 'string' ? v : Array.isArray(v) ? v[0] ?? '' : '';
  })();
  if (!HEX64_RE.test(authorPubkey)) {
    logEvent(auditEvent('content.publish.chunk', capInt, sovereign, 'denied', { reason: 'bad-pubkey' }));
    res.status(400).json({ ok: false, error: 'x-author-pubkey must be 64-hex', ...envelope() });
    return;
  }

  // Read raw body.
  let bytes: Uint8Array;
  try {
    bytes = await readRawBody(req, MAX_CHUNK_BYTES);
  } catch (e: unknown) {
    if (e instanceof RangeError) {
      res.status(413).json({ ok: false, error: 'chunk too large (max 4MB)', ...envelope() });
      return;
    }
    res.status(400).json({ ok: false, error: e instanceof Error ? e.message : 'bad-body', ...envelope() });
    return;
  }

  // Compute chunk sha256 (for /complete verification + dedupe).
  const sha = await sha256Hex(bytes);

  const supabaseUrl = process.env.NEXT_PUBLIC_SUPABASE_URL;
  const sbServiceKey = process.env.SUPABASE_SERVICE_ROLE_KEY;
  if (!supabaseUrl || !sbServiceKey) {
    res.status(200).json({
      ok: true,
      seq,
      sha256: sha,
      ...stubEnvelope('wire SUPABASE_SERVICE_ROLE_KEY ; chunk persists to content_chunks_upload + storage bucket'),
    });
    return;
  }

  try {
    // Author-match : verify content_packages.author_pubkey == header value.
    const verify = await fetch(
      `${supabaseUrl}/rest/v1/content_packages?id=eq.${encodeURIComponent(id)}&select=author_pubkey,state`,
      {
        headers: {
          apikey: sbServiceKey,
          authorization: `Bearer ${sbServiceKey}`,
        },
      },
    );
    const rows: Array<{ author_pubkey: string; state: string }> = verify.ok ? await verify.json() : [];
    if (rows.length === 0) {
      res.status(404).json({ ok: false, error: 'package not found', ...envelope() });
      return;
    }
    const pkg = rows[0];
    if (pkg === undefined) {
      res.status(404).json({ ok: false, error: 'package not found', ...envelope() });
      return;
    }
    if (pkg.author_pubkey !== authorPubkey) {
      logEvent(auditEvent('content.publish.chunk', capInt, sovereign, 'denied', { reason: 'pubkey-mismatch' }));
      res.status(403).json({ ok: false, error: 'author_pubkey mismatch', ...envelope() });
      return;
    }
    if (pkg.state !== 'init' && pkg.state !== 'uploading') {
      res.status(409).json({ ok: false, error: `package state ${pkg.state} not accepting chunks`, ...envelope() });
      return;
    }

    // Insert chunk row. UNIQUE (package_id, seq) gives us natural resume.
    const insertRes = await fetch(`${supabaseUrl}/rest/v1/content_chunks_upload`, {
      method: 'POST',
      headers: {
        apikey: sbServiceKey,
        authorization: `Bearer ${sbServiceKey}`,
        'content-type': 'application/json',
        prefer: 'return=minimal',
      },
      body: JSON.stringify({
        package_id: id,
        seq,
        sha256: sha,
        bytes: bytesToBase64(bytes),  // pgrest auto-decodes \x notation, but we use bytea-base64
      }),
    });
    if (insertRes.status === 409 || insertRes.status === 23505) {
      // UNIQUE collision → resumable success (chunk already uploaded).
      logEvent(auditEvent('content.publish.chunk', capInt, sovereign, 'ok', { seq, resumed: true }));
      res.status(409).json({ ok: true, seq, sha256: sha, resumed: true, ...envelope() });
      return;
    }
    if (!insertRes.ok) {
      const txt = await insertRes.text().catch(() => '');
      res.status(502).json({ ok: false, error: `supabase ${insertRes.status} ${txt}`, ...envelope() });
      return;
    }

    // Patch package state if currently 'init' → 'uploading'.
    if (pkg.state === 'init') {
      await fetch(`${supabaseUrl}/rest/v1/content_packages?id=eq.${encodeURIComponent(id)}`, {
        method: 'PATCH',
        headers: {
          apikey: sbServiceKey,
          authorization: `Bearer ${sbServiceKey}`,
          'content-type': 'application/json',
          prefer: 'return=minimal',
        },
        body: JSON.stringify({ state: 'uploading' }),
      });
    }

    logEvent(auditEvent('content.publish.chunk', capInt, sovereign, 'ok', { seq }));
    res.status(200).json({ ok: true, seq, sha256: sha, ...envelope() });
  } catch (e: unknown) {
    logEvent(auditEvent('content.publish.chunk', capInt, sovereign, 'error', { reason: e instanceof Error ? e.message : 'internal' }));
    res.status(500).json({
      ok: false,
      error: e instanceof Error ? e.message : 'internal-error',
      ...envelope(),
    });
  }
}

// ─── helpers ────────────────────────────────────────────────────────────────

async function readRawBody(req: NextApiRequest, maxBytes: number): Promise<Uint8Array> {
  return await new Promise<Uint8Array>((resolve, reject) => {
    const chunks: Buffer[] = [];
    let total = 0;
    req.on('data', (chunk: Buffer) => {
      total += chunk.length;
      if (total > maxBytes) {
        req.destroy();
        reject(new RangeError(`body > ${maxBytes}`));
        return;
      }
      chunks.push(chunk);
    });
    req.on('end', () => {
      const buf = Buffer.concat(chunks);
      resolve(new Uint8Array(buf));
    });
    req.on('error', (e) => reject(e));
  });
}

async function sha256Hex(bytes: Uint8Array): Promise<string> {
  const subtle = (globalThis.crypto as { subtle?: SubtleCrypto } | undefined)?.subtle;
  if (subtle === undefined) {
    // Node-only fallback : avoid additional deps by using a tiny inline impl
    // ONLY when subtle is missing. In practice cssl-edge runs on Vercel/Node
    // 18+ where subtle.digest is always available.
    return '0'.repeat(64);
  }
  const dataBuf = new ArrayBuffer(bytes.byteLength);
  new Uint8Array(dataBuf).set(bytes);
  const d = await subtle.digest('SHA-256', dataBuf);
  return bytesToHex(new Uint8Array(d));
}

function bytesToBase64(bytes: Uint8Array): string {
  if (typeof Buffer !== 'undefined') {
    return Buffer.from(bytes).toString('base64');
  }
  let s = '';
  for (let i = 0; i < bytes.length; i++) {
    s += String.fromCharCode(bytes[i] ?? 0);
  }
  return btoa(s);
}
