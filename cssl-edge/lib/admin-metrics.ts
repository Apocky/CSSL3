// § lib/admin-metrics.ts · typed-fetch helpers + auto-refresh hook for /admin/analytics
// Sibling-W11-4 owns /api/analytics/* · this file is the CLIENT-SIDE surface
// I> stub-mode-aware : 404 → graceful zero-state · backoff-on-rate-limit · pause-on-error

import { useEffect, useRef, useState, useCallback } from 'react';

// ─────────────────────────────────────────────
// § Event-kind taxonomy (matches sibling-W11-4 contract)
// ─────────────────────────────────────────────
export type MetricKind =
  | 'engine.frame_tick'
  | 'engine.render_mode_changed'
  | 'input.text_typed'
  | 'input.text_submitted'
  | 'intent.classified'
  | 'intent.routed'
  | 'gm.response_emitted'
  | 'dm.phase_transition'
  | 'procgen.scene_built'
  | 'mcp.tool_called'
  | 'kan.classified'
  | 'mycelium.sync_event'
  | 'consent.cap_granted'
  | 'consent.cap_revoked';

export type MetricBucket = '1min' | '5min' | '1hr' | '1day';

// ─────────────────────────────────────────────
// § Response-shape (matches sibling-W11-4 contract)
// ─────────────────────────────────────────────
export interface MetricEvent {
  ts: number; // ⊑ epoch-ms
  v?: number; // ⊑ optional numeric value (e.g. fps)
  tag?: string; // ⊑ optional categorical tag (e.g. tool-name · render-mode)
  meta?: Record<string, unknown>;
}

export interface MetricRollup {
  count: number;
  avg?: number;
  p50?: number;
  p95?: number;
  p99?: number;
  rate_per_sec?: number;
  by_tag?: Record<string, number>; // ⊑ histogram by-tag
}

export interface MetricsResponse {
  kind: MetricKind;
  bucket: MetricBucket;
  events: MetricEvent[];
  rollup: MetricRollup;
  stub?: boolean;
  reason?: string;
}

// ─────────────────────────────────────────────
// § Defensive zero-state factory
// ─────────────────────────────────────────────
export function emptyMetrics(kind: MetricKind, bucket: MetricBucket, reason?: string): MetricsResponse {
  return {
    kind,
    bucket,
    events: [],
    rollup: { count: 0 },
    stub: true,
    reason: reason ?? 'telemetry endpoint not yet wired · sibling-agent will land it',
  };
}

// ─────────────────────────────────────────────
// § Single-fetch · returns zero-state on any failure
// ─────────────────────────────────────────────
export async function fetchMetrics(
  kind: MetricKind,
  bucket: MetricBucket = '1min',
  signal?: AbortSignal,
): Promise<MetricsResponse> {
  try {
    const url = `/api/analytics/metrics?kind=${encodeURIComponent(kind)}&bucket=${encodeURIComponent(bucket)}`;
    const res = await fetch(url, { signal });
    if (res.status === 404) {
      return emptyMetrics(kind, bucket, 'endpoint 404 · sibling-W11-4 not yet landed');
    }
    if (res.status === 429) {
      return emptyMetrics(kind, bucket, 'rate-limited · backoff-active');
    }
    if (!res.ok) {
      return emptyMetrics(kind, bucket, `${res.status} ${res.statusText}`);
    }
    const json = (await res.json()) as Partial<MetricsResponse>;
    // ⊑ defensive : ensure shape
    return {
      kind,
      bucket,
      events: Array.isArray(json.events) ? json.events : [],
      rollup: json.rollup ?? { count: 0 },
      stub: json.stub ?? false,
      reason: json.reason,
    };
  } catch (err) {
    if (err instanceof DOMException && err.name === 'AbortError') {
      throw err;
    }
    return emptyMetrics(kind, bucket, err instanceof Error ? err.message : 'network error');
  }
}

