import type { NextRequest } from 'next/server';
import { NextResponse } from 'next/server';

const RETIRED_EXACT_PATHS = new Set([
  '/apoc',
  '/apocrypha',
  '/apocrypha-manifest.json',
  '/apx',
  '/chat',
  '/admin/apex',
  '/admin/apocrypha',
  '/admin/chat',
  '/admin/coder',
  '/admin/cognition',
  '/admin/controls',
  '/admin/diagnostics',
  '/admin/sub-minds',
  '/admin/tools',
  '/api/admin/apocrypha',
  '/api/admin/apocv4',
  '/api/apocrypha',
  '/api/cron/apocrypha-sms',
]);

const RETIRED_PATH_PREFIXES = [
  '/apoc/',
  '/apocrypha/',
  '/apx/',
  '/chat/',
  '/admin/apocrypha/',
  '/api/apocrypha/',
  '/api/admin/apocrypha/',
  '/api/admin/apocv4/',
];

// Mneme routes currently accept a caller-supplied profile id while using a
// service-role database client. Keep the implementation available for local
// tests, but never expose it through the public runtime until an authenticated
// principal is cryptographically bound to exactly one profile.
const UNBROKERED_PRIVATE_PATH_PREFIXES = ['/api/mneme/'];

const RETIRED_HOST = 'apocrypha.apocky.com';

function requestHost(request: NextRequest): string {
  return (request.headers.get('host') ?? request.nextUrl.hostname)
    .split(':', 1)[0]
    ?.toLowerCase() ?? '';
}

export function isRetiredWebRuntimeRequest(request: NextRequest): boolean {
  const pathname = request.nextUrl.pathname;
  return requestHost(request) === RETIRED_HOST
    || RETIRED_EXACT_PATHS.has(pathname)
    || RETIRED_PATH_PREFIXES.some((prefix) => pathname.startsWith(prefix));
}

export function isUnbrokeredPrivateRuntimeRequest(request: NextRequest): boolean {
  return UNBROKERED_PRIVATE_PATH_PREFIXES.some((prefix) => request.nextUrl.pathname.startsWith(prefix));
}

function retiredNotFound(): NextResponse {
  const response = new NextResponse(null, { status: 404 });
  response.headers.set('Cache-Control', 'private, no-store, no-cache, must-revalidate, max-age=0');
  response.headers.set('Referrer-Policy', 'no-referrer');
  response.headers.set('X-Content-Type-Options', 'nosniff');
  response.headers.set('X-Frame-Options', 'DENY');
  response.headers.set('X-Robots-Tag', 'noindex, nofollow, noarchive, nosnippet');
  return response;
}

function makeCsp(nonce: string): string {
  const isDev = process.env.NODE_ENV === 'development';
  return [
    "default-src 'self'",
    `script-src 'self' 'nonce-${nonce}' 'strict-dynamic'${isDev ? " 'unsafe-eval'" : ''}`,
    // The existing Pages-Router site uses React style attributes. Script
    // execution remains nonce-bound; styles cannot execute JavaScript.
    "style-src 'self' 'unsafe-inline'",
    "img-src 'self' blob: data:",
    "font-src 'self' data:",
    "connect-src 'self'",
    "media-src 'self'",
    "worker-src 'self' blob:",
    "manifest-src 'self'",
    "object-src 'none'",
    "base-uri 'self'",
    "form-action 'self'",
    "frame-src 'none'",
    "frame-ancestors 'none'",
    ...(isDev ? [] : ['upgrade-insecure-requests']),
  ].join('; ');
}

export function middleware(request: NextRequest): NextResponse {
  if (isRetiredWebRuntimeRequest(request) || isUnbrokeredPrivateRuntimeRequest(request)) {
    return retiredNotFound();
  }

  const nonce = btoa(crypto.randomUUID());
  const clinical = request.nextUrl.pathname.startsWith('/shawn/clinical');
  const csp = makeCsp(nonce);
  const requestHeaders = new Headers(request.headers);
  requestHeaders.set('x-nonce', nonce);
  requestHeaders.set('Content-Security-Policy', csp);

  const response = NextResponse.next({ request: { headers: requestHeaders } });
  response.headers.set('Content-Security-Policy', csp);
  response.headers.set('Referrer-Policy', 'no-referrer');
  response.headers.set('X-Content-Type-Options', 'nosniff');
  response.headers.set('X-Frame-Options', 'DENY');
  response.headers.set(
    'Permissions-Policy',
    'camera=(), microphone=(), geolocation=(), payment=(), usb=(), browsing-topics=()'
  );

  if (clinical) {
    response.headers.set('Cache-Control', 'private, no-store, no-cache, must-revalidate, max-age=0');
    response.headers.set('Pragma', 'no-cache');
    response.headers.set('X-Robots-Tag', 'noindex, nofollow, noarchive, nosnippet');
  }

  return response;
}

export const config = {
  matcher: [
    '/shawn',
    '/shawn/:path*',
    '/apoc',
    '/apoc/:path*',
    '/apocrypha',
    '/apocrypha/:path*',
    '/apocrypha-manifest.json',
    '/apx',
    '/apx/:path*',
    '/chat',
    '/chat/:path*',
    '/admin/apex',
    '/admin/apocrypha',
    '/admin/apocrypha/:path*',
    '/admin/chat',
    '/admin/coder',
    '/admin/cognition',
    '/admin/controls',
    '/admin/diagnostics',
    '/admin/sub-minds',
    '/admin/tools',
    '/api/apocrypha',
    '/api/apocrypha/:path*',
    '/api/admin/apocrypha',
    '/api/admin/apocrypha/:path*',
    '/api/admin/apocv4',
    '/api/admin/apocv4/:path*',
    '/api/cron/apocrypha-sms',
    '/api/mneme/:path*',
    {
      source: '/:path*',
      has: [{ type: 'host', value: 'apocrypha.apocky.com' }],
    },
  ],
};
