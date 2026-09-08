import { NextRequest } from 'next/server';
import { createHash } from 'node:crypto';

import { middleware } from '@/middleware';
import { AKASHIC_PRE_HYDRATION_CSP_SHA256, AKASHIC_PRE_HYDRATION_SCRIPT } from '@/lib/akashic-telemetry/pre-hydration';

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

function scriptDirective(csp: string): string {
  return csp
    .split(';')
    .map((part) => part.trim())
    .find((part) => part.startsWith('script-src ')) ?? '';
}

function testPublicAtlasHeaders(): void {
  const response = middleware(new NextRequest('https://apocky.com/shawn'));
  const csp = response.headers.get('content-security-policy') ?? '';
  const scriptSrc = scriptDirective(csp);
  assert(scriptSrc.includes("'nonce-"), 'public CSP carries a request nonce');
  assert(scriptSrc.includes("'self'"), 'same-origin Next.js scripts may hydrate static pages');
  assert(!scriptSrc.includes("'strict-dynamic'"), 'static scripts are not discarded when a build-time page has no request nonce');
  assert(scriptSrc.includes(`'sha256-${AKASHIC_PRE_HYDRATION_CSP_SHA256}'`), 'the exact pre-hydration bootstrap is hash admitted');
  assert(createHash('sha256').update(AKASHIC_PRE_HYDRATION_SCRIPT).digest('base64') === AKASHIC_PRE_HYDRATION_CSP_SHA256, 'bootstrap CSP hash matches emitted bytes');
  assert(!scriptSrc.includes("'unsafe-inline'"), 'public script policy rejects unsafe-inline');
  assert(!csp.includes('supabase.co'), 'public atlas does not open a Supabase connection');
  assert(response.headers.get('referrer-policy') === 'no-referrer', 'no-referrer');
  assert(response.headers.get('x-content-type-options') === 'nosniff', 'nosniff');
  assert(response.headers.get('x-frame-options') === 'DENY', 'frame denial');
}

function testClinicalHeaders(): void {
  const response = middleware(new NextRequest('https://apocky.com/shawn/clinical'));
  const csp = response.headers.get('content-security-policy') ?? '';
  assert(!csp.includes('supabase.co'), 'clinical browser has no direct Supabase connection');
  assert(csp.includes("frame-ancestors 'none'"), 'clinical route cannot be framed');
  assert((response.headers.get('cache-control') ?? '').includes('no-store'), 'clinical is no-store');
  assert((response.headers.get('x-robots-tag') ?? '').includes('noindex'), 'clinical is noindex');
  assert((response.headers.get('x-robots-tag') ?? '').includes('noarchive'), 'clinical is noarchive');
}

function testNonceRotation(): void {
  const first = middleware(new NextRequest('https://apocky.com/shawn'));
  const second = middleware(new NextRequest('https://apocky.com/shawn'));
  assert(
    first.headers.get('content-security-policy') !== second.headers.get('content-security-policy'),
    'nonce rotates on every request'
  );
}

testPublicAtlasHeaders();
testClinicalHeaders();
testNonceRotation();
// eslint-disable-next-line no-console
console.log('shawn/security.test : OK · static hydration CSP + clinical privacy headers');
