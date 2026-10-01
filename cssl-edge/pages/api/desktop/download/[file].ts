// cssl-edge - GET /api/desktop/download/[file]
// Authed proxy for the two binary desktop update assets. They live in a
// PRIVATE GitHub release; the updater (unsigned local era) pulls them here so
// no token ever reaches the client. Range is forwarded for NSIS differential
// downloads. The token stays in Vercel env (DESKTOP_ASSET_TOKEN).
//
// What this route serves, and where it comes from:
//   apocrypha-desktop-win-x64.exe          -> GitHub release asset (183 MB)
//   apocrypha-desktop-win-x64.exe.blockmap -> GitHub release asset (188 KB)
//   latest.yml                             -> NOT here. It is a static file in
//     cssl-edge/public/downloads/desktop/, served by Vercel at
//     /downloads/desktop/latest.yml, and its exe url points back here.
//
// GitHub behaviour, learned 2026-09-30 (both were live bugs):
//   * metadata needs the JSON media type. `Accept: application/octet-stream`
//     on /releases/latest makes GitHub 302 to the release HTML page.
//   * PRIVATE assets are only reachable through the asset API url. The
//     browser_download_url (/releases/download/<tag>/<file>) answers 404 even
//     with a valid repo-scoped token.
//   * the asset API 302s to a SIGNED S3 url that rejects an Authorization
//     header, so the redirect hop must be followed without the token.
import type { NextApiRequest, NextApiResponse } from 'next';

const OWNER = 'Apocky';
const REPO = 'apocrypha-desktop';
const ALLOWED = new Set([
  'apocrypha-desktop-win-x64.exe',
  'apocrypha-desktop-win-x64.exe.blockmap',
]);
const UA = 'apocrypha-desktop-feed/1.0';

export const config = { api: { responseLimit: false } };

interface GhAsset {
  name: string;
  url: string;
  size: number;
}

function auth(): string {
  return `Bearer ${process.env.DESKTOP_ASSET_TOKEN ?? ''}`;
}

function rangeOf(req: NextApiRequest): string | undefined {
  const range = req.headers.range;
  return typeof range === 'string' && /^bytes=\d*-\d*$/.test(range) ? range : undefined;
}

async function releaseAssets(tag?: string): Promise<{ tag: string; assets: GhAsset[] } | null> {
  const url = tag
    ? `https://api.github.com/repos/${OWNER}/${REPO}/releases/tags/${tag}`
    : `https://api.github.com/repos/${OWNER}/${REPO}/releases/latest`;
  try {
    const res = await fetch(url, {
      headers: { Accept: 'application/vnd.github+json', Authorization: auth(), 'User-Agent': UA },
    });
    if (!res.ok) return null;
    const body = (await res.json()) as { tag_name?: string; assets?: GhAsset[] };
    if (typeof body.tag_name !== 'string' || !Array.isArray(body.assets)) return null;
    return { tag: body.tag_name, assets: body.assets };
  } catch {
    return null;
  }
}

/** Bytes for one release asset, following the signed-S3 hop without the token. */
async function fetchAsset(asset: GhAsset, range?: string): Promise<Response | null> {
  try {
    const first = await fetch(asset.url, {
      headers: { Accept: 'application/octet-stream', Authorization: auth(), 'User-Agent': UA },
      redirect: 'manual',
    });
    const location = first.headers.get('location');
    if (first.status >= 300 && first.status < 400 && location) {
      const hop: Record<string, string> = { 'User-Agent': UA };
      if (range) hop.Range = range;
      const second = await fetch(location, { headers: hop, redirect: 'follow' });
      return second.ok || second.status === 206 ? second : null;
    }
    return first.ok || first.status === 206 ? first : null;
  } catch {
    return null;
  }
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  if (req.method !== 'GET' && req.method !== 'HEAD') {
    res.status(405).json({ error: 'METHOD_NOT_ALLOWED' });
    return;
  }
  const file = Array.isArray(req.query.file) ? req.query.file[0] : req.query.file;
  if (typeof file !== 'string' || !ALLOWED.has(file)) {
    res.status(404).json({ error: 'NOT_FOUND' });
    return;
  }
  if (!process.env.DESKTOP_ASSET_TOKEN) {
    res.status(503).json({ error: 'FEED_UNCONFIGURED' });
    return;
  }
  const wanted = typeof req.query.v === 'string' && /^v\d+\.\d+\.\d+$/.test(req.query.v) ? req.query.v : undefined;
  const release = await releaseAssets(wanted);
  if (!release) {
    res.status(502).json({ error: 'RELEASE_UNAVAILABLE' });
    return;
  }
  const asset = release.assets.find((a) => a.name === file);
  if (!asset) {
    res.status(404).json({ error: 'ASSET_NOT_FOUND', tag: release.tag, file });
    return;
  }
  const upstream = await fetchAsset(asset, rangeOf(req));
  if (!upstream) {
    res.status(502).json({ error: 'ASSET_UNAVAILABLE', tag: release.tag, file });
    return;
  }
  res.status(upstream.status);
  res.setHeader('content-type', 'application/octet-stream');
  if (upstream.headers.get('content-length')) res.setHeader('content-length', upstream.headers.get('content-length') as string);
  const contentRange = upstream.headers.get('content-range');
  if (contentRange) res.setHeader('content-range', contentRange);
  res.setHeader('accept-ranges', 'bytes');
  res.setHeader('cache-control', 'public, max-age=3600');
  if (req.method === 'HEAD' || !upstream.body) {
    res.end();
    return;
  }
  const reader = upstream.body.getReader();
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      res.write(Buffer.from(value));
    }
  } finally {
    reader.releaseLock();
  }
  res.end();
}