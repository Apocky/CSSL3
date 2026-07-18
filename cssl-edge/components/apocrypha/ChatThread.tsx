// Modern Apocrypha chat — sidebar + bubble thread + streaming via SSE.
//
// Wires /api/admin/apocrypha/chat_stream to the native V2 turn route. The
// proxy preserves the existing SSE event contract while the V2 body returns
// its governed response envelope.
//
// Per HANDOFF_v10 § TRACK-A polish-pass (replaces the cockpit-monospace draft).

import React, { useCallback, useEffect, useRef, useState } from 'react';

import { authFetch } from '../../lib/browser-auth';
import { DeadlineExceededError, withDeadline } from '../../lib/apocrypha/deadline';
import { ApocryphaAvatar } from './ApocryphaAvatar';

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

type ConversationAction = 'pin' | 'unpin' | 'archive' | 'unarchive' | 'trash' | 'restore';
type ConversationScope = 'active' | 'archived' | 'trash';

interface ConvSummary {
  id: number;
  title: string | null;
  last_active_iso: string;
  version?: number;
  pinned?: boolean;
  state?: string;
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

const CHAT_BROWSER_DEADLINE_MS = 115_000;
const CHAT_BACKEND_TIMEOUT_S = 100;
const COMPACT_CHAT_QUERY = '(max-width: 767px)';
const CONVERSATION_MENU_WIDTH = 176;
const CONVERSATION_MENU_MAX_HEIGHT = 160;
const VIEWPORT_GUTTER = 8;
const MUTED_TEXT = '#85859a';

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
  const [scope, setScope] = useState<ConversationScope>('active');
  const [currentConv, setCurrentConv] = useState<number | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [draft, setDraft] = useState('');
  const [streaming, setStreaming] = useState(false);
  const [streamingTools, setStreamingTools] = useState<ToolCallChip[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [compactViewport, setCompactViewport] = useState(true);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [showTrace, setShowTrace] = useState(false);
  const [menu, setMenu] = useState<{ id: number; x: number; y: number } | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const menuTriggerRef = useRef<HTMLButtonElement | null>(null);
  const newChatButtonRef = useRef<HTMLButtonElement>(null);
  const sidebarToggleRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    const media = window.matchMedia(COMPACT_CHAT_QUERY);
    const syncViewport = (compact: boolean) => {
      setCompactViewport(compact);
      setSidebarOpen(!compact);
    };
    syncViewport(media.matches);
    const onChange = (event: MediaQueryListEvent) => syncViewport(event.matches);
    media.addEventListener('change', onChange);
    return () => media.removeEventListener('change', onChange);
  }, []);

  // ── data loading ──────────────────────────────────────────────

  const loadConvs = useCallback(async (requestedScope: ConversationScope = scope) => {
    try {
      const r = await authFetch(`/api/admin/apocrypha/conversations?scope=${requestedScope}`);
      const env = (await r.json()) as ApocryphaEnvelope<{ conversations: ConvSummary[] }>;
      setConvs(env.data?.conversations ?? []);
    } catch {
      /* silent ; sidebar empty is fine */
    }
  }, [scope]);

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
      if (compactViewport) {
        setSidebarOpen(false);
        requestAnimationFrame(() => textareaRef.current?.focus());
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }, [compactViewport]);

  const newChat = useCallback(() => {
    setMessages([]);
    setCurrentConv(null);
    setStreamingTools([]);
    setError(null);
    if (compactViewport) setSidebarOpen(false);
    setTimeout(() => textareaRef.current?.focus(), 0);
  }, [compactViewport]);

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

  const restoreMenuFocus = useCallback((origin: HTMLButtonElement | null = menuTriggerRef.current) => {
    requestAnimationFrame(() => {
      if (origin?.isConnected) {
        origin.focus();
      } else if (newChatButtonRef.current?.isConnected) {
        newChatButtonRef.current.focus();
      } else {
        textareaRef.current?.focus();
      }
    });
  }, []);

  const closeMenu = useCallback((restoreFocus: boolean) => {
    setMenu(null);
    if (restoreFocus) restoreMenuFocus();
  }, [restoreMenuFocus]);

  const openConversationMenu = useCallback((
    id: number,
    trigger: HTMLButtonElement,
    point?: { x: number; y: number },
  ) => {
    const rect = trigger.getBoundingClientRect();
    const desiredX = point?.x ?? rect.right - CONVERSATION_MENU_WIDTH;
    const desiredY = point?.y ?? rect.bottom + 4;
    menuTriggerRef.current = trigger;
    setMenu({
      id,
      x: Math.max(VIEWPORT_GUTTER, Math.min(desiredX, window.innerWidth - CONVERSATION_MENU_WIDTH - VIEWPORT_GUTTER)),
      y: Math.max(VIEWPORT_GUTTER, Math.min(desiredY, window.innerHeight - CONVERSATION_MENU_MAX_HEIGHT - VIEWPORT_GUTTER)),
    });
  }, []);

  useEffect(() => {
    if (!menu) return;
    const focusFirstItem = requestAnimationFrame(() => {
      menuRef.current?.querySelector<HTMLButtonElement>('[role="menuitem"]')?.focus();
    });
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) return;
      if (menuRef.current?.contains(target) || menuTriggerRef.current?.contains(target)) return;
      closeMenu(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      closeMenu(true);
    };
    document.addEventListener('pointerdown', onPointerDown);
    document.addEventListener('keydown', onKeyDown);
    return () => {
      cancelAnimationFrame(focusFirstItem);
      document.removeEventListener('pointerdown', onPointerDown);
      document.removeEventListener('keydown', onKeyDown);
    };
  }, [closeMenu, menu]);

  const closeCompactSidebar = useCallback(() => {
    setSidebarOpen(false);
    requestAnimationFrame(() => sidebarToggleRef.current?.focus());
  }, []);

  useEffect(() => {
    if (!compactViewport || !sidebarOpen) return;
    const focusFirstControl = requestAnimationFrame(() => newChatButtonRef.current?.focus());
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || menuRef.current) return;
      event.preventDefault();
      closeCompactSidebar();
    };
    document.addEventListener('keydown', onKeyDown);
    return () => {
      cancelAnimationFrame(focusFirstControl);
      document.removeEventListener('keydown', onKeyDown);
    };
  }, [closeCompactSidebar, compactViewport, sidebarOpen]);

  const handleMenuKeyDown = useCallback((event: React.KeyboardEvent<HTMLDivElement>) => {
    const items = Array.from(menuRef.current?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]') ?? []);
    if (items.length === 0) return;
    const current = Math.max(0, items.indexOf(document.activeElement as HTMLButtonElement));
    let next: number | null = null;
    if (event.key === 'ArrowDown') next = (current + 1) % items.length;
    else if (event.key === 'ArrowUp') next = (current - 1 + items.length) % items.length;
    else if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = items.length - 1;
    else if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      closeMenu(true);
      return;
    } else if (event.key === 'Tab') {
      event.preventDefault();
      closeMenu(true);
      return;
    }
    if (next == null) return;
    event.preventDefault();
    items[next]?.focus();
  }, [closeMenu]);

  const handleSidebarKeyDown = useCallback((event: React.KeyboardEvent<HTMLElement>) => {
    if (!compactViewport || event.key !== 'Tab' || menuRef.current) return;
    const controls = Array.from(event.currentTarget.querySelectorAll<HTMLElement>(
      'button:not([disabled]), a[href], input:not([disabled]), select:not([disabled]), textarea:not([disabled])',
    )).filter((control) => control.getClientRects().length > 0);
    if (controls.length === 0) return;
    const first = controls[0];
    const last = controls.at(-1);
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last?.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first?.focus();
    }
  }, [compactViewport]);

  const mutateConversation = useCallback(async (action: ConversationAction) => {
    if (!menu) return;
    const target = convs.find((c) => c.id === menu.id);
    const origin = menuTriggerRef.current;
    if (!target || target.version == null) {
      setError('Conversation state is stale; reload the list and try again.');
      setMenu(null);
      restoreMenuFocus(origin);
      return;
    }
    setMenu(null);
    try {
      const r = await authFetch('/api/admin/apocrypha/conversations', {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ action, expected_version: target.version }),
      });
      if (!r.ok) throw new Error(`HTTP ${r.status}`);
      await loadConvs(scope);
      if (action === 'archive' || action === 'trash' || action === 'restore' || action === 'unarchive') {
        if (currentConv === target.id) newChat();
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      restoreMenuFocus(origin);
    }
  }, [convs, currentConv, loadConvs, menu, newChat, restoreMenuFocus, scope]);

  // ── send + stream-consume ─────────────────────────────────────

  const handleSend = useCallback(async () => {
    const text = draft.trim();
    if (!text || streaming) return;
    setDraft('');
    setError(null);
    setStreamingTools([]);
    setMessages((prev) => [...prev, { role: 'user', text, ts: new Date() }]);
    setStreaming(true);

    const controller = new AbortController();
    try {
      await withDeadline((async () => {
        const r = await authFetch('/api/admin/apocrypha/chat_stream', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', Accept: 'text/event-stream' },
          credentials: 'include',
          signal: controller.signal,
          body: JSON.stringify({
            text,
            conversation_id: currentConv,
            max_tokens: 128,
            timeout_s: CHAT_BACKEND_TIMEOUT_S,
          }),
        });
        if (!r.ok || !r.body) {
          const errText = await r.text().catch(() => '');
          throw new Error(`HTTP ${r.status} ${errText.slice(0, 200)}`);
        }
        const reader = r.body.getReader();
        const decoder = new TextDecoder();
        let buffer = '';
        let gotFinal = false;
        let streamError: string | null = null;
        let facultyFailed = false;
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
              const halt = typeof ev.data['halted_reason'] === 'string' ? ev.data['halted_reason'] : undefined;
              const responseText = String(ev.data['final_response'] ?? '');
              facultyFailed = halt === 'unified_faculty_error';
              if (facultyFailed) continue;
              const finalMsg: ChatMessage = {
                role: 'apocrypha',
                text: responseText || (halt === 'unified_faculty_error'
                  ? 'Apocrypha lost the thread before the thought was complete. Try again.'
                  : 'Apocrypha completed the thought without words.'),
                ts: new Date(),
                toolCalls: Array.isArray(ev.data['tool_calls'])
                  ? (ev.data['tool_calls'] as ToolCallChip[])
                  : [],
                halt,
                elapsed_s: typeof ev.data['elapsed_s'] === 'number' ? ev.data['elapsed_s'] : undefined,
                cost_usd: typeof ev.data['total_cost_usd'] === 'number' ? ev.data['total_cost_usd'] : undefined,
              };
              setMessages((prev) => [...prev, finalMsg]);
              setStreamingTools([]);
            } else if (ev.type === 'error') {
              streamError = String(ev.data['error'] ?? 'stream error');
              setError(streamError);
            }
          }
        }
        // A cold or transient worker can emit a typed faculty error after the SSE
        // connection is healthy. Retry once through the non-streaming path so the
        // public chat does not strand the user on a synthetic empty answer.
        if (facultyFailed && !streamError) {
          const retry = await authFetch('/api/admin/apocrypha/chat', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            credentials: 'include',
            body: JSON.stringify({
              text,
              conversation_id: currentConv,
              max_tokens: 128,
              timeout_s: CHAT_BACKEND_TIMEOUT_S,
            }),
          });
          if (retry.ok) {
            const payload = await retry.json() as { data?: Record<string, unknown> };
            const data = payload.data ?? {};
            const responseText = String(data.final_response ?? '');
            if (responseText) {
              setMessages((prev) => [...prev, {
                role: 'apocrypha', text: responseText, ts: new Date(),
                halt: typeof data.halted_reason === 'string' ? data.halted_reason : undefined,
                elapsed_s: typeof data.elapsed_s === 'number' ? data.elapsed_s : undefined,
              }]);
              facultyFailed = false;
            }
          }
          if (facultyFailed) setError('Apocrypha is still waking its language faculty. Try again shortly.');
        }
        if (!gotFinal && !streamError) {
          setError('Apocrypha lost the thread before the thought was complete. Try again.');
        }
        void loadConvs();
      })(), CHAT_BROWSER_DEADLINE_MS, () => controller.abort());
    } catch (err) {
      setError(err instanceof DeadlineExceededError
        ? 'Apocrypha took too long to answer. Try again.'
        : err instanceof Error ? err.message : String(err));
    } finally {
      setStreaming(false);
    }
  }, [draft, streaming, currentConv, loadConvs]);

  const handleKey = useCallback((e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      void handleSend();
    }
  }, [handleSend]);

  const menuTarget = menu ? convs.find((conversation) => conversation.id === menu.id) : undefined;
  const menuActions: ConversationAction[] = !menuTarget
    ? []
    : scope === 'active'
      ? [menuTarget.pinned ? 'unpin' : 'pin', 'archive', 'trash']
      : scope === 'archived'
        ? ['unarchive', 'trash']
        : ['restore'];

  // ─── render ─────────────────────────────────────────────────

  return (
    <div className="chat-shell" style={{
      display: 'flex',
      height: '100%',
      background: '#0a0a10',
      color: '#e6e6f0',
      fontFamily: 'system-ui, -apple-system, "Segoe UI", Roboto, sans-serif',
    }}>
      {compactViewport && sidebarOpen && (
        <button
          type="button"
          className="chat-sidebar-backdrop"
          aria-label="Close conversations"
          tabIndex={-1}
          onClick={closeCompactSidebar}
        />
      )}
      {/* SIDEBAR */}
      {sidebarOpen && (
        <aside
          id="apocrypha-conversations"
          className="chat-sidebar"
          role={compactViewport ? 'dialog' : undefined}
          aria-modal={compactViewport || undefined}
          aria-label="Conversations"
          onKeyDown={handleSidebarKeyDown}
          style={{
          borderRight: '1px solid #1f1f2a',
          display: 'flex',
          flexDirection: 'column',
          background: 'rgba(15, 15, 22, 0.7)',
          }}
        >
          <div style={{ padding: '0.75rem', borderBottom: '1px solid #1f1f2a' }}>
            <button ref={newChatButtonRef} type="button" className="chat-new-button" onClick={newChat} style={{
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
            <div style={{ display: 'flex', gap: '0.25rem', marginTop: '0.5rem' }}>
              {(['active', 'archived', 'trash'] as ConversationScope[]).map((candidate) => (
                <button
                  key={candidate}
                  type="button"
                  className="chat-scope-button"
                  aria-pressed={scope === candidate}
                  onClick={() => setScope(candidate)}
                  style={{
                  flex: 1, padding: '0.3rem 0.2rem', borderRadius: 5,
                  border: scope === candidate ? '1px solid #8b7cff' : '1px solid #2a2a3a',
                  background: scope === candidate ? 'rgba(139,124,255,.16)' : 'transparent',
                  color: scope === candidate ? '#dcd7ff' : '#7a7a8c',
                  cursor: 'pointer', fontSize: '0.68rem', fontFamily: 'inherit',
                  }}
                >{candidate}</button>
              ))}
            </div>
          </div>
          <div style={{ flex: 1, overflowY: 'auto', padding: '0.4rem' }}>
            {convs.length === 0 && (
              <div style={{ padding: '0.6rem 0.7rem', color: '#7a7a8c', fontSize: '0.8rem' }}>
                no conversations yet
              </div>
            )}
            {convs.map((c) => (
              <div key={c.id} className="chat-conversation-row">
                <button
                  type="button"
                  className="chat-conversation-select"
                  aria-current={c.id === currentConv ? 'true' : undefined}
                  onClick={() => void loadConv(c.id)}
                  onContextMenu={(event) => {
                    event.preventDefault();
                    openConversationMenu(c.id, event.currentTarget, { x: event.clientX, y: event.clientY });
                  }}
                  style={{
                    flex: 1,
                    minWidth: 0,
                    padding: '0.55rem 0.75rem',
                    background: c.id === currentConv ? 'rgba(192, 132, 252, 0.18)' : 'transparent',
                    border: c.id === currentConv ? '1px solid rgba(192, 132, 252, 0.35)' : '1px solid transparent',
                    borderRadius: 6,
                    color: c.id === currentConv ? '#e6e6f0' : '#cdd6e4',
                    textAlign: 'left',
                    cursor: 'pointer',
                    fontSize: '0.85rem',
                    overflow: 'hidden',
                    fontFamily: 'inherit',
                  }}
                >
                  <span style={{ display: 'block', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                    {c.title || `Conversation #${c.id}`}
                  </span>
                  <span style={{ display: 'block', fontSize: '0.7rem', color: MUTED_TEXT, marginTop: 2 }}>
                    {new Date(c.last_active_iso).toLocaleString()}
                  </span>
                </button>
                <button
                  type="button"
                  className="chat-conversation-action"
                  aria-label={`Conversation actions for ${c.title || `Conversation ${c.id}`}`}
                  aria-haspopup="menu"
                  aria-expanded={menu?.id === c.id}
                  aria-controls={menu?.id === c.id ? `conversation-menu-${c.id}` : undefined}
                  onClick={(event) => openConversationMenu(c.id, event.currentTarget)}
                >
                  ⋯
                </button>
              </div>
            ))}
          </div>
          {menu && (
            <div
              id={`conversation-menu-${menu.id}`}
              ref={menuRef}
              role="menu"
              aria-label="Conversation lifecycle actions"
              onKeyDown={handleMenuKeyDown}
              style={{
                position: 'fixed', left: menu.x, top: menu.y, zIndex: 20,
                width: CONVERSATION_MENU_WIDTH, padding: '0.3rem', background: '#181824',
                border: '1px solid #3a3a50', borderRadius: 8,
                boxShadow: '0 10px 30px rgba(0,0,0,.45)',
              }}
            >
              {menuActions.map((action) => (
                <button key={action} type="button" role="menuitem" onClick={() => void mutateConversation(action)} style={{
                  display: 'block', width: '100%', minHeight: 44, padding: '0.45rem 0.6rem',
                  background: 'transparent', border: 0, color: '#d8d8e8',
                  textAlign: 'left', cursor: 'pointer', fontFamily: 'inherit',
                }}>{action}</button>
              ))}
            </div>
          )}
        </aside>
      )}

      {/* MAIN */}
      <div className="chat-main" style={{ flex: 1, display: 'flex', flexDirection: 'column', minWidth: 0 }}>
        {/* HEADER */}
        <header className="chat-header" style={{
          borderBottom: '1px solid #1f1f2a',
          display: 'flex',
          alignItems: 'center',
          fontSize: '0.85rem',
          color: '#9aa0a6',
        }}>
          <button
            ref={sidebarToggleRef}
            type="button"
            className="chat-icon-button"
            onClick={() => setSidebarOpen((open) => !open)}
            aria-label={sidebarOpen ? 'Close conversations' : 'Open conversations'}
            aria-expanded={sidebarOpen}
            aria-controls="apocrypha-conversations"
            style={{
            background: 'transparent',
            border: 0,
            color: '#9aa0a6',
            cursor: 'pointer',
            fontSize: '1.05rem',
            padding: '0.2rem 0.5rem',
            fontFamily: 'inherit',
            }}
            title="Toggle conversations"
          >
            ☰
          </button>
          <span className="chat-wordmark" style={{
            fontWeight: 600,
            backgroundImage: 'linear-gradient(135deg, #ffaa55, #c084fc)',
            WebkitBackgroundClip: 'text',
            WebkitTextFillColor: 'transparent',
          }}>
            Apocrypha
          </span>
          <ApocryphaAvatar className="chat-header-avatar" state={streaming ? 'thinking' : error ? 'degraded' : 'ready'} size={40} detail="compact" />
          <span style={{ flex: 1 }} />
          <button
            type="button"
            className="chat-settings-button"
            onClick={() => setSettingsOpen((open) => !open)}
            aria-expanded={settingsOpen}
            aria-controls="apocrypha-settings"
            style={{
              background: 'transparent', border: '1px solid #2a2a3a',
              borderRadius: 6, color: '#9aa0a6', cursor: 'pointer',
              padding: '0.25rem 0.5rem', fontFamily: 'inherit', fontSize: '0.75rem',
            }}
          >
            settings
          </button>
          <span className="chat-conversation-id" style={{ color: '#7a7a8c', fontSize: '0.75rem' }}>
            {currentConv ? `conv #${currentConv}` : 'new conversation'}
          </span>
        </header>

        {settingsOpen && (
          <section
            id="apocrypha-settings"
            aria-label="Chat settings"
            style={{
              padding: '0.65rem 1rem', borderBottom: '1px solid #1f1f2a',
              background: 'rgba(20, 20, 30, 0.9)', color: '#cdd6e4',
              fontSize: '0.8rem',
            }}
          >
            <label style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', cursor: 'pointer' }}>
              <input type="checkbox" checked={showTrace} onChange={(e) => setShowTrace(e.target.checked)} />
              show tool and run trace
            </label>
            <div style={{ marginTop: '0.35rem', color: '#7a7a8c', fontSize: '0.7rem' }}>
              Presentation only; model, authority, and security policy remain server-controlled.
            </div>
          </section>
        )}

        {/* THREAD */}
        <div
          className="chat-thread-scroll"
          style={{ flex: 1, overflowY: 'auto', padding: '1.5rem 0' }}
        >
          <div style={{ maxWidth: 760, margin: '0 auto', padding: '0 1.2rem' }}>
            <div
              role="log"
              aria-label="Conversation with Apocrypha"
              aria-live="polite"
              aria-relevant="additions text"
              aria-busy={streaming}
            >
              {messages.length === 0 && !streaming && (
                <div style={{
                  color: '#7a7a8c',
                  fontSize: '1rem',
                  textAlign: 'center',
                  marginTop: '1.8rem',
                  display: 'grid',
                  justifyItems: 'center',
                }}>
                  <ApocryphaAvatar state={error ? 'degraded' : 'ready'} size={190} />
                  <div style={{
                    fontSize: '1.8rem',
                    margin: '0.35rem 0 0.6rem',
                    fontWeight: 600,
                    backgroundImage: 'linear-gradient(135deg, #ffaa55, #c084fc)',
                    WebkitBackgroundClip: 'text',
                    WebkitTextFillColor: 'transparent',
                  }}>
                    Apocrypha
                  </div>
                  <div style={{ fontSize: '0.92rem' }}>
                    A private, persistent digital entity with native state continuity and governed faculties.
                  </div>
                  <div style={{ marginTop: '0.5rem', fontSize: '0.8rem', color: MUTED_TEXT }}>
                    Speak naturally. Apocrypha will choose how deeply to think.
                  </div>
                </div>
              )}

              {messages.map((m, i) => (
                <MessageBubble key={i} msg={m} showTrace={showTrace} />
              ))}
            </div>

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
                <div role="status" aria-live="polite" aria-atomic="true" style={{
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
                  <span>Apocrypha is thinking…</span>
                </div>
              </div>
            )}

            {error && (
              <div role="alert" style={{
                marginBottom: '1.5rem',
                padding: '0.65rem 0.9rem',
                background: 'rgba(255, 136, 136, 0.08)',
                border: '1px solid rgba(255, 136, 136, 0.3)',
                borderRadius: 8,
                color: '#ff8888',
                fontSize: '0.88rem',
              }}>
                Apocrypha paused: {error}
              </div>
            )}

            <div ref={messagesEndRef} aria-hidden="true" />
          </div>
        </div>

        {/* COMPOSER */}
        <div className="chat-composer" style={{ borderTop: '1px solid #1f1f2a' }}>
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
                aria-label="Message Apocrypha"
                aria-describedby="apocrypha-composer-help"
                placeholder="Message Apocrypha…"
                rows={1}
                style={{
                  flex: 1,
                  background: 'transparent',
                  color: '#e6e6f0',
                  border: 0,
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
                aria-label="Send message"
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
            <div id="apocrypha-composer-help" style={{
              marginTop: '0.4rem',
              fontSize: '0.7rem',
              color: MUTED_TEXT,
              textAlign: 'center',
            }}>
              Enter to send · Shift+Enter for newline · instruments remain governed by Apocrypha
            </div>
          </div>
        </div>
      </div>
      <style jsx>{`
        .chat-shell {
          position: relative;
          min-width: 0;
          min-height: 0;
          overflow: hidden;
        }
        .chat-sidebar {
          position: relative;
          z-index: 2;
          width: 280px;
          min-width: 280px;
          min-height: 0;
        }
        .chat-sidebar-backdrop { display: none; }
        .chat-main,
        .chat-thread-scroll { min-width: 0; min-height: 0; }
        .chat-header { gap: .6rem; padding: .6rem 1rem; }
        .chat-composer {
          flex: 0 0 auto;
          padding: .9rem 1rem 1.4rem;
        }
        .chat-new-button,
        .chat-scope-button,
        .chat-icon-button,
        .chat-settings-button,
        .chat-conversation-select,
        .chat-conversation-action { min-height: 44px; }
        .chat-icon-button { min-width: 44px; }
        .chat-conversation-row {
          display: flex;
          align-items: stretch;
          gap: 2px;
          width: 100%;
          margin-bottom: 2px;
        }
        .chat-conversation-action {
          flex: 0 0 44px;
          width: 44px;
          padding: 0;
          border: 1px solid transparent;
          border-radius: 6px;
          color: #a9a9bc;
          background: transparent;
          cursor: pointer;
          font-family: inherit;
          font-size: 1.1rem;
          font-weight: 700;
          line-height: 1;
        }
        .chat-conversation-action:hover { background: rgba(192, 132, 252, .1); }
        .chat-shell button:focus-visible,
        .chat-shell textarea:focus-visible,
        .chat-shell input:focus-visible {
          outline: 2px solid #c9b8ff;
          outline-offset: 2px;
        }
        @media (max-width: 767px) {
          .chat-sidebar-backdrop {
            display: block;
            position: fixed;
            inset: 0;
            z-index: 29;
            width: 100%;
            height: 100%;
            padding: 0;
            border: 0;
            background: rgba(0, 0, 8, .64);
            cursor: default;
          }
          .chat-sidebar {
            position: fixed;
            inset: 0 auto 0 0;
            z-index: 30;
            width: min(86vw, 280px);
            min-width: 0;
            max-width: calc(100vw - 44px);
            box-shadow: 18px 0 48px rgba(0, 0, 0, .52);
          }
          .chat-header { gap: .35rem; padding: .45rem .5rem; }
          .chat-conversation-id { display: none; }
          .chat-composer {
            padding:
              .65rem
              max(.6rem, env(safe-area-inset-right))
              calc(.65rem + env(safe-area-inset-bottom))
              max(.6rem, env(safe-area-inset-left));
          }
        }
        @media (max-width: 359px) {
          .chat-wordmark { display: none; }
        }
      `}</style>
    </div>
  );
}

// ─── presentation sub-components ──────────────────────────────────

function MessageBubble({ msg, showTrace }: { msg: ChatMessage; showTrace: boolean }) {
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
        {showTrace && msg.toolCalls && msg.toolCalls.length > 0 && (
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
      {!isUser && showTrace && (msg.halt || msg.elapsed_s != null || msg.cost_usd != null) && (
        <div style={{
          fontSize: '0.68rem',
          color: MUTED_TEXT,
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
      <span className="apocrypha-thinking-dot" style={{
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
        @media (prefers-reduced-motion: reduce) {
          .apocrypha-thinking-dot { animation: none !important; }
        }
      `}</style>
    </>
  );
}
