// The room picker and the people panel. Private is you and Apocrypha; lobbies are invite-only:
// you open a link from here, share it, and whoever accepts joins and becomes your friend. Friends
// can then be added to any lobby you are in without a new link.

import { useEffect, useRef, useState } from 'react';

import { ChevronIcon, CloseIcon, CopyIcon, PlusIcon } from './Icons';
import Tip from './Tip';
import type { FriendView, MemberView, RoomSummary } from './types';
import styles from './Room.module.css';

async function manage<T>(body: Record<string, unknown>): Promise<T> {
  const response = await fetch('/api/room/manage', {
    method: 'POST', credentials: 'same-origin', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body),
  });
  const payload = await response.json() as T & { ok?: boolean; error?: string };
  if (!response.ok || payload.ok !== true) throw new Error(payload.error ?? `HTTP ${response.status}`);
  return payload;
}
export { manage };

function useDismiss(open: boolean, close: () => void) {
  const ref = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    if (!open) return undefined;
    const out = (e: MouseEvent | TouchEvent) => { if (ref.current && !ref.current.contains(e.target as Node)) close(); };
    const esc = (e: KeyboardEvent) => { if (e.key === 'Escape') close(); };
    document.addEventListener('mousedown', out);
    document.addEventListener('touchstart', out);
    document.addEventListener('keydown', esc);
    return () => { document.removeEventListener('mousedown', out); document.removeEventListener('touchstart', out); document.removeEventListener('keydown', esc); };
  }, [open, close]);
  return ref;
}

export function RoomPicker({ rooms, current, onPick, onCreated }: {
  readonly rooms: readonly RoomSummary[];
  readonly current: string;
  readonly onPick: (key: string) => void;
  readonly onCreated: (key: string) => void;
}): JSX.Element {
  const [open, setOpen] = useState(false);
  const [naming, setNaming] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const ref = useDismiss(open, () => { setOpen(false); setNaming(null); });
  const here = rooms.find((r) => r.key === current);
  const label = here ? here.title : 'Private';

  const create = async () => {
    try {
      const { key } = await manage<{ key: string }>({ action: 'create', title: naming ?? 'Lobby' });
      setNaming(null); setOpen(false); onCreated(key);
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'Could not create it.'); }
  };

  return <div className={styles.pickerWrap} ref={ref}>
    <Tip label="Switch rooms: your private room, or a lobby you were invited to" side="bottom" align="start">
      <button type="button" className={styles.picker} aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen((v) => !v)}>
        <span className={styles.pickerLabel}>{label}</span><ChevronIcon size={14} />
      </button>
    </Tip>
    {open ? <div className={`${styles.menu} ${styles.menuDown}`} role="menu" aria-label="Rooms">
      {rooms.map((r) => <button key={r.key} type="button" role="menuitemradio" aria-checked={r.key === current}
        className={`${styles.menuItem} ${r.key === current ? styles.menuItemOn : ''}`}
        onClick={() => { onPick(r.key); setOpen(false); }}>
        <span className={styles.menuText}><span>{r.title}</span>
          <small>{r.kind === 'private' ? 'Just you and Apocrypha' : `Lobby · ${r.members} ${r.members === 1 ? 'person' : 'people'}${r.role === 'owner' ? ' · yours' : ''}`}</small></span>
        {r.key === current ? <span className={styles.menuCheck} aria-hidden="true">✓</span> : null}
      </button>)}
      <div className={styles.menuRule} role="separator" />
      {naming === null
        ? <button type="button" className={styles.menuItem} onClick={() => setNaming('')}>
          <span className={styles.menuIcon}><PlusIcon /></span><span className={styles.menuText}><span>New lobby</span><small>Invite-only: nobody gets in without your link</small></span>
        </button>
        : <form className={styles.inlineForm} onSubmit={(e) => { e.preventDefault(); void create(); }}>
          <input autoFocus maxLength={80} placeholder="Lobby name" value={naming} onChange={(e) => setNaming(e.target.value)} />
          <button type="submit">Create</button>
        </form>}
      {error ? <p className={styles.error}>{error}</p> : null}
    </div> : null}
  </div>;
}

