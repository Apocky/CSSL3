// Modern Apocrypha chat — sidebar + bubble thread + streaming via SSE.
//
// Wires /api/admin/apocrypha/chat_stream (POST → text/event-stream proxy of
// /api/v1/chat/stream) for incremental delivery : tool events surface as
// pending-chips while the loop runs ; final response arrives when complete.
//
// Per HANDOFF_v10 § TRACK-A polish-pass (replaces the cockpit-monospace draft).

import React, { useCallback, useEffect, useRef, useState } from 'react';

import { authFetch } from '../../lib/browser-auth';

// ─── Types ──────────────────────────────────────────────────────────

interface ToolCallChip {
  name: string;
  ok: boolean;
  elapsed_ms?: number;
  error?: string | null;
}

interface ChatMessage {
  role: 'user' | 'apocrypha';
  text: string;
  ts: Date;
  toolCalls?: ToolCallChip[];
  halt?: string;
  elapsed_s?: number;
  cost_usd?: number;
}

interface ConvSummary {
  id: number;
  title: string | null;
  last_active_iso: string;
}

interface ConvMessagesResponse {
  conversation: { id: number; title: string | null; last_active_iso: string };
  messages: Array<{
    id: number;
    role: string;
    text: string;
    ts_iso: string;
    tool_trace: ToolCallChip[];
  }>;
}

interface ApocryphaEnvelope<T> {
  upstream_status: number;
  data: T;
}

// ─── Streaming SSE helpers ─────────────────────────────────────────

interface SseEvent {
  type: string;
  data: Record<string, unknown>;
}

function parseSseBuffer(buffer: string): { events: SseEvent[]; remainder: string } {
  const events: SseEvent[] = [];
  let remainder = buffer;
  while (true) {
    const idx = remainder.indexOf('\n\n');
    if (idx === -1) break;
    const block = remainder.slice(0, idx);
    remainder = remainder.slice(idx + 2);
    let eventType = 'message';
    const dataLines: string[] = [];
    for (const line of block.split('\n')) {
      if (line.startsWith('event:')) eventType = line.slice(6).trim();
      else if (line.startsWith('data:')) dataLines.push(line.slice(5).trim());
    }
    if (dataLines.length === 0) continue;
    try {
      const data = JSON.parse(dataLines.join('\n')) as Record<string, unknown>;
      events.push({ type: eventType, data });
    } catch {
      // skip malformed event
    }
  }
  return { events, remainder };
}

// ─── Component ─────────────────────────────────────────────────────

