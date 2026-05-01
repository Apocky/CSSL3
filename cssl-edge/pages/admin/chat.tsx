// /admin/chat · phone-first chat interface to your desktop
// Routes to LoA.exe (MCP-server :3001) OR Mycelium-Desktop agent-loop via Supabase-realtime relay
// Phone posts to /api/admin/bridge · desktop polls + responds · phone renders streamed tokens

import type { NextPage } from 'next';
import { useEffect, useRef, useState } from 'react';
import AdminLayout from '../../components/AdminLayout';

type Role = 'you' | 'gm' | 'dm' | 'coder' | 'system';

interface Msg {
  id: string;
  role: Role;
  text: string;
  at: number; // ms epoch
  pending?: boolean;
}

const ROLE_COLOR: Record<Role, string> = {
  you: '#ffffff',
  gm: '#7dd3fc',
  dm: '#a78bfa',
  coder: '#fbbf24',
  system: '#7a7a8c',
};

const ROLE_LABEL: Record<Role, string> = {
  you: 'YOU',
  gm: 'GM',
  dm: 'DM',
  coder: 'CODER',
  system: 'SYSTEM',
};

interface BridgeStatus {
  online: boolean;
  desktop?: 'loa-exe' | 'mycelium-desktop' | 'none';
  last_heartbeat_ms?: number;
  stub?: boolean;
}

