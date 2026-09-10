import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import {
  clearingClient,
  clearingNonce,
  type ClearingLiveState,
  type ClearingMember,
  type ClearingMessage,
  type ClearingReaction,
  type ClearingRoom,
} from '../../lib/clearing/client';

type AuthState = 'signed-out' | 'signed-in' | 'checking';

type ClearingData = {
  rooms: ClearingRoom[];
  activeRoom: ClearingRoom | null;
  messages: ClearingMessage[];
  reactions: ClearingReaction[];
  members: ClearingMember[];
  liveState: ClearingLiveState;
  authState: AuthState;
  presenceCount: number;
  error: string;
  reload: () => Promise<void>;
  sendMessage: (body: string, replyToId?: string | null) => Promise<{ ok: boolean; error?: string }>;
  toggleReaction: (messageId: string, kind: ClearingReaction['kind']) => Promise<{ ok: boolean; error?: string }>;
};

function mergeMessage(rows: ClearingMessage[], next: ClearingMessage): ClearingMessage[] {
  const byId = new Map(rows.map((row) => [row.id, row]));
  byId.set(next.id, next);
  return [...byId.values()].filter((row) => !row.deleted_at).sort((a, b) => {
    const time = a.created_at.localeCompare(b.created_at);
    return time || a.id.localeCompare(b.id);
  });
}

function mergeReaction(rows: ClearingReaction[], next: ClearingReaction): ClearingReaction[] {
  const key = `${next.message_id}:${next.actor_ref}:${next.kind}`;
  const byKey = new Map(rows.map((row) => [`${row.message_id}:${row.actor_ref}:${row.kind}`, row]));
  byKey.set(key, next);
  return [...byKey.values()];
}

