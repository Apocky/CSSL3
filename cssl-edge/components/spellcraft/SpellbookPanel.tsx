import Link from 'next/link';
import { ChangeEvent, useEffect, useRef, useState } from 'react';

import { LOCAL_SPELLBOOK_KEY, readLocalSpellbook, writeLocalSpellbook } from '../../lib/spellbook-local';
import { parseSpellbook, serializeSpellbook, type SpellbookEntry } from '../../lib/spellcraft';
import styles from '../../styles/SymbolicStudio.module.css';

type Notice = { readonly tone: 'ok' | 'error'; readonly text: string; readonly code: string };

export default function SpellbookPanel(): JSX.Element {
  const [entries, setEntries] = useState<readonly SpellbookEntry[]>([]);
  const [ready, setReady] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [notice, setNotice] = useState<Notice | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    try {
      const result = readLocalSpellbook(window.localStorage);
      if (result.status === 'ready') { setEntries(result.entries); setLoaded(true); }
      else setNotice({ tone: 'error', text: result.message, code: result.code });
    } catch {
      setNotice({ tone: 'error', text: 'This browser is blocking storage. You can still create a spell or download a sigil.', code: 'SPELLBOOK_LOCAL_UNAVAILABLE' });
    }
    setReady(true);
  }, []);

  const persist = (next: readonly SpellbookEntry[]): boolean => {
    try {
      const result = writeLocalSpellbook(window.localStorage, next);
      if (result.status === 'ready') {
        setEntries(next);
        setLoaded(true);
        setNotice({ tone: 'ok', text: 'The local collection was updated.', code: 'SPELLBOOK_LOCAL_UPDATED' });
        return true;
      }
      setNotice({ tone: 'error', text: result.message, code: result.code });
    } catch {
      setNotice({ tone: 'error', text: 'This browser is blocking storage. Your current collection was preserved.', code: 'SPELLBOOK_LOCAL_UNAVAILABLE' });
    }
    return false;
  };

  const exportFile = (): void => {
    const href = URL.createObjectURL(new Blob([serializeSpellbook(entries)], { type: 'application/json' }));
    const anchor = document.createElement('a');
    anchor.href = href;
    anchor.download = `apocky-spellbook-${new Date().toISOString().slice(0, 10)}.json`;
    anchor.click();
    URL.revokeObjectURL(href);
    setNotice({ tone: 'ok', text: 'Your spellbook was downloaded. Keep the file to restore it here or in another browser.', code: 'SPELLBOOK_EXPORTED' });
  };

  const importFile = async (event: ChangeEvent<HTMLInputElement>): Promise<void> => {
    const file = event.target.files?.[0];
    event.target.value = '';
    if (!file) return;
    if (file.size > 512 * 1024) {
      setNotice({ tone: 'error', text: 'That file is too large. Choose a spellbook file smaller than 512 KB.', code: 'SPELLBOOK_SCHEMA_REJECTED' });
      return;
    }
    try {
      const parsed = parseSpellbook(await file.text());
      if (!parsed.valid || !parsed.payload) {
        setNotice({ tone: 'error', text: 'The file failed its version or integrity checks and was not imported.', code: 'SPELLBOOK_SCHEMA_REJECTED' });
        return;
      }
      if (persist(parsed.payload.entries)) setNotice({ tone: 'ok', text: `${parsed.payload.entries.length} saved spells were imported into this browser.`, code: 'SPELLBOOK_IMPORTED' });
    } catch {
      setNotice({ tone: 'error', text: 'The file could not be read and the current collection was preserved.', code: 'SPELLBOOK_SCHEMA_REJECTED' });
    }
  };

  const deleteAll = (): void => {
    if (!window.confirm('Delete every entry stored by the Apocky Spellbook in this browser? Export first if you need a copy.')) return;
    try {
      window.localStorage.removeItem(LOCAL_SPELLBOOK_KEY);
      setEntries([]);
      setLoaded(true);
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
        <div><h2>{loaded ? `${entries.length} saved ${entries.length === 1 ? 'spell' : 'spells'}` : 'Your saved collection'}</h2><p>Importing a spellbook replaces this collection. Export first if you want to keep a copy.</p></div>
        <div className={styles.collectionActions}>
          <button type="button" onClick={() => fileRef.current?.click()}>Import a spellbook</button>
          <input ref={fileRef} className={styles.visuallyHidden} type="file" accept="application/json,.json" aria-label="Import verified Spellbook JSON" onChange={(event) => { void importFile(event); }} />
          <button type="button" onClick={exportFile} disabled={entries.length === 0}>Export all</button>
          <button className={styles.dangerButton} type="button" onClick={deleteAll} disabled={entries.length === 0}>Delete all</button>
        </div>
      </div>

      {notice ? <p className={notice.tone === 'error' ? styles.error : styles.notice} role="status">{notice.text}</p> : null}

      {!loaded ? (
        <div className={styles.emptyCollection}><h3>We couldn’t open the saved collection.</h3><p>Nothing was changed. You can still use the creative tools, or try again in a browser that allows storage.</p><Link href="/spellcraft">Create a spell →</Link></div>
      ) : entries.length === 0 ? (
        <div className={styles.emptyCollection}>
          <span aria-hidden="true">◇</span><h3>A place for the words you want to keep.</h3><p>Create a spell, then choose “Save to spellbook.” It will be waiting here.</p>
          <Link href="/spellcraft">Create your first spell →</Link>
        </div>
      ) : (
        <ol className={styles.spellbookGrid}>
          {entries.map((entry) => (
            <li key={entry.entryId}>
              <article>
                <h3>{entry.label}</h3>
                <p>{entry.interpretation.text}</p>
                <p className={styles.smallNote}>{entry.savedAt ? `Saved ${new Date(entry.savedAt).toLocaleDateString()}` : 'Saved in this browser'}</p>
                <div className={styles.buttonRow}>
                  <button type="button" onClick={() => { void copySource(entry); }}>Copy symbolic words</button>
                  <button className={styles.dangerButton} type="button" onClick={() => persist(entries.filter((candidate) => candidate.entryId !== entry.entryId))}>Delete</button>
                </div>
                <details className={styles.customize}><summary>How it works</summary><p><code>{entry.input}</code></p><dl><div><dt>Engine</dt><dd>{entry.receipt.engineVersion}</dd></div><div><dt>Seal</dt><dd><code>{entry.contentHash}</code></dd></div></dl></details>
              </article>
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}
