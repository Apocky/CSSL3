import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import type { NextApiRequest, NextApiResponse } from 'next';
import { NextRequest } from 'next/server';

import {
  CONTAINMENT_HEADERS,
  CONTAINMENT_REASON_CODE,
  EXACT_CONTAINED_PATHS,
  containmentPayload,
  containmentSurface,
  type ContainmentResponse,
} from '@/lib/containment';
import { applyGlobalSecurityHeaders, makeCsp, middleware } from '@/middleware';

import mnemeSmoke from '@/pages/api/mneme/[profile]/smoke';
import mnemeRemember from '@/pages/api/mneme/[profile]/remember';
import mnemeRecall from '@/pages/api/mneme/[profile]/recall';
import mnemeList from '@/pages/api/mneme/[profile]/list';
import mnemeIngest from '@/pages/api/mneme/[profile]/ingest';
import mnemeHealth from '@/pages/api/mneme/[profile]/health';
import mnemeForget from '@/pages/api/mneme/[profile]/forget';
import mnemeExport from '@/pages/api/mneme/[profile]/export';
import notifications from '@/pages/api/content/notifications';
import aggregate from '@/pages/api/content/aggregate';
import attribution from '@/pages/api/content/attribution';
import subscribe from '@/pages/api/content/subscribe';
import unsubscribe from '@/pages/api/content/unsubscribe';
import cascadePublish from '@/pages/api/content/cascade-publish';
import publishInit from '@/pages/api/content/publish/init';
import publishChunk from '@/pages/api/content/publish/chunk';
import publishComplete from '@/pages/api/content/publish/complete';
import publishRevoke from '@/pages/api/content/publish/revoke';
import publishStatus from '@/pages/api/content/publish/status/[id]';
import moderationFlag from '@/pages/api/content/moderation/flag';
import moderationDecision from '@/pages/api/content/moderation/decision';
import moderationAppeal from '@/pages/api/content/moderation/appeal';
import moderationTransparency from '@/pages/api/content/moderation/transparency/[slug]';
import rate from '@/pages/api/content/rate';
import review from '@/pages/api/content/review';
import remix from '@/pages/api/content/remix';
import tip from '@/pages/api/content/tip';
import stripeCheckout from '@/pages/api/payments/stripe/checkout';
import stripeWebhook from '@/pages/api/payments/stripe/webhook';
import stripeRefund from '@/pages/api/payments/stripe/refund-request';
import adminTasks from '@/pages/api/admin/tasks';
import adminLogs from '@/pages/api/admin/logs';
import adminCoderPending from '@/pages/api/admin/coder/pending';
import visionSession from '@/pages/api/admin/apocrypha/vision/session';
import visionState from '@/pages/api/admin/apocrypha/vision/session/[session_ref]';
import visionFrame from '@/pages/api/admin/apocrypha/vision/session/[session_ref]/frame';
import visionControl from '@/pages/api/admin/apocrypha/vision/session/[session_ref]/control';
import akashicEvent from '@/pages/api/akashic/event';
import akashicBatch from '@/pages/api/akashic/batch';
import akashicPurge from '@/pages/api/akashic/purge';
import analyticsEvent from '@/pages/api/analytics/event';
import analyticsMetrics from '@/pages/api/analytics/metrics';

type RetiredHandler = (
  req: NextApiRequest,
  res: NextApiResponse<ContainmentResponse>,
) => void | Promise<void>;

interface RouteControl {
  file: string;
  livePath: string;
  surface: string;
  handler: RetiredHandler;
}

const MNEME_SURFACE = '/api/mneme/:profile/*';
const VISION_SURFACE = '/api/admin/apocrypha/vision/*';
const CONTENT_SURFACE = '/api/content/*';

