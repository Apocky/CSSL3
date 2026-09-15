// One chat, three ways in.
//
// There were three chat components — GuestChat, AccountChat, ChatThread — and they drifted, which
// is why signing in visibly downgraded the product: the signed-out room had been redesigned and
// the one an owner actually uses had not. Restyling all three would have rebuilt the same problem
// with fresher paint.
//
// What actually differs between them is TRANSPORT and CAPABILITY, not interface. All three do the
// same thing: submit a turn, poll a job, show an answer. So that shape lives here once, and the UI
// is written once against it.

import {
  fetchMemberChatHistoryPage,
  fetchMemberChatJob,
  isMemberChatUuid,
  projectMemberChatMessages,
  submitMemberChatJob,
} from '@/lib/apocrypha/member-chat-client';

export type LaneId = 'guest' | 'member' | 'owner';

export interface ChatToolCall {
  readonly name: string;
  readonly ok: boolean;
  readonly elapsed_ms?: number;
  readonly error?: string | null;
}

export interface LaneCapabilities {
  /** A conversation list, and history that survives the browser. */
  readonly conversations: boolean;
  /** A way to start a blank one. Separate from `conversations`: a lane can list without forking. */
  readonly newConversation: boolean;
  /** Tool and run trace, which only the owner has any authority to act on. */
  readonly trace: boolean;
  readonly cancel: boolean;
  /** False for guests: their thread lives in their own browser and nowhere else. */
  readonly durableHistory: boolean;
}

export interface LaneMessage {
  readonly id?: string;
  readonly role: 'user' | 'apocrypha';
  readonly text: string;
  readonly at: Date;
  readonly tools?: ChatToolCall[];
}

export interface ConversationSummary {
  readonly id: string;
  readonly title: string | null;
  readonly lastActiveIso: string;
  readonly messageCount?: number;
}

export interface SendInput {
  readonly text: string;
  readonly conversationId: string | null;
  /** Only the guest lane needs this: nothing of theirs is stored server-side to re-read. */
  readonly history: readonly LaneMessage[];
}

export interface SendResult {
  readonly jobId: string;
  readonly conversationId: string | null;
}

export interface PollResult {
  readonly done: boolean;
  readonly status: string;
  /** Partial while running, final when done. Empty is legitimate mid-flight. */
  readonly text: string;
  readonly tools?: ChatToolCall[];
  readonly conversationId?: string | null;
  readonly failure?: string | null;
}

export type LaneFetch = (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;

export interface ChatLane {
  readonly id: LaneId;
  readonly capabilities: LaneCapabilities;
  send(input: SendInput, signal?: AbortSignal): Promise<SendResult>;
  poll(jobId: string, signal?: AbortSignal): Promise<PollResult>;
  listConversations?(): Promise<ConversationSummary[]>;
  loadConversation?(id: string): Promise<LaneMessage[]>;
  cancel?(jobId: string): Promise<void>;
}

const CONVERSATION_ID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const TERMINAL = new Set(['succeeded', 'completed', 'failed', 'cancelled', 'dead']);

export function isConversationId(value: unknown): value is string {
  return typeof value === 'string' && CONVERSATION_ID.test(value);
}

function newId(): string {
  return typeof crypto !== 'undefined' && 'randomUUID' in crypto
    ? crypto.randomUUID()
    : `${Date.now()}-${Math.floor(Math.random() * 1e9)}`;
}

// ── guest ────────────────────────────────────────────────────────────────────────────────────
//
// Nothing of a guest's is retained beyond the job, so the thread is carried up with every turn
// rather than re-read from the server.

export function guestLane(fetchImpl: LaneFetch = fetch): ChatLane {
  return {
    id: 'guest',
    capabilities: { conversations: false, newConversation: false, trace: false, cancel: false, durableHistory: false },

    async send(input, signal) {
      const response = await fetchImpl('/api/apocrypha/guest/chat', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        signal,
        body: JSON.stringify({
          message: input.text,
          request_id: newId(),
          history: input.history.slice(-12).map((turn) => ({
            role: turn.role === 'user' ? 'user' : 'assistant',
            content: turn.text,
          })),
        }),
      });
      const payload = await response.json().catch(() => null) as { job_id?: string; error?: string } | null;
      if (!response.ok || !payload?.job_id) {
        throw new Error(payload?.error ?? 'Apocrypha could not take that message right now.');
      }
      return { jobId: payload.job_id, conversationId: null };
    },

    async poll(jobId, signal) {
      const response = await fetchImpl(`/api/apocrypha/guest/jobs/${encodeURIComponent(jobId)}`, {
        headers: { accept: 'application/json' },
        signal,
      });
      const payload = await response.json().catch(() => null) as
        { status?: string; terminal?: boolean; answer?: string | null; error_code?: string | null } | null;
      if (!response.ok || !payload) return { done: false, status: 'unknown', text: '' };
      return {
        done: payload.terminal === true,
        status: payload.status ?? 'queued',
        text: typeof payload.answer === 'string' ? payload.answer : '',
        failure: payload.error_code ?? null,
      };
    },
  };
}

