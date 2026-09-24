import { useRef, useState, type KeyboardEvent } from 'react';

import type { RoomName } from './types';
import styles from './Room.module.css';

export default function Composer({ room, owner, onRoom, onSend, error }: {
  readonly room: RoomName;
  readonly owner: boolean;
  readonly onRoom: (room: RoomName) => void;
  readonly onSend: (body: string) => Promise<boolean>;
  readonly error: string | null;
}): JSX.Element {
  const [text, setText] = useState('');
  const [busy, setBusy] = useState(false);
  const ref = useRef<HTMLTextAreaElement | null>(null);

  const submit = async () => {
    const body = text.trim();
    if (!body || busy) return;
    setBusy(true);
    const ok = await onSend(body);
    setBusy(false);
    if (ok) setText('');
    ref.current?.focus();
  };

  const onKey = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault();
      void submit();
    }
  };

  return <div className={styles.composer}>
    {owner ? <div className={styles.rooms} role="tablist" aria-label="Room">
      {(['lobby', 'owner'] as const).map((r) => <button
        key={r}
        type="button"
        role="tab"
        aria-selected={room === r}
        className={room === r ? `${styles.roomBtn} ${styles.roomOn}` : styles.roomBtn}
        onClick={() => onRoom(r)}
      >{r}</button>)}
    </div> : null}
    <form className={styles.form} onSubmit={(e) => { e.preventDefault(); void submit(); }}>
      <label htmlFor="room-say" className={styles.srOnly}>Say something</label>
      <textarea
        id="room-say"
        ref={ref}
        className={styles.textarea}
        rows={1}
        value={text}
        maxLength={4000}
        placeholder={room === 'owner' ? 'Private to Apocrypha' : 'Say something'}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={onKey}
        autoComplete="off"
      />
      <button type="submit" className={styles.send} disabled={busy || text.trim() === ''}>Send</button>
    </form>
    {error ? <p className={styles.error} role="alert">{error}</p> : null}
  </div>;
}
