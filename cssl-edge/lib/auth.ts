// cssl-edge/lib/auth.ts · Supabase-Auth wrapper for apocky.com hub
// Per spec/22 : single SSO across all Apocky-projects via JWT issued-by-hub-Supabase.
// Null-fallback when APOCKY_HUB_SUPABASE_URL env-var missing (stage-0 stub mode).

import { createClient, type SupabaseClient } from '@supabase/supabase-js';

let cachedClient: SupabaseClient | null = null;

/**
 * Returns the apocky-hub Supabase client OR null if env vars missing.
 * Pages/routes MUST handle null-case gracefully (show stub-mode UI).
 */
export function getAuthClient(): SupabaseClient | null {
  if (cachedClient) return cachedClient;
  const url = process.env.APOCKY_HUB_SUPABASE_URL ?? process.env.NEXT_PUBLIC_SUPABASE_URL;
  const anonKey = process.env.APOCKY_HUB_SUPABASE_ANON_KEY ?? process.env.NEXT_PUBLIC_SUPABASE_ANON_KEY;
  if (!url || !anonKey) return null;
  cachedClient = createClient(url, anonKey, {
    auth: {
      persistSession: true,
      autoRefreshToken: true,
      // detectSessionInUrl=true so magic-link callback URL hash gets parsed into a session
      // automatically when the client mounts on /account or /auth/callback.
      // Using implicit flow (default) for OTP magic-link · PKCE only for OAuth providers.
      detectSessionInUrl: true,
      flowType: 'implicit',
    },
  });
  return cachedClient;
}

// Persist Supabase session into a server-readable cookie so /api/auth/me
// (server-side) can resolve the user. Idempotent · safe to call multiple times.
// Called from /account and /auth/callback after a successful sign-in.
export function persistSessionToCookie(accessToken: string, refreshToken?: string): void {
  if (typeof document === 'undefined') return;
  // 7-day cookie · HttpOnly cannot be set from client-side · so this is readable
  // by JS on apocky.com. /api/auth/me reads it for-server-side validation.
  const maxAge = 7 * 24 * 60 * 60;
  const secure = location.protocol === 'https:' ? '; Secure' : '';
  document.cookie = `sb-access-token=${encodeURIComponent(accessToken)}; Path=/; Max-Age=${maxAge}; SameSite=Lax${secure}`;
  if (refreshToken) {
    document.cookie = `sb-refresh-token=${encodeURIComponent(refreshToken)}; Path=/; Max-Age=${maxAge}; SameSite=Lax${secure}`;
  }
}

/** Auth-provider configuration · what's available · what's required to enable. */
export const AUTH_PROVIDERS = [
  { id: 'google', label: 'Google', enabled: true, gradient: '#4285f4' },
  { id: 'apple', label: 'Apple', enabled: true, gradient: '#000000' },
  { id: 'github', label: 'GitHub', enabled: true, gradient: '#24292e' },
  { id: 'discord', label: 'Discord', enabled: true, gradient: '#5865f2' },
  { id: 'twitter', label: 'X / Twitter', enabled: false, gradient: '#000000' },
  { id: 'spotify', label: 'Spotify', enabled: false, gradient: '#1db954' },
] as const;

export type AuthProviderId = typeof AUTH_PROVIDERS[number]['id'];

/** Apocky's external channels (not OAuth, just profile links). */
export const APOCKY_CHANNELS = [
  { label: '@noneisone.oneisall (medium)', href: 'https://medium.com/@noneisone.oneisall' },
  { label: 'ko-fi.com/oneinfinity', href: 'https://ko-fi.com/oneinfinity' },
  { label: 'patreon.com/0ne1nfinity', href: 'https://www.patreon.com/0ne1nfinity' },
  { label: 'github.com/Apocky', href: 'https://github.com/Apocky' },
] as const;

/** Profile-linkable social channels the player can attach to their profile. */
export const PROFILE_LINKABLE = [
  { id: 'medium', label: 'Medium', placeholder: '@yourhandle' },
  { id: 'twitter', label: 'X / Twitter', placeholder: '@yourhandle' },
  { id: 'bluesky', label: 'Bluesky', placeholder: 'yourhandle.bsky.social' },
  { id: 'mastodon', label: 'Mastodon', placeholder: '@yourhandle@instance' },
  { id: 'github', label: 'GitHub', placeholder: 'yourhandle' },
  { id: 'youtube', label: 'YouTube', placeholder: '@yourchannel' },
  { id: 'twitch', label: 'Twitch', placeholder: 'yourhandle' },
  { id: 'kofi', label: 'Ko-fi', placeholder: 'yourhandle' },
  { id: 'patreon', label: 'Patreon', placeholder: 'yourhandle' },
  { id: 'website', label: 'Personal site', placeholder: 'https://you.example' },
] as const;

export type LinkableId = typeof PROFILE_LINKABLE[number]['id'];

/** Magic-link sign-in. Returns true on success, false on stub-mode. */
export async function signInWithMagicLink(email: string, redirectTo: string): Promise<{ ok: boolean; reason?: string }> {
  const client = getAuthClient();
  if (!client) {
    return { ok: false, reason: 'stub-mode · APOCKY_HUB_SUPABASE_URL not set' };
  }
  const { error } = await client.auth.signInWithOtp({
    email,
    options: { emailRedirectTo: redirectTo },
  });
  if (error) return { ok: false, reason: error.message };
  return { ok: true };
}

/** OAuth sign-in. Redirects browser. */
export async function signInWithOAuth(provider: AuthProviderId, redirectTo: string): Promise<{ ok: boolean; reason?: string }> {
  const client = getAuthClient();
  if (!client) {
    return { ok: false, reason: 'stub-mode · APOCKY_HUB_SUPABASE_URL not set' };
  }
  const { error } = await client.auth.signInWithOAuth({
    provider: provider as 'google' | 'apple' | 'github' | 'discord',
    options: { redirectTo },
  });
  if (error) return { ok: false, reason: error.message };
  return { ok: true };
}

export async function signOut(): Promise<{ ok: boolean }> {
  const client = getAuthClient();
  if (!client) return { ok: true }; // no-op in stub mode
  await client.auth.signOut();
  return { ok: true };
}

export async function getCurrentUser(): Promise<{
  email: string;
  id: string;
  provider: string;
  createdAt: string;
} | null> {
  const client = getAuthClient();
  if (!client) return null;
  const { data, error } = await client.auth.getUser();
  if (error || !data.user) return null;
  return {
    email: data.user.email ?? '(no email)',
    id: data.user.id,
    provider: data.user.app_metadata?.provider ?? 'unknown',
    createdAt: data.user.created_at ?? new Date().toISOString(),
  };
}

/** Inline tests · exercised via npm test scripts. */
if (require.main === module) {
  // Smoke tests
  const stubClient = getAuthClient();
  console.log('§ auth smoke-test');
  console.log('  client present @', !!stubClient ? '✓' : '✗ stub-mode');
  console.log('  AUTH_PROVIDERS count =', AUTH_PROVIDERS.length);
  console.log('  PROFILE_LINKABLE count =', PROFILE_LINKABLE.length);
  console.log('  APOCKY_CHANNELS count =', APOCKY_CHANNELS.length);
  console.log('  expected ≥ 4 providers · ≥ 8 linkables · 4 channels');
  if (AUTH_PROVIDERS.length < 4) process.exit(1);
  if (PROFILE_LINKABLE.length < 8) process.exit(1);
  if (APOCKY_CHANNELS.length !== 4) process.exit(1);
  console.log('✓ all smoke checks passed');
}
