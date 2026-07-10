import Head from 'next/head';
import Link from 'next/link';
import { useEffect, useRef, useState } from 'react';

import { getAuthClient, getCurrentUser } from '../lib/auth';
import { authFetch } from '../lib/browser-auth';

type Msg = { role: 'user' | 'apocrypha' | 'err'; text: string };
type TurnRow = { status: string; response: string; error: string | null };
type ChunkRow = { seq: number; delta: string };

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export default function ChatPage() {
  const [authState, setAuthState] = useState<'checking' | 'in' | 'out'>('checking');
  const [msgs, setMsgs] = useState<Msg[]>([]);
  const [live, setLive] = useState<string | null>(null); // in-flight assistant reply (streaming)
  const [resting, setResting] = useState(false);
  const [busy, setBusy] = useState(false);
  const [input, setInput] = useState('');
  const [sessionId, setSessionId] = useState<string | null>(null);
  const lastSeenRef = useRef<string>(new Date().toISOString());
  const logRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    (async () => {
      try {
        const u = await getCurrentUser();
        setAuthState(u ? 'in' : 'out');
      } catch {
        setAuthState('out');
      }
    })();
  }, []);

  useEffect(() => {
    if (logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight;
  }, [msgs, live]);

  // Apocrypha can speak unprompted — poll the user's current session for any kind='mind' turns
  // that have arrived since we last looked, and render them as if they came from the chat partner.
  useEffect(() => {
    if (authState !== 'in' || !sessionId) return;
    const sb = getAuthClient();
    if (!sb) return;
    let cancelled = false;
    const tick = async () => {
      if (cancelled) return;
      const { data } = await sb
        .from('chat_turn')
        .select('id,response,created_at')
        .eq('session_id', sessionId)
        .eq('kind', 'mind')
        .gt('created_at', lastSeenRef.current)
        .order('created_at', { ascending: true });
      const rows = (data ?? []) as { id: string; response: string; created_at: string }[];
      if (rows.length > 0) {
        setMsgs((m) => [...m, ...rows.map((r) => ({ role: 'apocrypha' as const, text: (r.response ?? '').trim() || '…' }))]);
        const last = rows[rows.length - 1];
        if (last) lastSeenRef.current = last.created_at;
      }
    };
    const handle = setInterval(tick, 4000);
    return () => { cancelled = true; clearInterval(handle); };
  }, [authState, sessionId]);

  async function streamTurn(turnId: string): Promise<void> {
    const sb = getAuthClient();
    if (!sb) throw new Error('Auth is not configured for this deployment.');
    const t0 = Date.now();
    for (;;) {
      await sleep(700);
      const { data: chunkData } = await sb
        .from('chat_chunk')
        .select('seq,delta')
        .eq('turn_id', turnId)
        .order('seq', { ascending: true });
      const chunks = (chunkData ?? []) as ChunkRow[];
      if (chunks.length > 0) {
        setResting(false);
        setLive(chunks.map((c) => c.delta).join(''));
      }
      const { data: turnData } = await sb
        .from('chat_turn')
        .select('status,response,error')
        .eq('id', turnId)
        .maybeSingle();
      const turn = (turnData ?? null) as TurnRow | null;
      const status = turn?.status;
      if (status === 'streaming' || status === 'leased') setResting(false);
      if (status === 'done') {
        const finalText = (turn?.response ?? '').trim() || '…';
        setMsgs((m) => [...m, { role: 'apocrypha', text: finalText }]);
        setLive(null);
        return;
      }
      if (status === 'failed') {
        setMsgs((m) => [...m, { role: 'err', text: `The mind could not answer: ${turn?.error ?? 'unknown error'}` }]);
        setLive(null);
        return;
      }
      if (status === 'queued' && Date.now() - t0 > 8000) setResting(true);
      if (Date.now() - t0 > 180000) {
        setMsgs((m) => [...m, { role: 'err', text: 'Timed out waiting for the mind. Try again in a moment.' }]);
        setLive(null);
        return;
      }
    }
  }

  async function send() {
    const prompt = input.trim();
    if (!prompt || busy) return;
    setInput('');
    setMsgs((m) => [...m, { role: 'user', text: prompt }]);
    setLive('');
    setBusy(true);
    try {
      const body: Record<string, unknown> = { prompt };
      if (sessionId) body['session_id'] = sessionId;
      const res = await authFetch('/api/chat/send', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      });
      if (res.status === 401) {
        setAuthState('out');
        throw new Error('Please sign in to talk with Apocrypha.');
      }
      const data = (await res.json()) as { ok?: boolean; turn_id?: string; session_id?: string; error?: string; reason?: string };
      if (!data.ok || !data.turn_id) {
        const msg = data.error === 'rate_limited' ? 'Slow down a moment — you have hit the rate limit.' : data.reason ?? data.error ?? 'Send failed.';
        throw new Error(msg);
      }
      if (data.session_id) setSessionId(data.session_id);
      await streamTurn(data.turn_id);
    } catch (e) {
      setMsgs((m) => [...m, { role: 'err', text: e instanceof Error ? e.message : String(e) }]);
      setLive(null);
    } finally {
      setBusy(false);
      setResting(false);
    }
  }

  return (
    <>
      <Head>
        <title>Talk with Apocrypha</title>
        <meta name="description" content="Converse with Apocrypha — your own instanced sub-mind that remembers you and learns, sovereign and local." />
      </Head>
      <main style={S.main}>
        <section style={S.panel}>
          <header style={S.header}>
            <Link href="/" style={S.home}>← apocky.com</Link>
            <h1 style={S.h1}>Apocrypha</h1>
            <span style={S.sub}>your own sub-mind · it remembers you · sovereign &amp; local</span>
          </header>

          {authState === 'checking' && <div style={S.log}><div style={S.note}>Checking your session…</div></div>}

          {authState === 'out' && (
            <div style={S.log}>
              <div style={S.gate}>
                <p style={{ margin: '0 0 12px' }}>Apocrypha gives every signed-in person their <strong>own</strong> instanced sub-mind — it learns you, remembers across sessions, and never mixes with anyone else.</p>
                <a href="/login" style={S.signin}>Sign in to begin</a>
              </div>
            </div>
          )}

          {authState === 'in' && (
            <>
              <div id="log" ref={logRef} style={S.log}>
                {msgs.length === 0 && live === null && (
                  <div style={S.note}>This is your private thread with Apocrypha. Say something — it will remember.</div>
                )}
                {msgs.map((m, i) => (
                  <div key={i} style={{ ...S.msg, ...bubble(m.role) }}>{m.text}</div>
                ))}
                {live !== null && (
                  <div style={{ ...S.msg, ...bubble('apocrypha') }}>{live === '' ? 'thinking…' : live}</div>
                )}
                {resting && (
                  <div style={S.note}>Apocrypha is resting — it only wakes when its machine is online. Your message is queued and will be answered when it stirs.</div>
                )}
              </div>
              <form
                style={S.form}
                onSubmit={(e) => {
                  e.preventDefault();
                  void send();
                }}
              >
                <textarea
                  style={S.textarea}
                  value={input}
                  disabled={busy}
                  placeholder="Message Apocrypha…  (Enter to send)"
                  onChange={(e) => setInput(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' && !e.shiftKey) {
                      e.preventDefault();
                      void send();
                    }
                  }}
                />
                <button type="submit" style={S.send} disabled={busy || input.trim().length === 0}>
                  {busy ? '…' : 'Send'}
                </button>
              </form>
            </>
          )}
        </section>
      </main>
    </>
  );
}

