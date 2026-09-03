import Link from 'next/link';
import { useCallback, useEffect, useMemo, useState } from 'react';

import type { MemoryPublic, MemoryType } from '../../lib/mneme/types';
import { authFetch } from '../../lib/browser-auth';
import { readLocalSpellbook } from '../../lib/spellbook-local';
import { useSiteSession } from '../hub/SiteSession';
import HelpTip from '../ui/HelpTip';
import styles from './MemoryExperience.module.css';

interface MnemeHealth {
  readonly profile_ready: boolean;
  readonly storage_ready: boolean;
  readonly semantic_ready: boolean;
}

interface ApiErrorBody {
  readonly error?: string;
  readonly code?: string;
}

type PrivatePhase = 'session' | 'health' | 'locked' | 'ready' | 'degraded';

function apiError(payload: ApiErrorBody, status: number): string {
  const code = payload.code ?? `MNEME_HTTP_${status}`;
  return `${payload.error ?? 'Private memory could not answer.'} (${code})`;
}

function slugPart(value: string): string {
  const normalized = value.toLocaleLowerCase()
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 48);
  return normalized || 'note';
}

function bytesToHex(value: string): string {
  return Array.from(new TextEncoder().encode(value), (byte) => byte.toString(16).padStart(2, '0')).join('');
}

function makeCsl(label: string, note: string, existingTopic?: string): { csl: string; topicKey: string } {
  const topicKey = existingTopic ?? `private.memory.${slugPart(label)}`;
  return { csl: `${topicKey} ⊗ utf8.${bytesToHex(note)}`, topicKey };
}

function formatDate(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat('en-US', { dateStyle: 'medium', timeStyle: 'short' }).format(date);
}

