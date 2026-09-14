import { authFetch } from '../browser-auth';

export type RiskTier = 'read' | 'write' | 'execute';
export type ConsentDecision = 'allow' | 'allow_session' | 'deny';

export interface WorkRoot { label: string; path: string; writable: boolean }

export interface ArbiterStatus {
  mode: 'off' | 'manual' | 'auto';
  resident: 'chat' | 'work' | 'none';
  chatUp: boolean;
  workUp: boolean;
  chatBusy: boolean | null;
  handoverInFlight: boolean;
  lastHandoverAt: string | null;
  lastError: string | null;
}

export interface WorkHealth {
  status: 'ok' | 'degraded';
  engine: { healthy: boolean; model: string; contextWindow?: number; detail?: string; base_url: string; alias: string };
  workspace: { label: string; writable: boolean }[];
  policy: { shell: boolean; auto_approve: RiskTier[]; max_iterations: number };
  active_turns: number;
  arbiter: ArbiterStatus;
}

export interface WorkSessionSummary {
  id: string;
  title: string;
  createdAt: string;
  lastActiveAt: string;
  standingGrants: string[];
}

export interface ToolCallOutcome {
  id: string;
  name: string;
  ok: boolean;
  elapsedMs: number;
  summary: string;
  content: string;
  error?: string;
  denied?: boolean;
  diff?: { path: string; added: number; removed: number; patch: string };
}

export interface WorkTurnRecord {
  id: string;
  prompt: string;
  phase: string;
  startedAt: string;
  endedAt?: string;
  error?: string;
  toolCalls: ToolCallOutcome[];
  output: string;
  usage?: { promptTokens?: number; completionTokens?: number; totalTokens?: number; elapsedS?: number };
}

export interface WorkEvent {
  seq: number;
  at: string;
  kind: 'phase' | 'token' | 'tool_request' | 'tool_result' | 'consent_request' | 'consent_resolved' | 'usage' | 'error' | 'session';
  data: Record<string, unknown>;
}

export class WorkLaneOffline extends Error {
  readonly hint: string;

  constructor(detail: string, hint: string) {
    super(detail);
    this.name = 'WorkLaneOffline';
    this.hint = hint;
  }
}

async function call<T>(path: string, init: RequestInit = {}): Promise<T> {
  const response = await authFetch(`/api/work${path}`, {
    ...init,
    headers: { ...(init.body ? { 'content-type': 'application/json' } : {}), ...(init.headers ?? {}) },
  });
  const text = await response.text();
  let payload: Record<string, unknown> = {};
  try {
    payload = text ? JSON.parse(text) as Record<string, unknown> : {};
  } catch {
    throw new Error(`The Work lane returned an unreadable response (${response.status}).`);
  }
  if (response.status === 503 && payload.error === 'work_lane_unavailable') {
    throw new WorkLaneOffline(String(payload.detail ?? 'The Work lane is not running.'), String(payload.hint ?? ''));
  }
  if (!response.ok) {
    throw new Error(String(payload.detail ?? payload.error ?? `Request failed (${response.status}).`));
  }
  return payload as T;
}

export const workApi = {
  health: () => call<WorkHealth>('/health'),
  listSessions: () => call<{ sessions: WorkSessionSummary[] }>('/sessions').then((body) => body.sessions),
  createSession: (title: string) =>
    call<{ session: WorkSessionSummary }>('/sessions', { method: 'POST', body: JSON.stringify({ title }) }).then((body) => body.session),
  loadSession: (id: string) => call<{ session: WorkSessionSummary; turns: WorkTurnRecord[] }>(`/sessions/${id}`),
  submit: (id: string, prompt: string) =>
    call<{ turn_id: string }>(`/sessions/${id}/turns`, { method: 'POST', body: JSON.stringify({ prompt }) }),
  cancel: (id: string) => call<{ cancelled: boolean }>(`/sessions/${id}/cancel`, { method: 'POST', body: '{}' }),
  consent: (requestId: string, decision: ConsentDecision) =>
    call<{ resolved: boolean }>(`/consent/${requestId}`, { method: 'POST', body: JSON.stringify({ decision }) }),
  workspace: () => call<{ roots: WorkRoot[]; tools: { name: string; risk: RiskTier; description: string }[] }>('/workspace'),
  engine: () => call<ArbiterStatus>('/engine'),
  acquire: (force = false) => call<ArbiterStatus>('/engine/acquire', { method: 'POST', body: JSON.stringify({ force }) }),
  release: () => call<ArbiterStatus>('/engine/release', { method: 'POST', body: '{}' }),
};

/**
 * Subscribe to a session's event stream.
 *
 * EventSource cannot carry an Authorization header and the proxy is owner-gated by cookie, so the
 * stream is read with fetch and parsed by hand. That also lets a dropped connection be retried
 * without the browser's own opaque backoff.
 */
export function streamSession(
  sessionId: string,
  onEvent: (event: WorkEvent) => void,
  onError: (error: Error) => void,
  signal: AbortSignal,
): void {
  void (async () => {
    let backoffMs = 1_000;
    while (!signal.aborted) {
      try {
        const response = await authFetch(`/api/work/sessions/${sessionId}/stream`, { signal });
        if (!response.ok || !response.body) throw new Error(`stream refused (${response.status})`);
        backoffMs = 1_000;
        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        let buffer = '';
        while (!signal.aborted) {
          const { done, value } = await reader.read();
          if (done) break;
          buffer += decoder.decode(value, { stream: true });
          const frames = buffer.split('\n\n');
          buffer = frames.pop() ?? '';
          for (const frame of frames) {
            const line = frame.split('\n').find((entry) => entry.startsWith('data:'));
            if (!line) continue;
            try {
              onEvent(JSON.parse(line.slice(5).trim()) as WorkEvent);
            } catch {
              // A partial frame at the edge of a chunk is normal; the next read completes it.
            }
          }
        }
      } catch (error) {
        if (signal.aborted) return;
        onError(error instanceof Error ? error : new Error('stream failed'));
      }
      if (signal.aborted) return;
      await new Promise((resolve) => setTimeout(resolve, backoffMs));
      backoffMs = Math.min(backoffMs * 2, 15_000);
    }
  })();
}
