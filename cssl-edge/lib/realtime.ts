// cssl-edge · lib/realtime.ts
// Supabase Realtime channel-subscription wrappers for two cross-device
// streams used by LoA-v13 :
//
//   subscribeToRoom(room_id, peer_id, onSignal)
//     ← multiplayer signaling : subscribes to NEW rows in
//       `public.signaling_messages` filtered by room_id (and addressed to
//       peer_id OR broadcast `*`). Replaces poll-mode latency.
//
//   subscribeToCocreative(player_id, onUpdate)
//     ← cocreative-bias-vector cross-device sync : subscribes to UPDATEs
//       (and INSERTs) on `public.cocreative_state` filtered by player_id.
//       When the player edits bias on phone, desktop receives the update.
//
// Stage-0 fallback : when env-vars are missing, both helpers return a
// no-op unsubscribe function so callers don't have to branch. The signature
// matches a real Supabase channel cleanup.

import type { RealtimeChannel } from '@supabase/supabase-js';
import { getSupabase } from './supabase';

// Callback shape : caller decides what to do with the row payload.
export type RealtimePayload = {
  eventType: 'INSERT' | 'UPDATE' | 'DELETE';
  new: Record<string, unknown> | null;
  old: Record<string, unknown> | null;
};

// Test-only override : when set, getSupabase() is bypassed and this client
// is used. Lets unit-tests inject a mock without touching env-vars.
let _testClient: { channel: (name: string) => RealtimeChannel } | null = null;

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function _setRealtimeClientForTests(c: any | null): void {
  _testClient = c as { channel: (name: string) => RealtimeChannel } | null;
}

// Subscribe to all signaling-message INSERTs for a given room. Filters by
// `to_peer.eq.<peer_id>` OR `to_peer.eq.*` (broadcast).
//
// Returns an unsubscribe function. When env-vars are missing, the returned
// function is a noop AND no subscription is opened.
export async function subscribeToRoom(
  room_id: string,
  peer_id: string,
  onSignal: (msg: RealtimePayload) => void
): Promise<() => void> {
  // Resolve client : test-injected client wins, else env-backed singleton.
  const sb = (_testClient as unknown as ReturnType<typeof getSupabase>) ?? getSupabase();
  if (sb === null) {
    // Stage-0 fallback : noop unsubscribe.
    return () => {};
  }

  const channelName = `room:${room_id}:peer:${peer_id}`;
  const channel = sb.channel(channelName);

  // postgres_changes filter : INSERTs on signaling_messages where room_id
  // matches AND to_peer is either us or broadcast.
  channel.on(
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    'postgres_changes' as any,
    {
      event: 'INSERT',
      schema: 'public',
      table: 'signaling_messages',
      filter: `room_id=eq.${room_id}`,
    },
    (payload: unknown) => {
      const p = payload as {
        eventType?: 'INSERT' | 'UPDATE' | 'DELETE';
        new?: Record<string, unknown> | null;
        old?: Record<string, unknown> | null;
      };
      const newRow = p.new ?? null;
      // Server-side filter narrows by room_id; we additionally narrow by
      // to_peer client-side because supabase realtime filter syntax doesn't
      // support OR across `to_peer` values.
      if (newRow !== null) {
        const to = newRow['to_peer'];
        if (to !== peer_id && to !== '*') return;
      }
      onSignal({
        eventType: p.eventType ?? 'INSERT',
        new: newRow,
        old: p.old ?? null,
      });
    }
  );

  await channel.subscribe();

  // Return an unsubscribe function that's safe to call multiple times.
  let removed = false;
  return () => {
    if (removed) return;
    removed = true;
    void channel.unsubscribe();
  };
}

// Subscribe to cocreative-state UPDATEs (+ INSERTs) for a given player.
// Used to sync bias-vector edits across the player's devices in real time.
export async function subscribeToCocreative(
  player_id: string,
  onUpdate: (row: RealtimePayload) => void
): Promise<() => void> {
  const sb = (_testClient as unknown as ReturnType<typeof getSupabase>) ?? getSupabase();
  if (sb === null) {
    return () => {};
  }

  const channelName = `cocreative:${player_id}`;
  const channel = sb.channel(channelName);

  // postgres_changes : INSERT + UPDATE both surface as separate events.
  // Use event:'*' to receive both without two subscribe calls.
  channel.on(
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    'postgres_changes' as any,
    {
      event: '*',
      schema: 'public',
      table: 'cocreative_state',
      filter: `player_id=eq.${player_id}`,
    },
    (payload: unknown) => {
      const p = payload as {
        eventType?: 'INSERT' | 'UPDATE' | 'DELETE';
        new?: Record<string, unknown> | null;
        old?: Record<string, unknown> | null;
      };
      onUpdate({
        eventType: p.eventType ?? 'UPDATE',
        new: p.new ?? null,
        old: p.old ?? null,
      });
    }
  );

  await channel.subscribe();

  let removed = false;
  return () => {
    if (removed) return;
    removed = true;
    void channel.unsubscribe();
  };
}

// ─── inline tests · framework-agnostic ─────────────────────────────────────
// Run via `npx tsx lib/realtime.ts`. Uses an in-memory mock Supabase client
// to validate wiring without touching env-vars or network.

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

