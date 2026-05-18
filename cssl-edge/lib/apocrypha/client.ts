// Typed client helpers for the Apocrypha cockpit pages.
// All paths go through cssl-edge's /api/admin/apocrypha/* proxies which forward
// to the Apocrypha tunnel with CF Access service-token auth.
//
// Per HANDOFF_v10 § TRACK-A A4.

import { authFetch } from '../browser-auth';

export interface ApocryphaEnvelopeError {
  error: string;
  detail?: string;
}

export interface ApocryphaEnvelope<T> {
  upstream_status: number;
  data: T;
  tunnel_host: string;
}

async function callJson<T>(
  path: string,
  init?: RequestInit,
): Promise<ApocryphaEnvelope<T>> {
  const r = await authFetch(path, {
    ...init,
    headers: {
      Accept: 'application/json',
      'Content-Type': 'application/json',
      ...(init?.headers ?? {}),
    },
  });
  const json = (await r.json()) as ApocryphaEnvelope<T> | ApocryphaEnvelopeError;
  if (!r.ok || 'error' in json) {
    const e = json as ApocryphaEnvelopeError;
    throw new Error(e.detail || e.error || `apocrypha request failed (${r.status})`);
  }
  return json as ApocryphaEnvelope<T>;
}

// ── /api/v1/chat ────────────────────────────────────────────────────

export interface ChatRequest {
  text: string;
  conversation_id?: number | null;
  principal?: string | null;
  max_iters?: number;
  timeout_s?: number;
}

export interface ChatToolCall {
  name: string;
  ok: boolean;
  elapsed_ms: number;
  cost_usd: number;
  error: string | null;
}

export interface ChatResponse {
  conversation_id: number;
  final_response: string;
  halted_reason: string;
  iters_done: number;
  elapsed_s: number;
  total_cost_usd: number;
  tool_calls: ChatToolCall[];
}

export async function sendChat(req: ChatRequest): Promise<ChatResponse> {
  const env = await callJson<ChatResponse>('/api/admin/apocrypha/chat', {
    method: 'POST',
    body: JSON.stringify(req),
  });
  return env.data;
}

// ── /api/v1/tools ───────────────────────────────────────────────────

export interface ToolInfo {
  name: string;
  description: string;
  category: string;
  permission_tier: string;
  independent: boolean;
  timeout_s: number;
  accepts_hv: boolean;
}

export interface ToolsList {
  tools: ToolInfo[];
  count: number;
}

export async function listTools(): Promise<ToolsList> {
  const env = await callJson<ToolsList>('/api/admin/apocrypha/tools');
  return env.data;
}

// ── /api/v1/tool_calls/recent ───────────────────────────────────────

export interface ToolCallRecord {
  id: number;
  message_id: number;
  tool_name: string;
  ok: boolean;
  cost_usd: number;
  elapsed_ms: number;
  error: string | null;
}

export interface ToolCallsRecent {
  tool_calls: ToolCallRecord[];
  count: number;
  limit: number;
}

export async function recentToolCalls(limit = 50): Promise<ToolCallsRecent> {
  const env = await callJson<ToolCallsRecent>(
    `/api/admin/apocrypha/tool_calls?limit=${limit}`,
  );
  return env.data;
}

// ── /api/v1/keys ────────────────────────────────────────────────────

export interface ApiKeyInfo {
  key_id: string;
  label: string;
  principal: string;
  created_at_iso: string;
  last_used_at_iso: string | null;
  expires_at_iso: string | null;
  revoked: boolean;
}

export interface CreateKeyResponse {
  key_id: string;
  label: string;
  principal: string;
  plaintext: string;
  expires_at_iso: string | null;
}

export async function listKeys(): Promise<ApiKeyInfo[]> {
  const env = await callJson<ApiKeyInfo[]>('/api/admin/apocrypha/keys');
  return env.data;
}

export async function createKey(
  label: string,
  principal: string,
  expires_at_iso?: string,
): Promise<CreateKeyResponse> {
  const env = await callJson<CreateKeyResponse>('/api/admin/apocrypha/keys', {
    method: 'POST',
    body: JSON.stringify({ label, principal, expires_at_iso }),
  });
  return env.data;
}

export async function revokeKey(keyId: string): Promise<void> {
  await callJson<unknown>('/api/admin/apocrypha/keys', {
    method: 'DELETE',
    body: JSON.stringify({ key_id: keyId }),
  });
}

// ── /api/v1/sub_minds/health ────────────────────────────────────────

export interface LazarusHealth {
  omega_id: number;
  label: string;
  tier: string;
  runner_loop_running: boolean;
  runner_id: string | null;
  in_flight_dispatches: number;
  task_count: number;
  queued_count: number;
  active_run_count: number;
  pending_approval_count: number;
  online_runner_count: number;
  tool_count: number;
}

export interface TesseraHealth {
  omega_id: number;
  label: string;
  tier: string;
  started: boolean;
  codebook_size: number;
  episode_count: number;
  escape_configured: boolean;
}

export interface SubMindsHealth {
  lazarus: LazarusHealth;
  tessera: TesseraHealth;
}

export async function subMindsHealth(): Promise<SubMindsHealth> {
  const env = await callJson<SubMindsHealth>('/api/admin/apocrypha/sub_minds');
  return env.data;
}

// ── /api/v1/cost ────────────────────────────────────────────────────

export interface CostCallRecord {
  ts_utc: string;
  model: string;
  input_tokens: number;
  output_tokens: number;
  cost_usd: number;
}

export interface CostSnapshot {
  spent_today_usd: number;
  daily_cap_usd: number;
  remaining_today_usd: number;
  total_calls_last_100: number;
  spent_by_model_last_100: Record<string, number>;
  recent_calls: CostCallRecord[];
}

export async function costSnapshot(): Promise<CostSnapshot> {
  const env = await callJson<CostSnapshot>('/api/admin/apocrypha/cost');
  return env.data;
}

// ── /api/v1/mcp/info ────────────────────────────────────────────────

export interface McpToolEntry {
  name: string;
  description: string;
  category: string;
  permission_tier: string;
  accepts_hv: boolean;
}

export interface McpInfo {
  mcp_endpoint: string;
  transport: string;
  exposed_count: number;
  blocked_count: number;
  exposed_tools: McpToolEntry[];
  blocked_tools: McpToolEntry[];
}

export async function mcpInfo(): Promise<McpInfo> {
  const env = await callJson<McpInfo>('/api/admin/apocrypha/mcp_info');
  return env.data;
}

// ── /api/status (basic health ; no auth) ────────────────────────────

export interface ApocryphaStatus {
  version: string;
  tiers_available: { tier0: boolean; tier_a: boolean; tier_b: boolean };
  spent_today_usd: number;
  daily_cap_usd: number;
  tools_registered: number;
}

export async function apocryphaStatus(): Promise<ApocryphaStatus | null> {
  try {
    const r = await authFetch('/api/admin/apocrypha/status', { cache: 'no-store' });
    const json = (await r.json()) as { upstream_payload?: ApocryphaStatus };
    return json.upstream_payload ?? null;
  } catch {
    return null;
  }
}
