import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { SupabaseClient } from '@supabase/supabase-js';
import { getAuthClient } from '@/lib/auth';

export type ClearingRoom = { id: string; slug: string; title: string; description: string | null; glyph: string | null; visibility: string; created_at: string; archived_at: string | null };
export type ClearingMessage = { id: string; room_id: string; thread_id: string | null; reply_to_id: string | null; author_ref: string; author_label: string; body: string; created_at: string; edited_at: string | null; deleted_at: string | null };
export type ClearingReaction = { message_id: string; actor_ref: string; kind: string; created_at: string };
export type ClearingMember = { room_id: string; actor_ref: string; display_name: string; joined_at: string; last_posted_at: string | null };

const mergeMessage = (items: ClearingMessage[], next: ClearingMessage) => {
  const map = new Map(items.map((item) => [item.id, item]));
  map.set(next.id, next);
  return [...map.values()].filter((item) => !item.deleted_at).sort((a, b) => a.created_at.localeCompare(b.created_at) || a.id.localeCompare(b.id));
};
const mergeReaction = (items: ClearingReaction[], next: ClearingReaction) => {
  const key = (item: ClearingReaction) => `${item.message_id}:${item.actor_ref}:${item.kind}`;
  const map = new Map(items.map((item) => [key(item), item]));
  map.set(key(next), next);
  return [...map.values()];
};

