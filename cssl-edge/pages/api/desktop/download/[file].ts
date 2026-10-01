// cssl-edge - GET /api/desktop/download/[file]
// Authed proxy: desktop update assets live as PRIVATE GitHub release files;
// the app updater (unsigned local era) fetches them here with Range support
// for NSIS differential updates. Server token stays in Vercel env
// (DESKTOP_ASSET_TOKEN); nothing secret reaches the client or the repo.
//
// Two GitHub shapes, learned the hard way (2026-09-30):
//   * metadata: api.github.com/repos/{o}/{r}/releases/latest must be asked
//     with the JSON media type. Asking for octet-stream makes GitHub answer
//     302 to the release HTML page, and the code then fails to read a tag.
//   * bytes: for a PRIVATE repo the browser_download_url 302s to a SIGNED
//     S3 url that rejects an Authorization header, so the redirect hop must
//     be followed WITHOUT the token. For a PUBLIC repo the same url serves
//     bytes directly and still must not carry the token.
import type { NextApiRequest, NextApiResponse } from 'next';

const OWNER = 'Apocky';
const REPO = 'apocrypha-desktop';
const ALLOWED = new Set([
  'apocrypha-desktop-win-x64.exe',
  'apocrypha-desktop-win-x64.exe.blockmap',
  'latest.yml',
]);
const JSON_ACCEPT = 'application/vnd.github+json';

export const config = { api: { responseLimit: false } };

function rangeOf(req: NextApiRequest): string | undefined {
  const range = req.headers.range;
  return typeof range === 'string' && /^bytes=\d*-\d*$/.test(range) ? range : undefined;
}

/** Download bytes from a github.com release url, following the signed-S3
 *  redirect without the token. Returns null on any upstream failure. */
async function fetchReleaseBytes(tag: string, file: string, range?: string): Promise<Response | null> {
  const url = `https://github.com/${OWNER}/${REPO}/releases/download/${tag}/${file}`;
  try {
    const first = await fetch(url, {
      headers: { Authorization: `Bearer ${process.env.DESKTOP_ASSET_TOKEN ?? ''}`, 'User-Agent': 'apocky-desktop-feed/1.0' },
      redirect: 'manual',
    });
    if (first.status >= 300 && first.status < 400 && first.headers.get('location')) {
      const hop: Record<string, string> = { 'User-Agent': 'apocky-desktop-feed/1.0' };
      if (range) hop.Range = range;
      const second = await fetch(first.headers.get('location') as string, { headers: hop, redirect: 'follow' });
      return second.ok || second.status === 206 ? second : null;
    }
    if (first.ok || first.status === 206) return first;
    return null;
  } catch {
    return null;
  }
}

async function latestTag(): Promise<string | null> {
  try {
    const meta = await fetch(`https://api.github.com/repos/${OWNER}/${REPO}/releases/latest`, {
      headers: {
        Accept: JSON_ACCEPT,
        Authorization: `Bearer ${process.env.DESKTOP_ASSET_TOKEN ?? ''}`,
        'User-Agent': 'apocky-desktop-feed/1.0',
      },
    });
    if (!meta.ok) return null;
    const body = (await meta.json()) as { tag_name?: string };
    return typeof body.tag_name === 'string' ? body.tag_name : null;
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
  const tag =
    typeof req.query.v === 'string' && /^v\d+\.\d+\.\d+$/.test(req.query.v) ? req.query.v : await latestTag();
  if (!tag) {
    res.status(502).json({ error: 'UPSTREAM_UNAVAILABLE' });
    return;
  }
  const upstream = await fetchReleaseBytes(tag, file, rangeOf(req));
  if (!upstream) {
    res.status(502).json({ error: 'UPSTREAM_UNAVAILABLE', tag });
    return;
  }
  res.status(upstream.status);
  for (const h of ['content-type', 'content-length', 'content-range', 'accept-ranges', 'etag', 'last-modified']) {
    const v = upstream.headers.get(h);
    if (v) res.setHeader(h, v);
  }
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