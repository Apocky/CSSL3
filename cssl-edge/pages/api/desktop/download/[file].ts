// cssl-edge - GET /api/desktop/download/[file]
// Authed proxy: desktop update assets live as PRIVATE GitHub release files;
// the app updater (unsigned local era) fetches them here with Range support
// for NSIS differential updates. Server token stays in Vercel env
// (DESKTOP_ASSET_TOKEN); nothing secret reaches the client or the repo.
import type { NextApiRequest, NextApiResponse } from 'next';

const OWNER = 'Apocky';
const REPO = 'apocrypha-desktop';
const ALLOWED = new Set([
  'apocrypha-desktop-win-x64.exe',
  'apocrypha-desktop-win-x64.exe.blockmap',
  'latest.yml',
]);

export const config = { api: { responseLimit: false } };

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
  const token = process.env.DESKTOP_ASSET_TOKEN;
  if (!token) {
    res.status(503).json({ error: 'FEED_UNCONFIGURED' });
    return;
  }
  const tag = typeof req.query.v === 'string' && /^v\d+\.\d+\.\d+$/.test(req.query.v)
    ? req.query.v
    : 'latest';
  const upstream = tag === 'latest'
    ? `https://api.github.com/repos/${OWNER}/${REPO}/releases/latest`
    : `https://github.com/${OWNER}/${REPO}/releases/download/${tag}/${file}`;
  const headers: Record<string, string> = {
    Accept: 'application/octet-stream',
    Authorization: `Bearer ${token}`,
    'User-Agent': 'apocky-desktop-feed/1.0',
  };
  const range = req.headers.range;
  if (typeof range === 'string' && /^bytes=\d*-\d*$/.test(range)) headers.Range = range;

  const resolve = async (url: string): Promise<Response> => {
    const r = await fetch(url, { headers, redirect: 'manual' });
    if (r.status >= 300 && r.status < 400 && r.headers.get('location')) {
      const loc = r.headers.get('location') as string;
      const h2: Record<string, string> = { 'User-Agent': 'apocky-desktop-feed/1.0' };
      if (typeof range === 'string' && /^bytes=\d*-\d*$/.test(range)) h2.Range = range;
      return fetch(loc, { headers: h2 });
    }
    return r;
  };

  let upstreamRes: Response;
  try {
    if (tag === 'latest') {
      const meta = (await (await resolve(upstream)).json()) as { tag_name?: string };
      if (!meta.tag_name) throw new Error('no releases');
      upstreamRes = await resolve(
        `https://github.com/${OWNER}/${REPO}/releases/download/${meta.tag_name}/${file}`);
    } else {
      upstreamRes = await resolve(upstream);
    }
  } catch {
    res.status(502).json({ error: 'UPSTREAM_UNAVAILABLE' });
    return;
  }
  if (!upstreamRes.ok && upstreamRes.status !== 206) {
    res.status(502).json({ error: 'UPSTREAM_UNAVAILABLE' });
    return;
  }
  res.status(upstreamRes.status);
  for (const h of ['content-type', 'content-length', 'content-range', 'accept-ranges', 'etag', 'last-modified']) {
    const v = upstreamRes.headers.get(h);
    if (v) res.setHeader(h, v);
  }
  res.setHeader('cache-control', 'public, max-age=3600');
  if (req.method === 'HEAD' || !upstreamRes.body) {
    res.end();
    return;
  }
  const reader = upstreamRes.body.getReader();
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
