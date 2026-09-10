#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { lstat, mkdir, readFile, readdir, realpath, stat, writeFile } from 'node:fs/promises';
import { isIP } from 'node:net';
import { basename, dirname, isAbsolute, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const APPROVED_RECORD_COUNT = 204;
export const EXCLUDED_DRAFT_COUNT = 26;
export const MEDIUM_EXPORT_COUNT = 230;
export const APPROVED_SOURCE_BYTES = 4_203_802;
export const APPROVED_SOURCE_IDENTITY_SHA256 =
  '8fcb160f1cc19d09103e86f21596805b11763d18dadfcf681ef3baf649323674';
export const PUBLIC_PROJECTION_MAX_BYTES = 96 * 1024;

const CODEX_SELECTION_SCHEMA = 'vaultsync.codex-session-selection.v1';
const VAULTSYNC_GENERATED_MARKER = '<!-- vaultsync:generated -- do not hand-edit; regenerate with vaultsync.py apply -->';
const VAULTSYNC_TURN_MARKER = /^<!-- vaultsync:turn role=(user|assistant) -->\r?\n### (Human|Assistant)\r?\n\r?\n/gm;
const MEDIUM_SOURCE_SEAL_ALGORITHM =
  'ASCII lowercase ordinal filename order; filename<TAB>byteLength<TAB>lowercase-sha256<LF>; UTF-8 without BOM';

const APPROVED_NAME = /^(\d{4}-\d{2}-\d{2})_(.+)-([0-9a-f]{12})\.html$/i;
const DRAFT_NAME = /^draft_.+\.html$/i;

function compareFilenames(left, right) {
  const foldedLeft = left.toLowerCase();
  const foldedRight = right.toLowerCase();
  if (foldedLeft < foldedRight) return -1;
  if (foldedLeft > foldedRight) return 1;
  return left < right ? -1 : left > right ? 1 : 0;
}

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

function assertInside(root, candidate, label) {
  const pathFromRoot = relative(root, candidate);
  if (pathFromRoot === '' || (!pathFromRoot.startsWith('..') && !isAbsolute(pathFromRoot))) return;
  throw new Error(`${label} escaped the selected Medium posts directory`);
}

async function resolvePostsDirectory(sourceRoot) {
  const requestedRoot = resolve(sourceRoot);
  const requestedInfo = await lstat(requestedRoot);
  if (!requestedInfo.isDirectory() || requestedInfo.isSymbolicLink()) {
    throw new Error('Medium source root must be a real local directory, not a link');
  }

  const candidate = basename(requestedRoot).toLowerCase() === 'posts'
    ? requestedRoot
    : join(requestedRoot, 'posts');
  const candidateInfo = await lstat(candidate);
  if (!candidateInfo.isDirectory() || candidateInfo.isSymbolicLink()) {
    throw new Error('Medium posts source must be a real directory, not a link');
  }

  const [rootReal, postsReal] = await Promise.all([realpath(requestedRoot), realpath(candidate)]);
  assertInside(rootReal, postsReal, 'posts directory');
  return postsReal;
}

export async function inventoryApprovedMediumPosts(sourceRoot) {
  const postsDirectory = await resolvePostsDirectory(sourceRoot);
  const entries = await readdir(postsDirectory, { withFileTypes: true });
  const approved = [];
  let draftExcludedCount = 0;

  for (const entry of entries.sort((a, b) => compareFilenames(a.name, b.name))) {
    if (!entry.isFile() || entry.isSymbolicLink()) {
      throw new Error(`unexpected non-file entry in Medium posts export: ${entry.name}`);
    }
    if (DRAFT_NAME.test(entry.name)) {
      draftExcludedCount += 1;
      continue;
    }
    const nameMatch = entry.name.match(APPROVED_NAME);
    if (nameMatch === null) {
      throw new Error(`unexpected non-draft file in Medium posts export: ${entry.name}`);
    }

    const filePath = join(postsDirectory, entry.name);
    const [fileInfo, fileReal, bytes] = await Promise.all([
      stat(filePath),
      realpath(filePath),
      readFile(filePath),
    ]);
    assertInside(postsDirectory, fileReal, 'post file');
    if (!fileInfo.isFile() || fileInfo.nlink !== 1) {
      throw new Error(`post source must be a regular single-link file: ${entry.name}`);
    }
    if (fileInfo.size !== bytes.byteLength) {
      throw new Error(`post changed while being read: ${entry.name}`);
    }
    approved.push({
      filename: entry.name,
      publishedDate: nameMatch[1],
      filenameTitle: nameMatch[2],
      sourceId: nameMatch[3],
      bytes,
      byteLength: bytes.byteLength,
      sourceSha256: sha256(bytes),
    });
  }

  const sourceIdentity = approved
    .map((post) => `${post.filename}\t${post.byteLength}\t${post.sourceSha256}\n`)
    .join('');
  return {
    approved,
    approvedCount: approved.length,
    draftExcludedCount,
    sourceIdentitySha256: sha256(Buffer.from(sourceIdentity, 'utf8')),
  };
}

const TRACKING_PARAMETERS = new Set(['fbclid', 'gclid', 'mc_cid', 'mc_eid', 'mkt_tok']);

function decodeHtml(value) {
  return value.replace(/&(#(?:x[0-9a-f]+|\d+)|amp|gt|lt|quot|apos);/gi, (match, entity) => {
    const normalized = entity.toLowerCase();
    if (normalized === 'amp') return '&';
    if (normalized === 'gt') return '>';
    if (normalized === 'lt') return '<';
    if (normalized === 'quot') return '"';
    if (normalized === 'apos') return "'";
    const codePoint = normalized.startsWith('#x')
      ? Number.parseInt(normalized.slice(2), 16)
      : Number.parseInt(normalized.slice(1), 10);
    if (!Number.isInteger(codePoint) || codePoint < 0 || codePoint > 0x10ffff) return match;
    try {
      return String.fromCodePoint(codePoint);
    } catch {
      return match;
    }
  });
}

function readAttribute(tag, name) {
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const match = tag.match(new RegExp(`\\b${escaped}\\s*=\\s*(?:"([^"]*)"|'([^']*)'|([^\\s>]+))`, 'i'));
  return match === null ? undefined : decodeHtml(match[1] ?? match[2] ?? match[3] ?? '');
}

function textBuilder() {
  let value = '';
  return {
    append(raw) {
      const decoded = decodeHtml(raw).replace(/\r\n?/g, '\n');
      for (const character of decoded) {
        if (/\s/u.test(character)) {
          if (value.length > 0 && !/\s$/u.test(value)) value += ' ';
        } else {
          value += character;
        }
      }
    },
    break() {
      value = value.replace(/[ \t]+$/u, '');
      if (value.length > 0 && !value.endsWith('\n')) value += '\n';
    },
    get length() {
      return value.length;
    },
    finish() {
      return value.replace(/[ \t]+\n/gu, '\n').replace(/\n[ \t]+/gu, '\n').trim();
    },
  };
}

export function safeHttpUrl(value) {
  if (typeof value !== 'string' || value.length === 0) return undefined;
  try {
    const parsed = new URL(value);
    if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') return undefined;
    if (parsed.username.length > 0 || parsed.password.length > 0) return undefined;
    for (const key of [...parsed.searchParams.keys()]) {
      if (key.toLowerCase().startsWith('utm_') || TRACKING_PARAMETERS.has(key.toLowerCase())) {
        parsed.searchParams.delete(key);
      }
    }
    return parsed.toString();
  } catch {
    return undefined;
  }
}

function renderInline(innerHtml, includeLinks = false) {
  const builder = textBuilder();
  const links = [];
  let activeLink;
  const tokens = innerHtml.match(/<!--[\s\S]*?-->|<[^>]+>|[^<]+/g) ?? [];
  for (const token of tokens) {
    if (!token.startsWith('<')) {
      builder.append(token);
      continue;
    }
    if (/^<br\b/i.test(token)) {
      builder.break();
      continue;
    }
    if (includeLinks && /^<a\b/i.test(token)) {
      activeLink = { href: safeHttpUrl(readAttribute(token, 'href')), start: builder.length };
      continue;
    }
    if (includeLinks && /^<\/a\b/i.test(token) && activeLink !== undefined) {
      const end = builder.length;
      if (activeLink.href !== undefined && end > activeLink.start) {
        links.push({ start: activeLink.start, end, text: '', href: activeLink.href });
      }
      activeLink = undefined;
    }
  }
  const text = builder.finish();
  const validLinks = links
    .filter((link) => link.end <= text.length)
    .map((link) => ({ ...link, text: text.slice(link.start, link.end) }))
    .filter((link) => link.text.length > 0);
  return validLinks.length > 0 ? { text, links: validLinks } : { text };
}

function innerHtmlOf(fragment, tagName) {
  const match = fragment.match(new RegExp(`^<${tagName}\\b[^>]*>([\\s\\S]*)<\\/${tagName}\\s*>$`, 'i'));
  return match?.[1] ?? '';
}

function plainText(fragment) {
  return renderInline(fragment.replace(/<\/?(?:p|li|figcaption)\b[^>]*>/gi, '')).text;
}

function normalizeComparable(value) {
  return value.replace(/\u00a0/gu, ' ').replace(/\s+/gu, ' ').trim();
}

function parseExportDate(footer) {
  const match = footer.match(/Exported from[\s\S]*?\bon\s+([A-Z][a-z]+\s+\d{1,2},\s+\d{4})\s*\./);
  if (match === null) return undefined;
  const parsed = new Date(`${match[1]} 00:00:00 UTC`);
  return Number.isNaN(parsed.getTime()) ? undefined : parsed.toISOString().slice(0, 10);
}

function parseEmbed(figureHtml) {
  const iframe = figureHtml.match(/<iframe\b[^>]*>/i)?.[0];
  if (iframe === undefined) return undefined;
  const iframeUrl = safeHttpUrl(readAttribute(iframe, 'src'));
  if (iframeUrl === undefined) return { kind: 'embed', omitted: true };
  const parsed = new URL(iframeUrl);
  const host = parsed.hostname.toLowerCase();
  if (host === 'www.youtube.com' || host === 'youtube.com' || host === 'www.youtube-nocookie.com') {
    const id = parsed.pathname.match(/^\/embed\/([A-Za-z0-9_-]+)/)?.[1];
    return {
      kind: 'embed',
      provider: 'YouTube',
      ...(id === undefined ? {} : { href: `https://www.youtube.com/watch?v=${id}` }),
      omitted: true,
    };
  }
  if (host === 'w.soundcloud.com') {
    const target = safeHttpUrl(parsed.searchParams.get('url') ?? undefined);
    return { kind: 'embed', provider: 'SoundCloud', ...(target === undefined ? {} : { href: target }), omitted: true };
  }
  return { kind: 'embed', provider: host, href: iframeUrl, omitted: true };
}

export function parseMediumBlocks(bodyHtml, title, subtitle) {
  const blocks = [];
  const blockPattern = /<(p|h3|h4|blockquote|ul|ol|pre|figure)\b[^>]*>[\s\S]*?<\/\1\s*>|<div\b(?=[^>]*\bgraf--mixtapeEmbed\b)[^>]*>[\s\S]*?<\/div\s*>|<hr\b[^>]*>/gi;
  let leadingMetadata = true;
  for (const match of bodyHtml.matchAll(blockPattern)) {
    const fragment = match[0];
    if (/^<hr\b/i.test(fragment)) {
      if (blocks.length > 0 && blocks.at(-1)?.kind !== 'divider') blocks.push({ kind: 'divider' });
      continue;
    }
    if (/^<div\b/i.test(fragment)) {
      const anchorMatch = fragment.match(/(<a\b(?=[^>]*\bmarkup--mixtapeEmbed-anchor\b)[^>]*>)([\s\S]*?)<\/a\s*>/i);
      if (anchorMatch !== null) {
        const text = renderInline(anchorMatch[2] ?? '').text;
        const href = safeHttpUrl(readAttribute(anchorMatch[1] ?? '', 'href'));
        if (text.length > 0 && href !== undefined) blocks.push({ kind: 'linkCard', text, href });
        else if (text.length > 0) blocks.push({ kind: 'paragraph', text });
      }
      leadingMetadata = false;
      continue;
    }
    const tag = match[1]?.toLowerCase();
    if (tag === 'p') {
      const rendered = renderInline(innerHtmlOf(fragment, 'p'), true);
      if (rendered.text.length > 0) blocks.push({ kind: 'paragraph', ...rendered });
      leadingMetadata = false;
      continue;
    }
    if (tag === 'h3' || tag === 'h4') {
      const rendered = renderInline(innerHtmlOf(fragment, tag), true);
      const text = rendered.text;
      if (text.length === 0) continue;
      const matchesTitle = tag === 'h3' && normalizeComparable(text) === normalizeComparable(title);
      const matchesSubtitle = tag === 'h4' && subtitle.length > 0 && normalizeComparable(text) === normalizeComparable(subtitle);
      if (leadingMetadata && (matchesTitle || matchesSubtitle)) continue;
      blocks.push({ kind: 'heading', level: tag === 'h3' ? 2 : 3, ...rendered });
      continue;
    }
    if (tag === 'blockquote') {
      const rendered = renderInline(innerHtmlOf(fragment, 'blockquote').replace(/<\/?p\b[^>]*>/gi, ''), true);
      if (rendered.text.length > 0) blocks.push({ kind: 'blockquote', ...rendered });
      leadingMetadata = false;
      continue;
    }
    if (tag === 'ul' || tag === 'ol') {
      const items = [...innerHtmlOf(fragment, tag).matchAll(/<li\b[^>]*>([\s\S]*?)<\/li\s*>/gi)]
        .map((item) => plainText(item[1] ?? ''))
        .filter(Boolean);
      if (items.length > 0) blocks.push({ kind: 'list', ordered: tag === 'ol', items });
      leadingMetadata = false;
      continue;
    }
    if (tag === 'pre') {
      const text = decodeHtml(innerHtmlOf(fragment, 'pre').replace(/<br\s*\/?\s*>/gi, '\n').replace(/<[^>]+>/g, ''))
        .replace(/\r\n?/g, '\n')
        .trim();
      if (text.length > 0) blocks.push({ kind: 'pre', text });
      leadingMetadata = false;
      continue;
    }
    if (tag === 'figure') {
      const embed = parseEmbed(fragment);
      if (embed !== undefined) {
        blocks.push(embed);
      } else {
        const image = fragment.match(/<img\b[^>]*>/i)?.[0];
        const alt = image === undefined ? undefined : normalizeComparable(readAttribute(image, 'alt') ?? '');
        const captionMatch = fragment.match(/<figcaption\b[^>]*>([\s\S]*?)<\/figcaption\s*>/i);
        const caption = captionMatch === null ? undefined : plainText(captionMatch[1] ?? '');
        blocks.push({
          kind: 'figure',
          ...(alt === undefined || alt.length === 0 ? {} : { alt }),
          ...(caption === undefined || caption.length === 0 ? {} : { caption }),
          omitted: true,
        });
      }
      leadingMetadata = false;
    }
  }
  while (blocks[0]?.kind === 'divider') blocks.shift();
  while (blocks.at(-1)?.kind === 'divider') blocks.pop();
  return blocks;
}

export function blocksToText(blocks) {
  return blocks.map((block) => {
    if (block.kind === 'turn') {
      return `${block.role === 'user' ? 'User' : 'Assistant'}\n${block.text}`;
    }
    if (block.kind === 'paragraph' || block.kind === 'heading' || block.kind === 'blockquote' || block.kind === 'pre') {
      return block.text;
    }
    if (block.kind === 'linkCard') return `${block.text}\n${block.href}`;
    if (block.kind === 'list') return block.items.map((item, index) => `${block.ordered ? `${index + 1}.` : '•'} ${item}`).join('\n');
    if (block.kind === 'figure') {
      const details = [block.alt, block.caption].filter(Boolean).join(' — ');
      return details.length > 0 ? `[Image omitted: ${details}]` : '[Image omitted]';
    }
    if (block.kind === 'embed') return `[${block.provider ?? 'Media'} embed omitted${block.href === undefined ? '' : `: ${block.href}`}]`;
    return '—';
  }).join('\n\n').trim();
}

function makeExcerpt(subtitle, blocks, title) {
  const candidate = subtitle || blocks.find((block) => block.kind === 'paragraph')?.text || blocks.find((block) => block.kind === 'heading')?.text || title;
  if (candidate.length <= 280) return candidate;
  const prefix = candidate.slice(0, 277);
  const boundary = prefix.lastIndexOf(' ');
  return `${prefix.slice(0, boundary > 180 ? boundary : prefix.length).trim()}…`;
}

function canonicalAnchor(footer) {
  for (const match of footer.matchAll(/<a\b[^>]*>/gi)) {
    const tag = match[0];
    if (/\bp-canonical\b/i.test(readAttribute(tag, 'class') ?? '')) return safeHttpUrl(readAttribute(tag, 'href'));
  }
  return undefined;
}

function makeSlug(sourceUrl, filename) {
  let candidate;
  if (sourceUrl !== undefined) candidate = new URL(sourceUrl).pathname.split('/').filter(Boolean).at(-1);
  if (candidate === undefined || candidate.length === 0) {
    candidate = basename(filename, '.html').replace(/^\d{4}-\d{2}-\d{2}_/, '');
  }
  const slug = candidate.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '');
  if (slug.length === 0) throw new Error(`Could not derive public slug for ${filename}`);
  return slug;
}

