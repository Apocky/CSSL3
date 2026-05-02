// § lib/engine-status.ts · typed-fetch + auto-refresh hooks for /engine
// Sibling W14-J/K/L own /api/engine/* backends · this is CLIENT surface
// I> stub-mode-aware : 404 → graceful zero-state · 5s poll · backoff-on-fail
// I> public-readable · NO auth-gate · transparency-mandate

import { useCallback, useEffect, useRef, useState } from 'react';

// ─────────────────────────────────────────────
// § Response shapes (W14-K backend contract)
// ─────────────────────────────────────────────

export interface NodeHeartbeat {
  last_seen: number; // ⊑ epoch-ms · null when never
  uptime_secs: number;
  online: boolean; // ⊑ derived : (now - last_seen) < HEARTBEAT_GRACE
}

export interface HeartbeatResponse {
  local: NodeHeartbeat;
  cloud: NodeHeartbeat;
  now: number; // ⊑ server-now epoch-ms
  stub?: boolean;
  reason?: string;
}

export interface CycleCounts {
  self_author: number;
  playtest: number;
  kan_rollup: number;
  mycelium_sync: number;
  sigma_chain_anchors: number;
  bytes_processed: number;
  since: number; // ⊑ window-start epoch-ms · server-supplied
  until: number; // ⊑ window-end epoch-ms
  stub?: boolean;
  reason?: string;
}

export type EventKind =
  | 'self_author'
  | 'playtest'
  | 'kan_rollup'
  | 'mycelium_sync'
  | 'sigma_anchor'
  | 'idle_enter'
  | 'idle_exit'
  | 'sovereign_pause'
  | 'sovereign_resume';

export interface EngineEvent {
  ts: number; // ⊑ epoch-ms
  kind: EventKind;
  summary: string; // ⊑ k-anon-respecting · ¬ PII
  sigma_chain_anchor?: string; // ⊑ optional 16-hex-prefix
}

export interface EventsResponse {
  events: EngineEvent[];
  stub?: boolean;
  reason?: string;
}

export interface SovereignPauseState {
  paused: boolean;
  by?: string; // ⊑ pubkey-prefix only (privacy)
  since?: number;
  reason?: string;
  stub?: boolean;
}

// ─────────────────────────────────────────────
// § Defensive zero-state factories
// ─────────────────────────────────────────────

const ZERO_NODE: NodeHeartbeat = { last_seen: 0, uptime_secs: 0, online: false };

export function emptyHeartbeat(reason?: string): HeartbeatResponse {
  return {
    local: { ...ZERO_NODE },
    cloud: { ...ZERO_NODE },
    now: Date.now(),
    stub: true,
    reason: reason ?? 'engine-orchestrator (W14-J/K) not yet deployed · zero-state',
  };
}

export function emptyCycles(reason?: string): CycleCounts {
  const now = Date.now();
  return {
    self_author: 0,
    playtest: 0,
    kan_rollup: 0,
    mycelium_sync: 0,
    sigma_chain_anchors: 0,
    bytes_processed: 0,
    since: now - 3_600_000,
    until: now,
    stub: true,
    reason: reason ?? 'engine-orchestrator (W14-J) not yet deployed · zero-state',
  };
}

export function emptyEvents(reason?: string): EventsResponse {
  return {
    events: [],
    stub: true,
    reason: reason ?? 'engine-event-feed (W14-J/K) not yet deployed · zero-state',
  };
}

export function emptyPauseState(reason?: string): SovereignPauseState {
  return {
    paused: false,
    stub: true,
    reason: reason ?? 'sovereign-pause endpoint (W14-J) not yet deployed',
  };
}

// ─────────────────────────────────────────────
// § Single-fetch helpers · zero-state on any failure
// ─────────────────────────────────────────────

