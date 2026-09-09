import Link from 'next/link';
import { useEffect, useMemo, useRef, useState } from 'react';

import ChatTools from './ChatTools';
import ConversationMessageContent from '@/components/apocrypha/ConversationMessageContent';
import { useSiteSession } from '@/components/hub/SiteSession';
import { useToast } from '@/components/ui/Feedback';
import {
  activeMemberChatJob,
  clearMemberChatPending,
  fetchMemberChatHistoryPage,
  isActiveMemberChatStatus,
  isMemberChatUuid,
  MEMBER_CHAT_MAX_LOADED_HISTORY_PAGES,
  memberChatStatusText,
  mergeMemberChatHistory,
  normalizeMemberChatMessage,
  pollMemberChatJob,
  projectMemberChatMessages,
  readMemberChatPending,
  saveMemberChatPending,
  submitMemberChatJob,
  upsertMemberChatHistory,
  MemberChatClientError,
  type MemberChatHistoryEntry,
  type MemberChatPendingSubmission,
} from '@/lib/apocrypha/member-chat-client';
import { authFetch } from '@/lib/browser-auth';
import styles from '@/styles/AccountChat.module.css';

function errorText(error: unknown, fallback: string): string {
  if (error instanceof MemberChatClientError) return error.message;
  if (error instanceof Error && error.name === 'AbortError') {
    return 'Stopped waiting. Your message is saved; refresh this conversation later for the reply.';
  }
  return fallback;
}

function isDefinitivePreAcceptanceRejection(error: unknown): error is MemberChatClientError {
  return error instanceof MemberChatClientError
    && !error.retryable
    && error.status >= 400
    && error.status < 500
    && error.status !== 401;
}

