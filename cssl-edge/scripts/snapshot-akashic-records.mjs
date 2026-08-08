#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { lstat, mkdir, readFile, readdir, realpath, stat, writeFile } from 'node:fs/promises';
import { basename, dirname, isAbsolute, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const APPROVED_RECORD_COUNT = 204;
export const EXCLUDED_DRAFT_COUNT = 26;
export const MEDIUM_EXPORT_COUNT = 230;
export const APPROVED_SOURCE_BYTES = 4_203_802;
export const APPROVED_SOURCE_IDENTITY_SHA256 =
  '8fcb160f1cc19d09103e86f21596805b11763d18dadfcf681ef3baf649323674';

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

function validateRecords(records) {
  const slugs = new Set();
  for (const record of records) {
    if (slugs.has(record.slug)) throw new Error(`Duplicate public slug: ${record.slug}`);
    slugs.add(record.slug);
    if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(record.slug)) throw new Error(`Unsafe slug: ${record.slug}`);
    if (!/^[a-f0-9]{64}$/.test(record.sourceSha256)) throw new Error(`Invalid source hash: ${record.slug}`);
    for (const block of record.blocks) {
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
  }
  const serialized = JSON.stringify(records);
  const forbidden = [
    /[A-Z]:\\Users\\/i,
    /\/Users\//,
    /\/home\//,
    /Obsidian Vault/i,
    /file:\/\//i,
    /<\/?(?:script|iframe|img)\b/i,
    /javascript:/i,
    /-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----/,
    /\bAKIA[0-9A-Z]{16}\b/,
    /\b(?:ghp|github_pat)_[A-Za-z0-9_]{20,}\b/,
    /\bsk-[A-Za-z0-9_-]{20,}\b/,
  ];
  for (const pattern of forbidden) {
    if (pattern.test(serialized)) throw new Error(`Forbidden private/executable pattern in snapshot: ${pattern}`);
  }
}

export async function buildSnapshot(sourceRoot) {
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
  const records = inventory.approved.map((post) => parseMediumPost(post.bytes.toString('utf8'), post.filename, post.sourceSha256));
  records.sort((left, right) => right.publishedAt.localeCompare(left.publishedAt) || left.slug.localeCompare(right.slug));
  validateRecords(records);
  const snapshotDate = records.map((record) => record.updatedAt).filter(Boolean).sort().at(-1);
  return {
    schemaVersion: 1,
    archive: 'Akashic Records',
    sourceKind: 'author-approved Medium export posts',
    publicationRule: 'all non-draft posts in the frozen v1 source denominator',
    sourceCount: MEDIUM_EXPORT_COUNT,
    approvedCount: APPROVED_RECORD_COUNT,
    draftExcludedCount: EXCLUDED_DRAFT_COUNT,
    sourceBytes,
    sourceSeal: APPROVED_SOURCE_IDENTITY_SHA256,
    sourceSealAlgorithm: 'ASCII lowercase ordinal filename order; filename<TAB>byteLength<TAB>lowercase-sha256<LF>; UTF-8 without BOM',
    ...(snapshotDate === undefined ? {} : { snapshotDate }),
    records,
  };
}

function makePublicManifest(snapshot, snapshotSha256) {
  return {
    schemaVersion: 1,
    archive: snapshot.archive,
    archiveUrl: 'https://www.apocky.com/akashic-records',
    approvedCount: snapshot.approvedCount,
    draftExcludedCount: snapshot.draftExcludedCount,
    sourceSeal: snapshot.sourceSeal,
    sourceSealAlgorithm: snapshot.sourceSealAlgorithm,
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
      sourceSha256: record.sourceSha256,
    })),
  };
}

function parseArguments(argv) {
  let source = process.env.AKASHIC_MEDIUM_POSTS_DIR;
  let check = false;
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === '--check') check = true;
    else if (argument === '--source') source = argv[++index];
    else throw new Error(`Unknown argument: ${argument}`);
  }
  if (source === undefined || source.length === 0) {
    throw new Error('Provide --source <Medium posts directory> or AKASHIC_MEDIUM_POSTS_DIR');
  }
  return { source, check };
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
  const { source, check } = parseArguments(argv);
  const snapshot = await buildSnapshot(source);
  const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
  const snapshotContent = `${JSON.stringify(snapshot, null, 2)}\n`;
  const snapshotSha256 = sha256(Buffer.from(snapshotContent, 'utf8'));
  await writeOrCheck(join(repositoryRoot, 'data', 'akashic-records.v1.json'), snapshotContent, check);
  await writeOrCheck(
    join(repositoryRoot, 'public', 'akashic-records', 'manifest.json'),
    `${JSON.stringify(makePublicManifest(snapshot, snapshotSha256), null, 2)}\n`,
    check,
  );
  console.log(`akashic snapshot : ${check ? 'CURRENT' : 'WROTE'} · ${snapshot.approvedCount} approved · ${snapshot.sourceSeal}`);
}

if (process.argv[1] !== undefined && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  runCli().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}