export async function fetchHeartbeat(signal?: AbortSignal): Promise<HeartbeatResponse> {
  try {
    const res = await fetch('/api/engine/heartbeat', { signal });
    if (res.status === 404) return emptyHeartbeat('endpoint 404 · sibling-W14-K not landed');
    if (res.status === 429) return emptyHeartbeat('rate-limited · backoff-active');
    if (!res.ok) return emptyHeartbeat(`${res.status} ${res.statusText}`);
    const json = (await res.json()) as Partial<HeartbeatResponse>;
    const now = typeof json.now === 'number' ? json.now : Date.now();
    const local = normalizeNode(json.local, now);
    const cloud = normalizeNode(json.cloud, now);
    return { local, cloud, now, stub: json.stub ?? false, reason: json.reason };
  } catch (err) {
    if (err instanceof DOMException && err.name === 'AbortError') throw err;
    return emptyHeartbeat(err instanceof Error ? err.message : 'network error');
  }
}

export async function fetchCycles(sinceMs?: number, signal?: AbortSignal): Promise<CycleCounts> {
  try {
    const url = sinceMs !== undefined
      ? `/api/engine/cycles?since=${encodeURIComponent(String(sinceMs))}`
      : '/api/engine/cycles';
    const res = await fetch(url, { signal });
    if (res.status === 404) return emptyCycles('endpoint 404 · sibling-W14-K not landed');
    if (res.status === 429) return emptyCycles('rate-limited · backoff-active');
    if (!res.ok) return emptyCycles(`${res.status} ${res.statusText}`);
    const json = (await res.json()) as Partial<CycleCounts>;
    const now = Date.now();
    return {
      self_author: numOr(json.self_author, 0),
      playtest: numOr(json.playtest, 0),
      kan_rollup: numOr(json.kan_rollup, 0),
      mycelium_sync: numOr(json.mycelium_sync, 0),
      sigma_chain_anchors: numOr(json.sigma_chain_anchors, 0),
      bytes_processed: numOr(json.bytes_processed, 0),
      since: numOr(json.since, now - 3_600_000),
      until: numOr(json.until, now),
      stub: json.stub ?? false,
      reason: json.reason,
    };
  } catch (err) {
    if (err instanceof DOMException && err.name === 'AbortError') throw err;
    return emptyCycles(err instanceof Error ? err.message : 'network error');
  }
}

export async function fetchRecentEvents(
  limit = 50,
  signal?: AbortSignal,
): Promise<EventsResponse> {
  try {
    const url = `/api/engine/recent-events?limit=${encodeURIComponent(String(limit))}`;
    const res = await fetch(url, { signal });
    if (res.status === 404) return emptyEvents('endpoint 404 · sibling-W14-K not landed');
    if (res.status === 429) return emptyEvents('rate-limited · backoff-active');
    if (!res.ok) return emptyEvents(`${res.status} ${res.statusText}`);
    const json = (await res.json()) as Partial<EventsResponse> | EngineEvent[];
    // ⊑ accept either {events:[…]} or bare array
    const arr = Array.isArray(json)
      ? json
      : Array.isArray((json as EventsResponse).events)
        ? (json as EventsResponse).events
        : [];
    const events = arr.slice(0, limit).map(normalizeEvent).filter((e): e is EngineEvent => e !== null);
    return { events, stub: !Array.isArray(json) && (json as EventsResponse).stub === true, reason: !Array.isArray(json) ? (json as EventsResponse).reason : undefined };
  } catch (err) {
    if (err instanceof DOMException && err.name === 'AbortError') throw err;
    return emptyEvents(err instanceof Error ? err.message : 'network error');
  }
}