export function PeoplePanel({ room, onClose, onLeft, friends, reloadFriends }: {
  readonly room: RoomSummary;
  readonly onClose: () => void;
  readonly onLeft: () => void;
  readonly friends: readonly FriendView[];
  readonly reloadFriends: () => void;
}): JSX.Element {
  const [members, setMembers] = useState<MemberView[]>([]);
  const [link, setLink] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [name, setName] = useState('');
  const [note, setNote] = useState<string | null>(null);
  const ref = useDismiss(true, onClose);
  const lobby = room.kind === 'lobby';

  const load = async () => {
    if (!lobby) return;
    try { setMembers((await manage<{ members: MemberView[] }>({ action: 'members', room: room.key })).members); } catch { /* shown empty */ }
  };
  useEffect(() => { void load(); }, [room.key]);

  const invite = async () => {
    try { setLink((await manage<{ url: string }>({ action: 'invite', room: room.key })).url); setCopied(false); }
    catch (cause) { setNote(cause instanceof Error ? cause.message : 'Could not create a link.'); }
  };
  const copy = async () => { if (!link) return; try { await navigator.clipboard.writeText(link); setCopied(true); } catch { /* select manually */ } };
  const add = async (friend: string) => {
    try { await manage({ action: 'add', room: room.key, friend }); await load(); setNote('Added.'); }
    catch (cause) { setNote(cause instanceof Error ? cause.message : 'Could not add.'); }
  };
  const saveName = async () => {
    try { await manage({ action: 'name', name }); setNote('Name saved.'); setName(''); await load(); reloadFriends(); }
    catch (cause) { setNote(cause instanceof Error ? cause.message : 'Could not save.'); }
  };
  const leave = async () => {
    const owner = room.role === 'owner';
    if (!window.confirm(owner ? `Delete "${room.title}" for everyone?` : `Leave "${room.title}"?`)) return;
    try { await manage({ action: 'leave', room: room.key }); onLeft(); } catch (cause) { setNote(cause instanceof Error ? cause.message : 'Could not leave.'); }
  };

  const inRoom = new Set(members.map((m) => m.user_id));
  return <div className={styles.people} ref={ref} role="dialog" aria-label="People">
    <div className={styles.peopleHead}>
      <h2>{lobby ? room.title : 'Private'}</h2>
      <Tip label="Close" align="end" side="bottom"><button type="button" className={styles.iconBtn} aria-label="Close" onClick={onClose}><CloseIcon size={16} /></button></Tip>
    </div>
    {!lobby ? <p className={styles.peopleNote}>Only you and Apocrypha are here. Create a lobby from the room menu to talk with friends.</p> : <>
      <section>
        <h3>Invite</h3>
        {link ? <div className={styles.linkRow}>
          <input readOnly value={link} onFocus={(e) => e.target.select()} aria-label="Invitation link" />
          <Tip label={copied ? 'Copied' : 'Copy the link'} align="end"><button type="button" className={styles.iconBtn} aria-label="Copy the link" onClick={() => void copy()}><CopyIcon size={16} /></button></Tip>
        </div> : <button type="button" className={styles.softBtn} onClick={() => void invite()}>Create an invitation link</button>}
        <small className={styles.peopleNote}>Works 10 times, for 7 days. Whoever accepts joins this lobby and becomes your friend.</small>
      </section>
      <section>
        <h3>In this lobby</h3>
        <ul className={styles.list}>{members.map((m) => <li key={m.user_id}>{m.display_name}{m.role === 'owner' ? <small> · owner</small> : null}</li>)}</ul>
      </section>
      <section>
        <h3>Add a friend</h3>
        {friends.filter((f) => !inRoom.has(f.user_id)).length === 0
          ? <small className={styles.peopleNote}>No friends to add yet. Friends are people who accepted your invitation, or whose you accepted.</small>
          : <ul className={styles.list}>{friends.filter((f) => !inRoom.has(f.user_id)).map((f) => <li key={f.user_id}>
            {f.display_name} <button type="button" className={styles.softBtn} onClick={() => void add(f.user_id)}>Add</button>
          </li>)}</ul>}
      </section>
    </>}
    <section>
      <h3>Your name</h3>
      <form className={styles.inlineForm} onSubmit={(e) => { e.preventDefault(); void saveName(); }}>
        <input maxLength={40} placeholder="How others in your lobbies see you" value={name} onChange={(e) => setName(e.target.value)} />
        <button type="submit" disabled={name.trim() === ''}>Save</button>
      </form>
    </section>
    {lobby && room.key !== 'lobby' ? <button type="button" className={styles.dangerBtn} onClick={() => void leave()}>{room.role === 'owner' ? 'Delete this lobby' : 'Leave this lobby'}</button> : null}
    {note ? <p className={styles.peopleNote} role="status">{note}</p> : null}
  </div>;
}