export function ChatThread() {
  const [convs, setConvs] = useState<ConvSummary[]>([]);
  const [currentConv, setCurrentConv] = useState<number | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [draft, setDraft] = useState('');
  const [streaming, setStreaming] = useState(false);
  const [streamingTools, setStreamingTools] = useState<ToolCallChip[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // ── data loading ──────────────────────────────────────────────

  const loadConvs = useCallback(async () => {
    try {
      const r = await authFetch('/api/admin/apocrypha/conversations');
      const env = (await r.json()) as ApocryphaEnvelope<{ conversations: ConvSummary[] }>;
      setConvs(env.data?.conversations ?? []);
    } catch {
      /* silent ; sidebar empty is fine */
    }
  }, []);

  useEffect(() => {
    void loadConvs();
  }, [loadConvs]);

  const loadConv = useCallback(async (id: number) => {
    try {
      const r = await authFetch(`/api/admin/apocrypha/conversations?id=${id}`);
      const env = (await r.json()) as ApocryphaEnvelope<ConvMessagesResponse>;
      const msgs: ChatMessage[] = (env.data?.messages ?? []).map((m) => ({
        role: m.role === 'apocrypha' ? ('apocrypha' as const) : ('user' as const),
        text: m.text,
        ts: new Date(m.ts_iso),
        toolCalls: m.tool_trace ?? [],
      }));
      setMessages(msgs);
      setCurrentConv(id);
      setStreamingTools([]);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }, []);

  const newChat = useCallback(() => {
    setMessages([]);
    setCurrentConv(null);
    setStreamingTools([]);
    setError(null);
    setTimeout(() => textareaRef.current?.focus(), 0);
  }, []);

  // ── auto-scroll + textarea auto-grow ──────────────────────────

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth', block: 'end' });
  }, [messages, streamingTools, streaming]);

  useEffect(() => {
    const t = textareaRef.current;
    if (!t) return;
    t.style.height = 'auto';
    t.style.height = `${Math.min(t.scrollHeight, 200)}px`;
  }, [draft]);

  // ── send + stream-consume ─────────────────────────────────────

  const handleSend = useCallback(async () => {
    const text = draft.trim();
    if (!text || streaming) return;
    setDraft('');
    setError(null);
    setStreamingTools([]);
    setMessages((prev) => [...prev, { role: 'user', text, ts: new Date() }]);
    setStreaming(true);

    try {
      const r = await fetch('/api/admin/apocrypha/chat_stream', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Accept: 'text/event-stream' },
        credentials: 'include',
        body: JSON.stringify({ text, conversation_id: currentConv }),
      });
      if (!r.ok || !r.body) {
        const errText = await r.text().catch(() => '');
        throw new Error(`HTTP ${r.status} ${errText.slice(0, 200)}`);
      }
      const reader = r.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';
      let gotFinal = false;
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        const { events, remainder } = parseSseBuffer(buffer);
        buffer = remainder;
        for (const ev of events) {
          if (ev.type === 'conversation') {
            const id = ev.data['conversation_id'];
            if (typeof id === 'number') setCurrentConv(id);
          } else if (ev.type === 'tool_event') {
            setStreamingTools((prev) => [
              ...prev,
              {
                name: String(ev.data['tool_name'] ?? '?'),
                ok: ev.data['ok'] !== false,
                elapsed_ms: typeof ev.data['elapsed_ms'] === 'number' ? ev.data['elapsed_ms'] : undefined,
                error: typeof ev.data['error'] === 'string' ? ev.data['error'] : null,
              },
            ]);
          } else if (ev.type === 'final') {
            gotFinal = true;
            const finalMsg: ChatMessage = {
              role: 'apocrypha',
              text: String(ev.data['final_response'] ?? ''),
              ts: new Date(),
              toolCalls: Array.isArray(ev.data['tool_calls'])
                ? (ev.data['tool_calls'] as ToolCallChip[])
                : [],
              halt: typeof ev.data['halted_reason'] === 'string' ? ev.data['halted_reason'] : undefined,
              elapsed_s: typeof ev.data['elapsed_s'] === 'number' ? ev.data['elapsed_s'] : undefined,
              cost_usd: typeof ev.data['total_cost_usd'] === 'number' ? ev.data['total_cost_usd'] : undefined,
            };
            setMessages((prev) => [...prev, finalMsg]);
            setStreamingTools([]);
          } else if (ev.type === 'error') {
            setError(String(ev.data['error'] ?? 'stream error'));
          }
        }
      }
      if (!gotFinal && !error) {
        setError('stream ended without final response');
      }
      void loadConvs();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setStreaming(false);
    }
  }, [draft, streaming, currentConv, error, loadConvs]);

  const handleKey = useCallback((e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      void handleSend();
    }
  }, [handleSend]);

  // ─── render ─────────────────────────────────────────────────

  return (
    <div style={{
      display: 'flex',
      height: '100%',
      background: '#0a0a10',
      color: '#e6e6f0',
      fontFamily: 'system-ui, -apple-system, "Segoe UI", Roboto, sans-serif',
    }}>
      {/* SIDEBAR */}
      {sidebarOpen && (
        <aside style={{
          width: 280,
          minWidth: 280,
          borderRight: '1px solid #1f1f2a',
          display: 'flex',
          flexDirection: 'column',
          background: 'rgba(15, 15, 22, 0.7)',
        }}>
          <div style={{ padding: '0.75rem', borderBottom: '1px solid #1f1f2a' }}>
            <button onClick={newChat} style={{
              width: '100%',
              padding: '0.65rem 0.8rem',
              background: 'transparent',
              border: '1px solid #2a2a3a',
              borderRadius: 8,
              color: '#cdd6e4',
              cursor: 'pointer',
              fontSize: '0.88rem',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              fontFamily: 'inherit',
            }}>
              <span style={{ fontWeight: 500 }}>+ New chat</span>
              <span style={{ color: '#7a7a8c', fontSize: '0.75rem' }}>⌘N</span>
            </button>
          </div>
          <div style={{ flex: 1, overflowY: 'auto', padding: '0.4rem' }}>
            {convs.length === 0 && (
              <div style={{ padding: '0.6rem 0.7rem', color: '#7a7a8c', fontSize: '0.8rem' }}>
                no conversations yet
              </div>
            )}
            {convs.map((c) => (
              <button key={c.id}
                onClick={() => void loadConv(c.id)}
                style={{
                  display: 'block',
                  width: '100%',
                  padding: '0.55rem 0.75rem',
                  marginBottom: 2,
                  background: c.id === currentConv ? 'rgba(192, 132, 252, 0.18)' : 'transparent',
                  border: c.id === currentConv ? '1px solid rgba(192, 132, 252, 0.35)' : '1px solid transparent',
                  borderRadius: 6,
                  color: c.id === currentConv ? '#e6e6f0' : '#cdd6e4',
                  textAlign: 'left',
                  cursor: 'pointer',
                  fontSize: '0.85rem',
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  whiteSpace: 'nowrap',
                  fontFamily: 'inherit',
                }}>
                {c.title || `Conversation #${c.id}`}
                <div style={{ fontSize: '0.7rem', color: '#5a5a6a', marginTop: 2 }}>
                  {new Date(c.last_active_iso).toLocaleString()}
                </div>
              </button>
            ))}
          </div>
        </aside>
      )}

      {/* MAIN */}
      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', minWidth: 0 }}>
        {/* HEADER */}
        <header style={{
          padding: '0.6rem 1rem',
          borderBottom: '1px solid #1f1f2a',
          display: 'flex',
          alignItems: 'center',
          gap: '0.6rem',
          fontSize: '0.85rem',
          color: '#9aa0a6',
        }}>
          <button onClick={() => setSidebarOpen(!sidebarOpen)} style={{
            background: 'transparent',
            border: 0,
            color: '#9aa0a6',
            cursor: 'pointer',
            fontSize: '1.05rem',
            padding: '0.2rem 0.5rem',
            fontFamily: 'inherit',
          }} title="toggle sidebar">
            ☰
          </button>
          <span style={{
            fontWeight: 600,
            backgroundImage: 'linear-gradient(135deg, #ffaa55, #c084fc)',
            WebkitBackgroundClip: 'text',
            WebkitTextFillColor: 'transparent',
          }}>
            Apocrypha
          </span>
          <span style={{ flex: 1 }} />
          <span style={{ color: '#7a7a8c', fontSize: '0.75rem' }}>
            {currentConv ? `conv #${currentConv}` : 'new conversation'}
          </span>
        </header>

        {/* THREAD */}
        <div style={{ flex: 1, overflowY: 'auto', padding: '1.5rem 0' }}>
          <div style={{ maxWidth: 760, margin: '0 auto', padding: '0 1.2rem' }}>
            {messages.length === 0 && !streaming && (
              <div style={{
                color: '#7a7a8c',
                fontSize: '1rem',
                textAlign: 'center',
                marginTop: '3.5rem',
              }}>
                <div style={{
                  fontSize: '2.2rem',
                  marginBottom: '0.6rem',
                  fontWeight: 600,
                  backgroundImage: 'linear-gradient(135deg, #ffaa55, #c084fc)',
                  WebkitBackgroundClip: 'text',
                  WebkitTextFillColor: 'transparent',
                }}>
                  Apocrypha
                </div>
                <div style={{ fontSize: '0.92rem' }}>
                  Tier-0 sampler always-on · Mamba on XPU · DeepSeek escalation
                </div>
                <div style={{ marginTop: '0.5rem', fontSize: '0.8rem', color: '#5a5a6a' }}>
                  Tools auto-invoked when needed · Provenance tagged · Cost-capped
                </div>
              </div>
            )}

            {messages.map((m, i) => (
              <MessageBubble key={i} msg={m} />
            ))}

            {streaming && (
              <div style={{
                display: 'flex',
                flexDirection: 'column',
                alignItems: 'flex-start',
                marginBottom: '1.5rem',
              }}>
                {streamingTools.length > 0 && (
                  <div style={{
                    display: 'flex',
                    flexWrap: 'wrap',
                    gap: '0.3rem',
                    marginBottom: '0.5rem',
                    fontSize: '0.72rem',
                    fontFamily: 'ui-monospace, SFMono-Regular, monospace',
                  }}>
                    {streamingTools.map((t, i) => (
                      <ToolChip key={i} chip={t} />
                    ))}
                  </div>
                )}
                <div style={{
                  padding: '0.7rem 1rem',
                  borderRadius: 14,
                  background: 'rgba(192, 132, 252, 0.06)',
                  border: '1px solid rgba(192, 132, 252, 0.18)',
                  color: '#9aa0a6',
                  fontSize: '0.92rem',
                  display: 'flex',
                  alignItems: 'center',
                  gap: '0.4rem',
                }}>
                  <PulsingDot />
                  <span>thinking</span>
                </div>
              </div>
            )}

            {error && (
              <div style={{
                marginBottom: '1.5rem',
                padding: '0.65rem 0.9rem',
                background: 'rgba(255, 136, 136, 0.08)',
                border: '1px solid rgba(255, 136, 136, 0.3)',
                borderRadius: 8,
                color: '#ff8888',
                fontSize: '0.88rem',
              }}>
                error : {error}
              </div>
            )}

            <div ref={messagesEndRef} />
          </div>
        </div>

        {/* COMPOSER */}
        <div style={{ borderTop: '1px solid #1f1f2a', padding: '0.9rem 1rem 1.4rem' }}>
          <div style={{ maxWidth: 760, margin: '0 auto' }}>
            <div style={{
              display: 'flex',
              gap: '0.5rem',
              alignItems: 'flex-end',
              padding: '0.5rem',
              background: 'rgba(20, 20, 30, 0.7)',
              border: '1px solid #2a2a3a',
              borderRadius: 16,
            }}>
              <textarea
                ref={textareaRef}
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
                onKeyDown={handleKey}
                placeholder="Message Apocrypha…"
                rows={1}
                style={{
                  flex: 1,
                  background: 'transparent',
                  color: '#e6e6f0',
                  border: 0,
                  outline: 'none',
                  resize: 'none',
                  padding: '0.55rem 0.7rem',
                  fontSize: '0.95rem',
                  fontFamily: 'inherit',
                  minHeight: 36,
                  maxHeight: 200,
                  lineHeight: 1.45,
                }}
              />
              <button
                onClick={() => void handleSend()}
                disabled={streaming || !draft.trim()}
                aria-label="send"
                style={{
                  padding: '0.55rem 0.9rem',
                  background: draft.trim() && !streaming
                    ? 'linear-gradient(135deg, #ffaa55 0%, #c084fc 100%)'
                    : 'rgba(40, 40, 60, 0.5)',
                  color: draft.trim() && !streaming ? '#0a0a10' : '#5a5a6a',
                  border: 0,
                  borderRadius: 12,
                  cursor: draft.trim() && !streaming ? 'pointer' : 'not-allowed',
                  fontWeight: 700,
                  fontSize: '1rem',
                  fontFamily: 'inherit',
                  alignSelf: 'flex-end',
                  minWidth: 44,
                }}>
                {streaming ? '⋯' : '↑'}
              </button>
            </div>
            <div style={{
              marginTop: '0.4rem',
              fontSize: '0.7rem',
              color: '#5a5a6a',
              textAlign: 'center',
            }}>
              Enter to send · Shift+Enter for newline · Apocrypha auto-invokes tools when needed
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// ─── presentation sub-components ──────────────────────────────────

function MessageBubble({ msg }: { msg: ChatMessage }) {
  const isUser = msg.role === 'user';
  return (
    <div style={{
      marginBottom: '1.5rem',
      display: 'flex',
      flexDirection: 'column',
      alignItems: isUser ? 'flex-end' : 'flex-start',
    }}>
      <div style={{
        maxWidth: '85%',
        padding: '0.75rem 1.05rem',
        borderRadius: 16,
        background: isUser
          ? 'rgba(124, 211, 252, 0.13)'
          : 'rgba(192, 132, 252, 0.06)',
        border: isUser
          ? '1px solid rgba(124, 211, 252, 0.22)'
          : '1px solid rgba(192, 132, 252, 0.16)',
        fontSize: '0.96rem',
        lineHeight: 1.6,
        whiteSpace: 'pre-wrap',
        wordBreak: 'break-word',
        color: '#e6e6f0',
      }}>
        {msg.text}
        {msg.toolCalls && msg.toolCalls.length > 0 && (
          <div style={{
            marginTop: '0.7rem',
            paddingTop: '0.6rem',
            borderTop: '1px solid rgba(255, 255, 255, 0.06)',
            display: 'flex',
            flexWrap: 'wrap',
            gap: '0.3rem',
          }}>
            {msg.toolCalls.map((tc, j) => (
              <ToolChip key={j} chip={tc} />
            ))}
          </div>
        )}
      </div>
      {!isUser && (msg.halt || msg.elapsed_s != null || msg.cost_usd != null) && (
        <div style={{
          fontSize: '0.68rem',
          color: '#5a5a6a',
          marginTop: '0.3rem',
          marginLeft: '0.3rem',
          fontFamily: 'ui-monospace, SFMono-Regular, monospace',
        }}>
          {msg.halt && <span>halt={msg.halt}</span>}
          {msg.elapsed_s != null && <span> · {msg.elapsed_s.toFixed(2)}s</span>}
          {msg.cost_usd != null && <span> · ${msg.cost_usd.toFixed(4)}</span>}
        </div>
      )}
    </div>
  );
}

function ToolChip({ chip }: { chip: ToolCallChip }) {
  return (
    <span style={{
      padding: '0.18rem 0.5rem',
      borderRadius: 4,
      background: chip.ok ? 'rgba(127, 209, 127, 0.13)' : 'rgba(255, 136, 136, 0.13)',
      color: chip.ok ? '#9ddb9d' : '#ff8888',
      border: `1px solid ${chip.ok ? 'rgba(127, 209, 127, 0.22)' : 'rgba(255, 136, 136, 0.22)'}`,
      fontSize: '0.72rem',
      fontFamily: 'ui-monospace, SFMono-Regular, monospace',
      whiteSpace: 'nowrap',
    }}>
      {chip.ok ? '✓' : '✗'} {chip.name}
      {chip.elapsed_ms != null && ` · ${chip.elapsed_ms}ms`}
    </span>
  );
}

function PulsingDot() {
  return (
    <>
      <span style={{
        display: 'inline-block',
        width: 8,
        height: 8,
        borderRadius: '50%',
        background: '#c084fc',
        animation: 'apocrypha-pulse 1.4s ease-in-out infinite',
      }} />
      <style>{`
        @keyframes apocrypha-pulse {
          0%, 100% { opacity: 0.3; transform: scale(0.9); }
          50% { opacity: 1; transform: scale(1.1); }
        }
      `}</style>
    </>
  );
}