const ROUTES: readonly RouteControl[] = [
  { file: 'pages/api/mneme/[profile]/smoke.ts', livePath: '/api/mneme/scratch/smoke', surface: MNEME_SURFACE, handler: mnemeSmoke },
  { file: 'pages/api/mneme/[profile]/remember.ts', livePath: '/api/mneme/scratch/remember', surface: MNEME_SURFACE, handler: mnemeRemember },
  { file: 'pages/api/mneme/[profile]/recall.ts', livePath: '/api/mneme/scratch/recall', surface: MNEME_SURFACE, handler: mnemeRecall },
  { file: 'pages/api/mneme/[profile]/list.ts', livePath: '/api/mneme/scratch/list', surface: MNEME_SURFACE, handler: mnemeList },
  { file: 'pages/api/mneme/[profile]/ingest.ts', livePath: '/api/mneme/scratch/ingest', surface: MNEME_SURFACE, handler: mnemeIngest },
  { file: 'pages/api/mneme/[profile]/health.ts', livePath: '/api/mneme/scratch/health', surface: MNEME_SURFACE, handler: mnemeHealth },
  { file: 'pages/api/mneme/[profile]/forget.ts', livePath: '/api/mneme/scratch/forget', surface: MNEME_SURFACE, handler: mnemeForget },
  { file: 'pages/api/mneme/[profile]/export.ts', livePath: '/api/mneme/scratch/export', surface: MNEME_SURFACE, handler: mnemeExport },
  { file: 'pages/api/content/aggregate.ts', livePath: '/api/content/aggregate', surface: CONTENT_SURFACE, handler: aggregate },
  { file: 'pages/api/content/attribution.ts', livePath: '/api/content/attribution', surface: CONTENT_SURFACE, handler: attribution },
  { file: 'pages/api/content/notifications.ts', livePath: '/api/content/notifications', surface: CONTENT_SURFACE, handler: notifications },
  { file: 'pages/api/content/subscribe.ts', livePath: '/api/content/subscribe', surface: CONTENT_SURFACE, handler: subscribe },
  { file: 'pages/api/content/unsubscribe.ts', livePath: '/api/content/unsubscribe', surface: CONTENT_SURFACE, handler: unsubscribe },
  { file: 'pages/api/content/cascade-publish.ts', livePath: '/api/content/cascade-publish', surface: CONTENT_SURFACE, handler: cascadePublish },
  { file: 'pages/api/content/publish/init.ts', livePath: '/api/content/publish/init', surface: CONTENT_SURFACE, handler: publishInit },
  { file: 'pages/api/content/publish/chunk.ts', livePath: '/api/content/publish/chunk', surface: CONTENT_SURFACE, handler: publishChunk },
  { file: 'pages/api/content/publish/complete.ts', livePath: '/api/content/publish/complete', surface: CONTENT_SURFACE, handler: publishComplete },
  { file: 'pages/api/content/publish/revoke.ts', livePath: '/api/content/publish/revoke', surface: CONTENT_SURFACE, handler: publishRevoke },
  { file: 'pages/api/content/publish/status/[id].ts', livePath: '/api/content/publish/status/00000000-0000-0000-0000-000000000000', surface: CONTENT_SURFACE, handler: publishStatus },
  { file: 'pages/api/content/moderation/flag.ts', livePath: '/api/content/moderation/flag', surface: CONTENT_SURFACE, handler: moderationFlag },
  { file: 'pages/api/content/moderation/decision.ts', livePath: '/api/content/moderation/decision', surface: CONTENT_SURFACE, handler: moderationDecision },
  { file: 'pages/api/content/moderation/appeal.ts', livePath: '/api/content/moderation/appeal', surface: CONTENT_SURFACE, handler: moderationAppeal },
  { file: 'pages/api/content/moderation/transparency/[slug].ts', livePath: '/api/content/moderation/transparency/example', surface: CONTENT_SURFACE, handler: moderationTransparency },
  { file: 'pages/api/content/rate.ts', livePath: '/api/content/rate', surface: CONTENT_SURFACE, handler: rate },
  { file: 'pages/api/content/review.ts', livePath: '/api/content/review', surface: CONTENT_SURFACE, handler: review },
  { file: 'pages/api/content/remix.ts', livePath: '/api/content/remix', surface: CONTENT_SURFACE, handler: remix },
  { file: 'pages/api/content/tip.ts', livePath: '/api/content/tip', surface: CONTENT_SURFACE, handler: tip },
  { file: 'pages/api/payments/stripe/checkout.ts', livePath: '/api/payments/stripe/checkout', surface: '/api/payments/stripe/*', handler: stripeCheckout },
  { file: 'pages/api/payments/stripe/webhook.ts', livePath: '/api/payments/stripe/webhook', surface: '/api/payments/stripe/*', handler: stripeWebhook },
  { file: 'pages/api/payments/stripe/refund-request.ts', livePath: '/api/payments/stripe/refund-request', surface: '/api/payments/stripe/*', handler: stripeRefund },
  { file: 'pages/api/admin/tasks.ts', livePath: '/api/admin/tasks', surface: '/api/admin/tasks', handler: adminTasks },
  { file: 'pages/api/admin/logs.ts', livePath: '/api/admin/logs', surface: '/api/admin/logs', handler: adminLogs },
  { file: 'pages/api/admin/coder/pending.ts', livePath: '/api/admin/coder/pending', surface: '/api/admin/coder/pending', handler: adminCoderPending },
  { file: 'pages/api/admin/apocrypha/vision/session.ts', livePath: '/api/admin/apocrypha/vision/session', surface: VISION_SURFACE, handler: visionSession },
  { file: 'pages/api/admin/apocrypha/vision/session/[session_ref].ts', livePath: '/api/admin/apocrypha/vision/session/session-ref', surface: VISION_SURFACE, handler: visionState },
  { file: 'pages/api/admin/apocrypha/vision/session/[session_ref]/frame.ts', livePath: '/api/admin/apocrypha/vision/session/session-ref/frame', surface: VISION_SURFACE, handler: visionFrame },
  { file: 'pages/api/admin/apocrypha/vision/session/[session_ref]/control.ts', livePath: '/api/admin/apocrypha/vision/session/session-ref/control', surface: VISION_SURFACE, handler: visionControl },
  { file: 'pages/api/akashic/event.ts', livePath: '/api/akashic/event', surface: '/api/akashic/event', handler: akashicEvent },
  { file: 'pages/api/akashic/batch.ts', livePath: '/api/akashic/batch', surface: '/api/akashic/batch', handler: akashicBatch },
  { file: 'pages/api/akashic/purge.ts', livePath: '/api/akashic/purge', surface: '/api/akashic/purge', handler: akashicPurge },
  { file: 'pages/api/analytics/event.ts', livePath: '/api/analytics/event', surface: '/api/analytics/event', handler: analyticsEvent },
  { file: 'pages/api/analytics/metrics.ts', livePath: '/api/analytics/metrics', surface: '/api/analytics/metrics', handler: analyticsMetrics },
];