export async function fetchPauseState(signal?: AbortSignal): Promise<SovereignPauseState> {
  try {
    const res = await fetch('/api/engine/sovereign-pause', { signal });
    if (res.status === 404) return emptyPauseState('endpoint 404 · sibling-W14-J not landed');
    if (!res.ok) return emptyPauseState(`${res.status} ${res.statusText}`);
    const json = (await res.json()) as Partial<SovereignPauseState>;
    return {
      paused: json.paused === true,
      by: typeof json.by === 'string' ? sanitizePubkey(json.by) : undefined,
      since: typeof json.since === 'number' ? json.since : undefined,
      reason: typeof json.reason === 'string' ? json.reason : undefined,
      stub: json.stub ?? false,
    };
  } catch (err) {
    if (err instanceof DOMException && err.name === 'AbortError') throw err;
    return emptyPauseState(err instanceof Error ? err.message : 'network error');
  }
}

export interface PauseToggleInput {
  cap: number;
  pause: boolean;
  reason?: string;
}

export interface PauseToggleResult {
  ok: boolean;
  paused: boolean;
  reason?: string;
}

export async function postSovereignPause(
  input: PauseToggleInput,
): Promise<PauseToggleResult> {
  try {
    const res = await fetch('/api/engine/sovereign-pause', {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        'x-loa-cap': String(input.cap | 0),
      },
      body: JSON.stringify({ pause: input.pause, reason: input.reason }),
    });
    if (res.status === 404) {
      return { ok: false, paused: false, reason: 'endpoint 404 · sibling-W14-J not landed · stub-mode' };
    }
    if (res.status === 403) {
      return { ok: false, paused: false, reason: 'cap-denied · sovereign-cap required' };
    }
    if (!res.ok) {
      return { ok: false, paused: false, reason: `${res.status} ${res.statusText}` };
    }
    const json = (await res.json()) as Partial<PauseToggleResult & { paused?: boolean }>;
    return {
      ok: true,
      paused: json.paused === true,
      reason: typeof json.reason === 'string' ? json.reason : undefined,
    };
  } catch (err) {
    return { ok: false, paused: false, reason: err instanceof Error ? err.message : 'network error' };
  }
}

// ─────────────────────────────────────────────
// § Format helpers · phone-friendly
// ─────────────────────────────────────────────

const HEARTBEAT_GRACE_MS = 30_000; // ⊑ <30s = ONLINE · ≥30s = STALE

export function isOnline(node: NodeHeartbeat, now: number): boolean {
  if (!node || node.last_seen === 0) return false;
  return now - node.last_seen < HEARTBEAT_GRACE_MS;
}

export function fmtCompact(v: number | null | undefined): string {
  if (v === null || v === undefined || Number.isNaN(v)) return '—';
  if (v >= 1_000_000_000) return `${(v / 1_000_000_000).toFixed(1)}B`;
  if (v >= 1_000_000) return `${(v / 1_000_000).toFixed(1)}M`;
  if (v >= 1_000) return `${(v / 1_000).toFixed(1)}k`;
  if (Number.isInteger(v)) return String(v);
  return v.toFixed(1);
}

export function fmtBytes(v: number | null | undefined): string {
  if (v === null || v === undefined || Number.isNaN(v)) return '—';
  if (v >= 1_073_741_824) return `${(v / 1_073_741_824).toFixed(1)} GiB`;
  if (v >= 1_048_576) return `${(v / 1_048_576).toFixed(1)} MiB`;
  if (v >= 1024) return `${(v / 1024).toFixed(1)} KiB`;
  return `${Math.round(v)} B`;
}

export function fmtUptime(secs: number | null | undefined): string {
  if (secs === null || secs === undefined || Number.isNaN(secs) || secs < 0) return '—';
  if (secs < 60) return `${Math.round(secs)}s`;
  if (secs < 3600) return `${Math.round(secs / 60)}m`;
  if (secs < 86_400) return `${Math.floor(secs / 3600)}h ${Math.round((secs % 3600) / 60)}m`;
  return `${Math.floor(secs / 86_400)}d ${Math.floor((secs % 86_400) / 3600)}h`;
}

