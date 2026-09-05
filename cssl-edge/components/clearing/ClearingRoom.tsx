import type { FormEvent } from 'react';
import { useMemo, useState } from 'react';

import type { ClearingMember, ClearingMessage, ClearingReaction, ClearingRoom, ClearingLiveState } from '../../lib/clearing/client';
import CyberDreamField from '../cyber/CyberDreamField';
import styles from './Clearing.module.css';

export type ClearingContextAxis = 'People' | 'Meaning' | 'Visibility' | 'Time';
export type ClearingSessionState = 'checking' | 'signed-out' | 'signed-in';

export type ClearingRoomProps = {
  rooms: ClearingRoom[];
  activeRoomId: string;
  messages: ClearingMessage[];
  reactions: ClearingReaction[];
  members: ClearingMember[];
  selectedMessageId: string | null;
  contextAxis: ClearingContextAxis | null;
  draft: string;
  liveState: ClearingLiveState;
  session: ClearingSessionState;
  presenceCount: number;
  error: string;
  sending: boolean;
  onSelectRoom: (slug: string) => void;
  onSelectMessage: (id: string) => void;
  onOpenContext: (id: string) => void;
  onCloseContext: () => void;
  onSetContextAxis: (axis: ClearingContextAxis) => void;
  onDraftChange: (value: string) => void;
  onSend: (body: string, replyToId: string | null) => void | Promise<void>;
  onReact: (messageId: string, kind: ClearingReaction['kind']) => void;
  onReply: (id: string) => void;
  onInvite: () => void;
  onPrivacy: () => void;
  onSignIn: () => void;
};

const axes: ClearingContextAxis[] = ['People', 'Meaning', 'Visibility', 'Time'];
const reactionGlyph: Record<ClearingReaction['kind'], string> = { spark: '✦', heart: '♡', echo: '↟', curious: '◎' };

