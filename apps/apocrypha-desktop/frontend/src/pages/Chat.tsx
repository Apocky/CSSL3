import { useEffect, useRef, useState } from 'react';
import type { Act, SendMessage } from '../App.tsx';
import { ipc } from '../lib/ipc.ts';
import {
  canSend,
  conversationLabel,
  liveStatus,
  MAX_TEXT_BYTES,
  promptBytes,
  sendBlockedReason,
  type Live,
  type View,
} from '../lib/view.ts';

interface Props {
  view: View;
  busy: boolean;
  live: Live | null;
  act: Act;
  sendMessage: SendMessage;
}

/** Ticks while a reply is arriving so the wait is legible. */
function useElapsed(active: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 250);
    return () => window.clearInterval(timer);
  }, [active]);
  return now;
}

export function Chat({ view, busy, live, act, sendMessage }: Props) {
  const [draft, setDraft] = useState('');
  const stream = useRef<HTMLDivElement>(null);
  const now = useElapsed(live !== null);

  useEffect(() => {
    stream.current?.scrollTo({ top: stream.current.scrollHeight });
  }, [view.messages, view.session_id, live?.text]);

  const blocked = sendBlockedReason(view, draft, busy);
  const sendable = canSend(view, draft, busy);
  const bytes = promptBytes(draft);
  const status = liveStatus(live, now);

  const submit = () => {
    if (!sendable) return;
    const text = draft;
    setDraft('');
    void sendMessage(text).then((accepted) => {
      if (!accepted) setDraft(text);
    });
  };

  return (
    <main className="chat">
      <aside className="rail" aria-label="Conversations">
        <div className="rail-head">
          <span className="mark" aria-hidden="true">◇</span>
          <div>
            <strong>Apocrypha</strong>
            <small>{view.email}</small>
          </div>
        </div>
        <button className="primary block" disabled={busy} onClick={() => void act(() => ipc.newConversation())}>
          New conversation
        </button>
        <div className="rail-list">
          {view.conversations.length === 0 ? (
            <p className="rail-empty">No conversations yet.</p>
          ) : (
            view.conversations.map((conversation) => (
              <button
                key={conversation.id}
                className={conversation.id === view.session_id ? 'rail-item current' : 'rail-item'}
                aria-current={conversation.id === view.session_id ? 'true' : undefined}
                disabled={busy}
                onClick={() => void act(() => ipc.openConversation(conversation.id))}
              >
                {conversationLabel(conversation)}
              </button>
            ))
          )}
        </div>
        <div className="rail-foot">
          <button className="ghost" disabled={busy} onClick={() => void act(() => ipc.refresh())}>
            Refresh
          </button>
          <button className="link" disabled={busy} onClick={() => void act(() => ipc.signOut())}>
            Sign out
          </button>
        </div>
      </aside>

      <section className="conversation">
        <div className="stream" ref={stream} aria-live="polite">
          {view.messages.length === 0 && !live ? (
            <div className="stream-empty">
              <span aria-hidden="true">◇</span>
              <p>Start a conversation with Apocrypha.</p>
            </div>
          ) : (
            view.messages.map((message, index) => (
              <article key={`${message.request_id}-${index}`} className={`turn ${message.role}`}>
                <span className="who">{message.role === 'user' ? 'You' : 'Apocrypha'}</span>
                <p>{message.content}</p>
              </article>
            ))
          )}

          {live ? (
            <article className="turn assistant writing" aria-label="Apocrypha is replying">
              <span className="who">
                Apocrypha <span className="status">{status}</span>
              </span>
              {live.text ? (
                <p>
                  {live.text}
                  <span className="caret" aria-hidden="true" />
                </p>
              ) : (
                <p className="thinking" aria-hidden="true">
                  <span />
                  <span />
                  <span />
                </p>
              )}
            </article>
          ) : null}

          {view.pending_request && !live ? (
            <p className="unconfirmed">
              This message is not confirmed yet. Refresh to check for the reply — it will not be sent again
              automatically.
            </p>
          ) : null}
        </div>

        <form
          className="composer"
          onSubmit={(event) => {
            event.preventDefault();
            submit();
          }}
        >
          <textarea
            value={draft}
            disabled={busy || !!view.pending_request || view.access_denied}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && !event.shiftKey) {
                event.preventDefault();
                submit();
              }
            }}
            placeholder={view.pending_request ? 'Waiting on an unconfirmed reply…' : 'Write a message…'}
            aria-label="Message Apocrypha"
          />
          <div className="composer-foot">
            <span className={bytes > MAX_TEXT_BYTES ? 'count over' : 'count'}>
              {bytes.toLocaleString()} / {MAX_TEXT_BYTES.toLocaleString()} bytes
            </span>
            <span className="notice" role="status">
              {status ?? (busy ? 'Working…' : view.notice)}
            </span>
            <button className="primary" type="submit" disabled={!sendable} title={blocked ?? undefined}>
              Send
            </button>
          </div>
        </form>
      </section>
    </main>
  );
}