// Build a mock client whose .channel() returns a chainable stub that records
// .on() + .subscribe() + .unsubscribe() invocations.
interface ChannelTrace {
  channelName: string;
  onCalls: Array<{ kind: string; opts: Record<string, unknown> }>;
  subscribed: boolean;
  unsubscribed: boolean;
}

function buildMockClient(): {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  client: any;
  traces: ChannelTrace[];
} {
  const traces: ChannelTrace[] = [];
  const client = {
    channel(name: string): unknown {
      const trace: ChannelTrace = {
        channelName: name,
        onCalls: [],
        subscribed: false,
        unsubscribed: false,
      };
      traces.push(trace);
      const ch = {
        on(kind: string, opts: Record<string, unknown>, _cb: unknown) {
          trace.onCalls.push({ kind, opts });
          return ch;
        },
        async subscribe(): Promise<unknown> {
          trace.subscribed = true;
          return ch;
        },
        async unsubscribe(): Promise<unknown> {
          trace.unsubscribed = true;
          return ch;
        },
      };
      return ch;
    },
  };
  return { client, traces };
}

// 1. subscribeToRoom returns a callable unsubscribe function.
export async function testSubscribeToRoomReturnsCallable(): Promise<void> {
  const { client, traces } = buildMockClient();
  _setRealtimeClientForTests(client);
  const unsub = await subscribeToRoom('room-1', 'peer-1', () => {});
  assert(typeof unsub === 'function', 'subscribeToRoom must return a function');
  assert(traces.length === 1, `expected 1 channel trace, got ${traces.length}`);
  const t = traces[0];
  if (t === undefined) throw new Error('trace[0] missing');
  assert(t.subscribed === true, 'expected channel.subscribe() to be called');
  unsub();
  assert(t.unsubscribed === true, 'expected channel.unsubscribe() after unsub()');
  _setRealtimeClientForTests(null);
}

// 2. subscribeToCocreative returns a callable unsubscribe function.
export async function testSubscribeToCocreativeReturnsCallable(): Promise<void> {
  const { client, traces } = buildMockClient();
  _setRealtimeClientForTests(client);
  const unsub = await subscribeToCocreative('player-42', () => {});
  assert(typeof unsub === 'function', 'subscribeToCocreative must return a function');
  assert(traces.length === 1, `expected 1 channel trace, got ${traces.length}`);
  const t = traces[0];
  if (t === undefined) throw new Error('trace[0] missing');
  assert(t.channelName.startsWith('cocreative:'), 'channel name must start with cocreative:');
  unsub();
  assert(t.unsubscribed === true, 'expected unsubscribe after unsub()');
  _setRealtimeClientForTests(null);
}

// 3. Null-fallback when env-vars + test-client both unset.
export async function testNullFallbackOnMissingEnv(): Promise<void> {
  // Force null path : test-client cleared and env unset.
  _setRealtimeClientForTests(null);
  delete process.env['NEXT_PUBLIC_SUPABASE_URL'];
  delete process.env['SUPABASE_ANON_KEY'];
  // _resetSupabaseForTests is private to supabase.ts ; re-import to call.
  const { _resetSupabaseForTests } = await import('./supabase');
  _resetSupabaseForTests();

  const unsubA = await subscribeToRoom('room-x', 'peer-x', () => {});
  const unsubB = await subscribeToCocreative('player-x', () => {});
  assert(typeof unsubA === 'function', 'null-fallback room → noop fn');
  assert(typeof unsubB === 'function', 'null-fallback cocreative → noop fn');
  // Calling them must not throw.
  unsubA();
  unsubB();
}

// 4. Two subscriptions are isolated · double-subscribe → distinct channels.
export async function testDoubleSubscribeCleanupIsolated(): Promise<void> {
  const { client, traces } = buildMockClient();
  _setRealtimeClientForTests(client);
  const unsubA = await subscribeToRoom('room-A', 'peer-A', () => {});
  const unsubB = await subscribeToRoom('room-B', 'peer-B', () => {});
  assert(traces.length === 2, `expected 2 traces, got ${traces.length}`);
  unsubA();
  const tA = traces[0];
  const tB = traces[1];
  if (tA === undefined || tB === undefined) throw new Error('traces missing');
  assert(tA.unsubscribed === true, 'A must be unsubscribed after unsubA()');
  assert(tB.unsubscribed === false, 'B must NOT be unsubscribed yet');
  unsubB();
  assert(tB.unsubscribed === true, 'B must be unsubscribed after unsubB()');
  // Idempotency : second unsub must be a noop (no error).
  unsubA();
  unsubB();
  _setRealtimeClientForTests(null);
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

async function runAll(): Promise<void> {
  await testSubscribeToRoomReturnsCallable();
  await testSubscribeToCocreativeReturnsCallable();
  await testNullFallbackOnMissingEnv();
  await testDoubleSubscribeCleanupIsolated();
  // eslint-disable-next-line no-console
  console.log('realtime.ts : OK · 4 inline tests passed');
}

if (isMain) {
  runAll().catch((err) => {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  });
}
