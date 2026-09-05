import Link from 'next/link';
import ConversationMessageContent from '@/components/apocrypha/ConversationMessageContent';
import { useEffect, useRef, useState } from 'react';
import { useSiteSession } from '@/components/hub/SiteSession';
import { useToast } from '@/components/ui/Feedback';
import { authFetch } from '@/lib/browser-auth';
import { getAuthClient } from '@/lib/auth';
import { isAccountSessionId, parseAccountSession, parseAccountSessions, createAccountBoundTurnFetcher, accountHistoryCompletes, accountHistoryWithPending, openAccountPendingJournal, AccountTurnFailure, type AccountPendingJournal, type AccountPendingTurn, type AccountMessage, type AccountSessionSummary } from '@/lib/mobile/chat-contract';
import { accountDiagnostic, accountDiagnosticReason, accountDiagnosticText, diagnosticForAccount, readAccountDiagnostic, type AccountDiagnostic, type AccountOperation } from '@/lib/mobile/diagnostics';
import styles from '@/styles/AccountChat.module.css';
type PendingTurn = AccountPendingTurn;
class ChatNotice extends Error { constructor(message: string, readonly diagnostic?: AccountDiagnostic) { super(message); } }
const key = (subject: string) => `apocky.account-chat.session.v1.${subject}`;
async function failure(response: Response, operation: AccountOperation): Promise<ChatNotice> { const diagnostic = await readAccountDiagnostic(response, operation); return new ChatNotice(accountDiagnosticReason(diagnostic), diagnostic); }
function invalidResponse(response: Response, operation: AccountOperation): ChatNotice { const diagnostic = accountDiagnostic({ operation, status: response.status, code: operation === 'turn' ? 'ACCOUNT_TURN_UNVERIFIED' : 'ACCOUNT_HISTORY_UNVERIFIED', trace_id: response.headers.get('x-apocky-trace-id') }); return new ChatNotice(accountDiagnosticReason(diagnostic), diagnostic); }

