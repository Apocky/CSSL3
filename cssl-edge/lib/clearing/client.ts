import type { SupabaseClient } from '@supabase/supabase-js';

import { getAuthClient } from '../auth';

export type ClearingRoom = {
  id: string;
  slug: string;
  title: string;
  description: string;
  glyph: string;
  visibility: 'public' | 'closed';
  created_at: string;
  archived_at: string | null;
};

export type ClearingMessage = {
  id: string;
  room_id: string;
  thread_id: string | null;
  reply_to_id: string | null;
  author_ref: string;
  author_label: string;
  body: string;
  created_at: string;
  edited_at: string | null;
  deleted_at: string | null;
};

export type ClearingReaction = {
  message_id: string;
  actor_ref: string;
  kind: 'spark' | 'heart' | 'echo' | 'curious';
  created_at: string;
};

export type ClearingMember = {
  room_id: string;
  actor_ref: string;
  display_name: string;
  joined_at: string;
  last_posted_at: string | null;
};

export type ClearingLiveState = 'loading' | 'live' | 'reconnecting' | 'unavailable';

export function clearingClient(): SupabaseClient | null {
  return getAuthClient();
}

export function clearingNonce(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  if (typeof crypto !== 'undefined' && typeof crypto.getRandomValues === 'function') {
    const bytes = crypto.getRandomValues(new Uint8Array(16));
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    const hex = [...bytes].map((value) => value.toString(16).padStart(2, '0')).join('');
    return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
  }
  throw new Error('Secure message idempotency is unavailable in this browser.');
}

export async function clearingActorRef(userId: string): Promise<string> {
  if (typeof crypto === 'undefined' || !crypto.subtle) {
    throw new Error('Secure actor reference is unavailable in this browser.');
  }
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(userId));
  return [...new Uint8Array(digest)]
    .map((value) => value.toString(16).padStart(2, '0'))
    .join('')
    .slice(0, 16);
}