function bubble(role: Msg['role']): React.CSSProperties {
  if (role === 'user') return { alignSelf: 'flex-end', background: '#155e57', color: '#eafffb' };
  if (role === 'err') return { alignSelf: 'flex-start', background: '#2a1416', border: '1px solid #5b2327', color: '#ffb4b4' };
  return { alignSelf: 'flex-start', background: '#121922', border: '1px solid #18212a' };
}

const S: Record<string, React.CSSProperties> = {
  main: { display: 'flex', justifyContent: 'center', height: '100vh', background: '#06080a', color: '#e6f0ef', fontFamily: 'ui-sans-serif, system-ui, "Segoe UI", sans-serif' },
  home: { display: 'inline-block', color: '#7fb3ad', textDecoration: 'none', fontSize: 12, letterSpacing: '0.08em', marginBottom: 8 },
  panel: { flex: '1 1 auto', maxWidth: 880, width: '100%', display: 'flex', flexDirection: 'column', background: 'rgba(14,19,24,0.72)', backdropFilter: 'blur(7px)', borderLeft: '1px solid #18212a', borderRight: '1px solid #18212a' },
  header: { padding: '14px 18px', borderBottom: '1px solid #18212a' },
  h1: { margin: '4px 0 2px', fontSize: 18, fontWeight: 600, letterSpacing: '.4px' },
  sub: { fontSize: 11, color: '#6b7d80' },
  log: { flex: 1, overflowY: 'auto', padding: 16, display: 'flex', flexDirection: 'column', gap: 12 },
  msg: { maxWidth: '88%', padding: '9px 12px', borderRadius: 12, lineHeight: 1.45, whiteSpace: 'pre-wrap', wordWrap: 'break-word', fontSize: 13.5 },
  note: { color: '#6b7d80', fontStyle: 'italic', fontSize: 12.5, lineHeight: 1.5 },
  gate: { color: '#cfe7e3', fontSize: 14, lineHeight: 1.55 },
  signin: { display: 'inline-block', padding: '9px 16px', borderRadius: 10, background: '#2fd6c6', color: '#04110f', fontWeight: 700, textDecoration: 'none' },
  form: { display: 'flex', gap: 8, padding: '11px 12px', borderTop: '1px solid #18212a' },
  textarea: { flex: 1, resize: 'none', height: 44, padding: '10px 12px', borderRadius: 10, background: '#0a0f13', border: '1px solid #232c34', color: '#e6f0ef', font: 'inherit', fontSize: 13.5 },
  send: { padding: '0 16px', border: 0, borderRadius: 10, background: '#2fd6c6', color: '#04110f', fontWeight: 700, cursor: 'pointer' },
};
