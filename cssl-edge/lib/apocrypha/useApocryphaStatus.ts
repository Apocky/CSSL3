// Polling hook for /api/admin/apocrypha/status (proxied via Vercel → CF Tunnel → Apocrypha)
// Phase-2 of Apocrypha/specs/12_APOCKY_COM_INTEGRATION.csl. Real-time WS upgrade is Phase-3.

import { useEffect, useRef, useState } from 'react';
import { authFetch } from '../browser-auth';

export interface ApocryphaUpstream {
  version?: string;
  tiers_available?: { tier0: boolean; tier_a: boolean; tier_b: boolean };
  spent_today_usd?: number;
  daily_cap_usd?: number;
}

export interface ApocryphaStatus {
  phase: 'stub' | 'tunnel';
  reachable: boolean;
  tunnel_host: string | null;
  note: string;
  upstream_status?: number;
  upstream_payload?: ApocryphaUpstream;
  upstream_error?: string;
  served_by?: string;
  ts?: string;
}

export interface ApocryphaStatusState {
  data: ApocryphaStatus | null;
  loading: boolean;
  error: string | null;
  lastFetch: Date | null;
}

const DEFAULT_INTERVAL_MS = 10_000;

export function useApocryphaStatus(intervalMs: number = DEFAULT_INTERVAL_MS): ApocryphaStatusState {
  const [state, setState] = useState<ApocryphaStatusState>({
    data: null,
    loading: true,
    error: null,
    lastFetch: null,
  });
  const aliveRef = useRef(true);

  useEffect(() => {
    aliveRef.current = true;

    async function fetchOnce(): Promise<void> {
      try {
        const r = await authFetch('/api/admin/apocrypha/status', { cache: 'no-store' });
        const json = (await r.json()) as ApocryphaStatus;
        if (!aliveRef.current) return;
        setState({ data: json, loading: false, error: null, lastFetch: new Date() });
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
  }, [intervalMs]);

  return state;
}
