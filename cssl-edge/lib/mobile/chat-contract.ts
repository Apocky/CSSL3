export interface AccountMessage { role: 'user' | 'assistant'; content: string; request_id: string; recorded_at: string }
export interface AccountSessionSummary { session_id: string; title: string; message_count: number }
export interface AccountSession { session_id: string; title: string; messages: AccountMessage[]; events_truncated: boolean }
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
export const isAccountSessionId = (value: unknown): value is string => typeof value === 'string' && UUID.test(value);
const row = (value: unknown): Record<string, unknown> | null => value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : null;
const text = (value: unknown, limit: number): value is string => typeof value === 'string' && value.length > 0 && value.length <= limit;
export function parseAccountSessions(value: unknown): { sessions: AccountSessionSummary[]; discovery_scope: string } | null {
  const body = row(value);
  if (!body || body.schema_version !== 'apocky.mobile.sessions.v1' || body.status !== 'live' || !Array.isArray(body.sessions)
    || body.sessions.length > 128 || body.count !== body.sessions.length || typeof body.discovery_scope !== 'string' || !['account_conversations', 'latest_conversation_only'].includes(body.discovery_scope)) return null;
  const sessions: AccountSessionSummary[] = [];
  for (const value of body.sessions) {
    const item = row(value);
    if (!item || !isAccountSessionId(item.session_id) || !text(item.title, 1024) || !Number.isSafeInteger(item.message_count) || Number(item.message_count) < 0) return null;
    sessions.push({ session_id: item.session_id, title: item.title, message_count: Number(item.message_count) });
  }
  return { sessions, discovery_scope: String(body.discovery_scope) };
}
export function parseAccountSession(value: unknown, expectedId: string): AccountSession | null {
  const body = row(value); const session = row(body?.session);
  if (!body || body.schema_version !== 'apocky.mobile.session.v1' || body.status !== 'live' || !session
    || session.schema_version !== 'apocky.mobile.history-session.v1' || session.session_id !== expectedId || !text(session.title, 1024)
    || !Array.isArray(session.messages) || session.messages.length > 128 || typeof session.events_truncated !== 'boolean') return null;
  const messages: AccountMessage[] = [];
  for (const value of session.messages) {
    const item = row(value);
    if (!item || typeof item.role !== 'string' || !['user', 'assistant'].includes(item.role) || !text(item.content, 128 * 1024) || !isAccountSessionId(item.request_id) || !text(item.recorded_at, 64)) return null;
    messages.push({ role: item.role as AccountMessage['role'], content: item.content, request_id: item.request_id, recorded_at: item.recorded_at });
  }
  return { session_id: expectedId, title: session.title, messages, events_truncated: session.events_truncated };
}
export function parseAccountTurn(value: unknown, sessionId: string, requestId: string): { text: string } | null {
  const body = row(value);
  if (!body || body.schema_version !== 'apocky.mobile.turn.v1' || body.status !== 'completed' || body.session_id !== sessionId || body.request_id !== requestId
    || !text(body.text, 256 * 1024) || !text(body.model_id, 512) || typeof body.response_digest !== 'string' || !/^[0-9a-f]{64}$/.test(body.response_digest)) return null;
  return { text: body.text };
}
