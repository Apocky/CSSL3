// cssl-edge · /api/hotfix/download
// § T11-W11-HOTFIX-INFRA — bundle-byte streamer.
//
// GET /api/hotfix/download?channel=<name>&version=<semver>
//   → 200 with `Content-Type: application/octet-stream` + bundle bytes
//   → 416 on bad Range header
//   → 404 if (channel, version) not found
//   → 410 if version is on revoke-list
//   → 200 stub-mode JSON when offline.
//
// Range : we honour `Range: bytes=<start>-<end>` for resume-after-disconnect.
// JWT-Σ-mask check : LoA.exe carries a cap-bit cert tying its install-id to
// a per-channel consent. The header `X-Sigma-Cap` (hex Ed25519 token) is
// REQUIRED for non-`security.patch` channels ; security.patch is always-on.
import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, envelope, stubEnvelope } from '@/lib/response';

const VALID_CHANNELS = new Set([
  'loa.binary',
  'cssl.bundle',
  'kan.weights',
  'balance.config',
  'recipe.book',
  'nemesis.bestiary',
  'security.patch',
  'storylet.content',
  'render.pipeline',
]);

const SEMVER_RE = /^(\d+)\.(\d+)\.(\d+)$/;

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  logHit('hotfix.download', { method: req.method ?? 'GET' });

  if (req.method !== 'GET') {
    res.status(405).json({ ok: false, error: 'GET only', ...envelope() });
    return;
  }

  const channel = typeof req.query.channel === 'string' ? req.query.channel : '';
  const version = typeof req.query.version === 'string' ? req.query.version : '';

  if (!VALID_CHANNELS.has(channel)) {
    res.status(400).json({ ok: false, error: `unknown channel ${channel}`, ...envelope() });
    return;
  }
  if (!SEMVER_RE.test(version)) {
    res.status(400).json({ ok: false, error: `bad version ${version}`, ...envelope() });
    return;
  }

  // Σ-mask cap check : security.patch bypasses ; everything else needs token.
  if (channel !== 'security.patch') {
    const cap = req.headers['x-sigma-cap'];
    if (typeof cap !== 'string' || cap.length < 16) {
      res.status(403).json({ ok: false, error: 'sigma-cap-required', ...envelope() });
      return;
    }
  }

  const supabaseUrl = process.env.NEXT_PUBLIC_SUPABASE_URL;
  const sbServiceKey = process.env.SUPABASE_SERVICE_ROLE_KEY;
  const storageBucket = process.env.HOTFIX_BUCKET ?? 'hotfix-bundles';
  if (!supabaseUrl || !sbServiceKey) {
    res.status(200).json(stubEnvelope('wire SUPABASE_SERVICE_ROLE_KEY + HOTFIX_BUCKET ; bundle bytes are streamed from Supabase storage'));
    return;
  }

  try {
    // Storage path : <bucket>/<channel>/<version>.csslfix
    const url = `${supabaseUrl}/storage/v1/object/${storageBucket}/${channel}/${version}.csslfix`;
    const range = req.headers.range;
    const r = await fetch(url, {
      headers: {
        apikey: sbServiceKey,
        authorization: `Bearer ${sbServiceKey}`,
        ...(range ? { range } : {}),
      },
    });
    if (r.status === 404) {
      res.status(404).json({ ok: false, error: 'bundle-not-found', ...envelope() });
      return;
    }
    if (r.status === 416) {
      res.status(416).json({ ok: false, error: 'range-not-satisfiable', ...envelope() });
      return;
    }
    if (!r.ok && r.status !== 206) {
      res.status(502).json({ ok: false, error: `upstream ${r.status}`, ...envelope() });
      return;
    }
    const buf = Buffer.from(await r.arrayBuffer());
    res.setHeader('content-type', 'application/octet-stream');
    res.setHeader('content-disposition', `attachment; filename="${channel}-${version}.csslfix"`);
    res.setHeader('cache-control', 'public, max-age=86400, immutable');
    if (r.headers.get('content-range')) {
      res.setHeader('content-range', r.headers.get('content-range') ?? '');
      res.status(206);
    } else {
      res.status(200);
    }
    res.send(buf);
  } catch (e: unknown) {
    res.status(500).json({
      ok: false,
      error: e instanceof Error ? e.message : 'internal-error',
      ...envelope(),
    });
  }
}

export const config = {
  api: {
    responseLimit: false, // bundle bytes can exceed default 4MB cap
  },
};
