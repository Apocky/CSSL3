import { useEffect, useRef, useState } from 'react';
import type { Act } from '../App.tsx';
import { ipc } from '../lib/ipc.ts';
import { canSend, conversationLabel, MAX_TEXT_BYTES, promptBytes, scopeNote, sendBlockedReason, type View } from '../lib/view.ts';

interface Props {
  view: View;
  busy: boolean;
  act: Act;
}

export function Chat({ view, busy, act }: Props) {
  const [draft, setDraft] = useState('');
  const stream = useRef<HTMLDivElement>(null);

  useEffect(() => {
    stream.current?.scrollTo({ top: stream.current.scrollHeight });
  }, [view.messages, view.session_id]);

  const blocked = sendBlockedReason(view, draft, busy);
  const sendable = canSend(view, draft, busy);
  const bytes = promptBytes(draft);
  const partial = scopeNote(view);

  const submit = () => {
    if (!sendable) return;
    const text = draft;
    setDraft('');
    void act(async () => {
      const next = await ipc.send(text);
      // The controller hands the text back when the service refused it, so the
      // person does not lose what they wrote.
      if (!next.messages.some((message) => message.role === 'user' && message.content === text.trim())) {
        setDraft(text);
      }
      return next;
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
        {partial ? <p className="rail-note">{partial}</p> : null}
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
          {view.messages.length === 0 ? (
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
          {view.pending_request ? (
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
              {busy ? 'Working…' : view.notice}
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
