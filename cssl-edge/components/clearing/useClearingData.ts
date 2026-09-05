import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import {
  clearingActorRef,
  clearingClient,
  clearingNonce,
  type ClearingLiveState,
  type ClearingMessage,
  type ClearingReaction,
  type ClearingRoom,
} from '../../lib/clearing/client';

type AuthState = 'signed-out' | 'signed-in' | 'checking' | 'unavailable';
type MutationResult = { ok: boolean; error?: string };

type ClearingData = {
  rooms: ClearingRoom[];
  activeRoom: ClearingRoom | null;
  messages: ClearingMessage[];
  reactions: ClearingReaction[];
  liveState: ClearingLiveState;
  authState: AuthState;
  actorRef: string | null;
  error: string;
  reload: () => Promise<void>;
  sendMessage: (body: string, replyToId?: string | null) => Promise<MutationResult>;
  toggleReaction: (messageId: string, kind: ClearingReaction['kind']) => Promise<MutationResult>;
  withdrawMessage: (messageId: string) => Promise<MutationResult>;
};

const DATA_DEADLINE_MS = 10_000;
const WRITE_DEADLINE_MS = 10_000;
const REALTIME_DEADLINE_MS = 8_000;

function withClientDeadline<T>(promise: PromiseLike<T>, milliseconds: number): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  return Promise.race([
    Promise.resolve(promise),
    new Promise<T>((_, reject) => {
      timer = setTimeout(() => reject(new Error('deadline exceeded')), milliseconds);
    }),
  ]).finally(() => {
    if (timer) clearTimeout(timer);
  });
}

export function mergeClearingMessage(
  rows: ClearingMessage[],
  next: ClearingMessage,
): ClearingMessage[] {
  const byId = new Map(rows.map((row) => [row.id, row]));
  byId.set(next.id, next);
  return [...byId.values()].filter((row) => !row.deleted_at).sort((a, b) => {
    const time = a.created_at.localeCompare(b.created_at);
    return time || a.id.localeCompare(b.id);
  });
}