export default function AccountChat(
  { onPendingChange }: { onPendingChange?: (pending: boolean) => void } = {},
): JSX.Element {
  const toast = useToast();
  const { access, authenticated, subjectKey } = useSiteSession();
  const subject = authenticated ? subjectKey : null;
  const subjectRef = useRef(subject);
  subjectRef.current = subject;
  const pendingCallbackRef = useRef(onPendingChange);
  pendingCallbackRef.current = onPendingChange;
  const pendingRef = useRef<MemberChatPendingSubmission | null>(null);
  const generation = useRef(0);
  const active = useRef<AbortController | null>(null);
  const earlierRequest = useRef<AbortController | null>(null);
  const end = useRef<HTMLDivElement>(null);
  const logRef = useRef<HTMLDivElement | null>(null);
  const messageInput = useRef<HTMLTextAreaElement | null>(null);
  const panelTrigger = useRef<HTMLButtonElement | null>(null);
  const panelRegion = useRef<HTMLElement | null>(null);
  const followingRef = useRef(true);

  const [bound, setBound] = useState<string | null>(null);
  const [conversationId, setConversationId] = useState<string | null>(null);
  const [history, setHistory] = useState<MemberChatHistoryEntry[]>([]);
  const [pending, setPending] = useState<MemberChatPendingSubmission | null>(null);
  const [activeJob, setActiveJob] = useState<MemberChatHistoryEntry | null>(null);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [loadedHistoryPages, setLoadedHistoryPages] = useState(1);
  const [loadingEarlier, setLoadingEarlier] = useState(false);
  const [draft, setDraft] = useState('');
  const [loading, setLoading] = useState(false);
  const [sending, setSending] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [toolsOpen, setToolsOpen] = useState(false);
  const [showLatest, setShowLatest] = useState(false);

  const current = Boolean(subject && subject === bound && conversationId);
  const outstanding = Boolean(pending || (activeJob && isActiveMemberChatStatus(activeJob.status)));
  const messages = useMemo(() => projectMemberChatMessages(history, pending), [history, pending]);
  const title = messages.find((message) => message.role === 'user')?.content.slice(0, 80) ?? 'New conversation';

  function retainPending(next: MemberChatPendingSubmission | null): void {
    pendingRef.current = next;
    setPending(next);
  }

  useEffect(() => {
    pendingCallbackRef.current?.(outstanding);
  }, [outstanding]);
  useEffect(() => () => { pendingCallbackRef.current?.(false); }, []);
  useEffect(() => { if (toolsOpen) panelRegion.current?.focus(); }, [toolsOpen]);
  useEffect(() => {
    followingRef.current = true;
    setShowLatest(false);
  }, [conversationId, subject]);
  useEffect(() => {
    if (followingRef.current) end.current?.scrollIntoView({ block: 'nearest' });
    else setShowLatest(true);
  }, [messages, sending, loading]);
  useEffect(() => {
    const log = logRef.current;
    if (!log || typeof ResizeObserver === 'undefined') return undefined;
    const observer = new ResizeObserver(() => {
      if (followingRef.current) log.scrollTop = log.scrollHeight;
    });
    observer.observe(log);
    return () => observer.disconnect();
  }, [current]);

  async function followJob(
    account: string,
    jobId: string,
    rev: number,
    controller: AbortController,
  ): Promise<void> {
    setSending(true);
    setNotice(memberChatStatusText(activeJob));
    try {
      const final = await pollMemberChatJob(jobId, {
        fetcher: authFetch,
        signal: controller.signal,
        onJob: (job) => {
          if (subjectRef.current !== account || generation.current !== rev) return;
          setActiveJob(isActiveMemberChatStatus(job.status) ? job : null);
          setHistory((previous) => upsertMemberChatHistory(previous, job));
          const status = memberChatStatusText(job);
          setNotice(status || null);
        },
        onRetry: () => {
          if (subjectRef.current === account && generation.current === rev) {
            setNotice('Your message is saved. Reconnecting to its reply…');
          }
        },
      });
      if (subjectRef.current !== account || generation.current !== rev) return;
      setHistory((previous) => upsertMemberChatHistory(previous, final));
      setActiveJob(null);
      const saved = pendingRef.current;
      if (!saved || saved.request_id === final.request_id) {
        clearMemberChatPending(account, localStorage);
        retainPending(null);
        setNotice(memberChatStatusText(final) || null);
      } else {
        setNotice('A saved message still needs confirmation. Refresh or retry that same message.');
      }
      try {
        const refreshed = await fetchMemberChatHistoryPage(final.conversation_id, authFetch, {
          signal: controller.signal,
        });
        if (subjectRef.current === account && generation.current === rev) {
          setHistory((previous) => mergeMemberChatHistory(previous, refreshed.history));
        }
      } catch {
        // The completed job already carries the durable reply; a later refresh can resync the list.
      }
    } catch (error) {
      if (subjectRef.current !== account || generation.current !== rev) return;
      if (error instanceof MemberChatClientError && error.code === 'MEMBER_CHAT_JOB_NOT_FOUND') {
        setActiveJob(null);
        const saved = pendingRef.current;
        if (saved?.job_id === jobId) {
          const { job_id: _discardedJobId, ...retryable } = saved;
          try { saveMemberChatPending(account, localStorage, retryable); } catch { /* retained in memory */ }
          retainPending(retryable);
          setNotice('This saved reply could not be found. Retry the same message to recover it safely.');
          return;
        }
      }
      setNotice(errorText(error, 'The reply could not be checked. Your message is saved; refresh to continue.'));
    } finally {
      if (subjectRef.current === account && generation.current === rev) setSending(false);
    }
  }

  async function openConversation(account: string, id: string): Promise<void> {
    const rev = ++generation.current;
    active.current?.abort();
    earlierRequest.current?.abort();
    const controller = new AbortController();
    active.current = controller;
    setBound(account);
    setConversationId(id);
    setHistory([]);
    setActiveJob(null);
    setNextCursor(null);
    setLoadedHistoryPages(1);
    setLoadingEarlier(false);
    setToolsOpen(false);
    setLoading(true);
    setSending(false);
    setNotice(null);

    let saved = readMemberChatPending(account, localStorage);
    if (saved?.conversation_id !== id) saved = null;
    retainPending(saved);
    try {
      const page = await fetchMemberChatHistoryPage(id, authFetch, { signal: controller.signal });
      if (subjectRef.current !== account || generation.current !== rev) return;
      const next = page.history;
      setHistory(next);
      setNextCursor(page.next_cursor);

      const recorded = saved
        ? next.find((entry) => entry.request_id === saved?.request_id) ?? null
        : null;
      if (saved && recorded) {
        if (isActiveMemberChatStatus(recorded.status)) {
          saved = { ...saved, job_id: recorded.job_id };
          try { saveMemberChatPending(account, localStorage, saved); } catch { /* already retained in memory */ }
          retainPending(saved);
        } else {
          clearMemberChatPending(account, localStorage);
          retainPending(null);
          saved = null;
        }
      }

      const running = activeMemberChatJob(next);
      if (running) {
        setActiveJob(running);
        setLoading(false);
        await followJob(account, running.job_id, rev, controller);
        return;
      }
      if (saved?.job_id) {
        setLoading(false);
        await followJob(account, saved.job_id, rev, controller);
        return;
      }
      const latest = next[next.length - 1] ?? null;
      setNotice(saved
        ? 'This saved message has not been confirmed yet. Retry the same message or refresh the conversation.'
        : latest ? memberChatStatusText(latest) || null : null);
    } catch (error) {
      if (subjectRef.current !== account || generation.current !== rev) return;
      setNotice(errorText(error, 'This conversation could not be loaded. Refresh to try again.'));
      if (saved?.job_id) {
        setLoading(false);
        await followJob(account, saved.job_id, rev, controller);
        return;
      }
    } finally {
      if (subjectRef.current === account && generation.current === rev) setLoading(false);
    }
  }

  useEffect(() => {
    generation.current += 1;
    active.current?.abort();
    earlierRequest.current?.abort();
    setBound(subject);
    setConversationId(null);
    setHistory([]);
    setActiveJob(null);
    setNextCursor(null);
    setLoadedHistoryPages(1);
    setLoadingEarlier(false);
    setDraft('');
    retainPending(null);
    setNotice(null);
    setLoading(Boolean(subject));
    setSending(false);
    setToolsOpen(false);
    if (!subject) return undefined;
    const account = subject;
    if (!isMemberChatUuid(account)) {
      setBound(account);
      setLoading(false);
      setNotice('Your account could not be verified for chat. Sign in again to continue.');
      return undefined;
    }
    const id = account.toLowerCase();
    void openConversation(account, id);
    return () => {
      generation.current += 1;
      active.current?.abort();
      earlierRequest.current?.abort();
    };
  }, [subject]);

  async function send(retry?: MemberChatPendingSubmission): Promise<void> {
    if (!subject || !current || !conversationId || loading || sending) return;
    if (pendingRef.current && !retry) return;
    const candidate = retry?.message ?? draft;
    const text = normalizeMemberChatMessage(candidate);
    if (!text) {
      if (retry) {
        clearMemberChatPending(subject, localStorage);
        retainPending(null);
        setDraft(candidate);
      }
      setNotice('Remove unusual invisible characters or shorten this message, then try again.');
      return;
    }
    const account = subject;
    const rev = generation.current;
    const submission: MemberChatPendingSubmission = retry ?? {
      conversation_id: conversationId,
      request_id: crypto.randomUUID(),
      message: text,
      created_at: new Date().toISOString(),
    };
    try {
      saveMemberChatPending(account, localStorage, submission);
    } catch {
      setNotice('This browser blocked chat recovery. Allow site storage, then send again.');
      return;
    }
    retainPending(submission);
    if (!retry) setDraft('');
    setNotice('Saving your message…');
    setSending(true);
    active.current?.abort();
    const controller = new AbortController();
    active.current = controller;
    try {
      const receipt = await submitMemberChatJob({
        conversationId: submission.conversation_id,
        requestId: submission.request_id,
        message: submission.message,
      }, authFetch, controller.signal);
      if (subjectRef.current !== account || generation.current !== rev) return;
      const accepted = { ...submission, job_id: receipt.job_id };
      try { saveMemberChatPending(account, localStorage, accepted); } catch { /* original pending receipt remains */ }
      retainPending(accepted);
      setNotice('Message saved. Waiting for Apocrypha…');
      await followJob(account, receipt.job_id, rev, controller);
    } catch (error) {
      if (subjectRef.current !== account || generation.current !== rev) return;
      if (!submission.job_id && isDefinitivePreAcceptanceRejection(error)) {
        clearMemberChatPending(account, localStorage);
        retainPending(null);
        setDraft(submission.message);
        setNotice(`${error.message} Your message is ready to edit.`);
        messageInput.current?.focus();
      } else {
        setNotice(errorText(error, 'The send could not be confirmed. Retry the same message to recover safely.'));
      }
    } finally {
      if (subjectRef.current === account && generation.current === rev) setSending(false);
    }
  }

  async function loadEarlier(): Promise<void> {
    if (
      !subject || !conversationId || !current || nextCursor === null || loadingEarlier
      || loadedHistoryPages >= MEMBER_CHAT_MAX_LOADED_HISTORY_PAGES
    ) return;
    const account = subject;
    const id = conversationId;
    const cursor = nextCursor;
    const rev = generation.current;
    earlierRequest.current?.abort();
    const controller = new AbortController();
    earlierRequest.current = controller;
    setLoadingEarlier(true);
    try {
      const page = await fetchMemberChatHistoryPage(id, authFetch, {
        before: cursor,
        signal: controller.signal,
      });
      if (subjectRef.current !== account || generation.current !== rev) return;
      setHistory((currentHistory) => mergeMemberChatHistory(page.history, currentHistory));
      setNextCursor(page.next_cursor);
      setLoadedHistoryPages((count) => count + 1);
    } catch (error) {
      if (subjectRef.current !== account || generation.current !== rev) return;
      setNotice(errorText(error, 'Earlier messages could not be loaded. Try again.'));
    } finally {
      if (subjectRef.current === account && generation.current === rev) setLoadingEarlier(false);
    }
  }

  function insertCreation(text: string): boolean {
    if (!current || loading || sending || outstanding) return false;
    const next = draft ? `${draft}\n\n${text}` : text;
    if (!normalizeMemberChatMessage(next)) {
      setNotice('This creation will not fit in the current message. Shorten the draft or copy the result instead.');
      return false;
    }
    setDraft(next);
    setToolsOpen(false);
    messageInput.current?.focus();
    return true;
  }

  function followLatest(): void {
    followingRef.current = true;
    setShowLatest(false);
    end.current?.scrollIntoView({ block: 'nearest' });
  }

  async function copy(content: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(content);
      toast('Message copied.');
    } catch {
      setNotice('Copy is unavailable. Select the message text and copy it directly.');
    }
  }

  return <main
    id="main-content"
    className={styles.page}
    onKeyDown={(event) => {
      if (event.key === 'Escape' && toolsOpen) {
        event.stopPropagation();
        setToolsOpen(false);
        panelTrigger.current?.focus();
      }
    }}
  >
    <header className={styles.header}>
      <Link href="/" className={styles.brand} aria-label="Apocky home"><span className="apx-brand-mark" aria-hidden="true" /></Link>
      <div className={styles.roomTitle}>
        <h1>Apocrypha</h1>
        <p>{sending ? 'Responding…' : outstanding ? 'Message saved' : authenticated ? 'Your private conversation' : 'Room to think'}</p>
      </div>
      <nav aria-label="Apocrypha navigation">{authenticated && current ? <Link href="/account">Account</Link> : <>
        <Link href="/download/apocrypha">Get the app</Link>
        <Link href="/login?next=%2Fapocrypha">Sign in</Link>
      </>}</nav>
    </header>

    {!authenticated || !subject ? <section className={styles.welcome} aria-labelledby="welcome-title">
      <span className={styles.eyebrow}>APOCRYPHA</span>
      <h2 id="welcome-title">A conversation.<br /><em>Room to think.</em></h2>
      <p>Ask a question, explore an idea, or pick up where you left off. Sign in to keep your conversations together.</p>
      {access === 'checking' ? <p role="status">Checking your account…</p> : <div className={styles.welcomeActions}>
        <Link href="/login?next=%2Fapocrypha" className={styles.primary}>Sign in to chat</Link>
        <Link href="/register?next=%2Fapocrypha" className={styles.secondary}>Create an account</Link>
      </div>}
      {access === 'unavailable' ? <p role="status">Account verification is temporarily unavailable. Please try signing in again.</p> : null}
      <Link className={styles.phoneLink} href="/download/apocrypha">Apocrypha for Windows, iPhone and Android →</Link>
    </section>
      : !current ? <section className={styles.welcome} role={notice ? 'alert' : 'status'}>
        <p>{notice ?? 'Opening your conversation…'}</p>
        {notice ? <Link href="/login?next=%2Fapocrypha" className={styles.primary}>Sign in again</Link> : null}
      </section>
        : <div className={styles.workspace}>
          <section className={styles.conversation} aria-label="Apocrypha conversation">
            <div
              ref={logRef}
              onScroll={(event) => {
                const log = event.currentTarget;
                followingRef.current = log.scrollHeight - log.scrollTop - log.clientHeight < 100;
                setShowLatest(!followingRef.current);
              }}
              className={styles.messages}
              role="log"
              aria-label="Messages"
              aria-live={loading ? 'off' : 'polite'}
              aria-busy={loading || sending}
            >
              {loading ? <p className={styles.empty}>Loading your conversation…</p>
                : !messages.length ? <div className={styles.empty}><span className="apx-brand-mark" aria-hidden="true" /><h2>What’s on your mind?</h2><p>Ask, imagine, make something.</p></div>
                  : null}
              {!loading && nextCursor !== null && loadedHistoryPages < MEMBER_CHAT_MAX_LOADED_HISTORY_PAGES
                ? <p className={styles.conversationName}><button
                  type="button"
                  className={styles.latest}
                  disabled={loadingEarlier}
                  onClick={() => { void loadEarlier(); }}
                >{loadingEarlier ? 'Loading earlier messages…' : 'Earlier messages'}</button></p>
                : null}
              {!loading && nextCursor !== null && loadedHistoryPages >= MEMBER_CHAT_MAX_LOADED_HISTORY_PAGES
                ? <p className={styles.historyNotice}>More earlier messages remain safely stored.</p>
                : null}
              {messages.length > 0 ? <p className={styles.conversationName}>{title}</p> : null}
              {messages.map((message) => <article key={message.key} className={styles.message} data-role={message.role}>
                <div className={styles.messageMeta}>
                  <span className={styles.avatar} aria-hidden="true">{message.role === 'user' ? 'Y' : '∞'}</span>
                  <strong>{message.role === 'user' ? 'You' : 'Apocrypha'}</strong>
                  <time dateTime={message.recorded_at}>{new Date(message.recorded_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</time>
                  <button type="button" onClick={() => { void copy(message.content); }} aria-label={`Copy ${message.role === 'user' ? 'your message' : 'Apocrypha reply'}`}>Copy</button>
                </div>
                <ConversationMessageContent content={message.content} assistant={message.role === 'assistant'} />
                {message.truncated ? <p className={styles.historyNotice}>This reply was shortened in saved history.</p> : null}
              </article>)}
              {sending ? <p className={styles.waiting} role="status">Apocrypha is responding…</p> : null}
              <div ref={end} />
            </div>
            {showLatest ? <button type="button" className={styles.latest} onClick={followLatest}>Latest messages ↓</button> : null}
            <div className={styles.composer}>
              {notice ? <div className={styles.composerFeedback}>
                <div className={styles.notice} role="status">
                  <span>{notice}</span>
                  <span>
                    {notice.startsWith('Your sign-in') ? <Link href="/login?next=%2Fapocrypha">Sign in again</Link> : null}
                    {!sending && conversationId ? <button type="button" onClick={() => { void openConversation(subject, conversationId); }}>Refresh</button> : null}
                    {!sending && pending && !pending.job_id ? <button type="button" onClick={() => { void send(pending); }}>Retry same message</button> : null}
                  </span>
                </div>
              </div> : null}
              <form onSubmit={(event) => { event.preventDefault(); if (!outstanding) void send(); }}>
                <label className={styles.srOnly} htmlFor="apocrypha-message">Message Apocrypha</label>
                <textarea
                  ref={messageInput}
                  id="apocrypha-message"
                  value={draft}
                  onChange={(event) => setDraft(event.target.value)}
                  placeholder="Message Apocrypha…"
                  rows={2}
                  disabled={!current || loading || outstanding}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) {
                      event.preventDefault();
                      if (!outstanding) void send();
                    }
                  }}
                />
                <div className={styles.composerActions}>
                  <button
                    ref={panelTrigger}
                    type="button"
                    className={styles.toolsButton}
                    aria-expanded={toolsOpen}
                    aria-controls="chat-tools-panel"
                    onClick={() => setToolsOpen((value) => !value)}
                  ><span aria-hidden="true">＋</span> Create</button>
                  <span className={styles.keyHint}>Shift + Enter for a new line</span>
                  {sending ? <button className={styles.send} type="button" onClick={() => active.current?.abort()}>Stop waiting</button>
                    : <button className={styles.send} type="submit" disabled={!current || loading || !draft.trim() || outstanding}>Send <span aria-hidden="true">↑</span></button>}
                </div>
              </form>
            </div>
          </section>
          <aside
            id="chat-tools-panel"
            ref={panelRegion}
            tabIndex={-1}
            className={styles.inlinePanel}
            hidden={!toolsOpen}
            aria-label="Create in chat"
          >
            <header className={styles.panelHeader}><h2>Create in chat</h2><button type="button" onClick={() => { setToolsOpen(false); panelTrigger.current?.focus(); }} aria-label="Close panel">×</button></header>
            <div className={styles.toolBody}><ChatTools key={subject} disabled={!current || loading || sending || outstanding} onInsert={insertCreation} /></div>
          </aside>
        </div>}
  </main>;
}
