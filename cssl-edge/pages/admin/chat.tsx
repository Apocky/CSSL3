import type { NextPage } from 'next';
import { useEffect, useMemo, useRef, useState } from 'react';

import AdminLayout from '../../components/AdminLayout';
import AdminTooltip from '../../components/AdminTooltip';
import { authFetch } from '../../lib/browser-auth';

type Role = 'you' | 'assistant' | 'system';

interface Msg {
  id: string;
  role: Role;
  text: string;
  at: number;
  pending?: boolean;
  model?: string;
}

interface Conversation {
  id: string;
  title: string;
  updatedAt: number;
  messages: Msg[];
}

interface AttachmentDraft {
  id: string;
  name: string;
  size: number;
  type: string;
  text?: string;
}

interface BridgeStatus {
  online: boolean;
  model_ready?: boolean;
  provider?: 'deepseek' | 'anthropic' | 'none';
  model?: string | null;
}

const STORAGE_KEY = 'apocky-admin-chat-conversations-v1';
const ROLE_LABEL: Record<Role, string> = { you: 'You', assistant: 'Assistant', system: 'System' };
const ROLE_COLOR: Record<Role, string> = { you: '#ffffff', assistant: '#7dd3fc', system: '#f87171' };
const SUGGESTIONS = [
  'Summarize the current Lazarus state and next best action.',
  'Turn this rough idea into an implementation plan.',
  'Review the deployment risk before I ship this.',
  'Draft a precise runner task with validation evidence.',
];