const Chat: NextPage = () => {
  const [messages, setMessages] = useState<Msg[]>([]);
  const [input, setInput] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [status, setStatus] = useState<BridgeStatus | null>(null);
  const [target, setTarget] = useState<'gm' | 'dm' | 'coder'>('gm');
  const scrollRef = useRef<HTMLDivElement>(null);

  // Poll bridge status every 5s
  useEffect(() => {
    const fetchStatus = () => {
      fetch('/api/admin/bridge?action=status')
        .then((r) => r.json())
        .then((j: BridgeStatus) => setStatus(j))
        .catch(() => setStatus({ online: false, stub: true }));
    };
    fetchStatus();
    const t = setInterval(fetchStatus, 5000);
    return () => clearInterval(t);
  }, []);

  // Auto-scroll on new message
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [messages]);

  async function send(e: React.FormEvent) {
    e.preventDefault();
    if (!input.trim() || submitting) return;
    setSubmitting(true);
    const yourMsg: Msg = {
      id: `y-${Date.now()}`,
      role: 'you',
      text: input.trim(),
      at: Date.now(),
    };
    const pendingId = `r-${Date.now()}`;
    const pendingResponse: Msg = {
      id: pendingId,
      role: target,
      text: '…',
      at: Date.now(),
      pending: true,
    };
    setMessages((prev) => [...prev, yourMsg, pendingResponse]);
    setInput('');

    try {
      const res = await fetch('/api/admin/bridge?action=send', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          target,
          text: yourMsg.text,
        }),
      });
      const json = await res.json();
      const responseText: string = typeof json.response === 'string'
        ? json.response
        : json.stub
          ? '⚠ stub-mode · bridge to desktop pending APOCKY_HUB_SUPABASE_URL configuration. Once your Apocky-Hub Supabase project is set up, your phone-prompts will route via Realtime channel to LoA.exe (when running) OR Mycelium-Desktop (when running) and stream back here.'
          : '✗ bridge error';
      setMessages((prev) =>
        prev.map((m) =>
          m.id === pendingId
            ? { ...m, text: responseText, pending: false, role: (json.role as Role) ?? target }
            : m,
        ),
      );
    } catch (err) {
      setMessages((prev) =>
        prev.map((m) =>
          m.id === pendingId
            ? { ...m, text: '✗ network error · is your desktop online?', pending: false, role: 'system' }
            : m,
        ),
      );
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <AdminLayout title="✶ Chat → Desktop">
      <p style={{ color: '#7a7a8c', fontSize: '0.82rem', marginTop: 0, marginBottom: '1rem' }}>
        Talk to your local intelligence (LoA.exe GM/DM/Coder · or Mycelium-Desktop agent) from your phone via
        Supabase-Realtime relay. End-to-end · sovereign-cap-revoke-anytime · ¬ surveillance.
      </p>

      {/* DESKTOP STATUS PILL */}
      <div
        style={{
          padding: '0.75rem 1rem',
          background: status?.online
            ? 'rgba(52, 211, 153, 0.1)'
            : status?.stub
              ? 'rgba(251, 191, 36, 0.1)'
              : 'rgba(248, 113, 113, 0.08)',
          border: `1px solid ${
            status?.online
              ? 'rgba(52, 211, 153, 0.3)'
              : status?.stub
                ? 'rgba(251, 191, 36, 0.4)'
                : 'rgba(248, 113, 113, 0.3)'
          }`,
          borderRadius: 6,
          marginBottom: '1rem',
          fontSize: '0.82rem',
          color: status?.online ? '#34d399' : status?.stub ? '#fbbf24' : '#f87171',
        }}
      >
        <strong>
          {status?.online
            ? `✓ desktop online (${status.desktop ?? 'unknown'})`
            : status?.stub
              ? '⚠ bridge in stub-mode'
              : '✗ desktop offline'}
        </strong>
        {!status?.online && (
          <div style={{ marginTop: 4, fontSize: '0.78rem', color: '#a8a8b8' }}>
            {status?.stub
              ? 'Apocky-Hub Supabase not yet configured. Bridge will activate once realtime channel is wired.'
              : 'Run LoA.exe or Mycelium-Desktop on your machine to bring the bridge online.'}
          </div>
        )}
      </div>

      {/* TARGET PICKER */}
      <div style={{ display: 'flex', gap: '0.4rem', marginBottom: '1rem' }}>
        {(['gm', 'dm', 'coder'] as const).map((t) => (
          <button
            key={t}
            type="button"
            onClick={() => setTarget(t)}
            style={{
              flex: 1,
              padding: '0.6rem 0.5rem',
              background: target === t ? 'rgba(124, 211, 252, 0.15)' : 'rgba(20, 20, 30, 0.4)',
              border: `1px solid ${target === t ? 'rgba(124, 211, 252, 0.5)' : '#1f1f2a'}`,
              borderRadius: 4,
              color: target === t ? '#7dd3fc' : '#cdd6e4',
              fontSize: '0.82rem',
              cursor: 'pointer',
              minHeight: 44,
              textTransform: 'uppercase',
              letterSpacing: '0.1em',
              fontFamily: 'inherit',
            }}
          >
            /{t}
          </button>
        ))}
      </div>

      {/* CHAT WINDOW */}
      <div
        ref={scrollRef}
        style={{
          background: 'rgba(10, 10, 16, 0.4)',
          border: '1px solid #1f1f2a',
          borderRadius: 6,
          padding: '0.75rem',
          minHeight: 320,
          maxHeight: 'calc(100dvh - 380px)',
          overflowY: 'auto',
          marginBottom: '0.75rem',
          fontSize: '0.85rem',
          lineHeight: 1.5,
        }}
      >
        {messages.length === 0 && (
          <p style={{ color: '#5a5a6a', textAlign: 'center', margin: '2rem 0' }}>
            § no messages yet · type below to chat with your /{target}
          </p>
        )}
        {messages.map((m) => (
          <div key={m.id} style={{ marginBottom: '0.85rem' }}>
            <div style={{ fontSize: '0.65rem', letterSpacing: '0.1em', color: ROLE_COLOR[m.role], marginBottom: 2 }}>
              {ROLE_LABEL[m.role]}{m.pending ? ' · pending' : ''}
            </div>
            <div style={{ color: '#cdd6e4', whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>
              {m.text}
            </div>
          </div>
        ))}
      </div>

      {/* INPUT FORM */}
      <form onSubmit={send} style={{ display: 'flex', gap: '0.5rem' }}>
        <input
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder={`Message your /${target}…`}
          disabled={submitting}
          autoComplete="off"
          style={{
            flex: 1,
            padding: '0.75rem 0.85rem',
            background: 'rgba(20, 20, 30, 0.7)',
            border: '1px solid #2a2a3a',
            borderRadius: 4,
            color: '#e6e6f0',
            fontSize: '1rem',
            outline: 'none',
            minHeight: 44,
          }}
        />
        <button
          type="submit"
          disabled={submitting || !input.trim()}
          style={{
            padding: '0 1.1rem',
            background: 'linear-gradient(135deg, #c084fc 0%, #7dd3fc 100%)',
            color: '#0a0a0f',
            fontWeight: 700,
            border: 'none',
            borderRadius: 4,
            cursor: submitting || !input.trim() ? 'not-allowed' : 'pointer',
            opacity: submitting || !input.trim() ? 0.5 : 1,
            fontSize: '0.92rem',
            minHeight: 44,
            minWidth: 60,
            fontFamily: 'inherit',
          }}
        >
          →
        </button>
      </form>
    </AdminLayout>
  );
};

export default Chat;
