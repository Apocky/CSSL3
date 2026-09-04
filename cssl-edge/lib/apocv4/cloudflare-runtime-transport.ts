// Server-only Cloudflare Access transport for the authenticated Apocv4 runtime.

export const CLOUDFLARE_RUNTIME_HOST = 'apocrypha.apocky.com' as const;
export const CLOUDFLARE_RUNTIME_ORIGIN = `https://${CLOUDFLARE_RUNTIME_HOST}` as const;

const ACCESS_HEADER_VALUE_RE = /^[\x21-\x7e]{1,4096}$/;

export type CloudflareRuntimeTransportErrorCode =
  | 'runtime_configuration_invalid'
  | 'runtime_credential_unavailable'
  | 'runtime_request_invalid';

export class CloudflareRuntimeTransportError extends Error {
  readonly code: CloudflareRuntimeTransportErrorCode;

  constructor(code: CloudflareRuntimeTransportErrorCode) {
    super(code);
    this.name = 'CloudflareRuntimeTransportError';
    this.code = code;
  }
}

interface CloudflareAccessCredentials {
  clientId: string;
  clientSecret: string;
}

function requireServerRuntime(): void {
  if (typeof window !== 'undefined') {
    throw new CloudflareRuntimeTransportError('runtime_configuration_invalid');
  }
}

function configuredRuntimeOrigin(raw: string | undefined): string {
  requireServerRuntime();
  if (process.env.APOCV4_RUNTIME_TRANSPORT !== 'cloudflare-access') {
    throw new CloudflareRuntimeTransportError('runtime_configuration_invalid');
  }

  const configuredHost = process.env.APOCRYPHA_TUNNEL_HOST;
  if (configuredHost !== undefined && configuredHost !== CLOUDFLARE_RUNTIME_HOST) {
    throw new CloudflareRuntimeTransportError('runtime_configuration_invalid');
  }

  const candidate = raw ?? process.env.APOCV4_RUNTIME_URL ?? CLOUDFLARE_RUNTIME_ORIGIN;
  if (candidate !== candidate.trim() || candidate.endsWith('/')) {
    throw new CloudflareRuntimeTransportError('runtime_configuration_invalid');
  }

  let parsed: URL;
  try {
    parsed = new URL(candidate);
  } catch {
    throw new CloudflareRuntimeTransportError('runtime_configuration_invalid');
  }

  if (
    parsed.protocol !== 'https:'
    || parsed.hostname !== CLOUDFLARE_RUNTIME_HOST
    || parsed.port !== ''
    || parsed.username !== ''
    || parsed.password !== ''
    || parsed.pathname !== '/'
    || parsed.search !== ''
    || parsed.hash !== ''
    || parsed.origin !== candidate
  ) {
    throw new CloudflareRuntimeTransportError('runtime_configuration_invalid');
  }
  return CLOUDFLARE_RUNTIME_ORIGIN;
}

function cloudflareAccessCredentials(): CloudflareAccessCredentials {
  requireServerRuntime();
  const clientId = process.env.CF_ACCESS_CLIENT_ID;
  const clientSecret = process.env.CF_ACCESS_CLIENT_SECRET;
  if (
    !clientId
    || !clientSecret
    || !ACCESS_HEADER_VALUE_RE.test(clientId)
    || !ACCESS_HEADER_VALUE_RE.test(clientSecret)
  ) {
    throw new CloudflareRuntimeTransportError('runtime_credential_unavailable');
  }
  return { clientId, clientSecret };
}

function validateCloudflareRuntimeRequestUrl(raw: string, origin: string): string {
  if (!raw || raw !== raw.trim()) {
    throw new CloudflareRuntimeTransportError('runtime_request_invalid');
  }
  let parsed: URL;
  try {
    parsed = new URL(raw);
  } catch {
    throw new CloudflareRuntimeTransportError('runtime_request_invalid');
  }
  if (
    parsed.origin !== origin
    || parsed.protocol !== 'https:'
    || parsed.hostname !== CLOUDFLARE_RUNTIME_HOST
    || parsed.port !== ''
    || parsed.username !== ''
    || parsed.password !== ''
    || parsed.hash !== ''
  ) {
    throw new CloudflareRuntimeTransportError('runtime_request_invalid');
  }
  return parsed.href;
}

export function validateCloudflareRuntimeOrigin(raw?: string): string {
  return configuredRuntimeOrigin(raw);
}

export function cloudflareRuntimeProtectedValues(): readonly string[] {
  const credentials = cloudflareAccessCredentials();
  return Object.freeze([credentials.clientId, credentials.clientSecret]);
}

export async function fetchCloudflareRuntime(
  url: string,
  init: RequestInit = {},
): Promise<Response> {
  const origin = configuredRuntimeOrigin(undefined);
  const requestUrl = validateCloudflareRuntimeRequestUrl(url, origin);
  const credentials = cloudflareAccessCredentials();
  let headers: Headers;
  try {
    headers = new Headers(init.headers);
    headers.set('CF-Access-Client-Id', credentials.clientId);
    headers.set('CF-Access-Client-Secret', credentials.clientSecret);
  } catch {
    throw new CloudflareRuntimeTransportError('runtime_request_invalid');
  }

  return fetch(requestUrl, {
    ...init,
    headers,
    cache: 'no-store',
    redirect: 'error',
    signal: init.signal,
  });
}
