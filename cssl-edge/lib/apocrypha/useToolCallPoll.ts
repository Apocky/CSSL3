// Polling hook for the diag-face ToolCallTimeline.
// Polls /api/admin/apocrypha/tool_calls?limit=N every intervalMs ; new rows surface
// at the top. v0 uses polling (not WS) to avoid Vercel WebSocket proxy complexity.

import { useEffect, useRef, useState } from 'react';

import { recentToolCalls, type ToolCallRecord } from './client';

export interface ToolCallPollState {
  records: ToolCallRecord[];
  loading: boolean;
  error: string | null;
  lastFetch: Date | null;
}

const DEFAULT_INTERVAL_MS = 2_000;
const DEFAULT_LIMIT = 100;

export function useToolCallPoll(
  intervalMs: number = DEFAULT_INTERVAL_MS,
  limit: number = DEFAULT_LIMIT,
): ToolCallPollState {
  const [state, setState] = useState<ToolCallPollState>({
    records: [],
    loading: true,
    error: null,
    lastFetch: null,
  });
  const aliveRef = useRef(true);

  useEffect(() => {
    aliveRef.current = true;

    async function fetchOnce(): Promise<void> {
      try {
        const r = await recentToolCalls(limit);
        if (!aliveRef.current) return;
        setState({
          records: r.tool_calls,
          loading: false,
          error: null,
          lastFetch: new Date(),
        });
      } catch (err) {
        if (!aliveRef.current) return;
        setState((prev) => ({
          ...prev,
          loading: false,
          error: err instanceof Error ? err.message : String(err),
          lastFetch: new Date(),
        }));
      }
    }

    void fetchOnce();
    const timer = setInterval(() => void fetchOnce(), intervalMs);
    return () => {
      aliveRef.current = false;
      clearInterval(timer);
    };
  }, [intervalMs, limit]);

  return state;
}