// ── member ────────────────────────────────────────────────────────────────────
//
// The durable queue keyed to a verified account, so history follows the person across devices.
//
// This delegates to lib/apocrypha/member-chat-client rather than re-issuing the requests itself.
// That client validates every response field — receipt matches the request id, history belongs to
// the conversation asked for, byte ceilings hold — and re-deriving that here would have produced a
// second, weaker parser for the same wire format.

export function memberLane(authFetch: LaneFetch): ChatLane {
  return {
    id: 'member',
    capabilities: { conversations: true, newConversation: true, trace: false, cancel: false, durableHistory: true },

    async send(input, signal) {
      const conversationId = input.conversationId ?? newId();
      const receipt = await submitMemberChatJob(
        { conversationId, requestId: newId(), message: input.text },
        authFetch,
        signal,
      );
      return { jobId: receipt.job_id, conversationId: receipt.conversation_id };
    },

    async poll(jobId, signal) {
      const job = await fetchMemberChatJob(jobId, authFetch, signal);
      return {
        done: TERMINAL.has(job.status),
        status: job.status,
        text: job.assistant_message ?? '',
        conversationId: job.conversation_id,
        failure: job.error_code,
      };
    },

    async listConversations() {
      const response = await authFetch('/api/apocrypha/member/conversations', { method: 'GET' });
      if (!response.ok) throw new Error('Conversation history is unavailable.');
      const body = await response.json().catch(() => null) as { conversations?: unknown } | null;
      const rows = Array.isArray(body?.conversations) ? body!.conversations : [];
      const summaries: ConversationSummary[] = [];
      for (const entry of rows) {
        const row = entry as Record<string, unknown>;
        const id = typeof row.conversation_id === 'string' ? row.conversation_id.toLowerCase() : '';
        if (!isMemberChatUuid(id)) continue;
        summaries.push({
          id,
          title: typeof row.title === 'string' && row.title.trim() ? row.title.trim() : null,
          lastActiveIso: typeof row.last_active_at === 'string' ? row.last_active_at : '',
          messageCount: Number.isFinite(Number(row.turn_count)) ? Number(row.turn_count) : undefined,
        });
      }
      return summaries;
    },

    async loadConversation(id) {
      // One page. Earlier turns are reachable through the same client, but the first page is what
      // opening a conversation owes you — anything more is a scroll away, not a load away.
      const page = await fetchMemberChatHistoryPage(id, authFetch);
      return projectMemberChatMessages(page.history, null).map((message) => ({
        id: message.key,
        role: message.role === 'user' ? 'user' as const : 'apocrypha' as const,
        text: message.content,
        at: new Date(message.recorded_at),
      }));
    },
  };
}

// ── owner ────────────────────────────────────────────────────────────────────────────────────
//
// The admin job path: the same submit-and-poll, plus the tool trace and the ability to cancel,
// which are meaningful only to someone who can act on them.