export function mergeClearingReaction(
  rows: ClearingReaction[],
  next: ClearingReaction,
): ClearingReaction[] {
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
  const [liveState, setLiveState] = useState<ClearingLiveState>('loading');
  const [authState, setAuthState] = useState<AuthState>('checking');
  const [actorRef, setActorRef] = useState<string | null>(null);
  const [error, setError] = useState('');
  const reloadToken = useRef(0);
  const messageIds = useRef<Set<string>>(new Set());

  const replaceMessages = useCallback((next: ClearingMessage[]) => {
    messageIds.current = new Set(next.map((row) => row.id));
    setMessages(next);
  }, []);

  const mergeMessage = useCallback((next: ClearingMessage) => {
    setMessages((current) => {
      const merged = mergeClearingMessage(current, next);
      messageIds.current = new Set(merged.map((row) => row.id));
      return merged;
    });
  }, []);

  const resolveActor = useCallback(async (userId: string | null): Promise<void> => {
    if (!userId) {
      setActorRef(null);
      return;
    }
    try {
      setActorRef(await clearingActorRef(userId));
    } catch {
      setActorRef(null);
    }
  }, []);

  const reload = useCallback(async (): Promise<void> => {
    const client = clearingClient();
    const token = ++reloadToken.current;
    if (!client) {
      setLiveState('unavailable');
      setError('The Clearing data service is not configured.');
      setAuthState('unavailable');
      return;
    }

    setLiveState('loading');
    setError('');
    setActiveRoom(null);
    replaceMessages([]);
    setReactions([]);

    try {
      const [{ data: roomRows, error: roomError }, { data: sessionData, error: sessionError }] =
        await withClientDeadline(Promise.all([
          client
            .from('clearing_room')
            .select('id,slug,title,description,glyph,visibility,created_at,archived_at')
            .order('slug'),
          client.auth.getSession(),
        ]), DATA_DEADLINE_MS);

      if (token !== reloadToken.current) return;
      if (roomError || !roomRows) throw new Error('rooms');
      if (sessionError) {
        setAuthState('unavailable');
        await resolveActor(null);
      } else {
        setAuthState(sessionData.session ? 'signed-in' : 'signed-out');
        await resolveActor(sessionData.session?.user.id ?? null);
      }

      const nextRooms = roomRows as ClearingRoom[];
      const nextRoom = nextRooms.find((room) => room.slug === roomSlug) ?? null;
      setRooms(nextRooms);
      setActiveRoom(nextRoom);
      if (!nextRoom) throw new Error('room');

      const { data: messageRows, error: messageError } = await withClientDeadline(
        client
          .from('clearing_message')
          .select('id,room_id,thread_id,reply_to_id,author_ref,author_label,body,created_at,edited_at,deleted_at')
          .eq('room_id', nextRoom.id)
          .is('deleted_at', null)
          .order('created_at', { ascending: true })
          .limit(250),
        DATA_DEADLINE_MS,
      );
      if (token !== reloadToken.current) return;
      if (messageError) throw new Error('messages');

      const nextMessages = (messageRows ?? []) as ClearingMessage[];
      const ids = nextMessages.map((row) => row.id);
      let nextReactions: ClearingReaction[] = [];
      if (ids.length > 0) {
        const { data: reactionRows, error: reactionError } = await withClientDeadline(
          client
            .from('clearing_reaction')
            .select('message_id,actor_ref,kind,created_at')
            .in('message_id', ids),
          DATA_DEADLINE_MS,
        );
        if (token !== reloadToken.current) return;
        if (reactionError) throw new Error('reactions');
        nextReactions = (reactionRows ?? []) as ClearingReaction[];
      }

      replaceMessages(nextMessages);
      setReactions(nextReactions);
    } catch (caught) {
      if (token !== reloadToken.current) return;
      setLiveState('unavailable');
      const kind = caught instanceof Error ? caught.message : '';
      setError(
        kind === 'room'
          ? 'This Clearing room is unavailable.'
          : kind === 'rooms'
            ? 'The room list is unavailable.'
            : kind === 'messages'
              ? 'Messages are unavailable.'
              : kind === 'reactions'
                ? 'Reactions are unavailable.'
                : 'The Clearing timed out.',
      );
    }
  }, [replaceMessages, resolveActor, roomSlug]);

  useEffect(() => {
    void reload();
  }, [reload]);

  useEffect(() => {
    const client = clearingClient();
    if (!client) return undefined;
    const { data } = client.auth.onAuthStateChange((_event, session) => {
      setAuthState(session ? 'signed-in' : 'signed-out');
      void resolveActor(session?.user.id ?? null);
    });
    return () => data.subscription.unsubscribe();
  }, [resolveActor]);

  useEffect(() => {
    const client = clearingClient();
    if (!client || !activeRoom) return undefined;
    let mounted = true;
    let subscribed = false;
    const subscriptionDeadline = setTimeout(() => {
      if (mounted && !subscribed) {
        setLiveState('unavailable');
        setError('Live room sync timed out. Retry to reconnect.');
      }
    }, REALTIME_DEADLINE_MS);

    const channel = client.channel(`clearing:${activeRoom.id}`);
    channel
      .on(
        'postgres_changes',
        { event: 'INSERT', schema: 'public', table: 'clearing_message', filter: `room_id=eq.${activeRoom.id}` },
        (payload) => {
          if (mounted) mergeMessage(payload.new as ClearingMessage);
        },
      )
      .on(
        'postgres_changes',
        { event: 'UPDATE', schema: 'public', table: 'clearing_message', filter: `room_id=eq.${activeRoom.id}` },
        (payload) => {
          if (mounted) mergeMessage(payload.new as ClearingMessage);
        },
      )
      .on(
        'postgres_changes',
        { event: 'INSERT', schema: 'public', table: 'clearing_reaction' },
        (payload) => {
          if (!mounted) return;
          const next = payload.new as ClearingReaction;
          if (!messageIds.current.has(next.message_id)) return;
          setReactions((current) => mergeClearingReaction(current, next));
        },
      )
      .on(
        'postgres_changes',
        { event: 'DELETE', schema: 'public', table: 'clearing_reaction' },
        (payload) => {
          if (!mounted) return;
          const removed = payload.old as Partial<ClearingReaction>;
          if (!removed.message_id || !messageIds.current.has(removed.message_id)) return;
          setReactions((current) => current.filter((row) => !(
            row.message_id === removed.message_id
            && row.actor_ref === removed.actor_ref
            && row.kind === removed.kind
          )));
        },
      )
      .subscribe((status) => {
        if (!mounted) return;
        if (status === 'SUBSCRIBED') {
          const recovered = subscribed;
          subscribed = true;
          clearTimeout(subscriptionDeadline);
          setLiveState('live');
          setError('');
          if (recovered) void reload();
        } else if (status === 'CHANNEL_ERROR' || status === 'TIMED_OUT') {
          setLiveState('reconnecting');
          setError('Live room sync was interrupted. Reconnecting…');
        } else if (status === 'CLOSED') {
          setLiveState('unavailable');
          setError('Live room sync is closed. Retry to reconnect.');
        }
      });

    return () => {
      mounted = false;
      clearTimeout(subscriptionDeadline);
      void client.removeChannel(channel);
    };
  }, [activeRoom, mergeMessage, reload]);

  const sendMessage = useCallback(async (
    body: string,
    replyToId: string | null = null,
  ): Promise<MutationResult> => {
    const client = clearingClient();
    if (!client) return { ok: false, error: 'The Clearing data service is unavailable.' };
    const trimmed = body.trim();
    if (!trimmed) return { ok: false, error: 'Write a message first.' };
    if (trimmed.length > 2_000) return { ok: false, error: 'Messages are limited to 2,000 characters.' };

    try {
      const { data: sessionData } = await withClientDeadline(client.auth.getSession(), WRITE_DEADLINE_MS);
      if (!sessionData.session) return { ok: false, error: 'Sign in to join the conversation.' };
      const { data, error: sendError } = await withClientDeadline(
        client.rpc('clearing_send_message', {
          p_room_slug: activeRoom?.slug ?? roomSlug,
          p_body: trimmed,
          p_client_nonce: clearingNonce(),
          p_reply_to_id: replyToId,
        }),
        WRITE_DEADLINE_MS,
      );
      if (sendError || !data) return { ok: false, error: 'The message was not placed.' };
      mergeMessage(data as ClearingMessage);
      return { ok: true };
    } catch {
      return { ok: false, error: 'The message timed out without a confirmed result. Retry once.' };
    }
  }, [activeRoom, mergeMessage, roomSlug]);

  const toggleReaction = useCallback(async (
    messageId: string,
    kind: ClearingReaction['kind'],
  ): Promise<MutationResult> => {
    const client = clearingClient();
    if (!client) return { ok: false, error: 'The Clearing data service is unavailable.' };

    try {
      const { data: sessionData } = await withClientDeadline(client.auth.getSession(), WRITE_DEADLINE_MS);
      if (!sessionData.session) return { ok: false, error: 'Sign in to react.' };
      const { data, error: reactionError } = await withClientDeadline(
        client.rpc('clearing_toggle_reaction', { p_message_id: messageId, p_kind: kind }),
        WRITE_DEADLINE_MS,
      );
      if (reactionError) return { ok: false, error: 'The reaction was not changed.' };

      if (data) {
        setReactions((current) => mergeClearingReaction(current, data as ClearingReaction));
      } else if (actorRef) {
        setReactions((current) => current.filter((row) => !(
          row.message_id === messageId && row.actor_ref === actorRef && row.kind === kind
        )));
      }

      const { data: reactionRows, error: refreshError } = await withClientDeadline(
        client
          .from('clearing_reaction')
          .select('message_id,actor_ref,kind,created_at')
          .eq('message_id', messageId),
        WRITE_DEADLINE_MS,
      );
      if (!refreshError && reactionRows) {
        setReactions((current) => [
          ...current.filter((row) => row.message_id !== messageId),
          ...(reactionRows as ClearingReaction[]),
        ]);
      }
      return { ok: true };
    } catch {
      return { ok: false, error: 'The reaction timed out without a confirmed result.' };
    }
  }, [actorRef]);

  const withdrawMessage = useCallback(async (messageId: string): Promise<MutationResult> => {
    const client = clearingClient();
    if (!client) return { ok: false, error: 'The Clearing data service is unavailable.' };

    try {
      const { data: sessionData } = await withClientDeadline(client.auth.getSession(), WRITE_DEADLINE_MS);
      if (!sessionData.session) return { ok: false, error: 'Sign in to withdraw your message.' };
      const { error: withdrawError } = await withClientDeadline(
        client.rpc('clearing_withdraw_message', { p_message_id: messageId }),
        WRITE_DEADLINE_MS,
      );
      if (withdrawError) return { ok: false, error: 'The message was not withdrawn.' };
      setMessages((current) => {
        const next = current.filter((row) => row.id !== messageId);
        messageIds.current = new Set(next.map((row) => row.id));
        return next;
      });
      setReactions((current) => current.filter((row) => row.message_id !== messageId));
      return { ok: true };
    } catch {
      return { ok: false, error: 'Withdrawal timed out without a confirmed result.' };
    }
  }, []);

  return useMemo(() => ({
    rooms,
    activeRoom,
    messages,
    reactions,
    liveState,
    authState,
    actorRef,
    error,
    reload,
    sendMessage,
    toggleReaction,
    withdrawMessage,
  }), [
    rooms,
    activeRoom,
    messages,
    reactions,
    liveState,
    authState,
    actorRef,
    error,
    reload,
    sendMessage,
    toggleReaction,
    withdrawMessage,
  ]);
}
