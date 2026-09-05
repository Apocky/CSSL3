import type { NextRequest } from 'next/server';
import { NextResponse } from 'next/server';
import {
  applyContainmentHeaders,
  containmentPayload,
  containmentSurface,
} from '@/lib/containment';

export function makeCsp(isDev = process.env.NODE_ENV === 'development'): string {
  return [
    "default-src 'self'",
    `script-src 'self'${isDev ? " 'unsafe-eval'" : ''}`,
    "script-src-attr 'none'",
    // The existing Pages-Router site uses React style attributes. Executable
    // scripts remain same-origin-only and inline script attributes are denied.
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

export function applyGlobalSecurityHeaders(
  headers: Headers,
  isProduction = process.env.NODE_ENV === 'production',
): void {
  headers.set('Content-Security-Policy', makeCsp(!isProduction));
  headers.set('Referrer-Policy', 'no-referrer');
  headers.set('X-Content-Type-Options', 'nosniff');
  headers.set('X-Frame-Options', 'DENY');
  headers.set('X-DNS-Prefetch-Control', 'off');
  headers.set('X-Permitted-Cross-Domain-Policies', 'none');
  headers.set('Origin-Agent-Cluster', '?1');
  headers.set('Cross-Origin-Opener-Policy', 'same-origin-allow-popups');
  headers.set(
    'Permissions-Policy',
    'camera=(), microphone=(), geolocation=(), payment=(), usb=(), browsing-topics=()'
  );
  if (isProduction) {
    headers.set('Strict-Transport-Security', 'max-age=63072000; includeSubDomains; preload');
  }
}

export function middleware(request: NextRequest): NextResponse {
  const pathname = request.nextUrl.pathname;
  const clinical = pathname.startsWith('/shawn/clinical');
  const sensitive =
    clinical
    || pathname === '/admin'
    || pathname.startsWith('/admin/')
    || pathname === '/api/admin'
    || pathname.startsWith('/api/admin/')
    || pathname === '/auth'
    || pathname.startsWith('/auth/')
    || pathname === '/api/auth'
    || pathname.startsWith('/api/auth/')
    || pathname === '/login'
    || pathname === '/register'
    || pathname === '/account'
    || pathname === '/chat'
    || pathname === '/apocrypha';
  const retiredSurface = containmentSurface(pathname);
  if (retiredSurface !== null) {
    const response = NextResponse.json(containmentPayload(retiredSurface), { status: 410 });
    applyGlobalSecurityHeaders(response.headers);
    applyContainmentHeaders(response.headers);
    return response;
  }

  const response = NextResponse.next();
  applyGlobalSecurityHeaders(response.headers);

  if (sensitive) {
    applyContainmentHeaders(response.headers);
  }

  return response;
}

export const config = {
  matcher: ['/((?!_next/static|_next/image|favicon.ico).*)'],
};