// I> privacy-respecting · ¬ exact-timestamp · bucket-coarse
export function fmtRelTime(ts: number | null | undefined, now: number): string {
  if (ts === null || ts === undefined || ts <= 0) return 'never';
  const dt = Math.max(0, now - ts);
  if (dt < 5_000) return 'just-now';
  if (dt < 60_000) return `${Math.floor(dt / 1000)}s ago`;
  if (dt < 3_600_000) return `${Math.floor(dt / 60_000)}m ago`;
  if (dt < 86_400_000) return `${Math.floor(dt / 3_600_000)}h ago`;
  return `${Math.floor(dt / 86_400_000)}d ago`;
}

export function fmtRelTimeCoarse(ts: number | null | undefined, now: number): string {
  if (ts === null || ts === undefined || ts <= 0) return 'unknown';
  const dt = Math.max(0, now - ts);
  if (dt < 60_000) return 'within the last minute';
  if (dt < 3_600_000) return 'within the last hour';
  if (dt < 86_400_000) return 'within the last day';
  if (dt < 604_800_000) return 'within the last week';
  return 'over a week ago';
}

// ─────────────────────────────────────────────
// § Internal normalizers
// ─────────────────────────────────────────────

function numOr(v: unknown, fallback: number): number {
  return typeof v === 'number' && !Number.isNaN(v) ? v : fallback;
}

function normalizeNode(n: Partial<NodeHeartbeat> | undefined, now: number): NodeHeartbeat {
  if (!n) return { ...ZERO_NODE };
  const last_seen = numOr(n.last_seen, 0);
  const uptime_secs = numOr(n.uptime_secs, 0);
  const online = typeof n.online === 'boolean' ? n.online : last_seen > 0 && now - last_seen < HEARTBEAT_GRACE_MS;
  return { last_seen, uptime_secs, online };
}

function normalizeEvent(raw: unknown): EngineEvent | null {
  if (typeof raw !== 'object' || raw === null) return null;
  const r = raw as Partial<EngineEvent>;
  const ts = numOr(r.ts, 0);
  if (ts <= 0) return null;
  const kind = isEventKind(r.kind) ? r.kind : 'self_author';
  const summary = typeof r.summary === 'string' ? r.summary.slice(0, 280) : '';
  const sigma_chain_anchor =
    typeof r.sigma_chain_anchor === 'string' ? r.sigma_chain_anchor.slice(0, 16) : undefined;
  return sigma_chain_anchor !== undefined
    ? { ts, kind, summary, sigma_chain_anchor }
    : { ts, kind, summary };
}

function isEventKind(k: unknown): k is EventKind {
  return (
    k === 'self_author' ||
    k === 'playtest' ||
    k === 'kan_rollup' ||
    k === 'mycelium_sync' ||
    k === 'sigma_anchor' ||
    k === 'idle_enter' ||
    k === 'idle_exit' ||
    k === 'sovereign_pause' ||
    k === 'sovereign_resume'
  );
}

// I> privacy : pubkey shown 8-hex-prefix max
export function sanitizePubkey(s: string): string {
  return s.replace(/[^a-fA-F0-9]/g, '').slice(0, 8);
}

// ─────────────────────────────────────────────
// § Auto-refresh hooks · 5s poll · backoff-on-fail
// ─────────────────────────────────────────────

export interface UseEngineState<T> {
  data: T | null;
  loading: boolean;
  error: string | null;
  paused: boolean;
}