function id(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

function blankConversation(): Conversation {
  return { id: id('chat'), title: 'New chat', updatedAt: Date.now(), messages: [] };
}

function safeStoredConversations(value: string | null): Conversation[] {
  if (!value) return [blankConversation()];
  try {
    const parsed = JSON.parse(value) as unknown;
    if (!Array.isArray(parsed)) return [blankConversation()];
    const conversations = parsed.filter((item): item is Conversation => {
      if (typeof item !== 'object' || item === null) return false;
      const record = item as Record<string, unknown>;
      return typeof record.id === 'string' && typeof record.title === 'string' && typeof record.updatedAt === 'number' && Array.isArray(record.messages);
    });
    return conversations.length > 0 ? conversations : [blankConversation()];
  } catch {
    return [blankConversation()];
  }
}

function toApiMessages(messages: Msg[]): Array<{ role: 'user' | 'assistant' | 'system'; content: string }> {
  return messages
    .filter((message) => !message.pending && message.text.trim())
    .map((message) => ({
      role: message.role === 'you' ? 'user' : message.role,
      content: message.text,
    }));
}

function titleFrom(text: string): string {
  const firstLine = text.replace(/\s+/g, ' ').trim();
  if (!firstLine) return 'New chat';
  return firstLine.length > 48 ? `${firstLine.slice(0, 45)}...` : firstLine;
}

function fileSummary(file: AttachmentDraft): string {
  const kb = Math.max(1, Math.round(file.size / 1024));
  return `${file.name} (${kb} KB${file.type ? `, ${file.type}` : ''})`;
}

function buildUserText(input: string, attachments: AttachmentDraft[]): string {
  const base = input.trim();
  if (attachments.length === 0) return base;
  const rendered = attachments.map((file) => {
    if (file.text) return `File: ${fileSummary(file)}\n\n${file.text}`;
    return `File: ${fileSummary(file)}\nBinary or unreadable content was attached; use the filename and metadata only.`;
  }).join('\n\n---\n\n');
  return `${base || 'Review the attached file(s).'}\n\nAttached context:\n\n${rendered}`;
}

function formatStatus(status: BridgeStatus | null): string {
  if (!status) return 'checking';
  if (!status.model_ready) return 'not configured';
  return `${status.provider ?? 'model'} · ${status.model ?? 'ready'}`;
}

const Chat: NextPage = () => {
  const [conversations, setConversations] = useState<Conversation[]>([blankConversation()]);
  const [activeId, setActiveId] = useState<string>('');
  const [search, setSearch] = useState('');
  const [input, setInput] = useState('');
  const [attachments, setAttachments] = useState<AttachmentDraft[]>([]);
  const [submitting, setSubmitting] = useState(false);
  const [status, setStatus] = useState<BridgeStatus | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const abortRef = useRef<AbortController | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const loaded = safeStoredConversations(localStorage.getItem(STORAGE_KEY));
    setConversations(loaded);
    setActiveId(loaded[0]?.id ?? '');
  }, []);

  useEffect(() => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(conversations.slice(0, 30)));
  }, [conversations]);

  useEffect(() => {
    const fetchStatus = () => {
      authFetch('/api/admin/bridge?action=status', { cache: 'no-store' })
        .then((response) => response.json())
        .then((json: BridgeStatus) => setStatus(json))
        .catch(() => setStatus({ online: false, model_ready: false, provider: 'none', model: null }));
    };
    fetchStatus();
    const timer = setInterval(fetchStatus, 10_000);
    return () => clearInterval(timer);
  }, []);

  const activeConversation = conversations.find((conversation) => conversation.id === activeId) ?? conversations[0] ?? blankConversation();
  const messages = activeConversation.messages;
  const filteredConversations = useMemo(() => {
    const query = search.trim().toLowerCase();
    return [...conversations]
      .sort((a, b) => b.updatedAt - a.updatedAt)
      .filter((conversation) => !query || conversation.title.toLowerCase().includes(query));
  }, [conversations, search]);
  const lastUserIndex = useMemo(() => {
    for (let index = messages.length - 1; index >= 0; index -= 1) {
      if (messages[index]?.role === 'you') return index;
    }
    return -1;
  }, [messages]);

  useEffect(() => {
    if (scrollRef.current) scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
  }, [messages]);

  function updateActiveMessages(nextMessages: Msg[]): void {
    setConversations((prev) => prev.map((conversation) => {
      if (conversation.id !== activeConversation.id) return conversation;
      const firstUser = nextMessages.find((message) => message.role === 'you');
      return {
        ...conversation,
        title: firstUser ? titleFrom(firstUser.text) : conversation.title,
        updatedAt: Date.now(),
        messages: nextMessages,
      };
    }));
  }

  function startNewChat(): void {
    const conversation = blankConversation();
    setConversations((prev) => [conversation, ...prev]);
    setActiveId(conversation.id);
    setInput('');
    setAttachments([]);
  }

  function deleteConversation(conversationId: string): void {
    setConversations((prev) => {
      const remaining = prev.filter((conversation) => conversation.id !== conversationId);
      const next = remaining.length > 0 ? remaining : [blankConversation()];
      if (conversationId === activeId) setActiveId(next[0]?.id ?? '');
      return next;
    });
  }

  async function requestAssistant(conversationMessages: Msg[], pendingId: string): Promise<void> {
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;
    setSubmitting(true);
    try {
      const lastUser = conversationMessages.filter((message) => message.role === 'you').at(-1);
      const response = await authFetch('/api/admin/bridge?action=send', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        signal: controller.signal,
        body: JSON.stringify({
          text: lastUser?.text ?? '',
          messages: toApiMessages(conversationMessages),
        }),
      });
      const json = await response.json() as { response?: unknown; model?: unknown; error?: unknown };
      if (!response.ok) throw new Error(typeof json.error === 'string' ? json.error : `chat request failed (${response.status})`);
      const text = typeof json.response === 'string' ? json.response : 'The model returned no text.';
      updateActiveMessages(conversationMessages.map((message) =>
        message.id === pendingId
          ? { ...message, role: 'assistant', text, pending: false, model: typeof json.model === 'string' ? json.model : undefined }
          : message,
      ));
    } catch (err) {
      const text = err instanceof DOMException && err.name === 'AbortError'
        ? 'Request cancelled.'
        : err instanceof Error
          ? err.message
          : String(err);
      updateActiveMessages(conversationMessages.map((message) =>
        message.id === pendingId ? { ...message, role: 'system', text, pending: false } : message,
      ));
    } finally {
      if (abortRef.current === controller) abortRef.current = null;
      setSubmitting(false);
    }
  }

  async function submitCurrent(): Promise<void> {
    const text = buildUserText(input, attachments);
    if (!text || submitting) return;
    const now = Date.now();
    const cleanMessages = messages.filter((message) => !message.pending);
    const yourMsg: Msg = { id: id('user'), role: 'you', text, at: now };
    const pendingId = id('assistant');
    const pendingResponse: Msg = { id: pendingId, role: 'assistant', text: 'Thinking...', at: now, pending: true };
    const conversationMessages = [...cleanMessages, yourMsg, pendingResponse];
    updateActiveMessages(conversationMessages);
    setInput('');
    setAttachments([]);
    await requestAssistant(conversationMessages, pendingId);
  }

  async function send(e: React.FormEvent): Promise<void> {
    e.preventDefault();
    await submitCurrent();
  }

  async function regenerate(): Promise<void> {
    if (submitting || lastUserIndex < 0) return;
    const conversationMessages = messages.slice(0, lastUserIndex + 1).filter((message) => !message.pending);
    const pendingId = id('regen');
    const pendingResponse: Msg = { id: pendingId, role: 'assistant', text: 'Thinking...', at: Date.now(), pending: true };
    const nextMessages = [...conversationMessages, pendingResponse];
    updateActiveMessages(nextMessages);
    await requestAssistant(nextMessages, pendingId);
  }

  async function attachFiles(files: FileList | null): Promise<void> {
    if (!files) return;
    const next = await Promise.all(Array.from(files).slice(0, 6).map(async (file): Promise<AttachmentDraft> => {
      const base = { id: id('file'), name: file.name, size: file.size, type: file.type };
      if (file.type.startsWith('text/') || /\.(md|txt|json|ts|tsx|js|jsx|py|rs|csl|css|html)$/i.test(file.name)) {
        return { ...base, text: (await file.text()).slice(0, 32_000) };
      }
      return base;
    }));
    setAttachments((prev) => [...prev, ...next].slice(-8));
    if (fileRef.current) fileRef.current.value = '';
  }

  async function copyText(text: string): Promise<void> {
    await navigator.clipboard.writeText(text);
  }

  async function copyConversation(): Promise<void> {
    const transcript = messages
      .filter((message) => !message.pending)
      .map((message) => `${ROLE_LABEL[message.role]}: ${message.text}`)
      .join('\n\n');
    await navigator.clipboard.writeText(transcript);
  }

  return (
    <AdminLayout title="Chat">
      <style jsx>{`
        @media (max-width: 920px) {
          .chat-shell { grid-template-columns: minmax(0, 1fr) !important; }
          .chat-rail { max-height: 240px; overflow: auto; }
        }
      `}</style>
      <div className="chat-shell" style={{ display: 'grid', gridTemplateColumns: 'minmax(180px, 260px) minmax(0, 1fr)', gap: '1rem', minHeight: 'calc(100dvh - 120px)' }}>
        <aside className="chat-rail" style={{ border: '1px solid #1f1f2a', borderRadius: 6, background: 'rgba(10, 10, 16, 0.35)', padding: '0.75rem', alignSelf: 'stretch' }}>
          <button type="button" onClick={startNewChat} style={{ ...railButtonStyle, width: '100%', color: '#e6e6f0' }}>
            + New chat
          </button>
          <input
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder="Search chats"
            style={{ width: '100%', marginTop: '0.6rem', padding: '0.55rem 0.65rem', borderRadius: 4, border: '1px solid #2a2a3a', background: 'rgba(20, 20, 30, 0.65)', color: '#e6e6f0', outline: 'none' }}
          />
          <div style={{ display: 'grid', gap: '0.35rem', marginTop: '0.75rem' }}>
            {filteredConversations.map((conversation) => (
              <div key={conversation.id} style={{ display: 'grid', gridTemplateColumns: 'minmax(0, 1fr) auto', gap: '0.25rem', alignItems: 'center' }}>
                <button
                  type="button"
                  onClick={() => setActiveId(conversation.id)}
                  style={{
                    ...railButtonStyle,
                    minWidth: 0,
                    color: conversation.id === activeConversation.id ? '#7dd3fc' : '#cdd6e4',
                    background: conversation.id === activeConversation.id ? 'rgba(124, 211, 252, 0.1)' : 'transparent',
                  }}
                >
                  <span style={{ display: 'block', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{conversation.title}</span>
                </button>
                <button type="button" onClick={() => deleteConversation(conversation.id)} style={iconButtonStyle} aria-label="Delete chat">
                  x
                </button>
              </div>
            ))}
          </div>
        </aside>

        <section style={{ display: 'grid', gridTemplateRows: 'auto minmax(0, 1fr) auto', minHeight: 'calc(100dvh - 120px)' }}>
          <div style={{ display: 'flex', gap: '0.5rem', alignItems: 'center', justifyContent: 'space-between', flexWrap: 'wrap', marginBottom: '0.75rem' }}>
            <div style={{ display: 'inline-flex', alignItems: 'center', gap: '0.45rem', color: status?.model_ready ? '#34d399' : '#f87171', fontSize: '0.82rem' }}>
              <span>{formatStatus(status)}</span>
              <AdminTooltip label="This shows the server-side model available to private admin chat. Keys stay server-only." />
            </div>
            <div style={{ display: 'flex', gap: '0.45rem', flexWrap: 'wrap' }}>
              <button type="button" onClick={() => void regenerate()} disabled={submitting || lastUserIndex < 0} style={toolButtonStyle}>regenerate</button>
              <button type="button" onClick={() => void copyConversation()} disabled={messages.length === 0} style={toolButtonStyle}>copy</button>
              <button type="button" onClick={() => updateActiveMessages([])} disabled={messages.length === 0} style={toolButtonStyle}>clear</button>
              {submitting && <button type="button" onClick={() => abortRef.current?.abort()} style={{ ...toolButtonStyle, color: '#f87171' }}>stop</button>}
            </div>
          </div>

          <div ref={scrollRef} style={{ overflowY: 'auto', padding: messages.length === 0 ? '8vh 0 1rem' : '0.5rem 0 1rem' }}>
            {messages.length === 0 ? (
              <div style={{ maxWidth: 760, margin: '0 auto', textAlign: 'center' }}>
                <h2 style={{ margin: '0 0 1.2rem', fontSize: '1.8rem', color: '#e6e6f0' }}>How can I help?</h2>
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: '0.45rem', justifyContent: 'center' }}>
                  {SUGGESTIONS.map((suggestion) => (
                    <button key={suggestion} type="button" onClick={() => setInput(suggestion)} style={suggestionStyle}>
                      {suggestion}
                    </button>
                  ))}
                </div>
              </div>
            ) : (
              <div style={{ maxWidth: 980, margin: '0 auto' }}>
                {messages.map((message) => (
                  <article key={message.id} style={{ marginBottom: '1.25rem' }}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: '0.4rem', marginBottom: 4, flexWrap: 'wrap' }}>
                      <div style={{ fontSize: '0.72rem', color: ROLE_COLOR[message.role], fontWeight: 700 }}>{ROLE_LABEL[message.role]}</div>
                      {message.model && <div style={{ fontSize: '0.68rem', color: '#5a5a6a' }}>{message.model}</div>}
                      {!message.pending && <button type="button" onClick={() => void copyText(message.text)} style={inlineButtonStyle}>copy</button>}
                    </div>
                    <div style={{ color: message.role === 'system' ? '#f87171' : '#dbe7f3', whiteSpace: 'pre-wrap', wordBreak: 'break-word', lineHeight: 1.65 }}>
                      {message.text}
                    </div>
                  </article>
                ))}
              </div>
            )}
          </div>

          <form onSubmit={send} style={{ maxWidth: 760, width: '100%', margin: '0 auto' }}>
            {attachments.length > 0 && (
              <div style={{ display: 'flex', flexWrap: 'wrap', gap: '0.35rem', marginBottom: '0.45rem' }}>
                {attachments.map((file) => (
                  <span key={file.id} style={{ display: 'inline-flex', alignItems: 'center', gap: '0.35rem', border: '1px solid #2a2a3a', borderRadius: 999, padding: '0.2rem 0.55rem', color: '#cdd6e4', fontSize: '0.72rem' }}>
                    {file.name}
                    <button type="button" onClick={() => setAttachments((prev) => prev.filter((item) => item.id !== file.id))} style={{ ...inlineButtonStyle, padding: '0 0.25rem' }}>x</button>
                  </span>
                ))}
              </div>
            )}
            <div style={{ display: 'grid', gridTemplateColumns: 'auto minmax(0, 1fr) auto', alignItems: 'end', gap: '0.45rem', border: '1px solid #2a2a3a', borderRadius: 8, background: 'rgba(20, 20, 30, 0.78)', padding: '0.55rem' }}>
              <input ref={fileRef} type="file" multiple style={{ display: 'none' }} onChange={(event) => void attachFiles(event.target.files)} />
              <button type="button" onClick={() => fileRef.current?.click()} style={iconButtonStyle} aria-label="Attach files">+</button>
              <textarea
                value={input}
                onChange={(event) => setInput(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' && !event.shiftKey) {
                    event.preventDefault();
                    void submitCurrent();
                  }
                }}
                placeholder="Ask anything"
                disabled={submitting}
                rows={3}
                style={{ width: '100%', border: 'none', background: 'transparent', color: '#e6e6f0', outline: 'none', resize: 'vertical', minHeight: 60, maxHeight: 220, fontFamily: 'inherit', fontSize: '0.95rem' }}
              />
              <button type="submit" disabled={submitting || (!input.trim() && attachments.length === 0)} style={{ ...sendButtonStyle, opacity: submitting || (!input.trim() && attachments.length === 0) ? 0.5 : 1 }}>
                send
              </button>
            </div>
          </form>
        </section>
      </div>
    </AdminLayout>
  );
};

