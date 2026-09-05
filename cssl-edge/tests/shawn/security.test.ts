import { NextRequest } from 'next/server';

import { middleware } from '@/middleware';

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
  assert(scriptSrc.includes("'strict-dynamic'"), 'public scripts use strict-dynamic');
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

function testPrivateBrainAuthRefreshHeaders(): void {
  const previousPublic = process.env.NEXT_PUBLIC_SUPABASE_URL;
  const previousHub = process.env.APOCKY_HUB_SUPABASE_URL;
  process.env.NEXT_PUBLIC_SUPABASE_URL = 'https://wrong-project.supabase.co';
  process.env.APOCKY_HUB_SUPABASE_URL = 'https://pzirbmyfmrbtkllrtcmx.supabase.co';
  try {
    const response = middleware(new NextRequest('https://apocky.com/apocrypha'));
    const csp = response.headers.get('content-security-policy') ?? '';
    assert(csp.includes('https://wrong-project.supabase.co'), 'private Brain permits the exact Supabase origin exposed to its browser client');
    assert(csp.includes('wss://wrong-project.supabase.co'), 'private Brain permits the browser client realtime sibling');
    assert(!csp.includes('pzirbmyfmrbtkllrtcmx.supabase.co'), 'a server-only hub override cannot widen browser connect-src');
  } finally {
    if (previousPublic === undefined) delete process.env.NEXT_PUBLIC_SUPABASE_URL;
    else process.env.NEXT_PUBLIC_SUPABASE_URL = previousPublic;
    if (previousHub === undefined) delete process.env.APOCKY_HUB_SUPABASE_URL;
    else process.env.APOCKY_HUB_SUPABASE_URL = previousHub;
  }
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
testPrivateBrainAuthRefreshHeaders();
testNonceRotation();
// eslint-disable-next-line no-console
console.log('shawn/security.test : OK · nonce CSP + clinical privacy headers');
