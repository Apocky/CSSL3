export const MOBILE_SITE_URL = 'https://www.apocky.com' as const;
export const MOBILE_SUPABASE_URL = 'https://pzirbmyfmrbtkllrtcmx.supabase.co' as const;

export interface MobileConfig {
  readonly schema_version: 'apocky.mobile-config.v1';
  readonly site_url: typeof MOBILE_SITE_URL;
  readonly supabase_url: typeof MOBILE_SUPABASE_URL;
  readonly supabase_publishable_key: string;
  readonly api_base: '/api/mobile';
  readonly access: 'account';
}

export interface PublicMobileEnvironment {
  readonly NEXT_PUBLIC_SUPABASE_URL?: string;
  readonly NEXT_PUBLIC_SUPABASE_ANON_KEY?: string;
  readonly NEXT_PUBLIC_SUPABASE_PUBLISHABLE_KEY?: string;
}

function publicKey(value: unknown): value is string {
  if (typeof value !== 'string' || value.length > 8192) return false;
  if (/^sb_publishable_[A-Za-z0-9_-]{20,256}$/.test(value)) return true;
  if (!/^[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+$/.test(value)) return false;
  try {
    const encoded = value.split('.')[1];
    if (!encoded) return false;
    const payload: unknown = JSON.parse(Buffer.from(encoded, 'base64url').toString('utf8'));
    return Boolean(payload && typeof payload === 'object' && !Array.isArray(payload)
      && (payload as Record<string, unknown>).role === 'anon'
      && (payload as Record<string, unknown>).ref === 'pzirbmyfmrbtkllrtcmx');
  } catch {
    return false;
  }
}

export function mobileConfigFromEnvironment(env: PublicMobileEnvironment): MobileConfig | null {
  const rawUrl = env.NEXT_PUBLIC_SUPABASE_URL;
  if (rawUrl !== MOBILE_SUPABASE_URL && rawUrl !== `${MOBILE_SUPABASE_URL}/`) return null;
  const key = env.NEXT_PUBLIC_SUPABASE_PUBLISHABLE_KEY ?? env.NEXT_PUBLIC_SUPABASE_ANON_KEY;
  if (!publicKey(key)) return null;
  return {
    schema_version: 'apocky.mobile-config.v1',
    site_url: MOBILE_SITE_URL,
    supabase_url: MOBILE_SUPABASE_URL,
    supabase_publishable_key: key,
    api_base: '/api/mobile',
    access: 'account',
  };
}

export function readPublicMobileEnvironment(): PublicMobileEnvironment {
  return {
    NEXT_PUBLIC_SUPABASE_URL: process.env.NEXT_PUBLIC_SUPABASE_URL,
    NEXT_PUBLIC_SUPABASE_ANON_KEY: process.env.NEXT_PUBLIC_SUPABASE_ANON_KEY,
    NEXT_PUBLIC_SUPABASE_PUBLISHABLE_KEY: process.env.NEXT_PUBLIC_SUPABASE_PUBLISHABLE_KEY,
  };
}