export function parseMediumPost(html, filename, sourceSha256) {
  const titleMatch = html.match(/<h1\b[^>]*\bp-name\b[^>]*>([\s\S]*?)<\/h1\s*>/i)
    ?? html.match(/<title\b[^>]*>([\s\S]*?)<\/title\s*>/i);
  const subtitleMatch = html.match(/<section\b[^>]*data-field=["']subtitle["'][^>]*>([\s\S]*?)<\/section\s*>/i);
  const bodyMatch = html.match(/<section\b[^>]*data-field=["']body["'][^>]*>([\s\S]*?)<\/section\s*>\s*<footer\b/i);
  const footerMatch = html.match(/<footer\b[^>]*>([\s\S]*?)<\/footer\s*>/i);
  if (titleMatch === null || bodyMatch === null || footerMatch === null) throw new Error(`Malformed Medium export: ${filename}`);
  const title = plainText(titleMatch[1] ?? '');
  const subtitle = subtitleMatch === null ? '' : plainText(subtitleMatch[1] ?? '');
  const footer = footerMatch[1] ?? '';
  const publishedAt = footer.match(/<time\b[^>]*\bdt-published\b[^>]*\bdatetime=["']([^"']+)["'][^>]*>/i)?.[1];
  if (title.length === 0 || publishedAt === undefined || Number.isNaN(Date.parse(publishedAt))) {
    throw new Error(`Missing title or publication date: ${filename}`);
  }
  const sourceUrl = canonicalAnchor(footer);
  const slug = makeSlug(sourceUrl, filename);
  let blocks = parseMediumBlocks(bodyMatch[1] ?? '', title, subtitle);
  if (blocks.length === 0) blocks = parseMediumBlocks(bodyMatch[1] ?? '', '', '');
  const body = blocksToText(blocks);
  if (blocks.length === 0 || body.length === 0) throw new Error(`Empty approved body: ${filename}`);
  const updatedAt = parseExportDate(footer);
  const publishedIso = new Date(publishedAt).toISOString();
  return {
    slug,
    title,
    excerpt: makeExcerpt(subtitle, blocks, title),
    body,
    blocks,
    publishedAt: publishedIso,
    ...(updatedAt === undefined ? {} : { updatedAt }),
    year: new Date(publishedIso).getUTCFullYear(),
    source: 'Medium',
    type: 'Medium post',
    topics: [],
    canonicalUrl: `https://www.apocky.com/akashic-records/${slug}`,
    ...(sourceUrl === undefined ? {} : { sourceUrl, sourceUrlStatus: 'unverified' }),
    sourceSha256,
  };
}

const PUBLIC_REDACTIONS = [
  {
    label: 'private-key',
    pattern: /-----BEGIN (?:RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----[\s\S]*?-----END (?:RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----/gi,
  },
  {
    label: 'dsn',
    pattern: /\b(?:postgres(?:ql)?|mysql|mariadb|mongodb(?:\+srv)?|redis|amqp|mssql):\/\/[^\s<>"'`]+/gi,
  },
  {
    label: 'dsn',
    pattern: /\bhttps?:\/\/[^/\s:@]+(?::[^@\s/]*)?@[^\s<>"'`]+/gi,
  },
  {
    label: 'connection-string',
    pattern: /\b(?:Server|Data Source)=[^\r\n;]+;(?:[^\r\n;]+;)*(?:Password|Pwd)=[^\r\n;]+(?:;[^\r\n]*)?/gi,
  },
  {
    label: 'secret',
    pattern: /\b(?:[A-Z0-9_]*(?:API[_-]?KEY|ACCESS[_-]?TOKEN|SECRET[_-]?ACCESS[_-]?KEY|AUTH[_-]?TOKEN|CLIENT[_-]?SECRET|PASSWORD|PASSWD|SECRET|TOKEN))\s*[:=]\s*(?:"[^"\r\n]{8,}"|'[^'\r\n]{8,}'|[A-Za-z0-9_./+=-]{8,})/gi,
  },
  { label: 'secret', pattern: /\bAuthorization\s*:\s*(?:Bearer|Basic)\s+[A-Za-z0-9._~+/=-]{8,}/gi },
  { label: 'secret', pattern: /\bX-Amz-Signature=[A-Fa-f0-9]{16,}/gi },
  { label: 'token', pattern: /\bAKIA[0-9A-Z]{16}\b/g },
  { label: 'token', pattern: /\b(?:ghp|gho|ghu|ghs|ghr|github_pat)_[A-Za-z0-9_]{20,}\b/g },
  { label: 'token', pattern: /\bglpat-[A-Za-z0-9_-]{20,}\b/g },
  { label: 'token', pattern: /\bnpm_[A-Za-z0-9]{20,}\b/g },
  { label: 'token', pattern: /\bpypi-[A-Za-z0-9_-]{20,}\b/g },
  { label: 'token', pattern: /\bhf_[A-Za-z0-9]{20,}\b/g },
  { label: 'token', pattern: /\b[rp]_[A-Za-z0-9]{20,}\b/g },
  { label: 'token', pattern: /\b(?:sk|rk)_(?:live|test)_[A-Za-z0-9]{16,}\b/g },
  { label: 'token', pattern: /\bsk-(?:live-)?[A-Za-z0-9_-]{20,}\b/g },
  { label: 'token', pattern: /\bxox(?:a|b|p|r|s)-[A-Za-z0-9-]{10,}\b/g },
  { label: 'token', pattern: /\bAIza[0-9A-Za-z_-]{30,}\b/g },
  { label: 'token', pattern: /\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b/g },
  { label: 'file-uri', pattern: /\bfile:(?:\/\/)?/gi },
  { label: 'local-path', pattern: /(?<![A-Za-z])[A-Za-z]:[\\/]Users[\\/][^\s\\/<>"|?*]+/gi },
  { label: 'local-path', pattern: /(?<![A-Za-z])[A-Za-z]:[\\/]/g },
  { label: 'local-path', pattern: /\\\\[A-Za-z0-9._-]+\\(?:\\?[^\s\\<>"|?*]+(?:\\+|$))+/g },
  { label: 'local-path', pattern: /(?:\/Users\/[^\s/<>"'`]+|\/home\/[^\s/<>"'`]+|\/root|\/mnt\/[a-z]|\/tmp)(?=\/|\b)/gi },
  {
    label: 'email',
    pattern: /\b[A-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Z0-9](?:[A-Z0-9.-]*[A-Z0-9])?\b/gi,
  },
  {
    label: 'phone',
    pattern: /(?<!\d)(?:\+?1[ .-]?)?(?:\(\d{3}\)|\d{3})[ .-]\d{3}[ .-]\d{4}(?!\d)/g,
  },
  {
    label: 'phone',
    pattern: /(?<!\w)\+[2-9]\d{0,2}(?:[ .-]?\d){7,12}(?!\d)/g,
  },
  {
    label: 'ip-address',
    pattern: /(?<![\d.])(?:25[0-5]|2[0-4]\d|1?\d?\d)(?:\.(?:25[0-5]|2[0-4]\d|1?\d?\d)){3}(?![\d.])/g,
  },
  {
    label: 'ip-address',
    pattern: /\[(?:[A-Fa-f0-9]{0,4}:){2,7}[A-Fa-f0-9]{0,4}\]/g,
    validate: (candidate) => isIP(candidate.slice(1, -1)) === 6,
  },
  { label: 'ip-address', pattern: /(?<![A-Fa-f0-9:])::1(?![A-Fa-f0-9:])/g },
];

const UNIVERSAL_FORBIDDEN_PATTERNS = [
  /(?<![A-Za-z])[A-Za-z]:[\\/]/,
  /\\\\[A-Za-z0-9._-]+\\/,
  /(?:\/Users\/|\/home\/|\/root\/|\/mnt\/[a-z]\/|\/tmp\/)/i,
  /\bfile:/i,
  /<\/?(?:script|iframe|img)\b/i,
  /javascript:/i,
  /-----BEGIN (?:RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----/i,
  /\bAKIA[0-9A-Z]{16}\b/,
  /\b(?:ghp|gho|ghu|ghs|ghr|github_pat)_[A-Za-z0-9_]{20,}\b/,
  /\bglpat-[A-Za-z0-9_-]{20,}\b/,
  /\bnpm_[A-Za-z0-9]{20,}\b/,
  /\bpypi-[A-Za-z0-9_-]{20,}\b/,
  /\bhf_[A-Za-z0-9]{20,}\b/,
  /\b[rp]_[A-Za-z0-9]{20,}\b/,
  /\b(?:sk|rk)_(?:live|test)_[A-Za-z0-9]{16,}\b/,
  /\bsk-(?:live-)?[A-Za-z0-9_-]{20,}\b/,
  /\bxox(?:a|b|p|r|s)-[A-Za-z0-9-]{10,}\b/,
  /\bAIza[0-9A-Za-z_-]{30,}\b/,
  /\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b/,
  /\b(?:postgres(?:ql)?|mysql|mariadb|mongodb(?:\+srv)?|redis|amqp|mssql):\/\//i,
  /\bhttps?:\/\/[^/\s:@]+(?::[^@\s/]*)?@/i,
  /\b(?:Server|Data Source)=[^\r\n;]+;(?:[^\r\n;]+;)*(?:Password|Pwd)=/i,
  /\b(?:[A-Z0-9_]*(?:API[_-]?KEY|ACCESS[_-]?TOKEN|SECRET[_-]?ACCESS[_-]?KEY|AUTH[_-]?TOKEN|CLIENT[_-]?SECRET|PASSWORD|PASSWD|SECRET|TOKEN))\s*[:=]\s*(?:"[^"\r\n]{8,}"|'[^'\r\n]{8,}'|[A-Za-z0-9_./+=-]{8,})/i,
  /\bAuthorization\s*:\s*(?:Bearer|Basic)\s+[A-Za-z0-9._~+/=-]{8,}/i,
  /\bX-Amz-Signature=[A-Fa-f0-9]{16,}/i,
];

const CODEX_PII_PATTERNS = [
  /\b[A-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Z0-9](?:[A-Z0-9.-]*[A-Z0-9])?\b/i,
  /(?<!\d)(?:\+?1[ .-]?)?(?:\(\d{3}\)|\d{3})[ .-]\d{3}[ .-]\d{4}(?!\d)/,
  /(?<!\w)\+[2-9]\d{0,2}(?:[ .-]?\d){7,12}(?!\d)/,
  /(?<![\d.])(?:25[0-5]|2[0-4]\d|1?\d?\d)(?:\.(?:25[0-5]|2[0-4]\d|1?\d?\d)){3}(?![\d.])/,
];

const IPV6_CANDIDATE = /\[(?:[A-Fa-f0-9]{0,4}:){2,7}[A-Fa-f0-9]{0,4}\]|(?<![A-Fa-f0-9:])::1(?![A-Fa-f0-9:])/g;

export function sanitizePublicConversationText(value) {
  let text = String(value).replace(/\r\n?/g, '\n');
  let redactionCount = 0;
  for (const { label, pattern, validate } of PUBLIC_REDACTIONS) {
    pattern.lastIndex = 0;
    text = text.replace(pattern, (candidate) => {
      if (validate !== undefined && !validate(candidate)) return candidate;
      redactionCount += 1;
      return `[redacted:${label}]`;
    });
  }
  return { text, redactionCount };
}

function allStringValues(value, values = [], path = '') {
  if (typeof value === 'string') values.push(value);
  else if (Array.isArray(value)) value.forEach((item, index) => allStringValues(item, values, `${path}[${index}]`));
  else if (value !== null && typeof value === 'object') {
    Object.entries(value).forEach(([key, item]) => {
      if (key === 'sourceSha256' || key === 'sourceLineageSha256') return;
      allStringValues(item, values, path === '' ? key : `${path}.${key}`);
    });
  }
  return values;
}

function assertNoResidualPrivateContent(value, { includePii = false, label = 'snapshot' } = {}) {
  const patterns = includePii ? [...UNIVERSAL_FORBIDDEN_PATTERNS, ...CODEX_PII_PATTERNS] : UNIVERSAL_FORBIDDEN_PATTERNS;
  for (const stringValue of allStringValues(value)) {
    for (const pattern of patterns) {
      pattern.lastIndex = 0;
      if (pattern.test(stringValue)) throw new Error(`Residual private/executable pattern in ${label}: ${pattern}`);
    }
    IPV6_CANDIDATE.lastIndex = 0;
    for (const match of stringValue.matchAll(IPV6_CANDIDATE)) {
      const candidate = match[0].startsWith('[') ? match[0].slice(1, -1) : match[0];
      if (isIP(candidate) === 6) throw new Error(`Residual IPv6 address in ${label}`);
    }
  }
}

function parseFrontmatterValue(markdown, key) {
  const normalized = markdown.replace(/\r\n/g, '\n');
  if (!normalized.startsWith('---\n')) throw new Error('Selected Codex note is missing YAML frontmatter');
  const end = normalized.indexOf('\n---\n', 4);
  if (end === -1) throw new Error('Selected Codex note has unterminated YAML frontmatter');
  const frontmatter = normalized.slice(4, end);
  const escapedKey = key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const matches = [...frontmatter.matchAll(new RegExp(`^${escapedKey}:\\s*(.*?)\\s*$`, 'gm'))];
  if (matches.length !== 1) throw new Error(`Selected Codex note must contain exactly one ${key} field`);
  const raw = matches[0][1] ?? '';
  if (raw.startsWith('"')) {
    try {
      return JSON.parse(raw);
    } catch {
      throw new Error(`Selected Codex note contains invalid quoted ${key}`);
    }
  }
  if (raw.startsWith("'") && raw.endsWith("'")) return raw.slice(1, -1).replace(/''/g, "'");
  if (/^-?\d+$/.test(raw)) return Number.parseInt(raw, 10);
  if (raw === 'null' || raw === '~') return null;
  return raw;
}

function maybeFrontmatterValue(markdown, key) {
  try {
    return parseFrontmatterValue(markdown, key);
  } catch (error) {
    if (error instanceof Error && error.message.endsWith(`exactly one ${key} field`)) return undefined;
    throw error;
  }
}

function parseVaultTurns(markdown, expectedChunkBytes) {
  const generatedAt = markdown.indexOf(VAULTSYNC_GENERATED_MARKER);
  if (generatedAt === -1) throw new Error('Selected Codex note is not a vaultsync-generated projection');
  const transcript = markdown.slice(generatedAt + VAULTSYNC_GENERATED_MARKER.length);
  const matches = [...transcript.matchAll(VAULTSYNC_TURN_MARKER)];
  if (matches.length === 0) throw new Error('Selected Codex note has no authenticated turn markers');
  if ((transcript.match(/<!-- vaultsync:turn\b/g) ?? []).length !== matches.length) {
    throw new Error('Selected Codex note contains malformed or injected turn markers');
  }

  const firstMarkerAt = matches[0].index;
  const projectionWithMarkers = transcript.slice(firstMarkerAt);
  const canonicalMarkerlessProjection = projectionWithMarkers.replace(
    /^<!-- vaultsync:turn role=(?:user|assistant) -->\r?\n/gm,
    '',
  );
  const measuredChunkBytes = Buffer.byteLength(canonicalMarkerlessProjection, 'utf8');
  if (measuredChunkBytes !== expectedChunkBytes) {
    throw new Error(`Selected Codex note projection byte drift: expected ${expectedChunkBytes}, observed ${measuredChunkBytes}`);
  }

  return matches.map((match, index) => {
    const role = match[1];
    const expectedHeading = role === 'user' ? 'Human' : 'Assistant';
    if (match[2] !== expectedHeading) throw new Error(`Turn marker/heading mismatch for ${role}`);
    const start = (match.index ?? 0) + match[0].length;
    const hasNext = index + 1 < matches.length;
    const end = hasNext ? matches[index + 1].index : transcript.length;
    let text = transcript.slice(start, end);
    const separator = hasNext ? '\n\n' : '\n';
    if (!text.endsWith(separator)) throw new Error('Selected Codex note has malformed turn boundaries');
    text = text.slice(0, -separator.length);
    return { role, text };
  });
}

export function parseVaultCodexNote(markdown, selection, expectedChunkBytes) {
  const sessionFile = parseFrontmatterValue(markdown, 'session_file');
  const sessionId = parseFrontmatterValue(markdown, 'session_id');
  const sourceSha256 = parseFrontmatterValue(markdown, 'source_sha256');
  const sourceBytes = parseFrontmatterValue(markdown, 'source_bytes');
  const startUtc = parseFrontmatterValue(markdown, 'start_utc');
  const endUtc = parseFrontmatterValue(markdown, 'end_utc');
  const publicationState = parseFrontmatterValue(markdown, 'publication_state');
  const privacyReview = parseFrontmatterValue(markdown, 'privacy_review');
  if (sessionFile !== selection.rollout_rel) throw new Error(`Selected Codex note session drift for ${selection.id}`);
  if (sessionId !== selection.id) throw new Error(`Selected Codex note id drift for ${selection.id}`);
  if (sourceSha256 !== selection.source_sha256) throw new Error(`Selected Codex note source hash drift for ${selection.id}`);
  if (sourceBytes !== selection.source_bytes) throw new Error(`Selected Codex note source byte drift for ${selection.id}`);
  if (startUtc !== selection.start_utc || endUtc !== selection.end_utc) throw new Error(`Selected Codex note time drift for ${selection.id}`);
  if (publicationState !== 'approved') throw new Error(`Selected Codex note publication state drift for ${selection.id}`);
  if (privacyReview !== selection.privacy_review) throw new Error(`Selected Codex note privacy review drift for ${selection.id}`);
  return {
    part: maybeFrontmatterValue(markdown, 'part') ?? 1,
    parts: maybeFrontmatterValue(markdown, 'parts') ?? 1,
    messages: parseVaultTurns(markdown, expectedChunkBytes),
  };
}

function validateCodexSelection(manifest) {
  const allowedTopLevel = new Set(['schema', 'selection_created_utc', 'scope', 'approval', 'sessions']);
  const unexpectedTopLevel = Object.keys(manifest ?? {}).filter((key) => !allowedTopLevel.has(key));
  if (unexpectedTopLevel.length > 0) throw new Error(`Unexpected Codex selection fields: ${unexpectedTopLevel.join(', ')}`);
  if (manifest?.schema !== CODEX_SELECTION_SCHEMA) throw new Error(`Unsupported Codex selection schema: ${manifest?.schema}`);
  if (typeof manifest.selection_created_utc !== 'string' || Number.isNaN(Date.parse(manifest.selection_created_utc))) {
    throw new Error('Codex selection has an invalid creation timestamp');
  }
  if (typeof manifest.scope !== 'string' || manifest.scope.trim().length === 0) throw new Error('Codex selection has no scope');
  if (
    manifest.approval === null
    || typeof manifest.approval !== 'object'
    || Array.isArray(manifest.approval)
    || Object.keys(manifest.approval).sort().join('\0') !== ['approved_at', 'approved_by', 'instruction'].sort().join('\0')
  ) throw new Error('Codex selection approval shape is ambiguous');
  if (!Array.isArray(manifest.sessions) || manifest.sessions.length === 0) throw new Error('Codex selection has no sessions');
  if (
    manifest.approval?.approved_by !== 'vault owner'
    || typeof manifest.approval?.approved_at !== 'string'
    || Number.isNaN(Date.parse(`${manifest.approval.approved_at}T00:00:00Z`))
    || typeof manifest.approval?.instruction !== 'string'
    || manifest.approval.instruction.trim().length === 0
  ) {
    throw new Error('Codex selection lacks explicit vault-owner publication approval');
  }
  const ids = new Set();
  const rolloutPaths = new Set();
  for (const selected of manifest.sessions) {
    const allowedSessionFields = new Set([
      'id', 'rollout_rel', 'source_bytes', 'source_sha256', 'title', 'start_utc', 'end_utc', 'action',
      'chunk_count', 'chunk_bytes', 'privacy_review', 'publication_state', 'content_notice',
      'public_projection', 'withheld_reason',
    ]);
    const unexpectedSessionFields = Object.keys(selected).filter((key) => !allowedSessionFields.has(key));
    if (unexpectedSessionFields.length > 0) throw new Error(`Unexpected selected Codex session fields for ${selected.id}: ${unexpectedSessionFields.join(', ')}`);
    if (!/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i.test(selected.id ?? '')) {
      throw new Error(`Invalid selected Codex session id: ${selected.id}`);
    }
    if (ids.has(selected.id)) throw new Error(`Duplicate selected Codex session id: ${selected.id}`);
    ids.add(selected.id);
    if (
      typeof selected.rollout_rel !== 'string'
      || !/^\d{4}\/\d{2}\/\d{2}\/rollout-[^/]+\.jsonl$/.test(selected.rollout_rel)
      || selected.rollout_rel.includes('..')
    ) throw new Error(`Unsafe selected Codex rollout path: ${selected.rollout_rel}`);
    if (rolloutPaths.has(selected.rollout_rel)) throw new Error(`Duplicate selected Codex rollout path: ${selected.rollout_rel}`);
    rolloutPaths.add(selected.rollout_rel);
    if (!Number.isSafeInteger(selected.source_bytes) || selected.source_bytes <= 0) throw new Error(`Invalid source bytes for ${selected.id}`);
    if (!/^[a-f0-9]{64}$/.test(selected.source_sha256 ?? '')) throw new Error(`Invalid source hash for ${selected.id}`);
    if (typeof selected.title !== 'string' || selected.title.trim().length === 0) throw new Error(`Missing title for ${selected.id}`);
    if (Number.isNaN(Date.parse(selected.start_utc)) || Number.isNaN(Date.parse(selected.end_utc))) {
      throw new Error(`Invalid selected time range for ${selected.id}`);
    }
    if (!['new', 'refresh'].includes(selected.action)) throw new Error(`Invalid vault delta action for ${selected.id}`);
    if (!Number.isSafeInteger(selected.chunk_count) || selected.chunk_count <= 0) throw new Error(`Invalid chunk count for ${selected.id}`);
    if (
      !Array.isArray(selected.chunk_bytes)
      || selected.chunk_bytes.length !== selected.chunk_count
      || selected.chunk_bytes.some((bytes) => !Number.isSafeInteger(bytes) || bytes <= 0)
    ) throw new Error(`Invalid chunk byte denominator for ${selected.id}`);
    if (typeof selected.privacy_review !== 'string' || selected.privacy_review.trim().length === 0) {
      throw new Error(`Missing privacy review for ${selected.id}`);
    }
    if (selected.publication_state !== 'approved') throw new Error(`Unapproved selected Codex session: ${selected.id}`);
    if (selected.content_notice !== undefined && (typeof selected.content_notice !== 'string' || selected.content_notice.trim().length === 0)) {
      throw new Error(`Invalid content notice for ${selected.id}`);
    }
    const publicProjection = selected.public_projection ?? 'transcript';
    if (!['transcript', 'withheld'].includes(publicProjection)) throw new Error(`Invalid public projection for ${selected.id}`);
    if (
      publicProjection === 'withheld'
      && (
        typeof selected.withheld_reason !== 'string'
        || selected.withheld_reason.trim().length === 0
        || typeof selected.content_notice !== 'string'
        || selected.content_notice.trim().length === 0
      )
    ) throw new Error(`Withheld Codex projection lacks public reason/notice for ${selected.id}`);
    if (publicProjection === 'transcript' && selected.withheld_reason !== undefined) {
      throw new Error(`Published Codex transcript has a contradictory withheld reason: ${selected.id}`);
    }
  }
}

function validateCodexReceipt(receipt, manifest, manifestBytes, manifestPath) {
  const allowedTopLevel = new Set([
    'schema', 'observed_at', 'mode', 'selection_manifest', 'selection_sha256', 'selection_created_utc',
    'scope', 'approval', 'denominator', 'sessions', 'index_note', 'stale_parts', 'boundaries', 'effects',
  ]);
  const unexpectedTopLevel = Object.keys(receipt ?? {}).filter((key) => !allowedTopLevel.has(key));
  if (unexpectedTopLevel.length > 0) throw new Error(`Unexpected Codex receipt fields: ${unexpectedTopLevel.join(', ')}`);
  if (receipt?.schema !== 'vaultsync.codex-session-import-receipt.v1') throw new Error(`Unsupported Codex receipt schema: ${receipt?.schema}`);
  if (receipt.mode !== 'apply') throw new Error('Codex receipt does not prove applied vault effects');
  if (receipt.selection_sha256 !== sha256(manifestBytes)) throw new Error('Codex receipt selection hash drift');
  const normalizedSelectionReference = String(receipt.selection_manifest ?? '').replace(/\\/g, '/');
  if (!normalizedSelectionReference.endsWith(`/selections/${basename(manifestPath)}`)) {
    throw new Error('Codex receipt selection path drift');
  }
  if (
    receipt.selection_created_utc !== manifest.selection_created_utc
    || JSON.stringify(receipt.approval) !== JSON.stringify(manifest.approval)
    || receipt.scope !== manifest.scope
  ) throw new Error('Codex receipt selection metadata drift');
  if (
    receipt.denominator?.selected_sessions !== manifest.sessions.length
    || !Number.isSafeInteger(receipt.denominator?.rendered_notes)
    || receipt.denominator.rendered_notes <= 0
    || receipt.denominator?.session_scoped_stale_parts !== 0
    || !Array.isArray(receipt.sessions)
    || receipt.sessions.length !== manifest.sessions.length
    || !Array.isArray(receipt.stale_parts)
    || receipt.stale_parts.length !== 0
  ) throw new Error('Codex receipt denominator drift');
  if (
    receipt.boundaries?.global_inventory_run !== false
    || receipt.boundaries?.global_moc_written !== false
    || receipt.boundaries?.global_index_written !== false
    || receipt.boundaries?.global_prune_run !== false
    || receipt.boundaries?.source_files_written !== false
  ) throw new Error('Codex receipt exceeded the scoped import boundary');
  if (receipt.effects !== undefined) {
    const effectKeys = ['created', 'updated', 'unchanged', 'refused', 'pruned_session_stale'];
    if (
      receipt.effects === null
      || typeof receipt.effects !== 'object'
      || Array.isArray(receipt.effects)
      || Object.keys(receipt.effects).sort().join('\0') !== effectKeys.sort().join('\0')
      || effectKeys.some((key) => !Number.isSafeInteger(receipt.effects[key]) || receipt.effects[key] < 0)
      || receipt.effects.refused !== 0
      || receipt.effects.pruned_session_stale !== 0
    ) throw new Error('Codex receipt optional effect accounting is malformed');
  }
  if (
    receipt.denominator?.selection_indexes !== 1
    || receipt.index_note === null
    || typeof receipt.index_note !== 'object'
    || typeof receipt.index_note.vault_rel !== 'string'
    || !receipt.index_note.vault_rel.startsWith('03 Research/AI Conversations/Codex sessions/')
    || receipt.index_note.vault_rel.includes('..')
    || !Number.isSafeInteger(receipt.index_note.bytes)
    || receipt.index_note.bytes <= 0
    || !/^[a-f0-9]{64}$/.test(receipt.index_note.sha256 ?? '')
  ) throw new Error('Codex receipt index-note drift');
  const receiptById = new Map(receipt.sessions.map((session) => [session.id, session]));
  if (receiptById.size !== manifest.sessions.length) throw new Error('Codex receipt contains duplicate session ids');
  let noteCount = 0;
  for (const selected of manifest.sessions) {
    const session = receiptById.get(selected.id);
    if (
      session === undefined
      || session.rollout_rel !== selected.rollout_rel
      || session.source_bytes !== selected.source_bytes
      || session.source_sha256 !== selected.source_sha256
      || (session.public_projection ?? 'transcript') !== (selected.public_projection ?? 'transcript')
      || (session.withheld_reason ?? undefined) !== (selected.withheld_reason ?? undefined)
      || !Array.isArray(session.notes)
      || session.notes.length !== selected.chunk_count
    ) throw new Error(`Codex receipt session drift for ${selected.id}`);
    noteCount += session.notes.length;
    const parts = new Set();
    for (const note of session.notes) {
      if (
        typeof note.vault_rel !== 'string'
        || !note.vault_rel.startsWith('03 Research/AI Conversations/Codex sessions/')
        || note.vault_rel.includes('..')
        || !Number.isSafeInteger(note.part)
        || note.part < 1
        || note.part > selected.chunk_count
        || !Number.isSafeInteger(note.bytes)
        || note.bytes <= 0
        || !/^[a-f0-9]{64}$/.test(note.sha256 ?? '')
      ) throw new Error(`Invalid Codex receipt note for ${selected.id}`);
      if (parts.has(note.part)) throw new Error(`Duplicate Codex receipt note part for ${selected.id}`);
      parts.add(note.part);
    }
  }
  if (noteCount !== receipt.denominator.rendered_notes) throw new Error('Codex receipt note coverage drift');
  return receiptById;
}

async function readVerifiedVaultNote(sourceDirectory, receiptNote) {
  const root = resolve(sourceDirectory);
  const rootInfo = await lstat(root);
  if (!rootInfo.isDirectory() || rootInfo.isSymbolicLink()) throw new Error('Codex vault source must be a real local directory, not a link');
  const expectedPrefix = '03 Research/AI Conversations/Codex sessions/';
  const relativeNote = receiptNote.vault_rel.slice(expectedPrefix.length);
  if (relativeNote.includes('/') || relativeNote.includes('\\')) throw new Error(`Nested Codex receipt note is not allowed: ${receiptNote.vault_rel}`);
  const filePath = join(root, relativeNote);
  const [before, fileReal, bytes] = await Promise.all([lstat(filePath), realpath(filePath), readFile(filePath)]);
  assertInside(root, fileReal, 'Codex vault note');
  if (!before.isFile() || before.isSymbolicLink() || before.nlink !== 1) {
    throw new Error(`Selected Codex note must be a regular single-link file: ${relativeNote}`);
  }
  if (before.size !== bytes.byteLength || bytes.byteLength !== receiptNote.bytes || sha256(bytes) !== receiptNote.sha256) {
    throw new Error(`Codex receipt note byte/hash drift: ${relativeNote}`);
  }
  const [after, afterReal] = await Promise.all([lstat(filePath), realpath(filePath)]);
  if (
    !after.isFile()
    || after.isSymbolicLink()
    || after.nlink !== 1
    || after.size !== before.size
    || after.mtimeMs !== before.mtimeMs
    || afterReal !== fileReal
  ) throw new Error(`Selected Codex note changed while being read: ${relativeNote}`);
  return bytes.toString('utf8');
}

function makeConversationExcerpt(messages, title) {
  const candidate = messages.find((message) => message.role === 'user')?.text
    ?? messages[0]?.text
    ?? title;
  const normalized = candidate.replace(/\s+/g, ' ').trim() || title;
  if (normalized.length <= 280) return normalized;
  const prefix = normalized.slice(0, 277);
  const boundary = prefix.lastIndexOf(' ');
  return `${prefix.slice(0, boundary > 180 ? boundary : prefix.length).trim()}…`;
}

function projectionBytes(messages) {
  return Buffer.byteLength(JSON.stringify(messages.map(({ role, text }) => ({ role, text }))), 'utf8');
}

function splitConversationMessages(messages, conversationId) {
  const chunks = [];
  let current = [];
  for (const message of messages) {
    const candidate = [...current, message];
    if (projectionBytes(candidate) > PUBLIC_PROJECTION_MAX_BYTES) {
      if (current.length === 0) throw new Error(`Codex message exceeds the 96 KiB public projection boundary: ${conversationId}`);
      chunks.push(current);
      current = [message];
      if (projectionBytes(current) > PUBLIC_PROJECTION_MAX_BYTES) {
        throw new Error(`Codex message exceeds the 96 KiB public projection boundary: ${conversationId}`);
      }
    } else {
      current = candidate;
    }
  }
  if (current.length > 0) chunks.push(current);
  if (chunks.length === 0) throw new Error(`Selected Codex conversation is empty: ${conversationId}`);
  return chunks;
}

function makeCodexRecords(selection, sanitizedMessages) {
  const chunks = splitConversationMessages(sanitizedMessages, selection.id);
  const conversationProjection = Buffer.from(
    JSON.stringify(sanitizedMessages.map(({ role, text }) => ({ role, text }))),
    'utf8',
  );
  const conversationProjectionSha256 = sha256(conversationProjection);
  return chunks.map((messages, index) => {
    const part = index + 1;
    const parts = chunks.length;
    const publicMessages = messages.map(({ role, text }) => ({ role, text }));
    const projection = Buffer.from(JSON.stringify(publicMessages), 'utf8');
    const blocks = publicMessages.map(({ role, text }) => ({ kind: 'turn', role, text }));
    const publishedAt = new Date(selection.start_utc).toISOString();
    const updatedAt = new Date(selection.end_utc).toISOString();
    const slug = `codex-${selection.id}-part-${part}`;
    const title = parts === 1 ? selection.title : `${selection.title} — Part ${part} of ${parts}`;
    const redactionCount = messages.reduce((total, message) => total + message.redactionCount, 0)
      + (index === 0 ? (selection.metadataRedactionCount ?? 0) : 0);
    const record = {
      slug,
      title,
      excerpt: makeConversationExcerpt(publicMessages, title),
      body: blocksToText(blocks),
      blocks,
      publishedAt,
      updatedAt,
      recordedAt: publishedAt,
      year: new Date(publishedAt).getUTCFullYear(),
      source: 'Codex',
      type: 'Conversation transcript',
      topics: [],
      canonicalUrl: `https://www.apocky.com/akashic-records/${slug}`,
      sourceSha256: conversationProjectionSha256,
      conversationId: selection.id,
      part,
      parts,
      messageCount: messages.length,
      redactionCount,
      projectionBytes: projection.byteLength,
      projectionSha256: sha256(projection),
      ...(selection.content_notice === undefined ? {} : { contentNotice: selection.content_notice }),
    };
    if (
      record.title.trim().length === 0
      || record.excerpt.trim().length === 0
      || (record.contentNotice !== undefined && record.contentNotice.trim().length === 0)
    ) throw new Error(`Codex sanitizer erased required public metadata: ${slug}`);
    if (record.projectionBytes > PUBLIC_PROJECTION_MAX_BYTES) throw new Error(`Oversize Codex projection record: ${slug}`);
    assertNoResidualPrivateContent(record, { includePii: true, label: slug });
    return record;
  });
}

function makeWithheldCodexRecord(selection, withheldMessageCount) {
  const sanitizedTitle = sanitizePublicConversationText(selection.title);
  const sanitizedNotice = sanitizePublicConversationText(selection.content_notice);
  const sanitizedReason = sanitizePublicConversationText(selection.withheld_reason);
  const title = sanitizedTitle.text;
  const contentNotice = sanitizedNotice.text;
  const withheldReason = sanitizedReason.text;
  if ([title, contentNotice, withheldReason].some((value) => value.trim().length === 0)) {
    throw new Error(`Codex sanitizer erased withheld projection metadata: ${selection.id}`);
  }
  const text = `${contentNotice}\n\nTranscript withheld: ${withheldReason}`;
  const blocks = [{ kind: 'paragraph', text: contentNotice }, { kind: 'paragraph', text: `Transcript withheld: ${withheldReason}` }];
  const projection = Buffer.from(JSON.stringify(blocks), 'utf8');
  const publishedAt = new Date(selection.start_utc).toISOString();
  const updatedAt = new Date(selection.end_utc).toISOString();
  const slug = `codex-${selection.id}-part-1`;
  const record = {
    slug,
    title,
    excerpt: contentNotice,
    body: text,
    blocks,
    publishedAt,
    updatedAt,
    recordedAt: publishedAt,
    year: new Date(publishedAt).getUTCFullYear(),
    source: 'Codex',
    type: 'Conversation transcript',
    topics: [],
    canonicalUrl: `https://www.apocky.com/akashic-records/${slug}`,
    sourceSha256: sha256(projection),
    conversationId: selection.id,
    part: 1,
    parts: 1,
    messageCount: 0,
    withheldMessageCount,
    redactionCount: sanitizedTitle.redactionCount + sanitizedNotice.redactionCount + sanitizedReason.redactionCount,
    projectionBytes: projection.byteLength,
    projectionSha256: sha256(projection),
    publicationState: 'withheld',
    contentNotice,
    withheldReason,
  };
  assertNoResidualPrivateContent(record, { includePii: true, label: slug });
  return record;
}

export async function projectCodexConversations(sourceDirectory, manifestPath, receiptPath) {
  const manifestBytes = await readFile(resolve(manifestPath));
  let manifest;
  try {
    manifest = JSON.parse(manifestBytes.toString('utf8'));
  } catch {
    throw new Error('Codex selection manifest is not valid UTF-8 JSON');
  }
  validateCodexSelection(manifest);
  const receiptBytes = await readFile(resolve(receiptPath));
  let receipt;
  try {
    receipt = JSON.parse(receiptBytes.toString('utf8'));
  } catch {
    throw new Error('Codex import receipt is not valid UTF-8 JSON');
  }
  const receiptById = validateCodexReceipt(receipt, manifest, manifestBytes, manifestPath);
  await readVerifiedVaultNote(sourceDirectory, receipt.index_note);
  const records = [];
  let redactionCount = 0;
  let transcriptPublishedCount = 0;
  let withheldCount = 0;
  let verifiedMessageCount = 0;
  let publishedMessageCount = 0;
  let withheldMessageCount = 0;
  for (const selection of manifest.sessions) {
    const notes = receiptById.get(selection.id).notes;
    const parsed = [];
    for (const note of notes) {
      const markdown = await readVerifiedVaultNote(sourceDirectory, note);
      const declaredPart = maybeFrontmatterValue(markdown, 'part') ?? 1;
      if (!Number.isSafeInteger(declaredPart) || declaredPart < 1 || declaredPart > selection.chunk_count) {
        throw new Error(`Invalid selected Codex note part for ${selection.id}`);
      }
      if (declaredPart !== note.part) throw new Error(`Codex receipt/frontmatter part drift for ${selection.id}`);
      parsed.push(parseVaultCodexNote(markdown, selection, selection.chunk_bytes[declaredPart - 1]));
    }
    parsed.sort((left, right) => left.part - right.part);
    if (
      parsed.some((part, index) => part.part !== index + 1 || part.parts !== selection.chunk_count)
      || new Set(parsed.map((part) => part.part)).size !== selection.chunk_count
    ) throw new Error(`Selected Codex note part denominator drift for ${selection.id}`);
    const verifiedMessages = parsed.flatMap((part) => part.messages);
    verifiedMessageCount += verifiedMessages.length;
    if ((selection.public_projection ?? 'transcript') === 'withheld') {
      const withheldRecord = makeWithheldCodexRecord(selection, verifiedMessages.length);
      redactionCount += withheldRecord.redactionCount;
      withheldCount += 1;
      withheldMessageCount += verifiedMessages.length;
      records.push(withheldRecord);
      continue;
    }
    const sanitizedTitle = sanitizePublicConversationText(selection.title);
    const sanitizedContentNotice = selection.content_notice === undefined
      ? undefined
      : sanitizePublicConversationText(selection.content_notice);
    redactionCount += sanitizedTitle.redactionCount + (sanitizedContentNotice?.redactionCount ?? 0);
    const publicSelection = {
      ...selection,
      title: sanitizedTitle.text,
      metadataRedactionCount: sanitizedTitle.redactionCount + (sanitizedContentNotice?.redactionCount ?? 0),
      ...(sanitizedContentNotice === undefined ? {} : { content_notice: sanitizedContentNotice.text }),
    };
    const sanitizedMessages = verifiedMessages.map((message) => {
      const sanitized = sanitizePublicConversationText(message.text);
      redactionCount += sanitized.redactionCount;
      return { role: message.role, text: sanitized.text, redactionCount: sanitized.redactionCount };
    });
    records.push(...makeCodexRecords(publicSelection, sanitizedMessages));
    transcriptPublishedCount += 1;
    publishedMessageCount += sanitizedMessages.length;
  }
  return {
    records,
    conversationCount: manifest.sessions.length,
    transcriptPublishedCount,
    withheldCount,
    verifiedMessageCount,
    publishedMessageCount,
    withheldMessageCount,
    sourceBytes: manifest.sessions.reduce((total, selected) => total + selected.source_bytes, 0),
    vaultNoteCount: manifest.sessions.reduce((total, selected) => total + selected.chunk_count, 0),
    privateSelectionSeal: sha256(manifestBytes),
    redactionCount,
    approval: {
      approvedBy: manifest.approval.approved_by,
      approvedAt: manifest.approval.approved_at,
    },
  };
}

function validateRecords(records) {
  const slugs = new Set();
  for (const record of records) {
    if (slugs.has(record.slug)) throw new Error(`Duplicate public slug: ${record.slug}`);
    slugs.add(record.slug);
    if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(record.slug)) throw new Error(`Unsafe slug: ${record.slug}`);
    if (!/^[a-f0-9]{64}$/.test(record.sourceSha256)) throw new Error(`Invalid source hash: ${record.slug}`);
    for (const block of record.blocks) {
      if (block.kind === 'turn') {
        if (record.source !== 'Codex' || !['user', 'assistant'].includes(block.role) || typeof block.text !== 'string') {
          throw new Error(`Invalid transcript turn: ${record.slug}`);
        }
      }
      if (block.kind === 'paragraph' || block.kind === 'heading' || block.kind === 'blockquote') {
        for (const link of block.links ?? []) {
          if (safeHttpUrl(link.href) === undefined || link.text !== block.text.slice(link.start, link.end)) {
            throw new Error(`Invalid link projection: ${record.slug}`);
          }
        }
      }
      if (block.kind === 'embed' && block.href !== undefined && safeHttpUrl(block.href) === undefined) {
        throw new Error(`Unsafe embed link: ${record.slug}`);
      }
      if (block.kind === 'linkCard' && safeHttpUrl(block.href) === undefined) {
        throw new Error(`Unsafe link card: ${record.slug}`);
      }
    }
    if (record.body !== blocksToText(record.blocks)) throw new Error(`Body readback drift: ${record.slug}`);
    if (record.source === 'Codex') {
      if (record.publicationState === 'withheld') {
        const projection = Buffer.from(JSON.stringify(record.blocks), 'utf8');
        if (
          record.messageCount !== 0
          || !Number.isSafeInteger(record.withheldMessageCount)
          || record.withheldMessageCount <= 0
          || record.blocks.some((block) => block.kind === 'turn')
          || typeof record.withheldReason !== 'string'
          || record.withheldReason.length === 0
          || record.projectionBytes !== projection.byteLength
          || record.projectionSha256 !== sha256(projection)
        ) throw new Error(`Invalid withheld transcript projection: ${record.slug}`);
        assertNoResidualPrivateContent(record, { includePii: true, label: record.slug });
        continue;
      }
      const messages = record.blocks.map((block) => ({ role: block.role, text: block.text }));
      const projection = Buffer.from(JSON.stringify(messages), 'utf8');
      if (
        record.messageCount !== messages.length
        || record.projectionBytes !== projection.byteLength
        || record.projectionBytes > PUBLIC_PROJECTION_MAX_BYTES
        || record.projectionSha256 !== sha256(projection)
      ) throw new Error(`Transcript projection drift: ${record.slug}`);
      assertNoResidualPrivateContent(record, { includePii: true, label: record.slug });
    } else {
      assertNoResidualPrivateContent(record, { label: record.slug });
    }
  }
}

export async function buildSnapshot(sourceRoot, codexSourceDirectory, codexManifestPath, codexReceiptPath) {
  const inventory = await inventoryApprovedMediumPosts(sourceRoot);
  const sourceBytes = inventory.approved.reduce((total, post) => total + post.byteLength, 0);
  if (
    inventory.approvedCount !== APPROVED_RECORD_COUNT
    || inventory.draftExcludedCount !== EXCLUDED_DRAFT_COUNT
    || inventory.approvedCount + inventory.draftExcludedCount !== MEDIUM_EXPORT_COUNT
    || sourceBytes !== APPROVED_SOURCE_BYTES
    || inventory.sourceIdentitySha256 !== APPROVED_SOURCE_IDENTITY_SHA256
  ) {
    throw new Error(
      `Frozen source mismatch: ${inventory.approvedCount} approved, ${inventory.draftExcludedCount} drafts, ${sourceBytes} bytes, ${inventory.sourceIdentitySha256}`,
    );
  }
  const mediumRecords = inventory.approved.map((post) => parseMediumPost(post.bytes.toString('utf8'), post.filename, post.sourceSha256));
  const codex = await projectCodexConversations(codexSourceDirectory, codexManifestPath, codexReceiptPath);
  const records = [...mediumRecords, ...codex.records];
  records.sort((left, right) => right.publishedAt.localeCompare(left.publishedAt) || left.slug.localeCompare(right.slug));
  validateRecords(records);
  const snapshotDate = [...records.map((record) => record.updatedAt).filter(Boolean), `${codex.approval.approvedAt}T00:00:00.000Z`]
    .sort()
    .at(-1)
    ?.slice(0, 10);
  const sourceSets = [
    {
      id: 'medium',
      source: 'Medium',
      sourceKind: 'author-approved Medium export posts',
      publicationRule: 'all non-draft posts in the frozen v1 source denominator',
      sourceCount: MEDIUM_EXPORT_COUNT,
      approvedCount: APPROVED_RECORD_COUNT,
      excludedCount: EXCLUDED_DRAFT_COUNT,
      recordCount: mediumRecords.length,
      sourceBytes,
      sourceSeal: APPROVED_SOURCE_IDENTITY_SHA256,
      sourceSealAlgorithm: MEDIUM_SOURCE_SEAL_ALGORITHM,
    },
    {
      id: 'codex',
      source: 'Codex',
      sourceKind: 'owner-approved Codex platform conversations projected through the Obsidian vault',
      publicationRule: 'every session in the exact approved vaultsync selection denominator; no unapproved or incomplete session',
      sourceCount: codex.conversationCount,
      approvedCount: codex.conversationCount,
      conversationCount: codex.conversationCount,
      transcriptPublishedCount: codex.transcriptPublishedCount,
      withheldCount: codex.withheldCount,
      verifiedMessageCount: codex.verifiedMessageCount,
      publishedMessageCount: codex.publishedMessageCount,
      withheldMessageCount: codex.withheldMessageCount,
      vaultNoteCount: codex.vaultNoteCount,
      recordCount: codex.records.length,
      sourceBytes: codex.sourceBytes,
      sourceSeal: sha256(Buffer.from(codex.records
        .slice()
        .sort((left, right) => left.slug.localeCompare(right.slug))
        .map((record) => `${record.slug}\t${record.projectionBytes}\t${record.projectionSha256}\n`)
        .join(''), 'utf8')),
      sourceSealAlgorithm: 'UTF-8 lines in public slug order: slug<TAB>projectionBytes<TAB>projectionSha256<LF>',
      redactionCount: codex.redactionCount,
      approval: codex.approval,
    },
  ];
  const snapshot = {
    schemaVersion: 2,
    archive: 'Akashic Records',
    sourceKind: 'author-approved Medium posts and owner-approved Codex conversations',
    publicationRule: 'each source set is published only through its explicit, hash-locked approval denominator',
    sourceCount: MEDIUM_EXPORT_COUNT,
    approvedCount: records.length,
    recordCount: records.length,
    entryCount: APPROVED_RECORD_COUNT + codex.conversationCount,
    conversationCount: codex.conversationCount,
    draftExcludedCount: EXCLUDED_DRAFT_COUNT,
    sourceBytes,
    sourceSeal: APPROVED_SOURCE_IDENTITY_SHA256,
    sourceSealAlgorithm: MEDIUM_SOURCE_SEAL_ALGORITHM,
    ...(snapshotDate === undefined ? {} : { snapshotDate }),
    sourceSets,
    records,
  };
  assertNoResidualPrivateContent(snapshot, { label: 'full Akashic snapshot' });
  assertNoResidualPrivateContent(
    {
      sourceSet: snapshot.sourceSets.find((sourceSet) => sourceSet.source === 'Codex'),
      records: snapshot.records.filter((record) => record.source === 'Codex'),
    },
    { includePii: true, label: 'Codex-derived snapshot content' },
  );
  return snapshot;
}

function makePublicManifest(snapshot, snapshotSha256) {
  const manifest = {
    schemaVersion: 2,
    archive: snapshot.archive,
    archiveUrl: 'https://www.apocky.com/akashic-records',
    approvedCount: snapshot.approvedCount,
    recordCount: snapshot.recordCount,
    entryCount: snapshot.entryCount,
    conversationCount: snapshot.conversationCount,
    draftExcludedCount: snapshot.draftExcludedCount,
    sourceSeal: snapshot.sourceSeal,
    sourceSealAlgorithm: snapshot.sourceSealAlgorithm,
    sourceSets: snapshot.sourceSets,
    snapshotSha256,
    snapshotDate: snapshot.snapshotDate,
    mediaPolicy: 'Remote images and embeds are omitted. Authored alt text and captions are retained when present.',
    linkPolicy: 'Safe http(s) text links and one destination per link card are retained. Mail links and duplicate thumbnail anchors are omitted.',
    records: snapshot.records.map((record) => ({
      slug: record.slug,
      title: record.title,
      excerpt: record.excerpt,
      publishedAt: record.publishedAt,
      ...(record.updatedAt === undefined ? {} : { updatedAt: record.updatedAt }),
      year: record.year,
      source: record.source,
      type: record.type,
      topics: record.topics,
      href: `/akashic-records/${record.slug}`,
      canonicalUrl: record.canonicalUrl,
      ...(record.sourceUrl === undefined ? {} : { sourceUrl: record.sourceUrl, sourceUrlStatus: record.sourceUrlStatus }),
      sourceSha256: record.source === 'Codex' ? record.projectionSha256 : record.sourceSha256,
      ...(record.conversationId === undefined ? {} : {
        conversationId: record.conversationId,
        recordedAt: record.recordedAt,
        part: record.part,
        parts: record.parts,
        messageCount: record.messageCount,
        redactionCount: record.redactionCount,
        projectionBytes: record.projectionBytes,
        projectionSha256: record.projectionSha256,
        ...(record.publicationState === undefined ? {} : {
          publicationState: record.publicationState,
          withheldMessageCount: record.withheldMessageCount,
          withheldReason: record.withheldReason,
        }),
        ...(record.contentNotice === undefined ? {} : { contentNotice: record.contentNotice }),
      }),
    })),
  };
  assertNoResidualPrivateContent(manifest, { label: 'public Akashic manifest' });
  assertNoResidualPrivateContent(
    {
      sourceSet: manifest.sourceSets.find((sourceSet) => sourceSet.source === 'Codex'),
      records: manifest.records.filter((record) => record.source === 'Codex'),
    },
    { includePii: true, label: 'Codex-derived public manifest content' },
  );
  return manifest;
}

export { makePublicManifest };

const SITEMAP_V2_BEGIN = '  <!-- BEGIN generated Akashic Records v2 detail URLs -->';
const SITEMAP_V2_END = '  <!-- END generated Akashic Records v2 detail URLs -->';
const SITEMAP_V1_MARKER = '  <!-- Generated from the sealed Akashic Records v1 snapshot; freshness-gated by tests/pages/akashic-records.test.ts. -->';

export function renderAkashicSitemap(current, records) {
  const newline = current.includes('\r\n') ? '\r\n' : '\n';
  const detailLines = records.map(
    (record) => `  <url><loc>https://www.apocky.com/akashic-records/${record.slug}</loc><priority>0.7</priority></url>`,
  );
  const generated = [
    SITEMAP_V2_BEGIN,
    '  <!-- Generated from the sealed Akashic Records v2 snapshot; freshness-gated by tests/pages/akashic-records.test.ts. -->',
    ...detailLines,
    SITEMAP_V2_END,
  ].join(newline);

  const beginAt = current.indexOf(SITEMAP_V2_BEGIN);
  if (beginAt !== -1) {
    if (current.indexOf(SITEMAP_V2_BEGIN, beginAt + 1) !== -1) throw new Error('Sitemap has duplicate Akashic v2 generated blocks');
    const endAt = current.indexOf(SITEMAP_V2_END, beginAt);
    if (endAt === -1 || current.indexOf(SITEMAP_V2_END, endAt + 1) !== -1) throw new Error('Sitemap has malformed Akashic v2 generated block');
    return `${current.slice(0, beginAt)}${generated}${current.slice(endAt + SITEMAP_V2_END.length)}`;
  }

  const legacyAt = current.indexOf(SITEMAP_V1_MARKER);
  const closeAt = current.lastIndexOf('</urlset>');
  if (legacyAt === -1 || closeAt <= legacyAt) throw new Error('Sitemap lacks a recognized Akashic generated block');
  const legacyBlock = current.slice(legacyAt + SITEMAP_V1_MARKER.length, closeAt);
  for (const line of legacyBlock.split(/\r?\n/).map((line) => line.trim()).filter(Boolean)) {
    if (!/^<url><loc>https:\/\/www\.apocky\.com\/akashic-records\/[a-z0-9-]+<\/loc><priority>0\.7<\/priority><\/url>$/.test(line)) {
      throw new Error(`Unexpected non-Akashic content inside legacy sitemap block: ${line}`);
    }
  }
  return `${current.slice(0, legacyAt)}${generated}${newline}${current.slice(closeAt)}`;
}

function parseArguments(argv) {
  let source = process.env.AKASHIC_MEDIUM_POSTS_DIR;
  let codexSource = process.env.AKASHIC_CODEX_SESSIONS_DIR;
  let codexManifest = process.env.AKASHIC_CODEX_SELECTION_MANIFEST;
  let codexReceipt = process.env.AKASHIC_CODEX_IMPORT_RECEIPT;
  let check = false;
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === '--check') check = true;
    else if (argument === '--source') source = argv[++index];
    else if (argument === '--codex-source') codexSource = argv[++index];
    else if (argument === '--codex-manifest') codexManifest = argv[++index];
    else if (argument === '--codex-receipt') codexReceipt = argv[++index];
    else throw new Error(`Unknown argument: ${argument}`);
  }
  if (source === undefined || source.length === 0) {
    throw new Error('Provide --source <Medium posts directory> or AKASHIC_MEDIUM_POSTS_DIR');
  }
  if (codexSource === undefined || codexSource.length === 0) {
    throw new Error('Provide --codex-source <vault Codex sessions directory> or AKASHIC_CODEX_SESSIONS_DIR');
  }
  if (codexManifest === undefined || codexManifest.length === 0) {
    throw new Error('Provide --codex-manifest <approved selection JSON> or AKASHIC_CODEX_SELECTION_MANIFEST');
  }
  if (codexReceipt === undefined || codexReceipt.length === 0) {
    throw new Error('Provide --codex-receipt <scoped import receipt JSON> or AKASHIC_CODEX_IMPORT_RECEIPT');
  }
  return { source, codexSource, codexManifest, codexReceipt, check };
}

async function writeOrCheck(target, content, check) {
  if (check) {
    let current;
    try {
      current = await readFile(target, 'utf8');
    } catch {
      current = undefined;
    }
    if (current !== content) throw new Error(`Generated file is stale: ${relative(process.cwd(), target)}`);
    return;
  }
  await mkdir(dirname(target), { recursive: true });
  await writeFile(target, content, 'utf8');
}

export async function runCli(argv = process.argv.slice(2)) {
  const { source, codexSource, codexManifest, codexReceipt, check } = parseArguments(argv);
  const snapshot = await buildSnapshot(source, codexSource, codexManifest, codexReceipt);
  const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
  const snapshotContent = `${JSON.stringify(snapshot, null, 2)}\n`;
  const snapshotSha256 = sha256(Buffer.from(snapshotContent, 'utf8'));
  await writeOrCheck(join(repositoryRoot, 'data', 'akashic-records.v1.json'), snapshotContent, check);
  await writeOrCheck(
    join(repositoryRoot, 'public', 'akashic-records', 'manifest.json'),
    `${JSON.stringify(makePublicManifest(snapshot, snapshotSha256), null, 2)}\n`,
    check,
  );
  const sitemapPath = join(repositoryRoot, 'public', 'sitemap.xml');
  const sitemap = renderAkashicSitemap(await readFile(sitemapPath, 'utf8'), snapshot.records);
  await writeOrCheck(sitemapPath, sitemap, check);
  console.log(
    `akashic snapshot : ${check ? 'CURRENT' : 'WROTE'} · ${snapshot.recordCount} records · ${snapshot.conversationCount} conversations · ${snapshot.sourceSeal}`,
  );
}

if (process.argv[1] !== undefined && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  runCli().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}