function PrivateMemoryWorkbench(): JSX.Element {
  const { access, authenticated, refresh } = useSiteSession();
  const [phase, setPhase] = useState<PrivatePhase>('session');
  const [health, setHealth] = useState<MnemeHealth | null>(null);
  const [memories, setMemories] = useState<readonly MemoryPublic[]>([]);
  const [notice, setNotice] = useState<string>('');
  const [label, setLabel] = useState('');
  const [note, setNote] = useState('');
  const [type, setType] = useState<Extract<MemoryType, 'fact' | 'instruction'>>('fact');
  const [correctionTopic, setCorrectionTopic] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  const [recall, setRecall] = useState<{ text: string; confidence: number; citations: readonly string[] } | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const loadPrivate = useCallback(async (signal?: AbortSignal): Promise<void> => {
    setPhase('health');
    setNotice('');
    try {
      const healthResponse = await authFetch('/api/mneme/me/health', { cache: 'no-store', signal });
      const healthPayload = await healthResponse.json() as MnemeHealth & ApiErrorBody;
      if (!healthResponse.ok) throw new Error(apiError(healthPayload, healthResponse.status));
      setHealth(healthPayload);
      if (!healthPayload.profile_ready || !healthPayload.storage_ready) {
        setMemories([]);
        setPhase('locked');
        return;
      }

      const listResponse = await authFetch('/api/mneme/me/list?limit=50', { cache: 'no-store', signal });
      const listPayload = await listResponse.json() as { memories?: MemoryPublic[] } & ApiErrorBody;
      if (!listResponse.ok) throw new Error(apiError(listPayload, listResponse.status));
      setMemories(Array.isArray(listPayload.memories) ? listPayload.memories : []);
      setPhase('ready');
    } catch (error) {
      if (signal?.aborted) return;
      setPhase('degraded');
      setNotice(error instanceof Error ? error.message : 'Private memory could not answer.');
    }
  }, []);

  useEffect(() => {
    if (!authenticated) {
      setHealth(null);
      setMemories([]);
      setPhase(access === 'checking' ? 'session' : access === 'unavailable' ? 'degraded' : 'locked');
      return undefined;
    }
    const controller = new AbortController();
    void loadPrivate(controller.signal);
    return () => controller.abort();
  }, [access, authenticated, loadPrivate]);

  const submitRemember = async (event: React.FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    if (!health?.semantic_ready || !label.trim() || !note.trim()) return;
    setBusy('remember');
    setNotice('');
    const { csl, topicKey } = makeCsl(label, note, correctionTopic ?? undefined);
    try {
      const response = await authFetch('/api/mneme/me/remember', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ csl, paraphrase: note.trim(), topic_key: topicKey, type }),
      });
      const payload = await response.json() as ApiErrorBody;
      if (!response.ok) throw new Error(apiError(payload, response.status));
      setLabel('');
      setNote('');
      setCorrectionTopic(null);
      setNotice('Saved to your signed-in Mneme profile.');
      await loadPrivate();
    } catch (error) {
      setNotice(error instanceof Error ? error.message : 'The memory was not saved.');
    } finally {
      setBusy(null);
    }
  };

  const submitRecall = async (event: React.FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    if (!health?.semantic_ready || !query.trim()) return;
    setBusy('recall');
    setNotice('');
    setRecall(null);
    try {
      const response = await authFetch('/api/mneme/me/recall', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ query: query.trim(), k: 5 }),
      });
      const payload = await response.json() as ApiErrorBody & { result_nl?: string; confidence?: number; citations?: string[] };
      if (!response.ok) throw new Error(apiError(payload, response.status));
      setRecall({
        text: payload.result_nl ?? 'No answer was returned.',
        confidence: typeof payload.confidence === 'number' ? payload.confidence : 0,
        citations: Array.isArray(payload.citations) ? payload.citations : [],
      });
    } catch (error) {
      setNotice(error instanceof Error ? error.message : 'Private recall could not answer.');
    } finally {
      setBusy(null);
    }
  };

  const exportPrivate = async (): Promise<void> => {
    if (!health?.storage_ready) return;
    setBusy('export');
    setNotice('');
    try {
      const response = await authFetch('/api/mneme/me/export', { cache: 'no-store' });
      const payload = await response.json() as ApiErrorBody;
      if (!response.ok) throw new Error(apiError(payload, response.status));
      const url = URL.createObjectURL(new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' }));
      const anchor = document.createElement('a');
      anchor.href = url;
      anchor.download = `apocky-private-memory-${new Date().toISOString().slice(0, 10)}.json`;
      anchor.click();
      URL.revokeObjectURL(url);
      setNotice('Private memory export downloaded to this device.');
    } catch (error) {
      setNotice(error instanceof Error ? error.message : 'Private memory export failed.');
    } finally {
      setBusy(null);
    }
  };

  const forget = async (memoryId: string): Promise<void> => {
    setBusy(`delete-${memoryId}`);
    setNotice('');
    try {
      const response = await authFetch('/api/mneme/me/forget', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ memory_id: memoryId, reason: 'Deleted by the signed-in member from Memory & tools.' }),
      });
      const payload = await response.json() as ApiErrorBody & { revoked?: boolean };
      if (!response.ok) throw new Error(apiError(payload, response.status));
      setConfirmDelete(null);
      setNotice(payload.revoked ? 'Memory revoked from active recall.' : 'That memory was already absent or inactive.');
      await loadPrivate();
    } catch (error) {
      setNotice(error instanceof Error ? error.message : 'The memory was not deleted.');
    } finally {
      setBusy(null);
    }
  };

  if (access === 'checking' || phase === 'session' || phase === 'health') {
    return <div className={styles.privateState} role="status">Checking the signed-in memory boundary…</div>;
  }

  if (!authenticated) {
    return (
      <div className={styles.privateState} data-state="locked">
        <p className={styles.stateLabel}>LOCKED · NO PRIVATE DATA LOADED</p>
        <h3>{access === 'unavailable' ? 'Sign-in verification is temporarily unavailable.' : 'Sign in to open your own memory.'}</h3>
        <p>
          This public page cannot name, list, or change a Mneme profile. The private tools appear only after the server verifies your session.
        </p>
        <div className={styles.inlineActions}>
          {access === 'unavailable' ? (
            <button type="button" onClick={() => { void refresh(); }}>Retry sign-in check</button>
          ) : (
            <Link href="/login?next=%2Fmemory-tools">Sign in safely</Link>
          )}
          <Link href="/spellbook">Use device-local Spellbook instead</Link>
        </div>
      </div>
    );
  }

  if (phase === 'degraded') {
    return (
      <div className={styles.privateState} data-state="degraded">
        <p className={styles.stateLabel}>DEGRADED · NO CHANGES SENT</p>
        <h3>Your session is present, but private memory did not open.</h3>
        <p>{notice || 'The private memory service did not return a usable state.'}</p>
        <button type="button" onClick={() => { void loadPrivate(); }}>Retry private memory</button>
      </div>
    );
  }

  if (phase === 'locked' || !health?.storage_ready) {
    return (
      <div className={styles.privateState} data-state="locked">
        <p className={styles.stateLabel}>SIGNED IN · PROFILE NOT PROVISIONED</p>
        <h3>Your identity is verified; a private Mneme profile is not ready here.</h3>
        <p>
          No profile was created automatically and no memory controls are being simulated. Use the local Spellbook now, or return after private-profile onboarding is configured.
        </p>
        <div className={styles.inlineActions}>
          <button type="button" onClick={() => { void loadPrivate(); }}>Check again</button>
          <Link href="/spellbook">Open local Spellbook</Link>
        </div>
      </div>
    );
  }

  return (
    <div className={styles.workbench}>
      <div className={styles.workbenchHead}>
        <div>
          <p className={styles.stateLabel}>SIGNED IN · USER-BOUND PROFILE</p>
          <h3>Your private Mneme</h3>
          <p>Only the server-derived profile for this verified session can be reached. The browser never chooses a profile name.</p>
        </div>
        <button type="button" onClick={() => { void exportPrivate(); }} disabled={busy !== null}>Export my data</button>
      </div>

      {!health.semantic_ready ? (
        <div className={styles.degradedBar} role="status">
          Stored memories can be listed, exported, and revoked. Semantic remember and recall stay locked until their model dependencies are connected.
        </div>
      ) : (
        <div className={styles.forms}>
          <form onSubmit={(event) => { void submitRecall(event); }}>
            <div className={styles.formTitle}>
              <h4>Ask your memory</h4>
              <HelpTip label="How private recall works">Your question is sent only to your signed-in profile. Answers cite stored memory identifiers and are not public search results.</HelpTip>
            </div>
            <label>
              <span>What are you trying to remember?</span>
              <input value={query} onChange={(event) => setQuery(event.currentTarget.value)} maxLength={512} placeholder="What did I decide about…?" />
            </label>
            <button type="submit" disabled={busy !== null || !query.trim()}>{busy === 'recall' ? 'Remembering…' : 'Recall'}</button>
          </form>

          <form onSubmit={(event) => { void submitRemember(event); }}>
            <div className={styles.formTitle}>
              <h4>{correctionTopic ? 'Correct this memory' : 'Remember something'}</h4>
              <HelpTip label="How corrections work">A correction keeps the same topic label. Mneme stores the new version and marks the older active version as superseded; it does not silently rewrite history.</HelpTip>
            </div>
            <label>
              <span>Short label</span>
              <input value={label} onChange={(event) => setLabel(event.currentTarget.value)} maxLength={80} placeholder="Preferred creative rhythm" disabled={Boolean(correctionTopic)} />
            </label>
            <label>
              <span>Memory in your words</span>
              <textarea value={note} onChange={(event) => setNote(event.currentTarget.value)} maxLength={350} rows={4} placeholder="I do my clearest work when…" />
            </label>
            <label>
              <span>Use it as</span>
              <select value={type} onChange={(event) => setType(event.currentTarget.value as typeof type)} disabled={Boolean(correctionTopic)}>
                <option value="fact">Something true for me</option>
                <option value="instruction">A standing preference</option>
              </select>
            </label>
            <div className={styles.inlineActions}>
              <button type="submit" disabled={busy !== null || !label.trim() || !note.trim()}>{busy === 'remember' ? 'Saving…' : correctionTopic ? 'Save correction' : 'Remember this'}</button>
              {correctionTopic ? <button type="button" onClick={() => { setCorrectionTopic(null); setLabel(''); setNote(''); }}>Cancel correction</button> : null}
            </div>
          </form>
        </div>
      )}

      {recall ? (
        <section className={styles.recallResult} aria-labelledby="recall-result-title" aria-live="polite">
          <p className={styles.stateLabel}>SYNTHESIZED RECALL · {Math.round(recall.confidence * 100)}% MODEL CONFIDENCE</p>
          <h4 id="recall-result-title">Memory’s answer</h4>
          <p>{recall.text}</p>
          <p className={styles.citations}>{recall.citations.length > 0 ? `Memory IDs: ${recall.citations.join(', ')}` : 'No memory citations were returned.'}</p>
        </section>
      ) : null}

      {notice ? <p className={styles.notice} role="status" aria-live="polite">{notice}</p> : null}

      <section className={styles.memoryList} aria-labelledby="private-memory-list-title">
        <div className={styles.listHead}>
          <h4 id="private-memory-list-title">Active memories</h4>
          <span>{memories.length} shown</span>
        </div>
        {memories.length === 0 ? (
          <p className={styles.empty}>Nothing is stored in this profile yet.</p>
        ) : memories.map((memory) => (
          <details key={memory.id} className={styles.memoryItem}>
            <summary>
              <span>{memory.paraphrase}</span>
              <small>{memory.type} · {formatDate(memory.created_at)}</small>
            </summary>
            <div className={styles.memoryBody}>
              <dl>
                <div><dt>Topic</dt><dd>{memory.topic_key ?? 'No replacement topic'}</dd></div>
                <div><dt>Memory ID</dt><dd><code>{memory.id}</code></dd></div>
              </dl>
              <details className={styles.technical}>
                <summary>Show technical source</summary>
                <code>{memory.csl}</code>
              </details>
              <div className={styles.inlineActions}>
                {health.semantic_ready && memory.topic_key && (memory.type === 'fact' || memory.type === 'instruction') ? (
                  <button type="button" onClick={() => {
                    setCorrectionTopic(memory.topic_key);
                    setLabel(memory.topic_key?.split('.').at(-1)?.replace(/-/g, ' ') ?? 'Memory');
                    setNote(memory.paraphrase);
                    setType(memory.type === 'instruction' ? 'instruction' : 'fact');
                  }}>Correct</button>
                ) : null}
                <button type="button" className={styles.deleteButton} onClick={() => setConfirmDelete(memory.id)}>Delete</button>
              </div>
              {confirmDelete === memory.id ? (
                <div className={styles.confirm} role="group" aria-label="Confirm memory deletion">
                  <p>Revoke this memory and its supersession chain from active recall?</p>
                  <button type="button" onClick={() => { void forget(memory.id); }} disabled={busy !== null}>{busy === `delete-${memory.id}` ? 'Deleting…' : 'Yes, delete it'}</button>
                  <button type="button" onClick={() => setConfirmDelete(null)}>Keep it</button>
                </div>
              ) : null}
            </div>
          </details>
        ))}
      </section>
    </div>
  );
}

