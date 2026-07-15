import { atlasData } from './atlas';
import { publicationBlockers, referenceCatalog, validateCatalog } from './catalog';
import type { AtlasData, CitationRelation, ReferenceRecord } from './types';

export type AuditFetcher = (input: string, init?: RequestInit) => Promise<Response>;

export type TitleAgreement = 'match' | 'mismatch' | 'unavailable' | 'not-html';

export interface LinkAttempt {
  readonly method: 'HEAD' | 'GET';
  readonly requestedUrl: string;
  readonly finalUrl: string;
  readonly status: number | null;
  readonly ok: boolean;
  readonly redirected: boolean;
  readonly error?: string;
}

export interface UrlAudit {
  readonly url: string;
  readonly healthy: boolean;
  readonly attempts: readonly LinkAttempt[];
  readonly pageTitle?: string;
  readonly titleAgreement: TitleAgreement;
}

export interface ReferenceAudit {
  readonly slug: string;
  readonly role: ReferenceRecord['role'];
  readonly loadBearing: boolean;
  readonly metadataIssues: readonly string[];
  readonly canonical: UrlAudit;
  readonly fallbacks: readonly UrlAudit[];
  readonly resolvedBy: 'canonical' | 'openAccess' | 'archive' | null;
  readonly healthy: boolean;
  readonly blockers: readonly string[];
  readonly warnings: readonly string[];
}

export interface LinkAuditReport {
  readonly generatedAt: string;
  readonly timedOut: boolean;
  readonly catalogErrors: readonly string[];
  readonly references: readonly ReferenceAudit[];
  readonly audited: number;
  readonly healthy: number;
  readonly blocking: number;
  readonly warnings: number;
  readonly publicationReady: boolean;
  readonly blockers: readonly string[];
}

export interface LinkAuditOptions {
  readonly fetcher?: AuditFetcher;
  readonly catalog?: readonly ReferenceRecord[];
  readonly atlas?: AtlasData;
  readonly timeoutMs?: number;
  readonly concurrency?: number;
  readonly verifyTitles?: boolean;
  readonly deadlineMs?: number;
  readonly now?: () => Date;
}

const LOAD_BEARING_RELATIONS = new Set<CitationRelation>([
  'proves',
  'verifies',
  'supports',
  'refutes',
  'defines',
  'attests',
]);
const TITLE_STOP_WORDS = new Set([
  'about', 'after', 'along', 'also', 'among', 'from', 'into', 'over', 'that', 'their',
  'these', 'this', 'through', 'using', 'with', 'without', 'and', 'for', 'the',
]);
const TITLE_READ_LIMIT = 128 * 1024;

function safeUrl(value: string): URL | null {
  try {
    return new URL(value);
  } catch {
    return null;
  }
}

function normalizeIdentifier(value: string): string {
  return decodeURIComponent(value).replace(/^doi:/i, '').replace(/\/$/, '').toLowerCase();
}

function normalizeTitle(value: string): string[] {
  return value
    .normalize('NFKD')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, ' ')
    .split(/\s+/)
    .filter((token) => token.length >= 3 && !TITLE_STOP_WORDS.has(token));
}

function titleAgreement(expected: string, observed?: string): TitleAgreement {
  if (!observed) return 'unavailable';
  const expectedTokens = new Set(normalizeTitle(expected));
  const observedTokens = new Set(normalizeTitle(observed));
  if (expectedTokens.size === 0 || observedTokens.size === 0) return 'unavailable';
  const overlap = [...expectedTokens].filter((token) => observedTokens.has(token)).length;
  const threshold = Math.min(3, Math.max(1, Math.ceil(expectedTokens.size / 4)));
  return overlap >= threshold ? 'match' : 'mismatch';
}