function assert(condition: boolean, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

function mockReqRes(): {
  req: NextApiRequest;
  res: NextApiResponse<ContainmentResponse>;
  out: { status: number; body: ContainmentResponse | null; headers: Record<string, string> };
} {
  const out: { status: number; body: ContainmentResponse | null; headers: Record<string, string> } = {
    status: 0,
    body: null,
    headers: {},
  };
  const req = {
    method: 'POST',
    query: { profile: 'scratch', session_ref: 'attacker-controlled' },
    body: { cap: Number.MAX_SAFE_INTEGER, sovereign: true, consent: true },
    headers: {
      authorization: 'Bearer attacker-controlled',
      cookie: '__Host-apocky-access-token=attacker-controlled',
      'x-loa-cap': String(Number.MAX_SAFE_INTEGER),
      'x-sovereign-cap': String(Number.MAX_SAFE_INTEGER),
      'x-apocky-sovereign': 'true',
      'x-author-pubkey': 'a'.repeat(64),
      'x-akashic-cap-witness': 'attacker-controlled',
    },
  } as unknown as NextApiRequest;
  const res = {
    status(code: number) {
      out.status = code;
      return this;
    },
    json(value: ContainmentResponse) {
      out.body = value;
      return this;
    },
    setHeader(name: string, value: string | number | readonly string[]) {
      out.headers[name.toLowerCase()] = Array.isArray(value) ? value.join(', ') : String(value);
      return this;
    },
  } as unknown as NextApiResponse<ContainmentResponse>;
  return { req, res, out };
}

async function testEveryHandlerFailsClosed(): Promise<void> {
  for (const route of ROUTES) {
    const { req, res, out } = mockReqRes();
    await route.handler(req, res);
    assert(out.status === 410, `${route.file} returns 410 despite caller assertions`);
    assert(out.body?.ok === false, `${route.file} returns typed ok:false`);
    assert(out.body?.reason_code === CONTAINMENT_REASON_CODE, `${route.file} returns stable reason code`);
    assert(out.body?.surface === route.surface, `${route.file} returns canonical surface`);
    assert(out.body?.replacement === null, `${route.file} does not advertise a fake replacement`);
    assert(out.headers['cache-control'] === CONTAINMENT_HEADERS['Cache-Control'], `${route.file} is private/no-store`);
    assert(out.headers['surrogate-control'] === 'no-store', `${route.file} forbids CDN storage`);
    assert(out.headers['x-robots-tag']?.includes('noindex') === true, `${route.file} is not indexed`);
  }
}

function testMiddlewareInventoryAndSafeExceptions(): void {
  for (const route of ROUTES) {
    assert(containmentSurface(route.livePath) === route.surface, `${route.livePath} is intercepted before parsing`);
    assert(containmentSurface(`${route.livePath}/`) === route.surface, `${route.livePath}/ normalizes trailing slash`);
  }
  assert(EXACT_CONTAINED_PATHS.length === 8, 'exact route inventory count is deliberate');
  for (const safe of [
    '/',
    '/api/health',
    '/api/admin/apocrypha/status',
    '/api/akashic/version',
    '/api/akashic/sourcemap',
  ]) {
    assert(containmentSurface(safe) === null, `${safe} remains outside this containment`);
  }
}

function testDeterministicPayloadAndModernHeaders(): void {
  assert(
    JSON.stringify(containmentPayload('/api/test')) === JSON.stringify(containmentPayload('/api/test')),
    'retirement payload is deterministic',
  );
  const csp = makeCsp(false);
  for (const directive of [
    "default-src 'self'",
    "script-src 'self'",
    "script-src-attr 'none'",
    "object-src 'none'",
    "base-uri 'self'",
    "frame-ancestors 'none'",
  ]) {
    assert(csp.includes(directive), `CSP contains ${directive}`);
  }
  const headers = new Headers();
  applyGlobalSecurityHeaders(headers, true);
  assert(headers.get('content-security-policy') === csp, 'global CSP is static-page compatible');
  assert(headers.get('strict-transport-security')?.includes('includeSubDomains') === true, 'production HSTS is global');
  assert(headers.get('x-content-type-options') === 'nosniff', 'MIME sniffing is disabled');
  assert(headers.get('x-frame-options') === 'DENY', 'legacy frame protection is present');
  assert(headers.get('cross-origin-opener-policy') === 'same-origin-allow-popups', 'opener isolation is explicit');
  assert(headers.get('permissions-policy')?.includes('payment=()') === true, 'browser payment capability is disabled');
}

async function testMiddlewareFailsBeforeCallerControlledParsing(): Promise<void> {
  const response = middleware(new NextRequest('https://www.apocky.com/api/content/publish/revoke', {
    method: 'POST',
    headers: {
      authorization: 'Bearer attacker-controlled',
      'content-type': 'application/json',
      'x-loa-cap': String(Number.MAX_SAFE_INTEGER),
      'x-apocky-sovereign': 'true',
    },
    body: '{malformed',
  }));
  assert(response.status === 410, 'middleware retires route before Pages API body parsing');
  const body = await response.json() as ContainmentResponse;
  assert(body.reason_code === CONTAINMENT_REASON_CODE, 'middleware returns typed reason');
  assert(response.headers.get('cache-control') === CONTAINMENT_HEADERS['Cache-Control'], 'middleware 410 is private/no-store');
  assert(response.headers.get('content-security-policy')?.includes("script-src 'self'") === true, 'middleware 410 carries CSP');

  const sensitive = middleware(new NextRequest('https://www.apocky.com/login'));
  assert(sensitive.headers.get('cache-control') === CONTAINMENT_HEADERS['Cache-Control'], 'auth surface is private/no-store');
  assert(sensitive.headers.get('x-robots-tag')?.includes('noindex') === true, 'auth surface is not indexed');

  const ordinary = middleware(new NextRequest('https://www.apocky.com/'));
  assert(ordinary.headers.get('content-security-policy')?.includes("script-src-attr 'none'") === true, 'ordinary page receives global CSP');
  assert(ordinary.headers.get('x-content-type-options') === 'nosniff', 'ordinary page receives global security headers');
}

function testRetiredSourcesHaveNoEffectDependencies(): void {
  const forbidden = [
    'SUPABASE_SERVICE_ROLE_KEY',
    'getMnemeClient',
    'getStripe',
    'checkCap',
    'resolveCap',
    'isSovereign',
    'fetchApocryphaV2',
    'createClient(',
    'fetch(',
    'stub',
    'TODO',
  ];
  for (const route of ROUTES) {
    const source = readFileSync(resolve(process.cwd(), route.file), 'utf8');
    assert(source.includes('retireApiEndpoint'), `${route.file} delegates to the shared retirement control`);
    for (const token of forbidden) {
      assert(!source.includes(token), `${route.file} contains no effect/stub token ${token}`);
    }
  }
}

function testPublicSurfacesTellTheTruth(): void {
  const app = readFileSync(resolve(process.cwd(), 'pages/_app.tsx'), 'utf8');
  const document = readFileSync(resolve(process.cwd(), 'pages/_document.tsx'), 'utf8');
  const chat = readFileSync(resolve(process.cwd(), 'components/apocrypha/ChatThread.tsx'), 'utf8');
  const contentFetch = readFileSync(resolve(process.cwd(), 'lib/content-fetch.ts'), 'utf8');
  const contentFeed = readFileSync(resolve(process.cwd(), 'components/ContentFeed.tsx'), 'utf8');
  const contentDetailPage = readFileSync(resolve(process.cwd(), 'pages/content/[slug].tsx'), 'utf8');
  assert(!app.includes('akashicInstall'), 'public app cannot activate Akashic collection');
  assert(!app.includes('AkashicConsent'), 'public app does not offer a nonfunctional telemetry tier');
  assert(!document.includes('__akashic_pre_init'), 'pre-hydration telemetry buffer is absent');
  assert(!chat.includes('VisionPanel'), 'retired vision controls are absent from chat');
  assert(chat.includes('Vision · temporarily retired'), 'chat declares the vision boundary');
  assert(contentFetch.includes('res.status === 404 || res.status === 410'), 'content client recognizes deliberate containment');
  assert(contentFeed.includes('Synthetic cards stay hidden'), 'contained feeds do not render synthetic cards');
  assert(contentDetailPage.includes('No placeholder'), 'contained detail page refuses fake package data');
  for (const source of [contentFetch, contentFeed, contentDetailPage]) {
    assert(!source.includes('STUB_'), 'contained content runtime exports no synthetic production data');
  }

  const manifest = JSON.parse(readFileSync(resolve(process.cwd(), 'public/apocrypha-manifest.json'), 'utf8')) as {
    apocrypha: { operator_surfaces: Array<{ rel: string }>; claims: { vision: string } };
  };
  const wellKnown = JSON.parse(readFileSync(resolve(process.cwd(), 'public/.well-known/apocky.json'), 'utf8')) as typeof manifest;
  assert(JSON.stringify(manifest) === JSON.stringify(wellKnown), 'discovery manifests remain identical');
  assert(!manifest.apocrypha.operator_surfaces.some((surface) => surface.rel === 'vision_session'), 'manifest does not advertise retired vision');
  assert(manifest.apocrypha.claims.vision === 'temporarily_retired_for_security_containment', 'vision claim is truthful');
}

async function main(): Promise<void> {
  await testEveryHandlerFailsClosed();
  testMiddlewareInventoryAndSafeExceptions();
  testDeterministicPayloadAndModernHeaders();
  await testMiddlewareFailsBeforeCallerControlledParsing();
  testRetiredSourcesHaveNoEffectDependencies();
  testPublicSurfacesTellTheTruth();
  console.log(`containment.test : OK · ${ROUTES.length} retired handlers + middleware, headers, source, and public-truth checks`);
}

main().catch((error: unknown) => {
  console.error(error);
  process.exitCode = 1;
});
