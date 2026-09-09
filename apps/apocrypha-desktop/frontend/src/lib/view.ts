// Shapes and pure rules shared by the window. Everything here is decided
// without touching the network or the Tauri bridge, so it can be tested on its
// own; anything that needs a token or an endpoint belongs in the Rust side.

export interface Message {
  role: 'user' | 'assistant';
  content: string;
  request_id: string;
}

export interface Conversation {
  id: string;
  title: string;
}

export interface View {
  configured: boolean;
  signed_in: boolean;
  email: string;
  email_draft: string;
  code_sent: boolean;
  access_denied: boolean;
  notice: string;
  session_id: string;
  pending_request: string;
  history_scope: string;
  messages: Message[];
  conversations: Conversation[];
}

export const EMPTY_VIEW: View = {
  configured: false,
  signed_in: false,
  email: '',
  email_draft: '',
  code_sent: false,
  access_denied: false,
  notice: 'Connecting to apocky.com…',
  session_id: '',
  pending_request: '',
  history_scope: 'account_conversations',
  messages: [],
  conversations: [],
};

export const MAX_TEXT_BYTES = 16_384;

/** Mirrors the byte budget the service enforces, so the count shown is real. */
export function promptBytes(value: string): number {
  return new TextEncoder().encode(value.trim()).length;
}

/**
 * Why the composer is closed, or null when it is open.
 *
 * An unconfirmed reply keeps the composer shut on purpose: the service may
 * still be working on that message, and sending again would duplicate it.
 */
export function sendBlockedReason(view: View, draft: string, busy: boolean): string | null {
  if (busy) return 'Working…';
  if (!view.signed_in) return 'Sign in to send a message.';
  if (view.access_denied) return 'This account cannot use this conversation. Start a new one.';
  if (view.pending_request) return 'Waiting on an unconfirmed reply. Refresh to check it.';
  const bytes = promptBytes(draft);
  if (bytes < 1) return null;
  if (bytes > MAX_TEXT_BYTES) return 'This message is longer than 16 KB.';
  return null;
}

export function canSend(view: View, draft: string, busy: boolean): boolean {
  const bytes = promptBytes(draft);
  return bytes >= 1 && bytes <= MAX_TEXT_BYTES && sendBlockedReason(view, draft, busy) === null;
}

/** A short label for a conversation in the sidebar. */
export function conversationLabel(conversation: Conversation): string {
  const title = conversation.title.replace(/\s+/g, ' ').trim();
  if (!title) return 'Untitled conversation';
  return title.length > 64 ? `${title.slice(0, 63)}…` : title;
}

/** True when this history came back partial and the person should know. */
export function scopeNote(view: View): string | null {
  if (!view.signed_in) return null;
  return view.history_scope === 'latest_conversation_only'
    ? 'This service is listing only your most recent conversation.'
    : null;
}