function identifierAgrees(record: ReferenceRecord, scheme: string, value: string): boolean {
  const normalized = normalizeIdentifier(value);
  const urls = [record.urls.canonical, record.urls.openAccess, record.urls.archive]
    .filter((url): url is string => typeof url === 'string')
    .map((url) => normalizeIdentifier(url));
  switch (scheme) {
    case 'DOI':
      return urls.some((url) => url.includes(`doi.org/${normalized}`));
    case 'arXiv':
      return urls.some((url) => url.includes(`arxiv.org/abs/${normalized}`) || url.includes(`arxiv.org/pdf/${normalized}`));
    case 'W3C':
      return urls.some((url) => url.includes(`w3.org/tr/${normalized}`));
    case 'RFC': {
      const digits = normalized.replace(/^rfc/i, '');
      return urls.some((url) => url.includes(`rfc${digits}`) || url.includes(`/rfc/${digits}`));
    }
    case 'PMID':
      return urls.some((url) => url.includes(`pubmed.ncbi.nlm.nih.gov/${normalized}`));
    case 'SWHID':
      return urls.some((url) => url.includes(encodeURIComponent(value).toLowerCase()) || url.includes(normalized));
    case 'ISBN':
    case 'standard':
    case 'catalog':
      return true;
    default:
      return false;
  }
}

export function validateReferenceMetadata(record: ReferenceRecord): string[] {
  const issues: string[] = [];
  const required: ReadonlyArray<[string, string]> = [
    ['title', record.title],
    ['edition', record.edition],
    ['version', record.version],
    ['exact locator', record.exactLocator],
    ['display citation', record.displayCitation],
  ];
  for (const [label, value] of required) {
    if (value.trim().length === 0) issues.push(`${label} is missing`);
  }
  if (!record.displayCitation.toLowerCase().includes(record.title.toLowerCase())) {
    issues.push('display citation title disagrees with reference title');
  }

  const seenUrls = new Set<string>();
  for (const [label, value] of [
    ['canonical', record.urls.canonical],
    ['openAccess', record.urls.openAccess],
    ['archive', record.urls.archive],
  ] as const) {
    if (value === undefined) continue;
    const parsed = safeUrl(value);
    if (!parsed || parsed.protocol !== 'https:') issues.push(`${label} URL must be valid HTTPS`);
    if (seenUrls.has(value)) issues.push(`${label} URL duplicates another target`);
    seenUrls.add(value);
  }

  for (const identifier of record.identifiers) {
    if (!identifierAgrees(record, identifier.scheme, identifier.value)) {
      issues.push(`${identifier.scheme} identifier disagrees with reference URLs`);
    }
  }
  return issues;
}

export function isLoadBearingReference(record: ReferenceRecord, atlas: AtlasData = atlasData): boolean {
  if (record.role === 'R0' || record.role === 'R1') return true;
  const names = new Set([record.slug, ...record.aliases]);
  return atlas.citations.some(
    (citation) => names.has(citation.referenceSlug) && LOAD_BEARING_RELATIONS.has(citation.relation),
  );
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message.slice(0, 240);
  return String(error).slice(0, 240);
}

async function request(
  fetcher: AuditFetcher,
  url: string,
  method: 'HEAD' | 'GET',
  timeoutMs: number,
  auditSignal?: AbortSignal,
): Promise<{ attempt: LinkAttempt; response?: Response }> {
  const controller = new AbortController();
  const abortFromAudit = (): void => controller.abort();
  if (auditSignal?.aborted) controller.abort();
  else auditSignal?.addEventListener('abort', abortFromAudit, { once: true });
  const timeout = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const response = await fetcher(url, {
      method,
      redirect: 'follow',
      cache: 'no-store',
      headers: method === 'GET'
        ? { accept: 'text/html,application/xhtml+xml;q=0.9,*/*;q=0.1', range: `bytes=0-${TITLE_READ_LIMIT - 1}` }
        : { accept: '*/*' },
      signal: controller.signal,
    });
    const finalUrl = response.url || url;
    return {
      attempt: {
        method,
        requestedUrl: url,
        finalUrl,
        status: response.status,
        ok: response.status >= 200 && response.status < 300,
        redirected: response.redirected || finalUrl !== url,
      },
      response,
    };
  } catch (error) {
    return {
      attempt: {
        method,
        requestedUrl: url,
        finalUrl: url,
        status: null,
        ok: false,
        redirected: false,
        error: errorMessage(error),
      },
    };
  } finally {
    clearTimeout(timeout);
    auditSignal?.removeEventListener('abort', abortFromAudit);
  }
}