function useAutoFetch<T>(
  fetcher: (signal: AbortSignal) => Promise<T>,
  deps: ReadonlyArray<unknown>,
  intervalMs = 5000,
): UseEngineState<T> {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [paused, setPaused] = useState(false);
  const failsRef = useRef(0);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const tick = useCallback(
    async (controller: AbortController) => {
      try {
        const result = await fetcher(controller.signal);
        if (controller.signal.aborted) return;
        setData(result);
        setLoading(false);
        // ⊑ stub-aware reset
        const stub = (result as { stub?: boolean })?.stub === true;
        if (stub) {
          setError((result as { reason?: string })?.reason ?? null);
          failsRef.current = Math.min(failsRef.current + 1, 5);
        } else {
          setError(null);
          failsRef.current = 0;
        }
      } catch (err) {
        if (controller.signal.aborted) return;
        setError(err instanceof Error ? err.message : 'unknown error');
        setLoading(false);
        failsRef.current = Math.min(failsRef.current + 1, 5);
        if (failsRef.current >= 3) setPaused(true);
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    deps,
  );

  useEffect(() => {
    const controller = new AbortController();
    let cancelled = false;
    const loop = async () => {
      while (!cancelled && !controller.signal.aborted) {
        if (!paused) await tick(controller);
        const delay = Math.min(intervalMs * Math.pow(2, failsRef.current), 60_000);
        await new Promise<void>((resolve) => {
          timerRef.current = setTimeout(resolve, delay);
        });
      }
    };
    void loop();
    return () => {
      cancelled = true;
      controller.abort();
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [tick, intervalMs, paused]);

  return { data, loading, error, paused };
}

export function useHeartbeat(intervalMs = 5000): UseEngineState<HeartbeatResponse> {
  return useAutoFetch<HeartbeatResponse>((s) => fetchHeartbeat(s), [], intervalMs);
}

export function useCycles(sinceMs?: number, intervalMs = 5000): UseEngineState<CycleCounts> {
  return useAutoFetch<CycleCounts>((s) => fetchCycles(sinceMs, s), [sinceMs ?? 0], intervalMs);
}

export function useRecentEvents(
  limit = 50,
  intervalMs = 5000,
): UseEngineState<EventsResponse> {
  return useAutoFetch<EventsResponse>((s) => fetchRecentEvents(limit, s), [limit], intervalMs);
}

export function usePauseState(intervalMs = 10000): UseEngineState<SovereignPauseState> {
  return useAutoFetch<SovereignPauseState>((s) => fetchPauseState(s), [], intervalMs);
}

// ─────────────────────────────────────────────
// § Inline tests · framework-agnostic · matches health-w9 pattern
// ─────────────────────────────────────────────

function assert(cond: unknown, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}
function eq<T>(actual: T, expected: T, msg: string): void {
  if (actual !== expected) {
    throw new Error(`expected ${JSON.stringify(expected)} · got ${JSON.stringify(actual)} :: ${msg}`);
  }
}

export function testEmptyHeartbeat(): void {
  const z = emptyHeartbeat();
  eq(z.local.online, false, 'local offline');
  eq(z.cloud.online, false, 'cloud offline');
  eq(z.local.last_seen, 0, 'last_seen zero');
  eq(z.stub, true, 'stub true');
  assert(typeof z.reason === 'string' && z.reason.length > 0, 'reason set');
}

export function testEmptyCycles(): void {
  const z = emptyCycles();
  eq(z.self_author, 0, 'self_author zero');
  eq(z.playtest, 0, 'playtest zero');
  eq(z.kan_rollup, 0, 'kan_rollup zero');
  eq(z.mycelium_sync, 0, 'mycelium_sync zero');
  eq(z.sigma_chain_anchors, 0, 'sigma anchors zero');
  eq(z.bytes_processed, 0, 'bytes zero');
  assert(z.until > z.since, 'window valid');
  eq(z.stub, true, 'stub true');
}

export function testEmptyEvents(): void {
  const z = emptyEvents();
  eq(z.events.length, 0, 'no events');
  eq(z.stub, true, 'stub true');
}

export function testEmptyPauseState(): void {
  const z = emptyPauseState();
  eq(z.paused, false, 'not paused');
  eq(z.stub, true, 'stub true');
}

export function testFmtCompact(): void {
  eq(fmtCompact(null), '—', 'null');
  eq(fmtCompact(undefined), '—', 'undef');
  eq(fmtCompact(NaN), '—', 'NaN');
  eq(fmtCompact(0), '0', 'zero');
  eq(fmtCompact(42), '42', '42');
  eq(fmtCompact(1234), '1.2k', '1.2k');
  eq(fmtCompact(2_500_000), '2.5M', '2.5M');
  eq(fmtCompact(1_500_000_000), '1.5B', '1.5B');
  eq(fmtCompact(3.14), '3.1', 'frac');
}

export function testFmtBytes(): void {
  eq(fmtBytes(null), '—', 'null');
  eq(fmtBytes(0), '0 B', 'zero');
  eq(fmtBytes(512), '512 B', '512B');
  eq(fmtBytes(2048), '2.0 KiB', 'kib');
  eq(fmtBytes(1_500_000), '1.4 MiB', 'mib');
  eq(fmtBytes(2 * 1_073_741_824), '2.0 GiB', 'gib');
}

export function testFmtUptime(): void {
  eq(fmtUptime(null), '—', 'null');
  eq(fmtUptime(45), '45s', '45s');
  eq(fmtUptime(120), '2m', '2m');
  eq(fmtUptime(3661), '1h 1m', '1h1m');
  eq(fmtUptime(90_000), '1d 1h', '1d1h');
}

export function testFmtRelTime(): void {
  const now = 1_000_000_000_000;
  eq(fmtRelTime(null, now), 'never', 'null');
  eq(fmtRelTime(0, now), 'never', 'zero');
  eq(fmtRelTime(now - 2000, now), 'just-now', 'just-now');
  eq(fmtRelTime(now - 30_000, now), '30s ago', '30s');
  eq(fmtRelTime(now - 600_000, now), '10m ago', '10m');
  eq(fmtRelTime(now - 7_200_000, now), '2h ago', '2h');
  eq(fmtRelTime(now - 2 * 86_400_000, now), '2d ago', '2d');
}

export function testFmtRelTimeCoarse(): void {
  const now = 1_000_000_000_000;
  eq(fmtRelTimeCoarse(null, now), 'unknown', 'null');
  eq(fmtRelTimeCoarse(now - 30_000, now), 'within the last minute', '<1m');
  eq(fmtRelTimeCoarse(now - 600_000, now), 'within the last hour', '<1h');
  eq(fmtRelTimeCoarse(now - 7_200_000, now), 'within the last day', '<1d');
  eq(fmtRelTimeCoarse(now - 2 * 86_400_000, now), 'within the last week', '<1w');
  eq(fmtRelTimeCoarse(now - 30 * 86_400_000, now), 'over a week ago', '>1w');
}

export function testIsOnline(): void {
  const now = 1_000_000_000_000;
  eq(isOnline({ last_seen: 0, uptime_secs: 0, online: false }, now), false, 'never');
  eq(isOnline({ last_seen: now - 5_000, uptime_secs: 100, online: true }, now), true, 'fresh');
  eq(isOnline({ last_seen: now - 60_000, uptime_secs: 100, online: true }, now), false, 'stale');
}

export function testSanitizePubkey(): void {
  eq(sanitizePubkey('abc123def456789'), 'abc123de', 'truncate');
  eq(sanitizePubkey('!@#$%^&*()'), '', 'strip all non-hex');
  eq(sanitizePubkey('xyz!ghij'), '', 'no hex chars');
  eq(sanitizePubkey('not-hex!@#'), 'e', 'pulls e (valid hex) from "hex"');
  eq(sanitizePubkey('AABBCCDDEEFF1122'), 'AABBCCDD', 'mixed case');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

if (isMain) {
  testEmptyHeartbeat();
  testEmptyCycles();
  testEmptyEvents();
  testEmptyPauseState();
  testFmtCompact();
  testFmtBytes();
  testFmtUptime();
  testFmtRelTime();
  testFmtRelTimeCoarse();
  testIsOnline();
  testSanitizePubkey();
  // eslint-disable-next-line no-console
  console.log('engine-status.ts : OK · 11 inline tests passed');
}