export function ownerLane(authFetch: LaneFetch): ChatLane {
  return {
    id: 'owner',
    capabilities: { conversations: true, newConversation: true, trace: true, cancel: true, durableHistory: true },

    async send(input, signal) {
      const conversationId = input.conversationId ?? newId();
      const response = await authFetch('/api/admin/apocrypha/jobs', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'include',
        signal,
        body: JSON.stringify({
          prompt: input.text,
          conversation_id: conversationId,
          output_budget: 2048,
          // Long prompts get the deeper budget. Kept from the original: a two-line question and a
          // pasted page of context are not the same request.
          response_mode: input.text.length > 1200 ? 'deep' : 'standard',
          idempotency_key: newId(),
        }),
      });
      const payload = await response.json().catch(() => null) as
        { conversation_id?: string; job?: { id?: string }; error?: string } | null;
      if (!response.ok || !payload?.job?.id || payload.conversation_id !== conversationId) {
        // The conversation-id check is not paranoia: a job that came back bound to a DIFFERENT
        // conversation would silently append this turn to someone else's thread.
        throw new Error(payload?.error ?? 'That message could not be sent.');
      }
      return { jobId: payload.job.id, conversationId };
    },

    async poll(jobId, signal) {
      const response = await authFetch(`/api/admin/apocrypha/jobs/${encodeURIComponent(jobId)}`, {
        cache: 'no-store',
        credentials: 'include',
        signal,
      });
      if (!response.ok) {
        throw new Error(response.status === 404
          ? 'The accepted job could not be found.'
          : `Status service returned ${response.status}.`);
      }
      const snapshot = await response.json() as {
        job?: { id: string; status: string; request?: { conversation_id?: string | null }; error_detail?: string | null };
        chunks?: Array<{ seq: number; delta: string }>;
        revisions?: Array<{ content: string; provenance?: { tool_calls?: ChatToolCall[] } }>;
      };
      if (!snapshot.job) throw new Error('The job status response was incomplete.');
      // Partial text is assembled from ordered chunks; the revision replaces it once it exists.
      const partial = [...(snapshot.chunks ?? [])]
        .sort((left, right) => left.seq - right.seq)
        .map((chunk) => chunk.delta)
        .join('');
      const revision = snapshot.revisions?.[0];
      const conversationId = snapshot.job.request?.conversation_id ?? null;
      return {
        done: TERMINAL.has(snapshot.job.status),
        status: snapshot.job.status,
        text: revision?.content || partial,
        tools: revision?.provenance?.tool_calls,
        conversationId: isConversationId(conversationId) ? conversationId.toLowerCase() : null,
        failure: snapshot.job.error_detail ?? null,
      };
    },

    async listConversations() {
      const response = await authFetch('/api/admin/apocrypha/conversations?scope=active');
      if (!response.ok) throw new Error(`Conversation history returned ${response.status}.`);
      const envelope = await response.json() as
        { data?: { conversations?: Array<{ id: string; title: string | null; last_active_iso: string; message_count?: number }> } };
      return (envelope.data?.conversations ?? []).map((row) => ({
        id: row.id,
        title: row.title,
        lastActiveIso: row.last_active_iso,
        messageCount: row.message_count,
      }));
    },

    async loadConversation(id) {
      const response = await authFetch(`/api/admin/apocrypha/conversations?id=${encodeURIComponent(id)}`);
      if (!response.ok) throw new Error(`Conversation history returned ${response.status}.`);
      const envelope = await response.json() as {
        data?: { messages?: Array<{ id: string; role: string; text: string; ts_iso: string; tool_trace?: ChatToolCall[] }> };
      };
      return (envelope.data?.messages ?? []).map((row) => ({
        id: row.id,
        role: row.role === 'user' ? 'user' as const : 'apocrypha' as const,
        text: row.text,
        at: new Date(row.ts_iso),
        tools: row.tool_trace,
      }));
    },

    async cancel(jobId) {
      await authFetch(`/api/admin/apocrypha/jobs/${encodeURIComponent(jobId)}`, {
        method: 'DELETE',
        credentials: 'include',
      });
    },
  };
}

export function laneFor(role: LaneId, authFetch: LaneFetch): ChatLane {
  if (role === 'owner') return ownerLane(authFetch);
  if (role === 'member') return memberLane(authFetch);
  return guestLane();
}