async function readTextBounded(response: Response): Promise<string> {
  if (!response.body) return (await response.text()).slice(0, TITLE_READ_LIMIT);
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let output = '';
  let read = 0;
  try {
    while (read < TITLE_READ_LIMIT) {
      const { done, value } = await reader.read();
      if (done) break;
      if (!value) continue;
      const remaining = TITLE_READ_LIMIT - read;
      const chunk = value.byteLength > remaining ? value.slice(0, remaining) : value;
      output += decoder.decode(chunk, { stream: true });
      read += chunk.byteLength;
    }
    output += decoder.decode();
  } finally {
    await reader.cancel().catch(() => undefined);
  }
  return output;
}

async function extractPageTitle(response: Response): Promise<{ title?: string; status: TitleAgreement }> {
  const contentType = response.headers.get('content-type')?.toLowerCase() ?? '';
  if (!contentType.includes('text/html') && !contentType.includes('application/xhtml+xml')) {
    return { status: 'not-html' };
  }
  try {
    const html = await readTextBounded(response);
    const metadataTitle = (html.match(/<meta\b[^>]*>/gi) ?? []).map((tag) => {
      const key = /(?:name|property)=["']([^"']+)["']/i.exec(tag)?.[1]?.toLowerCase();
      if (key !== 'citation_title' && key !== 'og:title') return undefined;
      return /content=["']([^"']+)["']/i.exec(tag)?.[1];
    }).find((value): value is string => typeof value === 'string');
    const rawTitle = metadataTitle ?? /<title[^>]*>([\s\S]*?)<\/title>/i.exec(html)?.[1];
    const title = rawTitle?.replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim();
    return title ? { title, status: 'unavailable' } : { status: 'unavailable' };
  } catch {
    return { status: 'unavailable' };
  }
}

export async function probeReferenceUrl(
  url: string,
  title: string,
  fetcher: AuditFetcher,
  timeoutMs = 8_000,
  verifyTitles = true,
  auditSignal?: AbortSignal,
): Promise<UrlAudit> {
  const attempts: LinkAttempt[] = [];
  const head = await request(fetcher, url, 'HEAD', timeoutMs, auditSignal);
  attempts.push(head.attempt);
  let healthAttempt = head.attempt;
  let titleResponse: Response | undefined;

  if (!head.attempt.ok) {
    const get = await request(fetcher, url, 'GET', timeoutMs, auditSignal);
    attempts.push(get.attempt);
    healthAttempt = get.attempt;
    titleResponse = get.attempt.ok ? get.response : undefined;
  } else if (verifyTitles) {
    const get = await request(fetcher, url, 'GET', timeoutMs, auditSignal);
    attempts.push(get.attempt);
    if (get.attempt.ok) titleResponse = get.response;
  }

  let observedTitle: string | undefined;
  let agreement: TitleAgreement = verifyTitles ? 'unavailable' : 'unavailable';
  if (titleResponse) {
    const extracted = await extractPageTitle(titleResponse);
    observedTitle = extracted.title;
    agreement = extracted.status === 'not-html' ? 'not-html' : titleAgreement(title, observedTitle);
  }

  return {
    url,
    healthy: healthAttempt.ok,
    attempts,
    ...(observedTitle ? { pageTitle: observedTitle } : {}),
    titleAgreement: agreement,
  };
}

function fallbackTargets(record: ReferenceRecord): ReadonlyArray<{ kind: 'openAccess' | 'archive'; url: string }> {
  const output: Array<{ kind: 'openAccess' | 'archive'; url: string }> = [];
  if (record.urls.openAccess && record.urls.openAccess !== record.urls.canonical) {
    output.push({ kind: 'openAccess', url: record.urls.openAccess });
  }
  if (
    record.urls.archive
    && record.urls.archive !== record.urls.canonical
    && record.urls.archive !== record.urls.openAccess
  ) {
    output.push({ kind: 'archive', url: record.urls.archive });
  }
  return output;
}

