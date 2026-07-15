import type { NextApiRequest, NextApiResponse } from 'next';
import { atlasData } from '@/lib/shawn/atlas';
import {
  auditReference,
  auditReferenceLinks,
  probeReferenceUrl,
  validateReferenceMetadata,
  type AuditFetcher,
  type LinkAuditReport,
} from '@/lib/shawn/link-audit';
import { referenceCatalog } from '@/lib/shawn/catalog';
import type { AtlasData, ReferenceRecord } from '@/lib/shawn/types';
import { createShawnReferenceAuditHandler } from '@/pages/api/cron/shawn-reference-audit';

function assert(condition: boolean, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

function response(
  status: number,
  body = '',
  options: { readonly url?: string; readonly redirected?: boolean; readonly contentType?: string } = {},
): Response {
  const result = new Response(body, {
    status,
    headers: options.contentType ? { 'content-type': options.contentType } : undefined,
  });
  if (options.url) Object.defineProperty(result, 'url', { value: options.url });
  if (options.redirected !== undefined) Object.defineProperty(result, 'redirected', { value: options.redirected });
  return result;
}

function fixtureRecord(overrides: Partial<ReferenceRecord> = {}): ReferenceRecord {
  const base = referenceCatalog[0];
  assert(base !== undefined, 'catalog fixture exists');
  const title = overrides.title ?? 'Test reference title';
  return {
    ...base,
    slug: 'test-reference',
    aliases: [],
    title,
    edition: 'First reviewed edition',
    version: '1.0',
    exactLocator: '§1',
    identifiers: [],
    urls: { canonical: 'https://canonical.example/reference' },
    fullRead: false,
    displayCitation: `Example Author. ${title}. Example Publisher, 2026.`,
    role: 'R0',
    prerequisites: [],
    ...overrides,
  };
}

function fixtureAtlas(record: ReferenceRecord, citations: AtlasData['citations'] = []): AtlasData {
  return {
    ...atlasData,
    sourceRefs: [],
    claims: [],
    citations,
    chronology: [],
    reasoningChains: [],
    episodes: [],
    variables: [],
    artifacts: [],
    bridges: [],
    lenses: [],
    topicSlugs: [record.slug],
  };
}

async function testHeadFallsBackToGetAndExtractsTitle(): Promise<void> {
  const calls: string[] = [];
  const fetcher: AuditFetcher = async (_url, init) => {
    calls.push(String(init?.method));
    if (init?.method === 'HEAD') return response(405);
    return response(200, '<html><head><title>Test reference title</title></head></html>', {
      url: 'https://canonical.example/reference',
      contentType: 'text/html; charset=utf-8',
    });
  };
  const result = await probeReferenceUrl(
    'https://canonical.example/reference',
    'Test reference title',
    fetcher,
  );
  assert(result.healthy, 'GET fallback restores health after HEAD rejection');
  assert(calls.join(',') === 'HEAD,GET', 'HEAD precedes GET fallback');
  assert(result.titleAgreement === 'match', 'HTML title agrees with catalog title');
}

async function testRedirectAndTitleMismatchAreRecorded(): Promise<void> {
  const fetcher: AuditFetcher = async (_url, init) => {
    if (init?.method === 'HEAD') {
      return response(200, '', {
        url: 'https://publisher.example/final',
        redirected: true,
        contentType: 'text/html',
      });
    }
    return response(200, '<title>Completely unrelated destination</title>', {
      url: 'https://publisher.example/final',
      redirected: true,
      contentType: 'text/html',
    });
  };
  const record = fixtureRecord();
  const result = await auditReference(record, fixtureAtlas(record), fetcher);
  assert(result.canonical.attempts.every((attempt) => attempt.redirected), 'redirect final URL retained');
  assert(result.canonical.titleAgreement === 'mismatch', 'title disagreement detected');
  assert(result.warnings.some((warning) => warning.includes('human identifier/title review')), 'title mismatch requires review');
}

async function testHealthyFallbackRescuesLoadBearingReference(): Promise<void> {
  const record = fixtureRecord({
    urls: {
      canonical: 'https://canonical.example/dead',
      openAccess: 'https://archive.example/intact',
    },
  });
  const fetcher: AuditFetcher = async (url) => url.includes('/intact') ? response(200) : response(503);
  const result = await auditReference(record, fixtureAtlas(record), fetcher, 100, false);
  assert(result.loadBearing, 'R0 reference is load-bearing');
  assert(result.healthy && result.resolvedBy === 'openAccess', 'intact fallback resolves reference');
  assert(result.blockers.length === 0, 'healthy fallback prevents broken-link publication blocker');
  assert(result.warnings.some((warning) => warning.includes('resolved through openAccess')), 'fallback use remains visible');
}

async function testBrokenLoadBearingReferenceBlocksPublication(): Promise<void> {
  const record = fixtureRecord();
  const fetcher: AuditFetcher = async () => response(503);
  const result = await auditReference(record, fixtureAtlas(record), fetcher, 100, false);
  assert(!result.healthy, 'unreachable reference is unhealthy');
  assert(result.blockers.some((blocker) => blocker.includes('unreachable')), 'unreachable R0 reference blocks');
}

async function testBrokenNonLoadBearingReferenceWarnsOnly(): Promise<void> {
  const record = fixtureRecord({ role: 'R4' });
  const fetcher: AuditFetcher = async () => response(503);
  const result = await auditReference(record, fixtureAtlas(record), fetcher, 100, false);
  assert(!result.loadBearing, 'uncited R4 reference is not load-bearing');
  assert(result.blockers.length === 0, 'non-load-bearing outage is not a publication blocker');
  assert(result.warnings.some((warning) => warning.includes('unreachable')), 'non-load-bearing outage remains visible');
}

function testIdentifierTitleEditionAndFallbackMetadata(): void {
  const record = fixtureRecord({
    edition: '',
    identifiers: [{ scheme: 'DOI', value: '10.0000/wrong' }],
    urls: {
      canonical: 'https://doi.org/10.0000/right',
      openAccess: 'https://doi.org/10.0000/right',
    },
    displayCitation: 'A citation with the wrong work title.',
  });
  const issues = validateReferenceMetadata(record);
  assert(issues.some((issue) => issue.includes('edition is missing')), 'edition required');
  assert(issues.some((issue) => issue.includes('display citation title disagrees')), 'display title agreement checked');
  assert(issues.some((issue) => issue.includes('DOI identifier disagrees')), 'identifier agrees with URL');
  assert(issues.some((issue) => issue.includes('openAccess URL duplicates')), 'fallback must be distinct');
}

async function testWholeAuditRunsCatalogValidator(): Promise<void> {
  const record = fixtureRecord({ backlinks: [] });
  const report = await auditReferenceLinks({
    catalog: [record],
    atlas: fixtureAtlas(record),
    fetcher: async () => response(200),
    verifyTitles: false,
    now: () => new Date('2026-07-15T16:00:00.000Z'),
  });
  assert(report.catalogErrors.some((error) => error.includes('no atlas backlink')), 'validateCatalog errors preserved');
  assert(!report.publicationReady, 'catalog validation error blocks publication readiness');
  assert(report.blockers.some((error) => error.includes('model remains candidate')), 'ratification gate is part of link-audit readiness');
  assert(report.blockers.some((error) => error.includes('full-text review pending')), 'full-reading gate is part of link-audit readiness');
  assert(report.generatedAt === '2026-07-15T16:00:00.000Z', 'audit timestamp is explicit');
}

async function testWholeAuditDeadlineIsVisible(): Promise<void> {
  const record = fixtureRecord();
  const neverCompletesWithoutAbort: AuditFetcher = async (_url, init) => new Promise<Response>((_resolve, reject) => {
    if (init?.signal?.aborted) {
      reject(new Error('aborted by audit deadline'));
      return;
    }
    init?.signal?.addEventListener('abort', () => reject(new Error('aborted by audit deadline')), { once: true });
  });
  const report = await auditReferenceLinks({
    catalog: [record],
    atlas: fixtureAtlas(record),
    fetcher: neverCompletesWithoutAbort,
    timeoutMs: 1_000,
    deadlineMs: 5,
    verifyTitles: false,
  });
  assert(report.timedOut, 'global deadline state is explicit');
  assert(report.blockers.some((blocker) => blocker.includes('degraded evidence')), 'deadline degradation blocks readiness');
}

interface MockedResponse {
  statusCode: number;
  body: unknown;
  headers: Record<string, string | readonly string[]>;
}

function mockReqRes(method: string, headers: Record<string, string> = {}): {
  req: NextApiRequest;
  res: NextApiResponse;
  out: MockedResponse;
} {
  const out: MockedResponse = { statusCode: 0, body: null, headers: {} };
  const req = { method, headers, query: {} } as unknown as NextApiRequest;
  const res = {
    status(code: number) { out.statusCode = code; return this; },
    json(body: unknown) { out.body = body; return this; },
    setHeader(key: string, value: string | readonly string[]) { out.headers[key] = value; return this; },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

const emptyReport: LinkAuditReport = {
  generatedAt: '2026-07-15T16:00:00.000Z',
  timedOut: false,
  catalogErrors: [],
  references: [],
  audited: 0,
  healthy: 0,
  blocking: 0,
  warnings: 0,
  publicationReady: true,
  blockers: [],
};

async function testCronStubDoesNotRunAudit(): Promise<void> {
  const previous = process.env['CRON_SECRET'];
  delete process.env['CRON_SECRET'];
  let runs = 0;
  const handler = createShawnReferenceAuditHandler(async () => {
    runs += 1;
    return emptyReport;
  });
  const { req, res, out } = mockReqRes('GET');
  await handler(req, res);
  assert(out.statusCode === 200, 'unconfigured cron uses bounded stub');
  assert((out.body as Record<string, unknown>)['stub'] === true, 'stub state explicit');
  assert(runs === 0, 'stub performs no external audit');
  if (previous === undefined) delete process.env['CRON_SECRET'];
  else process.env['CRON_SECRET'] = previous;
}

async function testCronRequiresAuthAndRunsWhenAuthorized(): Promise<void> {
  const previous = process.env['CRON_SECRET'];
  process.env['CRON_SECRET'] = 'reference-secret';
  let runs = 0;
  const handler = createShawnReferenceAuditHandler(async () => {
    runs += 1;
    return emptyReport;
  });

  const denied = mockReqRes('GET');
  await handler(denied.req, denied.res);
  assert(denied.out.statusCode === 401, 'configured cron rejects missing credentials');
  assert(runs === 0, 'rejected request cannot start audit');

  const accepted = mockReqRes('GET', { authorization: 'Bearer reference-secret' });
  await handler(accepted.req, accepted.res);
  assert(accepted.out.statusCode === 200, 'authorized cron completes');
  assert((accepted.out.body as Record<string, unknown>)['stub'] === false, 'authorized response is not stub');
  assert(Number(runs) === 1, 'authorized request runs exactly one audit');

  if (previous === undefined) delete process.env['CRON_SECRET'];
  else process.env['CRON_SECRET'] = previous;
}

async function run(): Promise<void> {
  await testHeadFallsBackToGetAndExtractsTitle();
  await testRedirectAndTitleMismatchAreRecorded();
  await testHealthyFallbackRescuesLoadBearingReference();
  await testBrokenLoadBearingReferenceBlocksPublication();
  await testBrokenNonLoadBearingReferenceWarnsOnly();
  testIdentifierTitleEditionAndFallbackMetadata();
  await testWholeAuditRunsCatalogValidator();
  await testWholeAuditDeadlineIsVisible();
  await testCronStubDoesNotRunAudit();
  await testCronRequiresAuthAndRunsWhenAuthorized();
}

run().then(() => {
  // eslint-disable-next-line no-console
  console.log('shawn/link-audit.test : OK · 10 tests passed');
}).catch((error: unknown) => {
  // eslint-disable-next-line no-console
  console.error(error);
  process.exitCode = 1;
});
