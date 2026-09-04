import Link from 'next/link';
import { useEffect, useRef, useState } from 'react';
import { useSiteSession } from '@/components/hub/SiteSession';
import { authFetch } from '@/lib/browser-auth';
import { isAccountSessionId, parseAccountSession, parseAccountSessions, parseAccountTurn, type AccountMessage, type AccountSessionSummary } from '@/lib/mobile/chat-contract';
import styles from '@/styles/AccountChat.module.css';
interface PendingTurn { text: string; session_id: string; request_id: string }
class ChatNotice extends Error {}
const key = (subject: string) => `apocky.account-chat.session.v1.${subject}`;
const failure = (status: number) => status === 401 ? 'Please sign in again to continue.' : status === 429 ? 'Please wait a moment before sending again.' : status === 404 ? 'This conversation is no longer available.' : 'Apocrypha could not be reached. Please try again shortly.';

export default function AccountChat(): JSX.Element {
  const { access, authenticated, subjectKey } = useSiteSession();
  const subject = authenticated ? subjectKey : null;
  const subjectRef = useRef(subject); subjectRef.current = subject;
  const generation = useRef(0); const active = useRef<AbortController | null>(null); const end = useRef<HTMLDivElement>(null);
  const [bound, setBound] = useState<string | null>(null);
  const [sessions, setSessions] = useState<AccountSessionSummary[]>([]);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [messages, setMessages] = useState<AccountMessage[]>([]);
  const [draft, setDraft] = useState(''); const [title, setTitle] = useState('New conversation');
  const [historyOpen, setHistoryOpen] = useState(false); const [limited, setLimited] = useState(false); const [truncated, setTruncated] = useState(false);
  const [loading, setLoading] = useState(false); const [sending, setSending] = useState(false);
  const [notice, setNotice] = useState<string | null>(null); const [pending, setPending] = useState<PendingTurn | null>(null);
  const current = Boolean(subject && subject === bound);
  const remember = (id: string, account: string) => { try { localStorage.setItem(key(account), id); } catch { /* ○ storage.unavailable */ } };
  function newChat(account: string): void {
    generation.current += 1; active.current?.abort(); const id = crypto.randomUUID();
    setSessionId(id); setMessages([]); setTitle('New conversation'); setDraft(''); setPending(null); setSending(false); setLoading(false); setNotice(null); setTruncated(false); remember(id, account);
  }
  async function refreshList(account: string): Promise<void> {
    try {
      const response = await authFetch('/api/mobile/sessions', { cache: 'no-store' });
      if (!response.ok) return; const list = parseAccountSessions(await response.json());
      if (subjectRef.current !== account || !list) return;
      setSessions(list.sessions); setLimited(list.discovery_scope === 'latest_conversation_only');
    } catch { /* ○ discovery.degraded ; conversation.preserved */ }
  }
  async function restore(id: string, account: string): Promise<void> {
    const rev = ++generation.current; active.current?.abort(); const controller = new AbortController(); active.current = controller;
    setLoading(true); setSending(false); setMessages([]); setPending(null); setNotice(null); setTruncated(false); setSessionId(null);
    try {
      const response = await authFetch(`/api/mobile/sessions?session_id=${encodeURIComponent(id)}`, { cache: 'no-store', signal: controller.signal });
      if (!response.ok) throw new ChatNotice(failure(response.status));
      const history = parseAccountSession(await response.json(), id); if (!history) throw new ChatNotice('This conversation could not be read safely. Please refresh it.');
      if (subjectRef.current !== account || generation.current !== rev) return;
      setMessages(history.messages); setTitle(history.title); setSessionId(id); setTruncated(history.events_truncated); remember(id, account);
    } catch (error) {
      if (subjectRef.current !== account || generation.current !== rev) return;
      newChat(account); setNotice(error instanceof ChatNotice ? error.message : 'This conversation could not be loaded. Please try again.');
    } finally { if (subjectRef.current === account && generation.current === rev) setLoading(false); }
  }
  useEffect(() => {
    const rev = ++generation.current; active.current?.abort();
    setBound(subject); setMessages([]); setSessions([]); setSessionId(null); setDraft(''); setPending(null); setNotice(null); setLoading(Boolean(subject)); setSending(false); setTitle('New conversation'); setTruncated(false);
    if (!subject) return; const account = subject;
    void (async () => {
      try {
        const response = await authFetch('/api/mobile/sessions', { cache: 'no-store' }); if (!response.ok) throw new ChatNotice(failure(response.status));
        const list = parseAccountSessions(await response.json()); if (!list) throw new ChatNotice('Your conversation list could not be read. Please refresh it.');
        if (subjectRef.current !== account || generation.current !== rev) return;
        setSessions(list.sessions); setLimited(list.discovery_scope === 'latest_conversation_only');
        let saved: string | null = null; try { saved = localStorage.getItem(key(account)); } catch { /* ○ storage.unavailable */ }
        const selected = isAccountSessionId(saved) ? saved : list.sessions[0]?.session_id;
        if (selected) await restore(selected, account); else newChat(account);
      } catch (error) {
        if (subjectRef.current !== account || generation.current !== rev) return;
        newChat(account); setNotice(error instanceof ChatNotice ? error.message : 'Your conversation list is unavailable. Please try again.');
      }
    })();
    return () => { generation.current += 1; active.current?.abort(); };
  }, [subject]);
  useEffect(() => { end.current?.scrollIntoView({ block: 'nearest' }); }, [messages, sending]);
  async function send(retry?: PendingTurn): Promise<void> {
    if (!subject || !current || !sessionId || loading || sending) return;
    const text = retry?.text ?? draft.trim(); if (!text || new TextEncoder().encode(text).length > 16_384) { setNotice('Keep this message under 16 KB, or send it in smaller parts.'); return; }
    const account = subject; const rev = generation.current; const turn = retry ?? { text, session_id: sessionId, request_id: crypto.randomUUID() };
    const controller = new AbortController(); active.current = controller; setSending(true); setNotice(null); setPending(turn);
    if (!retry) { setDraft(''); setMessages(previous => [...previous, { role: 'user', content: text, request_id: turn.request_id, recorded_at: new Date().toISOString() }]); if (!messages.length) setTitle(text.slice(0, 80)); }
    try {
      const response = await authFetch('/api/mobile/turn', { method: 'POST', headers: { 'Content-Type': 'application/json', Accept: 'application/json' }, body: JSON.stringify(turn), signal: controller.signal, cache: 'no-store' });
      if (!response.ok) throw new ChatNotice(failure(response.status)); const result = parseAccountTurn(await response.json(), turn.session_id, turn.request_id);
      if (!result) throw new ChatNotice('The reply could not be confirmed. Refresh this conversation before trying again.');
      if (subjectRef.current !== account || generation.current !== rev) return;
      setMessages(previous => [...previous.filter(message => !(message.role === 'assistant' && message.request_id === turn.request_id)), { role: 'assistant', content: result.text, request_id: turn.request_id, recorded_at: new Date().toISOString() }]);
      setPending(null); remember(turn.session_id, account); void refreshList(account);
    } catch (error) {
      if (subjectRef.current !== account || generation.current !== rev) return;
      setNotice(error instanceof Error && error.name === 'AbortError' ? 'Stopped waiting. Apocrypha may still finish; refresh the conversation to check.' : error instanceof ChatNotice ? error.message : 'The reply could not be confirmed. Please try again.');
    } finally { if (subjectRef.current === account && generation.current === rev) setSending(false); }
  }
  async function copy(content: string): Promise<void> { try { await navigator.clipboard.writeText(content); setNotice('Copied.'); } catch { setNotice('Copy is unavailable. You can select and copy the message text.'); } }
  return <main id="main-content" className={styles.page}>
    <header className={styles.header}><Link href="/" className={styles.brand}><span className="apx-brand-mark" aria-hidden="true" /><span>Apocrypha</span></Link><nav aria-label="Apocrypha navigation"><Link href="/download/apocrypha">Get the app</Link><Link href={authenticated ? '/account' : '/login?next=%2Fapocrypha'}>{authenticated ? 'Account' : 'Sign in'}</Link>{access === 'owner' ? <Link href="/brain">Private Brain</Link> : null}</nav></header>
    {!authenticated || !subject ? <section className={styles.welcome} aria-labelledby="welcome-title"><span className={styles.eyebrow}>APOCRYPHA</span><h1 id="welcome-title">A conversation.<br /><em>Room to think.</em></h1><p>Ask a question, explore an idea, or pick up where you left off. Sign in to keep your conversations together.</p>{access === 'checking' ? <p role="status">Checking your account…</p> : <div className={styles.welcomeActions}><Link href="/login?next=%2Fapocrypha" className={styles.primary}>Sign in to chat</Link><Link href="/register?next=%2Fapocrypha" className={styles.secondary}>Create an account</Link></div>}{access === 'unavailable' ? <p role="status">Account verification is temporarily unavailable. Please try signing in again.</p> : null}<Link className={styles.phoneLink} href="/download/apocrypha">Apocrypha for iPhone and Android →</Link></section>
    : !current ? <section className={styles.welcome} role="status"><p>Opening your conversations…</p></section>
    : <div className={`${styles.workspace} ${historyOpen ? styles.withHistory : ''}`}>
      {historyOpen ? <aside className={styles.history} aria-label="Conversation history"><h2>Your conversations</h2>{limited ? <p>Only the latest conversation is available in this list.</p> : null}{sessions.length === 0 ? <p>Your conversations will appear here.</p> : sessions.map(session => <button key={session.session_id} type="button" aria-current={session.session_id === sessionId ? 'true' : undefined} onClick={() => { void restore(session.session_id, subject); setHistoryOpen(false); }}><strong>{session.title}</strong><span>{session.message_count} messages</span></button>)}</aside> : null}
      <section className={styles.conversation} aria-label="Apocrypha conversation"><div className={styles.toolbar}><button type="button" onClick={() => setHistoryOpen(!historyOpen)} aria-expanded={historyOpen}>History</button><h1>{current ? title : 'Your conversation'}</h1><button type="button" onClick={() => newChat(subject)}>New chat</button></div>
        <div className={styles.messages} role="log" aria-label="Messages" aria-live={loading ? 'off' : 'polite'} aria-busy={loading || sending}>{loading ? <p className={styles.empty}>Loading your conversation…</p> : current && !messages.length ? <div className={styles.empty}><span className="apx-brand-mark" aria-hidden="true" /><h2>What’s on your mind?</h2><p>Start anywhere.</p></div> : null}{current && truncated ? <p className={styles.historyNotice}>Showing the most recent messages available from this conversation.</p> : null}{current ? messages.map((message, index) => <article key={`${message.request_id}-${message.role}-${index}`} className={`${styles.message} ${message.role === 'user' ? styles.user : styles.assistant}`}><div className={styles.messageMeta}><strong>{message.role === 'user' ? 'You' : 'Apocrypha'}</strong><button type="button" onClick={() => { void copy(message.content); }} aria-label={`Copy ${message.role === 'user' ? 'your message' : 'Apocrypha reply'}`}>Copy</button></div><p>{message.content}</p></article>) : null}{sending ? <p className={styles.waiting} role="status">Apocrypha is responding…</p> : null}<div ref={end} /></div>
        <div className={styles.composer}>{notice ? <div className={styles.notice} role="status"><span>{notice}</span>{current && sessionId && !sending ? <button type="button" onClick={() => { void restore(sessionId, subject); }}>Refresh conversation</button> : null}</div> : null}{pending && !sending ? <div className={styles.retry}><span>Your last message is awaiting a confirmed reply.</span><button type="button" onClick={() => { void send(pending); }}>Retry same message</button></div> : null}<form onSubmit={event => { event.preventDefault(); if (!pending) void send(); }}><label className={styles.srOnly} htmlFor="apocrypha-message">Message Apocrypha</label><textarea id="apocrypha-message" value={draft} onChange={event => setDraft(event.target.value)} placeholder="Message Apocrypha…" rows={2} disabled={!current || loading || Boolean(pending)} onKeyDown={event => { if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) { event.preventDefault(); if (!pending) void send(); } }} />{sending ? <button className={styles.send} type="button" onClick={() => active.current?.abort()}>Stop waiting</button> : <button className={styles.send} type="submit" disabled={!current || loading || !draft.trim() || Boolean(pending)}>Send <span aria-hidden="true">↑</span></button>}</form><p className={styles.caption}>Your history belongs to your account. Apocrypha can make mistakes.</p></div>
      </section>
    </div>}
  </main>;
}
