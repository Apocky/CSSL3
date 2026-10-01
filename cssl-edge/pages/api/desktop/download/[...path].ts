// cssl-edge - GET /api/desktop/download/<tag>/<file>   (and the legacy flat
// /api/desktop/download/<file>?v=<tag>)
//
// Authed proxy for the two binary desktop update assets. They live in a
// PRIVATE GitHub release; the updater pulls them here so no token reaches the
// client. Range is forwarded for NSIS differential downloads. The token stays
// in Vercel env (DESKTOP_ASSET_TOKEN).
//
// The tag MUST be a path segment, not a query string. electron-updater builds
// the OLD blockmap url by string-replacing the new version into the PATHNAME
// (out/providers/Provider.js getBlockMapFiles):
//     pathname.replace(new RegExp(escapeRegExp(newVersion), "g"), oldVersion)
// With the version in ?v=, the pathname held no version, the replace was a
// no-op, and the old and new blockmap urls came out IDENTICAL. The
// differential downloader then patched the cached installer against the NEW
// blockmap and would have installed a corrupt exe with no error anywhere. The
// versioned path shape makes the swap work:
//     /api/desktop/download/v1.18.35/x.exe -> old url /api/desktop/download/v1.18.34/x.exe.blockmap
// The flat shape is still accepted so a feed published before this change
// keeps working (it just loses correct differential behaviour).
//
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
import type { NextApiRequest, NextApiResponse } from "next";

const OWNER = "Apocky";
const REPO = "apocrypha-desktop";
const ALLOWED = new Set(["apocrypha-desktop-win-x64.exe", "apocrypha-desktop-win-x64.exe.blockmap"]);
const TAG = /^v\d+\.\d+\.\d+$/;
const UA = "apocrypha-desktop-feed/1.0";

export const config = { api: { responseLimit: false } };

interface GhAsset {
  name: string;
  url: string;
  size: number;
}

function auth(): string {
  return `Bearer ${process.env.DESKTOP_ASSET_TOKEN ?? ""}`;
}

/** Accept both shapes: ["v1.18.35", "x.exe"] and ["x.exe"] with ?v=1.18.35. */
function resolve(path: string[], query: unknown): { tag?: string; file: string } | undefined {
  const parts = path.filter((p) => p.length > 0)
  const head = parts[0]
  const second = parts[1]
  if (head === undefined) return undefined
  if (parts.length === 2 && second !== undefined && TAG.test(head) && ALLOWED.has(second))
    return { tag: head, file: second }
  if (parts.length !== 1 || !ALLOWED.has(head)) return undefined
  const raw = Array.isArray(query) ? query[0] : query
  const tag = typeof raw === "string" && TAG.test(raw) ? raw : undefined
  return tag ? { tag, file: head } : { file: head }
}

function rangeOf(req: NextApiRequest): string | undefined {
  const range = req.headers.range;
  return typeof range === "string" && /^bytes=\d*-\d*$/.test(range) ? range : undefined;
}

async function releaseAssets(tag?: string): Promise<{ tag: string; assets: GhAsset[] } | null> {
  const url = tag
    ? `https://api.github.com/repos/${OWNER}/${REPO}/releases/tags/${tag}`
    : `https://api.github.com/repos/${OWNER}/${REPO}/releases/latest`;
  try {
    const res = await fetch(url, { headers: { Accept: "application/vnd.github+json", Authorization: auth(), "User-Agent": UA } });
    if (!res.ok) return null;
    const body = (await res.json()) as { tag_name?: string; assets?: GhAsset[] };
    if (typeof body.tag_name !== "string" || !Array.isArray(body.assets)) return null;
    return { tag: body.tag_name, assets: body.assets };
  } catch {
    return null;
  }
}

/** Bytes for one release asset, following the signed-S3 hop without the token. */
async function fetchAsset(asset: GhAsset, range?: string): Promise<Response | null> {
  try {
    const first = await fetch(asset.url, {
      headers: { Accept: "application/octet-stream", Authorization: auth(), "User-Agent": UA },
      redirect: "manual",
    });
    const location = first.headers.get("location");
    if (first.status >= 300 && first.status < 400 && location) {
      const hop: Record<string, string> = { "User-Agent": UA };
      if (range) hop.Range = range;
      const second = await fetch(location, { headers: hop, redirect: "follow" });
      return second.ok || second.status === 206 ? second : null;
    }
    return first.ok || first.status === 206 ? first : null;
  } catch {
    return null;
  }
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  if (req.method !== "GET" && req.method !== "HEAD") {
    res.status(405).json({ error: "METHOD_NOT_ALLOWED" });
    return;
  }
  const path = Array.isArray(req.query.path) ? req.query.path : typeof req.query.path === "string" ? [req.query.path] : [];
  const target = resolve(path, req.query.v);
  if (!target) {
    res.status(404).json({ error: "NOT_FOUND" });
    return;
  }
  if (!process.env.DESKTOP_ASSET_TOKEN) {
    res.status(503).json({ error: "FEED_UNCONFIGURED" });
    return;
  }
  const release = await releaseAssets(target.tag);
  if (!release) {
    res.status(502).json({ error: "RELEASE_UNAVAILABLE" });
    return;
  }
  const asset = release.assets.find((a) => a.name === target.file);
  if (!asset) {
    res.status(404).json({ error: "ASSET_NOT_FOUND", tag: release.tag, file: target.file });
    return;
  }
  const upstream = await fetchAsset(asset, rangeOf(req));
  if (!upstream) {
    res.status(502).json({ error: "ASSET_UNAVAILABLE", tag: release.tag, file: target.file });
    return;
  }
  res.status(upstream.status);
  res.setHeader("content-type", "application/octet-stream");
  if (upstream.headers.get("content-length")) res.setHeader("content-length", upstream.headers.get("content-length") as string);
  const contentRange = upstream.headers.get("content-range");
  if (contentRange) res.setHeader("content-range", contentRange);
  res.setHeader("accept-ranges", "bytes");
  res.setHeader("cache-control", "public, max-age=3600");
  if (req.method === "HEAD" || !upstream.body) {
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