export default function AccountChat({ onPendingChange }: { onPendingChange?: (pending: boolean) => void } = {}): JSX.Element {
  const toast = useToast();
  const { access, authenticated, subjectKey } = useSiteSession();
  const subject = authenticated ? subjectKey : null;
  const subjectRef = useRef(subject); subjectRef.current = subject;
  const journal = useRef<Promise<AccountPendingJournal> | null>(null);
  const pendingRef = useRef<PendingTurn | null>(null);
  const pendingStore = () => { if (!journal.current) journal.current = openAccountPendingJournal(); return journal.current; };
  const generation = useRef(0); const active = useRef<AbortController | null>(null); const end = useRef<HTMLDivElement>(null);
  const [bound, setBound] = useState<string | null>(null);
  const [sessions, setSessions] = useState<AccountSessionSummary[]>([]);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [messages, setMessages] = useState<AccountMessage[]>([]);
  const [draft, setDraft] = useState(''); const [title, setTitle] = useState('New conversation');
  const [historyOpen, setHistoryOpen] = useState(false); const [limited, setLimited] = useState(false); const [truncated, setTruncated] = useState(false);
  const [loading, setLoading] = useState(false); const [sending, setSending] = useState(false);
  const [notice, setNotice] = useState<string | null>(null); const [pending, setPending] = useState<PendingTurn | null>(null);
  const [diagnosticBinding, setDiagnosticBinding] = useState<{ account: string; value: AccountDiagnostic } | null>(null);
  const [checkingConnection, setCheckingConnection] = useState(false); const [detailsCopied, setDetailsCopied] = useState(false);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const retainPending = (turn: PendingTurn | null) => { pendingRef.current = turn; setPending(turn); onPendingChange?.(turn !== null); };
  const current = Boolean(subject && subject === bound);
  const diagnostic = diagnosticForAccount(diagnosticBinding, subject);
  function recordFailure(error: unknown, account: string, operation: AccountOperation): void {
    const value = (error instanceof ChatNotice || error instanceof AccountTurnFailure) && error.diagnostic ? error.diagnostic : accountDiagnostic({ operation, status: 0,
      code: error instanceof Error && error.name === 'AbortError' ? 'ACCOUNT_WAIT_STOPPED' : 'ACCOUNT_SERVICE_UNAVAILABLE' });
    setDiagnosticBinding({ account, value }); setDetailsCopied(false);
  }
  const remember = (id: string, account: string) => { try { localStorage.setItem(key(account), id); } catch { /* ○ storage.unavailable */ } };
  function newChat(account: string): void {
    if (sending) return;
    if (pendingRef.current) { setNotice('Your saved message is still awaiting a reply. Refresh or retry it before starting another conversation.'); return; }
    generation.current += 1; active.current?.abort(); const id = crypto.randomUUID();
    setDiagnosticBinding(null); setDetailsCopied(false); setCheckingConnection(false);
    setSessionId(id); setMessages([]); setTitle('New conversation'); setDraft(''); setPending(null); setSending(false); setLoading(false); setNotice(null); setTruncated(false); remember(id, account);
  }
  async function refreshList(account: string): Promise<void> {
    const rev = generation.current;
    try {
      const response = await authFetch('/api/mobile/sessions', { cache: 'no-store' });
      if (!response.ok) throw await failure(response, 'sessions'); const list = parseAccountSessions(await response.json());
      if (!list) throw invalidResponse(response, 'sessions');
      if (subjectRef.current !== account || generation.current !== rev) return;
      setSessions(list.sessions); setLimited(list.discovery_scope === 'latest_conversation_only');
    } catch (error) { if (subjectRef.current === account && generation.current === rev) recordFailure(error, account, 'sessions'); }
  }
  async function restore(id: string, account: string): Promise<void> {
    if (pendingRef.current && pendingRef.current.session_id !== id) { setNotice('Your saved message is still awaiting a reply. Open its conversation before switching.'); return; }
    const rev = ++generation.current; active.current?.abort(); const controller = new AbortController(); active.current = controller;
    setLoading(true); setSending(false); setNotice(null);
    try {
      const saved = await (await pendingStore()).load(account);
      if (subjectRef.current !== account || generation.current !== rev) return;
      if (saved && saved.session_id !== id) { retainPending(saved); setSessionId(saved.session_id); setMessages(accountHistoryWithPending({ session_id: saved.session_id, title: saved.text.slice(0, 80), messages: [], events_truncated: false }, saved)); setNotice('A saved message in another conversation is still awaiting a reply.'); return; }
      const waiting = saved ?? pendingRef.current;
      if (waiting) retainPending(waiting);
      const response = await authFetch('/api/mobile/sessions?session_id=' + encodeURIComponent(id), { cache: 'no-store', signal: controller.signal });
      if (!response.ok) throw await failure(response, 'sessions');
      const history = parseAccountSession(await response.json(), id); if (!history) throw invalidResponse(response, 'sessions');
      if (subjectRef.current !== account || generation.current !== rev) return;
      const completed = Boolean(waiting && accountHistoryCompletes(history, waiting));
      if (completed && waiting) await (await pendingStore()).resolve(account, waiting);
      if (subjectRef.current !== account || generation.current !== rev) return;
      retainPending(completed ? null : waiting);
      setMessages(accountHistoryWithPending(history, completed ? null : waiting)); setTitle(history.title); setSessionId(id);
      setTruncated(history.events_truncated); remember(id, account);
      if (!completed && waiting) setNotice('Your saved message is still awaiting a confirmed reply. Retry keeps the same message and request.');
      else { setDiagnosticBinding(null); setDetailsCopied(false); }
    } catch (error) {
      if (subjectRef.current !== account || generation.current !== rev) return;
      recordFailure(error, account, 'sessions'); setNotice(error instanceof ChatNotice || error instanceof AccountTurnFailure ? error.message : 'This conversation could not be loaded. Your saved message is preserved.');
    } finally { if (subjectRef.current === account && generation.current === rev) setLoading(false); }
  }
  useEffect(() => {
    const rev = ++generation.current; active.current?.abort();
    setDetailsOpen(false);
    setDiagnosticBinding(null); setDetailsCopied(false); setCheckingConnection(false);
    setBound(subject); setMessages([]); setSessions([]); setSessionId(null); setDraft(''); retainPending(null); setNotice(null); setLoading(Boolean(subject)); setSending(false); setTitle('New conversation'); setTruncated(false);
    if (!subject) return; const account = subject;
    void (async () => {
      try {
        const waiting = await (await pendingStore()).load(account);
        if (subjectRef.current !== account || generation.current !== rev) return;
        if (waiting) { retainPending(waiting); setSessionId(waiting.session_id); setTitle(waiting.text.slice(0, 80));
          setMessages(accountHistoryWithPending({ session_id: waiting.session_id, title: '', messages: [], events_truncated: false }, waiting)); }
        const response = await authFetch('/api/mobile/sessions', { cache: 'no-store' }); if (!response.ok) throw await failure(response, 'sessions');
        const list = parseAccountSessions(await response.json()); if (!list) throw invalidResponse(response, 'sessions');
        if (subjectRef.current !== account || generation.current !== rev) return;
        setSessions(list.sessions); setLimited(list.discovery_scope === 'latest_conversation_only');
        let saved: string | null = null; try { saved = localStorage.getItem(key(account)); } catch { /* ○ storage.unavailable */ }
        const selected = waiting?.session_id ?? (isAccountSessionId(saved) ? saved : list.sessions[0]?.session_id);
        if (selected) { setSessionId(selected); await restore(selected, account); } else newChat(account);
      } catch (error) {
        if (subjectRef.current !== account || generation.current !== rev) return;
        setLoading(false); recordFailure(error, account, 'sessions'); setNotice(error instanceof ChatNotice || error instanceof AccountTurnFailure ? error.message : 'Your conversation list is unavailable. Your saved message is preserved.');
      }
    })();
    return () => { generation.current += 1; active.current?.abort(); };
  }, [subject]);
  useEffect(() => { end.current?.scrollIntoView({ block: 'nearest' }); }, [messages, sending]);
  async function send(retry?: PendingTurn): Promise<void> {
    if (!subject || !current || !sessionId || loading || sending) return;
    if ((pendingRef.current && !retry) || (retry && retry.session_id !== sessionId)) return;
    const text = retry?.text ?? draft.trim(); if (!text || new TextEncoder().encode(text).length > 16_384) { setNotice('Keep this message under 16 KB, or send it in smaller parts.'); return; }
    const account = subject; const rev = generation.current; const turn = retry ?? { text, session_id: sessionId, request_id: crypto.randomUUID() };
    const controller = new AbortController(); active.current = controller; setSending(true); setNotice(null);
    try {
      const store = await pendingStore(); await store.save(account, turn);
      if (subjectRef.current !== account || generation.current !== rev || controller.signal.aborted) return;
      retainPending(turn); remember(turn.session_id, account);
      if (!retry) { setDraft(''); setMessages(previous => [...previous, { role: 'user', content: text, request_id: turn.request_id, recorded_at: new Date().toISOString() }]); if (!messages.length) setTitle(text.slice(0, 80)); }
      const fetcher = createAccountBoundTurnFetcher(account, async () => {
        const client = getAuthClient(); if (!client) return null;
        const { data, error } = await client.auth.getSession(); if (error) throw error; return data.session;
      });
      const result = await store.deliver(account, turn, { fetcher, signal: controller.signal,
        onPending: () => { if (subjectRef.current === account && generation.current === rev) setNotice('Your message is saved and waiting for Apocrypha. Retrying the same request.'); } });
      if (subjectRef.current !== account || generation.current !== rev) return;
      if (!result) { await restore(turn.session_id, account); return; }
      setMessages(previous => [...previous.filter(message => !(message.role === 'assistant' && message.request_id === turn.request_id)), { role: 'assistant', content: result.text, request_id: turn.request_id, recorded_at: new Date().toISOString() }]);
      retainPending(null); setNotice(null); setDiagnosticBinding(null); setDetailsCopied(false); void refreshList(account);
    } catch (error) {
      if (subjectRef.current !== account || generation.current !== rev) return;
      recordFailure(error, account, 'turn');
      setNotice(error instanceof Error && error.name === 'AbortError' ? 'Stopped waiting. Your saved message is preserved; refresh the conversation to check its reply.' : error instanceof ChatNotice || error instanceof AccountTurnFailure ? error.message : 'The reply could not be confirmed. Your saved message is preserved.');
    } finally { if (subjectRef.current === account && generation.current === rev) setSending(false); }
  }
  async function checkConnection(): Promise<void> {
    if (!subject || !current || checkingConnection || sending) return;
    const account = subject; const rev = generation.current; setCheckingConnection(true); setDetailsCopied(false);
    try {
      const response = await authFetch('/api/mobile/status', { method: 'GET', cache: 'no-store' });
      const value = await readAccountDiagnostic(response, 'status', 'ACCOUNT_STATUS_UNVERIFIED');
      if (subjectRef.current === account && generation.current === rev) setDiagnosticBinding({ account, value });
    } catch (error) { if (subjectRef.current === account && generation.current === rev) recordFailure(error, account, 'status'); }
    finally { if (subjectRef.current === account && generation.current === rev) setCheckingConnection(false); }
  }
  async function copyDetails(): Promise<void> {
    if (!diagnostic || !subject) return; const account = subject; const rev = generation.current;
    try { await navigator.clipboard.writeText(accountDiagnosticText(diagnostic)); if (subjectRef.current === account && generation.current === rev) setDetailsCopied(true); }
    catch { if (subjectRef.current === account && generation.current === rev) setNotice('Copy is unavailable. You can select the connection details.'); }
  }
  async function copy(content: string): Promise<void> { try { await navigator.clipboard.writeText(content); toast('Message copied.'); } catch { setNotice('Copy is unavailable. You can select and copy the message text.'); } }
  return <main id="main-content" className={styles.page}>
    <header className={styles.header}><Link href="/" className={styles.brand}><span className="apx-brand-mark" aria-hidden="true" /><span>Apocrypha</span></Link><nav aria-label="Apocrypha navigation"><Link href="/download/apocrypha">Get the app</Link><Link href={authenticated ? '/account' : '/login?next=%2Fapocrypha'}>{authenticated ? 'Account' : 'Sign in'}</Link></nav></header>
    {!authenticated || !subject ? <section className={styles.welcome} aria-labelledby="welcome-title"><span className={styles.eyebrow}>APOCRYPHA</span><h1 id="welcome-title">A conversation.<br /><em>Room to think.</em></h1><p>Ask a question, explore an idea, or pick up where you left off. Sign in to keep your conversations together.</p>{access === 'checking' ? <p role="status">Checking your account…</p> : <div className={styles.welcomeActions}><Link href="/login?next=%2Fapocrypha" className={styles.primary}>Sign in to chat</Link><Link href="/register?next=%2Fapocrypha" className={styles.secondary}>Create an account</Link></div>}{access === 'unavailable' ? <p role="status">Account verification is temporarily unavailable. Please try signing in again.</p> : null}<Link className={styles.phoneLink} href="/download/apocrypha">Apocrypha for iPhone and Android →</Link></section>
    : !current ? <section className={styles.welcome} role="status"><p>Opening your conversations…</p></section>
    : <div className={`${styles.workspace} ${historyOpen ? styles.withHistory : ''}`}>
      {historyOpen ? <aside className={styles.history} aria-label="Conversation history"><h2>Your conversations</h2>{limited ? <p>Only the latest conversation is available in this list.</p> : null}{sessions.length === 0 ? <p>Your conversations will appear here.</p> : sessions.map(session => <button key={session.session_id} type="button" aria-current={session.session_id === sessionId ? 'true' : undefined} onClick={() => { void restore(session.session_id, subject); setHistoryOpen(false); }}><strong>{session.title}</strong><span>{session.message_count} messages</span></button>)}</aside> : null}
      <section className={styles.conversation} aria-label="Apocrypha conversation"><div className={styles.toolbar}><button type="button" onClick={() => setHistoryOpen(!historyOpen)} aria-expanded={historyOpen}>History</button><h1>{current ? title : 'Your conversation'}</h1><button type="button" disabled={loading || sending || Boolean(pending)} onClick={() => newChat(subject)}>New chat</button></div>
        <div className={styles.messages} role="log" aria-label="Messages" aria-live={loading ? 'off' : 'polite'} aria-busy={loading || sending}>{loading ? <p className={styles.empty}>Loading your conversation…</p> : current && !messages.length ? <div className={styles.empty}><span className="apx-brand-mark" aria-hidden="true" /><h2>What’s on your mind?</h2><p>Start anywhere.</p></div> : null}{current && truncated ? <p className={styles.historyNotice}>Showing the most recent messages available from this conversation.</p> : null}{current ? messages.map((message, index) => <article key={`${message.request_id}-${message.role}-${index}`} className={`${styles.message} ${message.role === 'user' ? styles.user : styles.assistant}`}><div className={styles.messageMeta}><strong>{message.role === 'user' ? 'You' : 'Apocrypha'}</strong><button type="button" onClick={() => { void copy(message.content); }} aria-label={`Copy ${message.role === 'user' ? 'your message' : 'Apocrypha reply'}`}>Copy</button></div><ConversationMessageContent content={message.content} assistant={message.role === 'assistant'} /></article>) : null}{sending ? <p className={styles.waiting} role="status">Apocrypha is responding…</p> : null}<div ref={end} /></div>
        <div className={styles.composer}>{notice ? <div className={styles.notice} role="status"><span>{notice}</span>{current && sessionId && !sending ? <button type="button" onClick={() => { void restore(sessionId, subject); }}>Refresh conversation</button> : null}</div> : null}{pending && !sending ? <div className={styles.retry}><span>Your last message is awaiting a confirmed reply.</span><button type="button" onClick={() => { void send(pending); }}>Retry same message</button></div> : null}<form onSubmit={event => { event.preventDefault(); if (!pending) void send(); }}><label className={styles.srOnly} htmlFor="apocrypha-message">Message Apocrypha</label><textarea id="apocrypha-message" value={draft} onChange={event => setDraft(event.target.value)} placeholder="Message Apocrypha…" rows={2} disabled={!current || loading || Boolean(pending)} onKeyDown={event => { if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) { event.preventDefault(); if (!pending) void send(); } }} />{sending ? <button className={styles.send} type="button" onClick={() => active.current?.abort()}>Stop waiting</button> : <button className={styles.send} type="submit" disabled={!current || loading || !draft.trim() || Boolean(pending)}>Send <span aria-hidden="true">↑</span></button>}</form><p className={styles.caption}>Your history belongs to your account. Apocrypha can make mistakes.</p></div>
        <details key={subject} className={styles.connection} onToggle={event => setDetailsOpen(event.currentTarget.open)}><summary>Connection details</summary>{detailsOpen ? <>
          {diagnostic ? <><p>{accountDiagnosticReason(diagnostic)}</p><dl><dt>Support code</dt><dd>{diagnostic.code}</dd><dt>Request reference</dt><dd>{diagnostic.trace_id ?? 'Unavailable — no server reference received'}</dd><dt>Time</dt><dd><time dateTime={diagnostic.time}>{diagnostic.time}</time></dd><dt>Stage</dt><dd>{diagnostic.stage}</dd></dl></> : <p>Check the account connection without sending a message.</p>}
          <div><button type="button" disabled={checkingConnection || sending || loading} onClick={() => { void checkConnection(); }}>{checkingConnection ? 'Checking…' : 'Check connection'}</button>{diagnostic ? <button type="button" onClick={() => { void copyDetails(); }}>Copy details</button> : null}{detailsCopied ? <span role="status">Connection details copied.</span> : null}</div>
        </> : null}</details>
      </section>
    </div>}
  </main>;
}