export default function MemoryExperience(): JSX.Element {
  const [spellbookCount, setSpellbookCount] = useState<number | null>(null);
  const [spellbookState, setSpellbookState] = useState<'ready' | 'unavailable' | 'rejected'>('ready');

  useEffect(() => {
    const result = readLocalSpellbook(window.localStorage);
    if (result.status === 'ready') {
      setSpellbookCount(result.entries.length);
      setSpellbookState('ready');
    } else {
      setSpellbookState(result.status);
    }
  }, []);

  const localSummary = useMemo(() => {
    if (spellbookState === 'unavailable') return 'Browser storage is unavailable. Nothing was read or changed.';
    if (spellbookState === 'rejected') return 'A local Spellbook record exists but did not pass its integrity checks.';
    if (spellbookCount === null) return 'Checking only this browser…';
    return `${spellbookCount} ${spellbookCount === 1 ? 'working' : 'workings'} saved in this browser.`;
  }, [spellbookCount, spellbookState]);

  return (
    <>
      <nav className={styles.breadcrumbs} aria-label="Breadcrumb">
        <ol>
          <li><Link href="/">Home</Link></li>
          <li><Link href="/atlas">Atlas</Link></li>
          <li aria-current="page">Memory &amp; tools</li>
        </ol>
      </nav>

      <section className={styles.taskSection} aria-labelledby="memory-task-title">
        <div className={styles.sectionHeading}>
          <div>
            <p className={styles.kicker}>Start with what you want to do</p>
            <h2 id="memory-task-title">Choose a door. See its boundary first.</h2>
          </div>
          <HelpTip label="Why memory has three layers">Public records can be read by anyone. Device-local records stay in this browser. Mneme is account-bound and stays locked until the server verifies one owned profile.</HelpTip>
        </div>
        <div className={styles.taskGrid}>
          <Link href="/akashic-records"><span>READ</span><strong>Explore public ideas and conversations</strong><small>Open to everyone · hash-sealed sources</small></Link>
          <a href="#private-mneme"><span>REMEMBER</span><strong>Recall or correct my private memory</strong><small>Sign-in and profile check required</small></a>
          <Link href="/spellbook"><span>MAKE</span><strong>Open workings saved on this device</strong><small>{localSummary}</small></Link>
          <Link href="/atlas?view=index&node=memory-tools"><span>MAP</span><strong>See how every public surface connects</strong><small>Shareable filters · four views</small></Link>
        </div>
      </section>

      <section className={styles.layerSection} aria-labelledby="memory-layers-title">
        <div className={styles.sectionHeading}>
          <div>
            <p className={styles.kicker}>Three different promises</p>
            <h2 id="memory-layers-title">“Memory” does not mean one hidden bucket.</h2>
          </div>
        </div>
        <div className={styles.layerGrid}>
          <article data-layer="public">
            <p className={styles.layerTag}>PUBLIC MEMORY</p>
            <h3>Published library</h3>
            <p>Approved works and public-safe conversations. Anyone can read them; each record names its source and publication boundary.</p>
            <details>
              <summary>Preview what is inside</summary>
              <ul><li>204 approved Medium works</li><li>Role-bearing public Codex conversations</li><li>Hash-sealed manifest and stable record pages</li></ul>
            </details>
            <Link href="/akashic-records">Search the library →</Link>
          </article>
          <article data-layer="local">
            <p className={styles.layerTag}>DEVICE-LOCAL MEMORY</p>
            <h3>This browser’s shelf</h3>
            <p>Spellbook saves and quest progress live in local browser storage. They are not an account history and do not follow you automatically.</p>
            <details>
              <summary>Preview this device</summary>
              <p>{localSummary}</p>
            </details>
            <Link href="/spellbook">Inspect or erase local saves →</Link>
          </article>
          <article data-layer="private">
            <p className={styles.layerTag}>SIGNED-IN PRIVATE MNEME</p>
            <h3>Your governed profile</h3>
            <p>Remember, recall, correct, export, and revoke only after the server binds the current session to one opaque profile.</p>
            <details>
              <summary>See the authorization path</summary>
              <ol><li>Verify the sign-in session.</li><li>Derive the profile on the server.</li><li>Confirm that profile already exists.</li><li>Unlock only capabilities whose dependencies answer.</li></ol>
            </details>
            <a href="#private-mneme">Check my access →</a>
          </article>
        </div>
      </section>

      <section className={styles.conversationSection} aria-labelledby="conversation-lenses-title">
        <div className={styles.sectionHeading}>
          <div>
            <p className={styles.kicker}>Human routes through the archive</p>
            <h2 id="conversation-lenses-title">Start with lived questions, not system jargon.</h2>
          </div>
          <p>Authored essays and role-labeled dialogue are different source types. The interfaces keep that distinction visible.</p>
        </div>
        <div className={styles.lensGrid}>
          <article>
            <p className={styles.layerTag}>AUTHORED ESSAY · SHAWN APOCKY</p>
            <h3>Unconditional Love</h3>
            <p className={styles.theme}>Personal and spiritual writing · “Spicy Brain. Spicy Heart.”</p>
            <Link href="/akashic-records/unconditional-love-ce0769c96e0f">Read the published essay →</Link>
          </article>
          <article>
            <p className={styles.layerTag}>AUTHORED ESSAY · SHAWN APOCKY</p>
            <h3>Nothing is real, therefore everything is real.</h3>
            <p className={styles.theme}>A short path from nihilism toward absurdism, meaning, and chosen care.</p>
            <Link href="/akashic-records/nothing-is-real-therefore-everything-is-real-b82e91e167e9">Read the published essay →</Link>
          </article>
          <article>
            <p className={styles.layerTag}>AUTHORED ESSAY · SHAWN APOCKY</p>
            <h3>Meditation.</h3>
            <p className={styles.theme}>A grounded spiritual practice in the author’s own words.</p>
            <Link href="/akashic-records/meditation-c79201e9ffb4">Read the published essay →</Link>
          </article>
          <article>
            <p className={styles.layerTag}>DIALOGUE · USER AND AI ROLES LABELED</p>
            <h3>Selected public conversations</h3>
            <p className={styles.theme}>The existing selected public conversations are expanding into a broader privacy-sanitized corpus. The conversation guide reports the exact current denominator and keeps Shawn’s words, AI replies, curator paraphrases, and allegorical lenses separately labeled.</p>
            <Link href="/conversations">Explore public conversations →</Link>
          </article>
        </div>
        <p className={styles.corpusBoundary}>
          <strong>Publication boundary:</strong> broader ChatGPT, Claude, Anthropic, Gemini, Copilot, Codewhale, and other provider exports remain local/private candidates until the conversation publication pipeline sanitizes and admits them. Raw exports, restricted T97 material, secrets, third-party private data, and unreviewed personal content are not public or searchable here.
        </p>
      </section>

      <section id="private-mneme" className={styles.privateSection} aria-labelledby="private-mneme-title">
        <div className={styles.sectionHeading}>
          <div>
            <p className={styles.kicker}>Private controls</p>
            <h2 id="private-mneme-title">Your memory, if the whole chain is real.</h2>
          </div>
          <p>Unavailable controls are replaced by an exact reason and next step. This page never pretends a public archive, a browser save, and an account profile are the same thing.</p>
        </div>
        <PrivateMemoryWorkbench />
      </section>
    </>
  );
}
