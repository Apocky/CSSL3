// cssl-edge · lib/chat-relay.ts
// Server-side relay for the Apocrypha public chat (per-user instanced sub-minds).
//
// Browser → /api/chat/send (enqueues a chat_turn) ; the local bridge on the A770 (service-role,
// OUTBOUND poll) leases queued turns, runs that user's sub-mind, and streams chat_chunk rows back.
// The browser tails chat_chunk via Supabase Realtime, RLS-filtered to its own rows.
//
// This module owns the SERVICE-ROLE client + enqueue/rate-limit logic (the bridge reuses
// getServiceClient()). RLS (migration 0044) protects browser clients; the service role bypasses it.

import type { SupabaseClient } from '@supabase/supabase-js';
import { createClient } from '@supabase/supabase-js';

export const RATE_LIMIT = { windowMs: 60_000, max: 12 } as const;

let _svc: SupabaseClient | null | undefined;

// Service-role client (server-only; bypasses RLS). Returns null when unconfigured → routes 503.
export function getServiceClient(): SupabaseClient | null {
  if (_svc !== undefined) return _svc;
  const url =
    process.env['SUPABASE_URL'] ??
    process.env['NEXT_PUBLIC_SUPABASE_URL'] ??
    process.env['APOCKY_HUB_SUPABASE_URL'];
  const key = process.env['SUPABASE_SERVICE_ROLE_KEY'];
  if (!url || !key) {
    _svc = null;
    return null;
  }
  _svc = createClient(url, key, { auth: { persistSession: false } });
  return _svc;
}

export function _resetRelayForTests(): void {
  _svc = undefined;
}

// Sliding-window per-user rate limit. ok=false when the user exceeds RATE_LIMIT.max within the
// window. Best-effort (not transactional) — adequate for login-gated community scale.
export async function checkAndBumpRate(
  sb: SupabaseClient,
  userId: string,
): Promise<{ ok: boolean; remaining: number }> {
  const now = Date.now();
  const { data } = await sb
    .from('chat_rate')
    .select('window_start, count')
    .eq('user_id', userId)
    .maybeSingle();
  const row = data as { window_start: string; count: number } | null;
  let windowStart = row ? new Date(row.window_start).getTime() : 0;
  let count = row?.count ?? 0;
  if (!row || now - windowStart >= RATE_LIMIT.windowMs) {
    windowStart = now;
    count = 0;
  }
  if (count >= RATE_LIMIT.max) return { ok: false, remaining: 0 };
  count += 1;
  await sb
    .from('chat_rate')
    .upsert(
      { user_id: userId, window_start: new Date(windowStart).toISOString(), count },
      { onConflict: 'user_id' },
    );
  return { ok: true, remaining: RATE_LIMIT.max - count };
}

// Resolve a session for the user: reuse a valid owned session_id, else create a fresh one.
export async function ensureSession(
  sb: SupabaseClient,
  userId: string,
  sessionId?: string,
): Promise<string | null> {
  if (sessionId) {
    const { data } = await sb
      .from('chat_session')
      .select('id')
      .eq('id', sessionId)
      .eq('user_id', userId)
      .maybeSingle();
    const found = data as { id: string } | null;
    if (found?.id) {
      await sb.from('chat_session').update({ last_active_at: new Date().toISOString() }).eq('id', found.id);
      return found.id;
    }
  }
  const { data, error } = await sb.from('chat_session').insert({ user_id: userId }).select('id').single();
  if (error || !data) return null;
  return (data as { id: string }).id;
}

// Enqueue a user turn for the bridge. Returns the new turn id, or null on failure.
export async function enqueueTurn(
  sb: SupabaseClient,
  userId: string,
  sessionId: string,
  prompt: string,
): Promise<string | null> {
  const { data, error } = await sb
    .from('chat_turn')
    .insert({ user_id: userId, session_id: sessionId, prompt })
    .select('id')
    .single();
  if (error || !data) return null;
  return (data as { id: string }).id;
}
