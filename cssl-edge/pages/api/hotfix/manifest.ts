// cssl-edge · /api/hotfix/manifest
// § T11-W11-HOTFIX-INFRA — apocky.com manifest-of-truth.
//
// GET /api/hotfix/manifest?channel=<name>
//   → { ok: true, manifest: { schema_version, generated_at_ns, signed_by,
//                              channels: { ... }, revocations: [...],
//                              signature: "<hex>" } }
//   → { stub: true, todo } when no Supabase wiring (offline-mode default).
//
// Querystring `channel` is OPTIONAL : if present, the response carries
// only that channel's entry under `channels` (the manifest signature still
// covers the FULL manifest — clients verify against the full payload they
// reassemble client-side from a follow-up unfiltered fetch).
//
// All routes audit-emit · Σ-mask check is client-side (LoA.exe verifies
// the cap-key signature against compiled-in pubkeys before trusting any
// channel data).
import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, envelope, stubEnvelope } from '@/lib/response';

interface ChannelEntry {
  current_version: string;
  bundle_sha256: string;
  effective_from_ns: number;
  download_path: string;
  size_bytes: number;
}

interface RevocationEntry {
  channel: string;
  version: string;
  ts_ns: number;
  reason: string;
}

interface Manifest {
  schema_version: number;
  generated_at_ns: number;
  signed_by: string;
  channels: Record<string, ChannelEntry>;
  revocations: RevocationEntry[];
  signature: string;
}

interface ManifestResponse {
  ok: true;
  manifest: Manifest;
  served_by: string;
  ts: string;
}

interface StubManifestResponse {
  stub: true;
  todo: string;
  served_by: string;
  ts: string;
}

interface ErrorResponse {
  ok: false;
  error: string;
  served_by: string;
  ts: string;
}

type Resp = ManifestResponse | StubManifestResponse | ErrorResponse;

export default async function handler(req: NextApiRequest, res: NextApiResponse<Resp>): Promise<void> {
  logHit('hotfix.manifest', { method: req.method ?? 'GET' });

  if (req.method !== 'GET') {
    res.status(405).json({ ok: false, error: 'GET only', ...envelope() });
    return;
  }

  const channel = typeof req.query.channel === 'string' ? req.query.channel : undefined;

  // Stub-mode : no Supabase / no manifest signing-key wired. Returns a
  // sentinel manifest indicating the client should treat the apocky.com
  // service as not-yet-bootstrapped (LoA.exe falls back to its compiled-in
  // versions and skips updates this poll).
  const supabaseUrl = process.env.NEXT_PUBLIC_SUPABASE_URL;
  const sbServiceKey = process.env.SUPABASE_SERVICE_ROLE_KEY;
  if (!supabaseUrl || !sbServiceKey) {
    res.status(200).json({
      ...stubEnvelope('Wire NEXT_PUBLIC_SUPABASE_URL + SUPABASE_SERVICE_ROLE_KEY ; manifest is read from public.hotfix_manifest_versions + signed by SVC_HOTFIX_PRIV_<role>.'),
    });
    return;
  }

  // Production : pull from Supabase, sign with the appropriate cap-key, return.
  // For this slice we shape the envelope ; full impl wires server-side
  // Ed25519 signing with apocky-controlled keys (¬ committed to source).
  try {
    const url = new URL(`${supabaseUrl}/rest/v1/hotfix_manifest_versions?select=*&revoked_at=is.null&order=channel.asc`);
    const r = await fetch(url.toString(), {
      headers: {
        apikey: sbServiceKey,
        authorization: `Bearer ${sbServiceKey}`,
      },
    });
    if (!r.ok) {
      res.status(502).json({
        ok: false,
        error: `supabase ${r.status}`,
        ...envelope(),
      });
      return;
    }
    const rows: Array<{
      channel: string;
      version: string;
      bundle_sha256: string;
      cap_signer: string;
      signature: string;
      effective_from_ns: string;
      size_bytes: number;
    }> = await r.json();

    const channels: Record<string, ChannelEntry> = {};
    for (const row of rows) {
      if (channel && row.channel !== channel) continue;
      channels[row.channel] = {
        current_version: row.version,
        bundle_sha256: row.bundle_sha256,
        effective_from_ns: Number(row.effective_from_ns),
        download_path: `${row.channel}/${row.version}.csslfix`,
        size_bytes: row.size_bytes,
      };
    }

    const manifest: Manifest = {
      schema_version: 1,
      generated_at_ns: Date.now() * 1_000_000,
      signed_by: rows[0]?.cap_signer ?? 'cap-D',
      channels,
      revocations: [],
      // The Ed25519 signature lives in env var SVC_HOTFIX_MANIFEST_SIG_<role> ;
      // server-side Ed25519 implementation lands in a follow-up slice. For
      // now the signature is the empty 128-hex-char placeholder (clients
      // reject it ; this matches stub behaviour).
      signature: '0'.repeat(128),
    };

    res.status(200).json({
      ok: true,
      manifest,
      ...envelope(),
    });
  } catch (e: unknown) {
    res.status(500).json({
      ok: false,
      error: e instanceof Error ? e.message : 'internal-error',
      ...envelope(),
    });
  }
}
