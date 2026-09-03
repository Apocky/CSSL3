import Link from 'next/link';
import { ChangeEvent, useEffect, useRef, useState } from 'react';

import { LOCAL_SPELLBOOK_KEY, readLocalSpellbook, writeLocalSpellbook } from '../../lib/spellbook-local';
import { parseSpellbook, serializeSpellbook, type SpellbookEntry } from '../../lib/spellcraft';
import styles from '../../styles/SymbolicStudio.module.css';

type Notice = { readonly tone: 'ok' | 'error'; readonly text: string; readonly code: string };

export default function SpellbookPanel(): JSX.Element {
  const [entries, setEntries] = useState<readonly SpellbookEntry[]>([]);
  const [ready, setReady] = useState(false);
  const [notice, setNotice] = useState<Notice | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const result = readLocalSpellbook(window.localStorage);
    if (result.status === 'ready') setEntries(result.entries);
    else setNotice({ tone: 'error', text: result.message, code: result.code });
    setReady(true);
  }, []);

  const persist = (next: readonly SpellbookEntry[]): void => {
    const result = writeLocalSpellbook(window.localStorage, next);
    if (result.status === 'ready') {
      setEntries(next);
      setNotice({ tone: 'ok', text: 'The local collection was updated.', code: 'SPELLBOOK_LOCAL_UPDATED' });
    } else setNotice({ tone: 'error', text: result.message, code: result.code });
  };

  const exportFile = (): void => {
    const href = URL.createObjectURL(new Blob([serializeSpellbook(entries)], { type: 'application/json' }));
    const anchor = document.createElement('a');
    anchor.href = href;
    anchor.download = `apocky-spellbook-${new Date().toISOString().slice(0, 10)}.json`;
    anchor.click();
    URL.revokeObjectURL(href);
    setNotice({ tone: 'ok', text: 'A verified JSON export was created on this device.', code: 'SPELLBOOK_EXPORTED' });
  };

  const importFile = async (event: ChangeEvent<HTMLInputElement>): Promise<void> => {
    const file = event.target.files?.[0];
    event.target.value = '';
    if (!file) return;
    if (file.size > 512 * 1024) {
      setNotice({ tone: 'error', text: 'That file exceeds the 512 KiB import boundary.', code: 'SPELLBOOK_SCHEMA_REJECTED' });
      return;
    }
    try {
      const parsed = parseSpellbook(await file.text());
      if (!parsed.valid || !parsed.payload) {
        setNotice({ tone: 'error', text: 'The file failed its version or integrity checks and was not imported.', code: 'SPELLBOOK_SCHEMA_REJECTED' });
        return;
      }
      persist(parsed.payload.entries);
      setNotice({ tone: 'ok', text: `${parsed.payload.entries.length} verified entries replaced the local collection.`, code: 'SPELLBOOK_IMPORTED' });
    } catch {
      setNotice({ tone: 'error', text: 'The file could not be read and the current collection was preserved.', code: 'SPELLBOOK_SCHEMA_REJECTED' });
    }
  };

  const deleteAll = (): void => {
    if (!window.confirm('Delete every entry stored by the Apocky Spellbook in this browser? Export first if you need a copy.')) return;
    try {
      window.localStorage.removeItem(LOCAL_SPELLBOOK_KEY);
      setEntries([]);
      setNotice({ tone: 'ok', text: 'All local Spellbook entries were deleted.', code: 'SPELLBOOK_LOCAL_CLEARED' });
    } catch {
      setNotice({ tone: 'error', text: 'Browser storage could not be changed.', code: 'SPELLBOOK_LOCAL_UNAVAILABLE' });
    }
  };

  const copySource = async (entry: SpellbookEntry): Promise<void> => {
    try {
      await navigator.clipboard.writeText(entry.input);
      setNotice({ tone: 'ok', text: 'The source was copied. Paste it into Spellcraft or Sigils when you choose.', code: 'SPELLBOOK_SOURCE_COPIED' });
    } catch {
      setNotice({ tone: 'error', text: 'Clipboard access is unavailable.', code: 'SPELLBOOK_CLIPBOARD_UNAVAILABLE' });
    }
  };

  if (!ready) return <div className={styles.spellbookPanel}><p role="status">Opening the local collection…</p></div>;

  return (
    <div className={styles.spellbookPanel}>
      <div className={styles.collectionHeader}>
        <div><p className={styles.resultKicker}>THIS BROWSER ONLY</p><h2>{entries.length} {entries.length === 1 ? 'working' : 'workings'}</h2><p>Nothing was uploaded. Imports replace the local collection only after every entry passes its integrity receipt.</p></div>
        <div className={styles.collectionActions}>
          <button type="button" onClick={() => fileRef.current?.click()}>Import verified JSON</button>
          <input ref={fileRef} className={styles.visuallyHidden} type="file" accept="application/json,.json" aria-label="Import verified Spellbook JSON" onChange={(event) => { void importFile(event); }} />
          <button type="button" onClick={exportFile} disabled={entries.length === 0}>Export all</button>
          <button className={styles.dangerButton} type="button" onClick={deleteAll} disabled={entries.length === 0}>Delete all</button>
        </div>
      </div>

      {notice ? <p className={notice.tone === 'error' ? styles.error : styles.notice} role="status">{notice.text} <code>{notice.code}</code></p> : null}

      {entries.length === 0 ? (
        <div className={styles.emptyCollection}>
          <span aria-hidden="true">◇</span><h3>Your private shelf is empty.</h3><p>Compile a valid working and choose “Save explicitly.” Typing and compiling alone never writes here.</p>
          <Link href="/spellcraft">Create the first working →</Link>
        </div>
      ) : (
        <ol className={styles.spellbookGrid}>
          {entries.map((entry) => (
            <li key={entry.entryId}>
              <article>
                <div className={styles.cardTop}><span>{entry.program.form}</span><code>{entry.entryId.slice(-8)}</code></div>
                <h3>{entry.label}</h3>
                <p>{entry.interpretation.text}</p>
                <dl><div><dt>Saved</dt><dd>{entry.savedAt ? new Date(entry.savedAt).toLocaleString() : 'Unstamped'}</dd></div><div><dt>Engine</dt><dd>{entry.receipt.engineVersion}</dd></div><div><dt>Seal</dt><dd><code>{entry.contentHash.slice(0, 16)}</code></dd></div></dl>
                <div className={styles.buttonRow}>
                  <button type="button" onClick={() => { void copySource(entry); }}>Copy source</button>
                  <button className={styles.dangerButton} type="button" onClick={() => persist(entries.filter((candidate) => candidate.entryId !== entry.entryId))}>Delete</button>
                </div>
              </article>
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}