export function useClearing(roomSlug: string) {
  const [rooms, setRooms] = useState<ClearingRoom[]>([]);
  const [activeRoom, setActiveRoom] = useState<ClearingRoom | null>(null);
  const [messages, setMessages] = useState<ClearingMessage[]>([]);
  const [reactions, setReactions] = useState<ClearingReaction[]>([]);
  const [members, setMembers] = useState<ClearingMember[]>([]);
  const [liveState, setLiveState] = useState<'loading' | 'live' | 'reconnecting' | 'unavailable'>('loading');
  const [authState, setAuthState] = useState<'checking' | 'signed-in' | 'signed-out' | 'unavailable'>('checking');
  const [presenceCount, setPresenceCount] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const generation = useRef(0);
  const client = getAuthClient();

  const reload = useCallback(async () => {
    const sb = getAuthClient();
    const current = ++generation.current;
    if (!sb) { setLiveState('unavailable'); setAuthState('unavailable'); setError('The Clearing data service is not configured.'); return; }
    setLiveState('loading'); setError(null);
    const [{ data: roomRows, error: roomError }, { data: session }] = await Promise.all([
      sb.from('clearing_room').select('id,slug,title,description,glyph,visibility,created_at,archived_at').order('slug'),
      sb.auth.getSession(),
    ]);
    if (current !== generation.current) return;
    setAuthState(session.session ? 'signed-in' : 'signed-out');
    if (roomError || !roomRows) { setLiveState('unavailable'); setError(roomError?.message ?? 'Rooms unavailable.'); return; }
    const nextRooms = roomRows as ClearingRoom[];
    setRooms(nextRooms);
    const room = nextRooms.find((item) => item.slug === roomSlug) ?? nextRooms[0] ?? null;
    setActiveRoom(room);
    if (!room) { setLiveState('live'); return; }
    const [{ data: messageRows, error: messageError }, { data: memberRows },] = await Promise.all([
      sb.from('clearing_message').select('id,room_id,thread_id,reply_to_id,author_ref,author_label,body,created_at,edited_at,deleted_at').eq('room_id', room.id).is('deleted_at', null).order('created_at', { ascending: true }).limit(250),
      sb.from('clearing_room_member').select('room_id,actor_ref,display_name,joined_at,last_posted_at').eq('room_id', room.id).order('last_posted_at', { ascending: false, nullsFirst: false }).limit(100),
    ]);
    const ids = (messageRows ?? []).map((item: ClearingMessage) => item.id);
    const { data: reactionRows } = ids.length ? await sb.from('clearing_reaction').select('message_id,actor_ref,kind,created_at').in('message_id', ids) : { data: [] };
    if (current !== generation.current) return;
    if (messageError) { setLiveState('unavailable'); setError('Messages unavailable.'); return; }
    setMessages((messageRows ?? []) as ClearingMessage[]); setMembers((memberRows ?? []) as ClearingMember[]); setReactions((reactionRows ?? []) as ClearingReaction[]); setLiveState('loading');
  }, [roomSlug]);

  useEffect(() => { void reload(); }, [reload]);
  useEffect(() => {
    const sb = client; const room = activeRoom; if (!sb || !room) return;
    let mounted = true; const key = `guest-${Math.random().toString(36).slice(2)}`;
    const channel = sb.channel(`clearing:${room.id}`, { config: { presence: { key } } });
    channel.on('postgres_changes', { event: 'INSERT', schema: 'public', table: 'clearing_message', filter: `room_id=eq.${room.id}` }, ({ new: row }) => mounted && setMessages((items) => mergeMessage(items, row as ClearingMessage)))
      .on('postgres_changes', { event: 'UPDATE', schema: 'public', table: 'clearing_message', filter: `room_id=eq.${room.id}` }, ({ new: row }) => mounted && setMessages((items) => mergeMessage(items, row as ClearingMessage)))
      .on('postgres_changes', { event: 'INSERT', schema: 'public', table: 'clearing_reaction' }, ({ new: row }) => mounted && setReactions((items) => mergeReaction(items, row as ClearingReaction)))
      .on('postgres_changes', { event: 'DELETE', schema: 'public', table: 'clearing_reaction' }, ({ old: row }) => mounted && setReactions((items) => items.filter((item) => item.message_id !== row.message_id || item.actor_ref !== row.actor_ref || item.kind !== row.kind)))
      .on('postgres_changes', { event: 'INSERT', schema: 'public', table: 'clearing_room_member', filter: `room_id=eq.${room.id}` }, ({ new: row }) => mounted && setMembers((items) => [...items.filter((item) => item.actor_ref !== row.actor_ref), row as ClearingMember]));
    channel.on('presence', { event: 'sync' }, () => mounted && setPresenceCount(Object.keys(channel.presenceState()).length));
    channel.subscribe(async (status) => { if (!mounted) return; if (status === 'SUBSCRIBED') { setLiveState('live'); await channel.track({ kind: 'viewer' }); } else if (status === 'CHANNEL_ERROR' || status === 'TIMED_OUT') setLiveState('reconnecting'); });
    return () => { mounted = false; void sb.removeChannel(channel); };
  }, [activeRoom, client]);

  const sendMessage = useCallback(async (body: string, replyToId: string | null = null) => {
    const sb = getAuthClient(); const text = body.trim(); if (!sb) return { ok: false, error: 'The Clearing data service is unavailable.' };
    if (!text) return { ok: false, error: 'Write a message first.' }; const { data: session } = await sb.auth.getSession(); if (!session.session) return { ok: false, error: 'Sign in to join the conversation.' };
    const { data, error: rpcError } = await sb.rpc('clearing_send_message', { p_room_slug: activeRoom?.slug ?? roomSlug, p_body: text, p_client_nonce: crypto.randomUUID(), p_reply_to_id: replyToId });
    if (rpcError || !data) return { ok: false, error: rpcError?.message ?? 'Message was not placed.' }; setMessages((items) => mergeMessage(items, data as ClearingMessage)); return { ok: true };
  }, [activeRoom, roomSlug]);
  const toggleReaction = useCallback(async (messageId: string, kind: string) => { const sb = getAuthClient(); if (!sb) return { ok: false, error: 'The Clearing data service is unavailable.' }; const { data: session } = await sb.auth.getSession(); if (!session.session) return { ok: false, error: 'Sign in to react.' }; const { data, error: rpcError } = await sb.rpc('clearing_toggle_reaction', { p_message_id: messageId, p_kind: kind }); if (rpcError) return { ok: false, error: rpcError.message }; if (data) setReactions((items) => mergeReaction(items, data as ClearingReaction)); return { ok: true }; }, []);
  return useMemo(() => ({ rooms, activeRoom, messages, reactions, members, liveState, authState, presenceCount, error, reload, sendMessage, toggleReaction }), [rooms, activeRoom, messages, reactions, members, liveState, authState, presenceCount, error, reload, sendMessage, toggleReaction]);
}