export async function auditReference(
  record: ReferenceRecord,
  atlas: AtlasData,
  fetcher: AuditFetcher,
  timeoutMs = 8_000,
  verifyTitles = true,
  auditSignal?: AbortSignal,
): Promise<ReferenceAudit> {
  const metadataIssues = validateReferenceMetadata(record);
  const loadBearing = isLoadBearingReference(record, atlas);
  const canonical = await probeReferenceUrl(
    record.urls.canonical,
    record.title,
    fetcher,
    timeoutMs,
    verifyTitles,
    auditSignal,
  );
  const fallbacks: UrlAudit[] = [];
  let resolvedBy: ReferenceAudit['resolvedBy'] = canonical.healthy ? 'canonical' : null;

  if (!canonical.healthy) {
    for (const target of fallbackTargets(record)) {
      const audit = await probeReferenceUrl(
        target.url,
        record.title,
        fetcher,
        timeoutMs,
        verifyTitles,
        auditSignal,
      );
      fallbacks.push(audit);
      if (audit.healthy) {
        resolvedBy = target.kind;
        break;
      }
    }
  }

  const healthy = resolvedBy !== null;
  const blockers = metadataIssues.map((issue) => `${record.slug}: ${issue}`);
  const warnings: string[] = [];
  if (!healthy) {
    const message = `${record.slug}: canonical and configured fallback targets are unreachable`;
    if (loadBearing) blockers.push(message);
    else warnings.push(message);
  } else if (resolvedBy !== 'canonical') {
    warnings.push(`${record.slug}: canonical target failed; resolved through ${resolvedBy}`);
  }
  const selected = resolvedBy === 'canonical'
    ? canonical
    : fallbacks.find((fallback) => fallback.healthy);
  if (selected?.titleAgreement === 'mismatch') {
    warnings.push(`${record.slug}: retrieved page title requires human identifier/title review`);
  }
  const redirected = [canonical, ...fallbacks].some((audit) => audit.attempts.some((attempt) => attempt.redirected));
  if (redirected) warnings.push(`${record.slug}: target redirected; final URL recorded in attempt evidence`);

  return {
    slug: record.slug,
    role: record.role,
    loadBearing,
    metadataIssues,
    canonical,
    fallbacks,
    resolvedBy,
    healthy,
    blockers,
    warnings,
  };
}

async function mapConcurrent<T, R>(
  values: readonly T[],
  concurrency: number,
  worker: (value: T) => Promise<R>,
): Promise<R[]> {
  const results = new Array<R>(values.length);
  let cursor = 0;
  const run = async (): Promise<void> => {
    while (true) {
      const index = cursor;
      cursor += 1;
      if (index >= values.length) return;
      const value = values[index];
      if (value !== undefined) results[index] = await worker(value);
    }
  };
  await Promise.all(Array.from({ length: Math.min(Math.max(1, concurrency), Math.max(1, values.length)) }, run));
  return results;
}

export async function auditReferenceLinks(options: LinkAuditOptions = {}): Promise<LinkAuditReport> {
  const catalog = options.catalog ?? referenceCatalog;
  const atlas = options.atlas ?? atlasData;
  const fetcher = options.fetcher ?? ((input, init) => fetch(input, init));
  const timeoutMs = options.timeoutMs ?? 8_000;
  const concurrency = options.concurrency ?? 6;
  const verifyTitles = options.verifyTitles ?? true;
  const deadlineMs = options.deadlineMs ?? 45_000;
  const catalogErrors = validateCatalog(catalog, atlas);
  const semanticPublicationBlockers = publicationBlockers(catalog, atlas);
  const auditController = new AbortController();
  const deadline = setTimeout(() => auditController.abort(), deadlineMs);
  let references: ReferenceAudit[];
  try {
    references = await mapConcurrent(
      catalog,
      concurrency,
      (record) => auditReference(record, atlas, fetcher, timeoutMs, verifyTitles, auditController.signal),
    );
  } finally {
    clearTimeout(deadline);
  }
  const timedOut = auditController.signal.aborted;
  const blockers = [
    ...(timedOut ? ['audit: global deadline exceeded; unreachable results are degraded evidence'] : []),
    ...catalogErrors.map((error) => `catalog: ${error}`),
    ...semanticPublicationBlockers.map((error) => `publication: ${error}`),
    ...references.flatMap((record) => record.blockers),
  ];
  return {
    generatedAt: (options.now ?? (() => new Date()))().toISOString(),
    timedOut,
    catalogErrors,
    references,
    audited: references.length,
    healthy: references.filter((record) => record.healthy).length,
    blocking: blockers.length,
    warnings: references.reduce((count, record) => count + record.warnings.length, 0),
    publicationReady: blockers.length === 0,
    blockers,
  };
}