// ─────────────────────────────────────────────
// § Auto-refresh hook · poll every 5s · pause-on-error · backoff-on-rate-limit
// ─────────────────────────────────────────────
export interface UseMetricsState {
  data: MetricsResponse | null;
  loading: boolean;
  error: string | null;
  paused: boolean;
}

export function useMetrics(
  kind: MetricKind,
  bucket: MetricBucket = '1min',
  intervalMs = 5000,
): UseMetricsState {
  const [data, setData] = useState<MetricsResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [paused, setPaused] = useState(false);
  const failsRef = useRef(0);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const tick = useCallback(
    async (controller: AbortController) => {
      try {
        const result = await fetchMetrics(kind, bucket, controller.signal);
        if (controller.signal.aborted) return;
        setData(result);
        setLoading(false);
        if (result.stub) {
          // ⊑ stub still counts as "received" but flag for UI
          setError(result.reason ?? null);
          // I> backoff-doubler on consecutive failures · max 60s
          failsRef.current = Math.min(failsRef.current + 1, 5);
        } else {
          setError(null);
          failsRef.current = 0;
        }
      } catch (err) {
        if (controller.signal.aborted) return;
        const msg = err instanceof Error ? err.message : 'unknown error';
        setError(msg);
        setLoading(false);
        failsRef.current = Math.min(failsRef.current + 1, 5);
        if (failsRef.current >= 3) setPaused(true);
      }
    },
    [kind, bucket],
  );

  useEffect(() => {
    const controller = new AbortController();
    let cancelled = false;
    const loop = async () => {
      while (!cancelled && !controller.signal.aborted) {
        if (!paused) {
          await tick(controller);
        }
        // ⊑ exponential-ish backoff · 5s · 10s · 20s · 40s · max 60s
        const backoffMul = Math.pow(2, failsRef.current);
        const delay = Math.min(intervalMs * backoffMul, 60_000);
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

// ─────────────────────────────────────────────
// § Format-helpers · phone-friendly compact-numbers
// ─────────────────────────────────────────────
export function fmtCompact(v: number | undefined | null): string {
  if (v === undefined || v === null || Number.isNaN(v)) return '—';
  if (v >= 1_000_000) return `${(v / 1_000_000).toFixed(1)}M`;
  if (v >= 1_000) return `${(v / 1_000).toFixed(1)}k`;
  if (Number.isInteger(v)) return String(v);
  return v.toFixed(1);
}

export function fmtMs(v: number | undefined | null): string {
  if (v === undefined || v === null || Number.isNaN(v)) return '—';
  if (v >= 1000) return `${(v / 1000).toFixed(1)}s`;
  if (v >= 1) return `${v.toFixed(1)}ms`;
  return `${(v * 1000).toFixed(0)}µs`;
}

export function fmtPercent(v: number | undefined | null): string {
  if (v === undefined || v === null || Number.isNaN(v)) return '—';
  return `${(v * 100).toFixed(1)}%`;
}

// ─────────────────────────────────────────────
// § histogram-helper : extract values-array from events for a given numeric-field
// ─────────────────────────────────────────────
export function extractSeries(events: MetricEvent[], field: 'v' = 'v'): number[] {
  const out = new Array<number>(events.length);
  for (let i = 0; i < events.length; i++) {
    const e = events[i];
    const v = e ? e[field] : undefined;
    out[i] = typeof v === 'number' && !Number.isNaN(v) ? v : 0;
  }
  return out;
}

// ⊑ tag-distribution · for donut/bar primitives
export function tagHistogram(events: MetricEvent[]): Array<{ label: string; value: number }> {
  const map = new Map<string, number>();
  for (const e of events) {
    const tag = e.tag ?? 'unknown';
    map.set(tag, (map.get(tag) ?? 0) + 1);
  }
  return Array.from(map.entries()).map(([label, value]) => ({ label, value }));
}