function relativeTime(iso: string): string {
  const seconds = Math.max(0, Math.floor((Date.now() - Date.parse(iso)) / 1000));
  if (seconds < 60) return 'now';
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h`;
  return `${Math.floor(seconds / 86400)}d`;
}

export function ClearingRoom(props: ClearingRoomProps): JSX.Element {
  const [localReply, setLocalReply] = useState<string | null>(null);
  const activeRoom = props.rooms.find((room) => room.id === props.activeRoomId) ?? props.rooms[0];
  const selected = props.messages.find((message) => message.id === props.selectedMessageId) ?? null;
  const counts = useMemo(() => props.reactions.reduce<Record<string, Partial<Record<ClearingReaction['kind'], number>>>>((all, reaction) => {
    const current = all[reaction.message_id] ?? {};
    current[reaction.kind] = (current[reaction.kind] ?? 0) + 1;
    all[reaction.message_id] = current;
    return all;
  }, {}), [props.reactions]);
  const replies = selected ? props.messages.filter((message) => message.reply_to_id === selected.id).length : 0;
  const liveLabel = props.liveState === 'live' ? 'LIVE' : props.liveState === 'reconnecting' ? 'RECONNECTING' : props.liveState === 'loading' ? 'OPENING' : 'UNAVAILABLE';

  const submit = (event: FormEvent<HTMLFormElement>): void => {
    event.preventDefault();
    if (!props.draft.trim() || props.session !== 'signed-in') return;
    void props.onSend(props.draft, localReply);
    setLocalReply(null);
  };

  return (
    <main className={styles.clearing} aria-label="The Clearing public room">
      <CyberDreamField variant="clearing" activity={props.sending ? 'thinking' : props.draft.trim() ? 'listening' : 'idle'} density={0.72} viewport />
      <div className={styles.world} aria-hidden="true"><span className={styles.orbit} /><span className={styles.glow} /></div>
      <div className={styles.frame}>
        <header className={styles.header}>
          <div className={styles.brand}><span className={styles.brandMark}>◇</span><span>THE CLEARING</span><span className={styles.liveDot} data-state={props.liveState} /></div>
          <div className={styles.headerRoom}><span>{activeRoom?.glyph ?? '◇'}</span><strong>{activeRoom?.title ?? 'North Clearing'}</strong><small>{props.presenceCount > 0 ? `${props.presenceCount} here now` : 'a public room'}</small></div>
          <nav className={styles.headerActions} aria-label="Room actions">
            <button type="button" onClick={props.onInvite}>Invite</button>
            <button type="button" onClick={props.onPrivacy}>Privacy</button>
          </nav>
        </header>

        <div className={styles.layout}>
          <aside className={styles.shelf} aria-label="Rooms">
            <div className={styles.shelfLabel}>ROOMS</div>
            <div className={styles.roomList}>
              {props.rooms.map((room) => (
                <button key={room.id} type="button" className={`${styles.roomDoor} ${room.id === props.activeRoomId ? styles.roomDoorActive : ''}`} onClick={() => props.onSelectRoom(room.slug)} aria-current={room.id === props.activeRoomId ? 'page' : undefined}>
                  <span className={styles.roomGlyph}>{room.glyph}</span><span>{room.title}</span><small>{room.slug === activeRoom?.slug ? 'open' : 'door'}</small>
                </button>
              ))}
            </div>
            <div className={styles.shelfFooter}><span className={styles.statusPip} data-state={props.liveState} /> {liveLabel}<small>{props.error || 'public projection'}</small></div>
          </aside>

          <section className={styles.stage} aria-label="Context stage">
            <div className={styles.stageBackdrop} aria-hidden="true"><span>✦</span><span>◌</span><span>◇</span></div>
            {!selected ? (
              <div className={styles.stageEmpty}><span className={styles.stageSigil}>◇</span><strong>Choose a message</strong><p>Its nearby relations will appear here without leaving the room.</p></div>
            ) : (
              <div className={styles.contextPanel} role="region" aria-label="Message Context">
                <div className={styles.contextHead}><span>CONTEXT</span><button type="button" onClick={props.onCloseContext} aria-label="Close Context">×</button></div>
                <p className={styles.contextQuote}>{selected.body}</p>
                <div className={styles.axisGrid}>
                  {axes.map((axis) => <button key={axis} type="button" className={props.contextAxis === axis ? styles.axisActive : ''} onClick={() => props.onSetContextAxis(axis)}><span aria-hidden="true">{axis === 'People' ? '◌' : axis === 'Meaning' ? '✦' : axis === 'Visibility' ? '◇' : '↟'}</span>{axis}</button>)}
                </div>
                <p className={styles.axisPeek}>{props.contextAxis === 'People' ? `${selected.author_label} · ${props.members.length} participants have left a trace here.` : props.contextAxis === 'Meaning' ? `${replies} replies branch from this message inside ${activeRoom?.title ?? 'the room'}.` : props.contextAxis === 'Visibility' ? 'Public room · readable by anyone · posting requires a signed-in account.' : `Placed ${relativeTime(selected.created_at)} · ${selected.edited_at ? 'revised' : 'unrevised'}.`}</p>
              </div>
            )}
          </section>

          <section className={styles.chat} aria-label="Conversation">
            <div className={styles.chatHead}><div><span className={styles.eyebrow}>CURRENT THREAD</span><h1>{activeRoom?.title ?? 'The Clearing'}</h1></div><span className={styles.chatState} data-state={props.liveState}>{liveLabel}</span></div>
            <div className={styles.stream} aria-live="polite">
              {props.messages.length === 0 && <div className={styles.emptyStream}><span>◇</span><p>No messages yet.</p><small>Leave the first trace in this room.</small></div>}
              {props.messages.map((message) => (
                <article key={message.id} className={`${styles.message} ${selected?.id === message.id ? styles.messageSelected : ''} ${message.reply_to_id ? styles.messageReply : ''}`}>
                  <button type="button" className={styles.messageBody} onClick={() => { props.onSelectMessage(message.id); props.onOpenContext(message.id); }} aria-label={`Open Context for ${message.author_label}'s message`}>
                    <span className={styles.avatar} aria-hidden="true">{message.author_label.slice(0, 1).toUpperCase()}</span><span className={styles.messageCopy}><span className={styles.messageMeta}><strong>{message.author_label}</strong><time dateTime={message.created_at}>{relativeTime(message.created_at)}</time></span><span className={styles.messageText}>{message.body}</span></span>
                  </button>
                  <div className={styles.messageTools} aria-label="Message actions">
                    {(Object.keys(reactionGlyph) as ClearingReaction['kind'][]).map((kind) => <button key={kind} type="button" onClick={() => props.onReact(message.id, kind)} aria-label={`React ${kind}`}>{reactionGlyph[kind]}<small>{counts[message.id]?.[kind] || ''}</small></button>)}
                    <button type="button" onClick={() => { setLocalReply(message.id); props.onReply(message.id); }} aria-label="Reply in thread">↳</button>
                    <button type="button" onClick={() => props.onOpenContext(message.id)} aria-label="Open message Context">◇</button>
                  </div>
                </article>
              ))}
            </div>
            <form className={styles.composer} onSubmit={submit}>
              {props.session === 'signed-in' ? (
                <>
                  {localReply && <div className={styles.replyNotice}>Replying in thread <button type="button" onClick={() => setLocalReply(null)}>cancel</button></div>}
                  <textarea value={props.draft} onChange={(event) => props.onDraftChange(event.target.value)} placeholder="Leave a trace…" aria-label="Message the Clearing" maxLength={2000} />
                  <div className={styles.composerTools}><button className={styles.send} type="submit" disabled={props.sending || !props.draft.trim()} aria-label="Send message">↑</button></div>
                </>
              ) : (
                <button className={styles.join} type="button" onClick={props.onSignIn} disabled={props.session === 'checking'}>
                  {props.session === 'checking' ? 'Checking your account…' : 'Sign in to join the room'}
                </button>
              )}
            </form>
          </section>
        </div>
      </div>
    </main>
  );
}