export function useClearingData(roomSlug: string): ClearingData {
  const [rooms, setRooms] = useState<ClearingRoom[]>([]);
  const [activeRoom, setActiveRoom] = useState<ClearingRoom | null>(null);
  const [messages, setMessages] = useState<ClearingMessage[]>([]);
  const [reactions, setReactions] = useState<ClearingReaction[]>([]);
  const [members, setMembers] = useState<ClearingMember[]>([]);
  const [liveState, setLiveState] = useState<ClearingLiveState>('loading');
  const [authState, setAuthState] = useState<AuthState>('checking');
  const [presenceCount, setPresenceCount] = useState(0);
  const [error, setError] = useState('');
  const reloadToken = useRef(0);

  const reload = useCallback(async () => {
    const client = clearingClient();
    const token = ++reloadToken.current;
    if (!client) {
      setLiveState('unavailable');
      setError('The Clearing data service is not configured.');
      setAuthState('signed-out');
      return;
    }
    setLiveState('loading');
    setError('');
    try {
      const [{ data: roomRows, error: roomError }, { data: sessionData }] = await Promise.all([
        client.from('clearing_room').select('id,slug,title,description,glyph,visibility,created_at,archived_at').order('slug'),
        client.auth.getSession(),
      ]);
      if (token !== reloadToken.current) return;
      if (roomError || !roomRows) throw new Error(roomError?.message ?? 'Rooms unavailable');
      const nextRooms = roomRows as ClearingRoom[];
      setRooms(nextRooms);
      setAuthState(sessionData.session ? 'signed-in' : 'signed-out');
      const nextRoom = nextRooms.find((room) => room.slug === roomSlug) ?? nextRooms.find((room) => room.slug === 'north-clearing') ?? null;
      setActiveRoom(nextRoom);
      if (!nextRoom) throw new Error('No public Clearing room is available.');
      const [{ data: messageRows, error: messageError }, { data: memberRows, error: memberError }] = await Promise.all([
        client.from('clearing_message').select('id,room_id,thread_id,reply_to_id,author_ref,author_label,body,created_at,edited_at,deleted_at').eq('room_id', nextRoom.id).is('deleted_at', null).order('created_at', { ascending: true }).limit(250),
        client.from('clearing_room_member').select('room_id,actor_ref,display_name,joined_at,last_posted_at').eq('room_id', nextRoom.id).order('last_posted_at', { ascending: false, nullsFirst: false }).limit(100),
      ]);
      if (token !== reloadToken.current) return;
      if (messageError) throw new Error(messageError.message);
      setMessages((messageRows ?? []) as ClearingMessage[]);
      const messageIds = (messageRows ?? []).map((row) => (row as ClearingMessage).id);
      if (messageIds.length > 0) {
        const { data: reactionRows, error: reactionError } = await client.from('clearing_reaction').select('message_id,actor_ref,kind,created_at').in('message_id', messageIds);
        setReactions(reactionError ? [] : (reactionRows ?? []) as ClearingReaction[]);
      } else {
        setReactions([]);
      }
      setMembers(memberError ? [] : (memberRows ?? []) as ClearingMember[]);
    } catch (caught) {
      if (token !== reloadToken.current) return;
      setLiveState('unavailable');
      setError(caught instanceof Error ? caught.message : 'Messages unavailable');
    }
  }, [roomSlug]);

  useEffect(() => { void reload(); }, [reload]);

  useEffect(() => {
    const client = clearingClient();
    if (!client || !activeRoom) return undefined;
    let mounted = true;
    const sessionKey = `guest-${Math.random().toString(36).slice(2)}`;
    const channel = client.channel(`clearing:${activeRoom.id}`, {
      config: { presence: { key: sessionKey } },
    });
    channel
      .on('postgres_changes', { event: 'INSERT', schema: 'public', table: 'clearing_message', filter: `room_id=eq.${activeRoom.id}` }, (payload) => {
        if (!mounted) return;
        setMessages((current) => mergeMessage(current, payload.new as ClearingMessage));
      })
      .on('postgres_changes', { event: 'UPDATE', schema: 'public', table: 'clearing_message', filter: `room_id=eq.${activeRoom.id}` }, (payload) => {
        if (!mounted) return;
        setMessages((current) => mergeMessage(current, payload.new as ClearingMessage));
      })
      .on('postgres_changes', { event: 'INSERT', schema: 'public', table: 'clearing_reaction' }, (payload) => {
        if (!mounted) return;
        const next = payload.new as ClearingReaction;
        setReactions((current) => mergeReaction(current, next));
      })
      .on('postgres_changes', { event: 'DELETE', schema: 'public', table: 'clearing_reaction' }, (payload) => {
        if (!mounted) return;
        const removed = payload.old as Partial<ClearingReaction>;
        setReactions((current) => current.filter((row) => !(row.message_id === removed.message_id && row.actor_ref === removed.actor_ref && row.kind === removed.kind)));
      })
      .on('postgres_changes', { event: 'INSERT', schema: 'public', table: 'clearing_room_member', filter: `room_id=eq.${activeRoom.id}` }, (payload) => {
        if (!mounted) return;
        const next = payload.new as ClearingMember;
        setMembers((current) => [...current.filter((row) => row.actor_ref !== next.actor_ref), next]);
      })
      .on('presence', { event: 'sync' }, () => {
        if (!mounted) return;
        const state = channel.presenceState();
        setPresenceCount(Object.keys(state).length);
      })
      .subscribe(async (status) => {
        if (!mounted) return;
        if (status === 'SUBSCRIBED') {
          setLiveState('live');
          await channel.track({ kind: 'viewer' });
        } else if (status === 'CHANNEL_ERROR' || status === 'TIMED_OUT') {
          setLiveState('reconnecting');
        }
      });
    return () => {
      mounted = false;
      void client.removeChannel(channel);
    };
  }, [activeRoom]);

  const sendMessage = useCallback(async (body: string, replyToId: string | null = null) => {
    const client = clearingClient();
    if (!client) return { ok: false, error: 'The Clearing data service is unavailable.' };
    const trimmed = body.trim();
    if (!trimmed) return { ok: false, error: 'Write a message first.' };
    const { data: session } = await client.auth.getSession();
    if (!session.session) return { ok: false, error: 'Sign in to join the conversation.' };
    const { data, error: sendError } = await client.rpc('clearing_send_message', {
      p_room_slug: activeRoom?.slug ?? roomSlug,
      p_body: trimmed,
      p_client_nonce: clearingNonce(),
      p_reply_to_id: replyToId,
    });
    if (sendError || !data) return { ok: false, error: sendError?.message ?? 'Message was not placed.' };
    setMessages((current) => mergeMessage(current, data as ClearingMessage));
    return { ok: true };
  }, [activeRoom, roomSlug]);

  const toggleReaction = useCallback(async (messageId: string, kind: ClearingReaction['kind']) => {
    const client = clearingClient();
    if (!client) return { ok: false, error: 'The Clearing data service is unavailable.' };
    const { data: session } = await client.auth.getSession();
    if (!session.session) return { ok: false, error: 'Sign in to react.' };
    const { data, error: reactionError } = await client.rpc('clearing_toggle_reaction', { p_message_id: messageId, p_kind: kind });
    if (reactionError) return { ok: false, error: reactionError.message };
    if (data) setReactions((current) => mergeReaction(current, data as ClearingReaction));
    return { ok: true };
  }, []);

  return useMemo(() => ({
    rooms,
    activeRoom,
    messages,
    reactions,
    members,
    liveState,
    authState,
    presenceCount,
    error,
    reload,
    sendMessage,
    toggleReaction,
  }), [rooms, activeRoom, messages, reactions, members, liveState, authState, presenceCount, error, reload, sendMessage, toggleReaction]);
}
