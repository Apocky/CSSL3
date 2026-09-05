import type { NextApiResponse } from 'next';

export const CONTAINMENT_REASON_CODE = 'temporary_security_containment' as const;
export const CONTAINMENT_MESSAGE =
  'This capability is temporarily retired while authenticated authority and consent controls are rebuilt.';

export interface ContainmentResponse {
  ok: false;
  error: typeof CONTAINMENT_MESSAGE;
  reason_code: typeof CONTAINMENT_REASON_CODE;
  surface: string;
  replacement: null;
  served_by: 'cssl-edge';
}

export const CONTAINMENT_HEADERS = Object.freeze({
  'Cache-Control': 'private, no-store, no-cache, must-revalidate, max-age=0',
  Pragma: 'no-cache',
  Expires: '0',
  'Surrogate-Control': 'no-store',
  'X-Robots-Tag': 'noindex, nofollow, noarchive, nosnippet',
  'X-Content-Type-Options': 'nosniff',
});

const PREFIX_SURFACES = Object.freeze([
  ['/api/mneme', '/api/mneme/:profile/*'],
  ['/api/payments/stripe', '/api/payments/stripe/*'],
  ['/api/admin/apocrypha/vision', '/api/admin/apocrypha/vision/*'],
  ['/api/content', '/api/content/*'],
] as const);

export const EXACT_CONTAINED_PATHS = Object.freeze([
  '/api/admin/tasks',
  '/api/admin/logs',
  '/api/admin/coder/pending',
  '/api/akashic/event',
  '/api/akashic/batch',
  '/api/akashic/purge',
  '/api/analytics/event',
  '/api/analytics/metrics',
] as const);

const EXACT_SURFACES = new Set<string>(EXACT_CONTAINED_PATHS);

export function containmentSurface(pathname: string): string | null {
  const normalized = pathname.length > 1 && pathname.endsWith('/')
    ? pathname.slice(0, -1)
    : pathname;

  for (const [prefix, surface] of PREFIX_SURFACES) {
    if (normalized === prefix || normalized.startsWith(`${prefix}/`)) return surface;
  }
  return EXACT_SURFACES.has(normalized) ? normalized : null;
}

export function containmentPayload(surface: string): ContainmentResponse {
  return {
    ok: false,
    error: CONTAINMENT_MESSAGE,
    reason_code: CONTAINMENT_REASON_CODE,
    surface,
    replacement: null,
    served_by: 'cssl-edge',
  };
}

export function applyContainmentHeaders(headers: Headers): void {
  for (const [name, value] of Object.entries(CONTAINMENT_HEADERS)) {
    headers.set(name, value);
  }
}

export function retireApiEndpoint(
  res: NextApiResponse<ContainmentResponse>,
  surface: string,
): void {
  for (const [name, value] of Object.entries(CONTAINMENT_HEADERS)) {
    res.setHeader(name, value);
  }
  res.status(410).json(containmentPayload(surface));
}
