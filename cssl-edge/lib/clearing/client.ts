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
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