const railButtonStyle = {
  minHeight: 34,
  border: '1px solid #1f1f2a',
  borderRadius: 4,
  background: 'rgba(20, 20, 30, 0.55)',
  padding: '0.45rem 0.55rem',
  fontFamily: 'inherit',
  cursor: 'pointer',
  textAlign: 'left',
} as const;

const toolButtonStyle = {
  minHeight: 34,
  border: '1px solid #2a2a3a',
  borderRadius: 4,
  background: 'rgba(20, 20, 30, 0.7)',
  color: '#cdd6e4',
  padding: '0 0.7rem',
  fontFamily: 'inherit',
  cursor: 'pointer',
} as const;

const iconButtonStyle = {
  width: 34,
  height: 34,
  border: '1px solid #2a2a3a',
  borderRadius: 4,
  background: 'rgba(20, 20, 30, 0.7)',
  color: '#cdd6e4',
  fontFamily: 'inherit',
  cursor: 'pointer',
} as const;

const inlineButtonStyle = {
  border: '1px solid #2a2a3a',
  borderRadius: 4,
  background: 'rgba(20, 20, 30, 0.5)',
  color: '#7a7a8c',
  padding: '0.05rem 0.35rem',
  fontSize: '0.65rem',
  fontFamily: 'inherit',
  cursor: 'pointer',
} as const;

const suggestionStyle = {
  minHeight: 36,
  border: '1px solid #2a2a3a',
  borderRadius: 999,
  background: 'rgba(20, 20, 30, 0.55)',
  color: '#cdd6e4',
  padding: '0 0.8rem',
  fontFamily: 'inherit',
  cursor: 'pointer',
} as const;

const sendButtonStyle = {
  minHeight: 42,
  border: 'none',
  borderRadius: 6,
  background: 'linear-gradient(135deg, #c084fc 0%, #7dd3fc 100%)',
  color: '#0a0a0f',
  fontWeight: 700,
  padding: '0 0.9rem',
  fontFamily: 'inherit',
  cursor: 'pointer',
} as const;

export function _testExportsAreFunctions(): boolean {
  return typeof Chat === 'function';
}

export default Chat;