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

/** The name of the window event that carries reply fragments as they arrive. */
export const TURN_DELTA_EVENT = 'apocrypha://turn-delta';

/** A reply currently being written. */
export interface Live {
  text: string;
  startedAt: number;
}

/** How long the reply has been arriving, in words rather than milliseconds. */
export function elapsedLabel(milliseconds: number): string {
  const seconds = Math.max(0, Math.floor(milliseconds / 1000));
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  return `${minutes}m ${String(seconds % 60).padStart(2, '0')}s`;
}

/**
 * What to say while a reply is still arriving.
 *
 * Silence is the thing to avoid: before the first fragment there is nothing to
 * read, so the wait itself has to be legible.
 */
export function liveStatus(live: Live | null, now: number): string | null {
  if (!live) return null;
  const elapsed = elapsedLabel(now - live.startedAt);
  return live.text ? `Writing · ${elapsed}` : `Thinking · ${elapsed}`;
}
