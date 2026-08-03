import type { NextPage } from 'next';
import Link from 'next/link';
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type KeyboardEvent,
} from 'react';

import AdminLayout from '../../components/AdminLayout';
import { authFetch } from '../../lib/browser-auth';
import type {
  RuntimeHealthProjection,
  RuntimeObjectiveProjection,
} from '../../lib/apocv4/runtime-proxy';
import styles from '../../styles/AdminApex.module.css';

type ApiFailure = { error?: string; upstream_status?: number };
type JsonRecord = Record<string, unknown>;
type TurnState = 'working' | 'accepted' | 'failed' | 'stopped';

interface ProposalView {
  summary: string;
  steps: string[];
  countercase: string | null;
  falsifier: string | null;
  sourceRefs: string[];
}

interface EvidenceView {
  status: string;
  terminalReason: string | null;
  candidateDigest: string | null;
  testDigest: string | null;
  checkpointDigest: string | null;
  facultyTeamId: string | null;
  observedAt: string;
  latencyMs: number;
  upstreamStatus: number;
  authMode: string | null;
  registryRef: string | null;
  bindingRef: string | null;
  principalRef: string | null;
}

interface ConversationTurn {
  id: string;
  prompt: string;
  reply: string | null;
  proposal: ProposalView | null;
  evidence: EvidenceView | null;
  state: TurnState;
  error: string | null;
  createdAt: string;
}

interface ConversationThread {
  id: string;
  title: string;
  createdAt: string;
  turns: ConversationTurn[];
}

interface SessionState {
  activeThreadId: string;
  threads: ConversationThread[];
}

const SESSION_KEY = 'apocky.apocv4.communication-hub.v1';
const MAX_OBJECTIVE_LENGTH = 16_384;

const STARTERS = [
  'Inspect this system and find the highest-leverage reversible improvement.',
  'Review a load-bearing code path and propose the smallest verified repair.',
  'Turn this idea into an exact implementation with a test and rollback.',
] as const;

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function stringValue(value: unknown): string | null {
  return typeof value === 'string' && value.trim() ? value : null;
}

function stringList(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((item) => {
    const direct = stringValue(item);
    if (direct) return [direct];
    if (!isRecord(item)) return [];
    const candidate = stringValue(item.ref)
      ?? stringValue(item.source_ref)
      ?? stringValue(item.path)
      ?? stringValue(item.title);
    return candidate ? [candidate] : [];
  }).slice(0, 24);
}

function proposalAt(value: unknown): ProposalView | null {
  if (!isRecord(value)) return null;
  const summary = stringValue(value.summary);
  const steps = stringList(value.steps);
  if (!summary && steps.length === 0) return null;
  return {
    summary: summary ?? 'Apocrypha returned an implementation sequence.',
    steps,
    countercase: stringValue(value.countercase),
    falsifier: stringValue(value.falsifier),
    sourceRefs: stringList(value.source_refs ?? value.sources),
  };
}

function proposalFrom(result: RuntimeObjectiveProjection): ProposalView | null {
  for (const attempt of [...result.model_reported.attempts].reverse()) {
    const council = isRecord(attempt.council_decision) ? attempt.council_decision : null;
    if (!council) continue;
    const candidates = [
      council.candidate,
      council.selected_candidate,
      isRecord(council.selection) ? council.selection.candidate : null,
    ];
    for (const candidate of candidates) {
      if (!isRecord(candidate)) continue;
      const proposal = proposalAt(candidate.proposal) ?? proposalAt(candidate);
      if (!proposal) continue;
      const sourceRefs = proposal.sourceRefs.length > 0
        ? proposal.sourceRefs
        : stringList(candidate.source_refs ?? council.source_refs);
      return {
        ...proposal,
        countercase: proposal.countercase ?? stringValue(candidate.countercase),
        falsifier: proposal.falsifier ?? stringValue(candidate.falsifier),
        sourceRefs,
      };
    }
  }
  return null;
}

function evidenceFrom(result: RuntimeObjectiveProjection): EvidenceView {
  const runtime = result.observed.runtime;
  const receipt = result.observed.receipt;
  return {
    status: stringValue(runtime.status) ?? 'UNKNOWN',
    terminalReason: stringValue(runtime.terminal_reason),
    candidateDigest: stringValue(runtime.accepted_candidate_digest),
    testDigest: stringValue(runtime.last_test_run_digest),
    checkpointDigest: stringValue(runtime.checkpoint_digest),
    facultyTeamId: stringValue(runtime.faculty_team_id),
    observedAt: receipt.observed_at,
    latencyMs: receipt.latency_ms,
    upstreamStatus: receipt.upstream_status,
    authMode: receipt.auth_mode,
    registryRef: receipt.auth_registry_ref,
    bindingRef: receipt.binding_ref,
    principalRef: receipt.principal_ref,
  };
}

function shortDigest(value: string | null): string {
  if (!value || value.length < 18) return value ?? 'not observed';
  return `${value.slice(0, 10)}…${value.slice(-8)}`;
}

function errorMessage(payload: ApiFailure, status: number): string {
  const upstream = typeof payload.upstream_status === 'number'
    ? ` · runtime HTTP ${payload.upstream_status}`
    : '';
  return `${payload.error ?? `request_failed_${status}`}${upstream}`;
}

function freshThread(): ConversationThread {
  return {
    id: crypto.randomUUID().toLowerCase(),
    title: 'New conversation',
    createdAt: new Date().toISOString(),
    turns: [],
  };
}

function validSession(value: unknown): value is SessionState {
  if (!isRecord(value) || typeof value.activeThreadId !== 'string' || !Array.isArray(value.threads)) return false;
  return value.threads.every((thread) => (
    isRecord(thread)
    && typeof thread.id === 'string'
    && typeof thread.title === 'string'
    && typeof thread.createdAt === 'string'
    && Array.isArray(thread.turns)
  ));
}

function displayTime(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? 'now'
    : date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

const Apex: NextPage = () => {
  const [adminAuthorized, setAdminAuthorized] = useState(false);
  const [health, setHealth] = useState<RuntimeHealthProjection | null>(null);
  const [healthError, setHealthError] = useState<string | null>(null);
  const [healthBusy, setHealthBusy] = useState(false);
  const [threads, setThreads] = useState<ConversationThread[]>([]);
  const [activeThreadId, setActiveThreadId] = useState('');
  const [hydrated, setHydrated] = useState(false);
  const [draft, setDraft] = useState('');
  const [requestError, setRequestError] = useState<string | null>(null);
  const [selectedTurnId, setSelectedTurnId] = useState<string | null>(null);
  const [drawerMode, setDrawerMode] = useState<'artifact' | 'evidence'>('artifact');
  const [railOpen, setRailOpen] = useState(false);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const abortRef = useRef<AbortController | null>(null);
  const composerRef = useRef<HTMLTextAreaElement>(null);
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    try {
      const stored = localStorage.getItem(SESSION_KEY);
      const parsed: unknown = stored ? JSON.parse(stored) : null;
      if (validSession(parsed) && parsed.threads.length > 0) {
        const safeThreads = parsed.threads.slice(0, 24) as ConversationThread[];
        setThreads(safeThreads);
        setActiveThreadId(safeThreads.some((thread) => thread.id === parsed.activeThreadId)
          ? parsed.activeThreadId
          : safeThreads[0]!.id);
      } else {
        const initial = freshThread();
        setThreads([initial]);
        setActiveThreadId(initial.id);
      }
    } catch {
      const initial = freshThread();
      setThreads([initial]);
      setActiveThreadId(initial.id);
    } finally {
      setHydrated(true);
    }
  }, []);

  useEffect(() => {
    if (!hydrated || !activeThreadId || threads.length === 0) return;
    try {
      localStorage.setItem(SESSION_KEY, JSON.stringify({ activeThreadId, threads } satisfies SessionState));
    } catch {
      setRequestError('This browser could not retain local history. The live relay still works.');
    }
  }, [activeThreadId, hydrated, threads]);

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: 'smooth', block: 'end' });
  }, [threads]);

  const activeThread = useMemo(
    () => threads.find((thread) => thread.id === activeThreadId) ?? null,
    [activeThreadId, threads],
  );
  const selectedTurn = useMemo(
    () => activeThread?.turns.find((turn) => turn.id === selectedTurnId)
      ?? activeThread?.turns.at(-1)
      ?? null,
    [activeThread, selectedTurnId],
  );
  const working = Boolean(activeThread?.turns.some((turn) => turn.state === 'working'));
  const runtimeReady = health?.observed.runtime.status === 'READY';
  const visionReady = health?.observed.runtime.vision === true;

  const refreshHealth = useCallback(async () => {
    setHealthBusy(true);
    setHealthError(null);
    try {
      const response = await authFetch('/api/admin/apocv4/health', { cache: 'no-store' });
      const payload = await response.json() as RuntimeHealthProjection & ApiFailure;
      if (!response.ok) throw new Error(errorMessage(payload, response.status));
      setHealth(payload);
    } catch (error) {
      setHealth(null);
      setHealthError(error instanceof Error ? error.message : 'health_request_failed');
    } finally {
      setHealthBusy(false);
    }
  }, []);

  useEffect(() => {
    if (adminAuthorized) void refreshHealth();
  }, [adminAuthorized, refreshHealth]);

  const updateTurn = useCallback((threadId: string, turnId: string, patch: Partial<ConversationTurn>) => {
    setThreads((current) => current.map((thread) => thread.id === threadId
      ? { ...thread, turns: thread.turns.map((turn) => turn.id === turnId ? { ...turn, ...patch } : turn) }
      : thread));
  }, []);

  const submitPrompt = useCallback(async (raw: string) => {
    const prompt = raw.trim();
    if (!activeThreadId || !prompt || prompt.length > MAX_OBJECTIVE_LENGTH || working || !runtimeReady) return;
    const threadId = activeThreadId;
    const turnId = crypto.randomUUID().toLowerCase();
    const turn: ConversationTurn = {
      id: turnId,
      prompt,
      reply: null,
      proposal: null,
      evidence: null,
      state: 'working',
      error: null,
      createdAt: new Date().toISOString(),
    };
    setThreads((current) => current.map((thread) => thread.id === threadId
      ? {
        ...thread,
        title: thread.turns.length === 0 ? prompt.slice(0, 54) : thread.title,
        turns: [...thread.turns, turn],
      }
      : thread));
    setSelectedTurnId(turnId);
    setDraft('');
    setRequestError(null);
    const controller = new AbortController();
    abortRef.current = controller;
    try {
      const response = await authFetch('/api/admin/apocv4/objective', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        cache: 'no-store',
        signal: controller.signal,
        body: JSON.stringify({ objective: prompt }),
      });
      const payload = await response.json() as RuntimeObjectiveProjection & ApiFailure;
      if (!response.ok) throw new Error(errorMessage(payload, response.status));
      const proposal = proposalFrom(payload);
      const evidence = evidenceFrom(payload);
      const reply = proposal
        ? proposal.summary
        : `The governed cycle finished with status ${evidence.status}. No displayable proposal was returned.`;
      updateTurn(threadId, turnId, {
        reply,
        proposal,
        evidence,
        state: evidence.status === 'ACCEPTED' ? 'accepted' : 'failed',
        error: evidence.status === 'ACCEPTED' ? null : `Cycle ended ${evidence.status}.`,
      });
      await refreshHealth();
    } catch (error) {
      if (controller.signal.aborted) {
        updateTurn(threadId, turnId, {
          state: 'stopped',
          error: 'Stopped locally. Upstream completion is unknown because no final receipt was received.',
        });
      } else {
        const message = error instanceof Error ? error.message : 'objective_request_failed';
        updateTurn(threadId, turnId, { state: 'failed', error: message });
        setRequestError(message);
      }
    } finally {
      if (abortRef.current === controller) abortRef.current = null;
    }
  }, [activeThreadId, refreshHealth, runtimeReady, updateTurn, working]);

  const submit = useCallback((event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    void submitPrompt(draft);
  }, [draft, submitPrompt]);

  const onComposerKeyDown = useCallback((event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault();
      void submitPrompt(draft);
    }
  }, [draft, submitPrompt]);

  const createThread = useCallback(() => {
    const thread = freshThread();
    setThreads((current) => [thread, ...current].slice(0, 24));
    setActiveThreadId(thread.id);
    setSelectedTurnId(null);
    setDraft('');
    setRailOpen(false);
    requestAnimationFrame(() => composerRef.current?.focus());
  }, []);

  const branchFrom = useCallback((turn: ConversationTurn) => {
    const thread = freshThread();
    thread.title = `Branch · ${turn.prompt.slice(0, 42)}`;
    setThreads((current) => [thread, ...current].slice(0, 24));
    setActiveThreadId(thread.id);
    setSelectedTurnId(null);
    setDraft(turn.prompt);
    requestAnimationFrame(() => composerRef.current?.focus());
  }, []);

  const exportSession = useCallback(() => {
    const payload = JSON.stringify({ exported_at: new Date().toISOString(), active_thread: activeThread }, null, 2);
    const url = URL.createObjectURL(new Blob([payload], { type: 'application/json' }));
    const link = document.createElement('a');
    link.href = url;
    link.download = `apocrypha-session-${new Date().toISOString().replace(/[:.]/g, '-')}.json`;
    link.click();
    URL.revokeObjectURL(url);
  }, [activeThread]);

  const clearHistory = useCallback(() => {
    const thread = freshThread();
    setThreads([thread]);
    setActiveThreadId(thread.id);
    setSelectedTurnId(null);
    setDraft('');
    try {
      localStorage.removeItem(SESSION_KEY);
    } catch {
      setRequestError('Browser history could not be cleared from local storage.');
    }
    requestAnimationFrame(() => composerRef.current?.focus());
  }, []);

  const copyText = useCallback(async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      setRequestError('Clipboard permission was denied.');
    }
  }, []);

  return (
    <AdminLayout
      title="Apocrypha"
      hideHeading
      immersive
      onAdminCheck={(check) => setAdminAuthorized(check.authorized)}
    >
      {!adminAuthorized ? (
        <section className={styles.authGate}>
          <span>private relay</span>
          <h1>Sign in to speak with Apocrypha.</h1>
          <p>The RunPod credential, privacy partition, and effect boundary remain server-side.</p>
        </section>
      ) : (
        <div className={styles.shell} data-working={working ? 'true' : 'false'}>
          <aside className={`${styles.rail} ${railOpen ? styles.railOpen : ''}`}>
            <div className={styles.brandRow}>
              <Link href="/" className={styles.brand} aria-label="Apocky home">A</Link>
              <div><strong>Apocrypha</strong><span>private relay</span></div>
              <button type="button" className={styles.mobileClose} onClick={() => setRailOpen(false)} aria-label="Close conversations">×</button>
            </div>
            <button type="button" className={styles.newThread} onClick={createThread}><span>＋</span> New conversation</button>
            <div className={styles.threadHeading}><span>Conversation history</span><small>this browser only</small></div>
            <nav className={styles.threadList} aria-label="Conversation history">
              {threads.map((thread) => (
                <button
                  key={thread.id}
                  type="button"
                  className={thread.id === activeThreadId ? styles.activeThread : undefined}
                  onClick={() => { setActiveThreadId(thread.id); setSelectedTurnId(null); setRailOpen(false); }}
                >
                  <span>{thread.title}</span>
                  <small>{thread.turns.length} {thread.turns.length === 1 ? 'turn' : 'turns'}</small>
                </button>
              ))}
            </nav>
            <div className={styles.railFooter}>
              <button type="button" onClick={exportSession} disabled={!activeThread}>Export current thread</button>
              <button type="button" onClick={clearHistory}>Clear local history</button>
              <Link href="/account">Account & privacy</Link>
            </div>
          </aside>

          <main className={styles.conversation}>
            <header className={styles.topbar}>
              <button type="button" className={styles.mobileMenu} onClick={() => setRailOpen(true)} aria-label="Open conversations">☰</button>
              <div className={styles.identity}>
                <span className={runtimeReady ? styles.readyDot : styles.offlineDot} aria-hidden="true" />
                <div><strong>Apocrypha</strong><small>{healthBusy ? 'checking' : runtimeReady ? 'ready' : 'unavailable'}</small></div>
              </div>
              <div className={styles.topActions}>
                <button type="button" onClick={() => void refreshHealth()} disabled={healthBusy}>Refresh</button>
                <button type="button" onClick={() => setDrawerOpen(true)}>Context</button>
              </div>
            </header>

            <section className={styles.timeline} aria-live="polite" aria-busy={working}>
              {activeThread?.turns.length === 0 && (
                <div className={styles.welcome}>
                  <span className={styles.sigil} aria-hidden="true">A</span>
                  <p className={styles.eyebrow}>DIRECT LINE · GOVERNED FACULTY COUNCIL</p>
                  <h1>What are we building?</h1>
                  <p>Speak naturally. Apocrypha will return the selected proposal; evidence and model reports remain available without crowding the conversation.</p>
                  <div className={styles.starters}>
                    {STARTERS.map((starter) => <button type="button" key={starter} onClick={() => setDraft(starter)}>{starter}</button>)}
                  </div>
                </div>
              )}

              {activeThread?.turns.map((turn) => (
                <div key={turn.id} className={styles.turn}>
                  <article className={`${styles.message} ${styles.userMessage}`}>
                    <header><span>You</span><time>{displayTime(turn.createdAt)}</time></header>
                    <p>{turn.prompt}</p>
                    <div className={styles.messageActions}>
                      <button type="button" onClick={() => setDraft(turn.prompt)}>Edit</button>
                      <button type="button" onClick={() => branchFrom(turn)}>Branch</button>
                    </div>
                  </article>

                  <article className={`${styles.message} ${styles.apocryphaMessage}`} data-state={turn.state}>
                    <header><span><i aria-hidden="true">A</i> Apocrypha</span><time>{turn.state}</time></header>
                    {turn.state === 'working' ? (
                      <div className={styles.thinking}><span /><span /><span /><p>Coordinating faculties and tests…</p></div>
                    ) : (
                      <>
                        {turn.reply && <p className={styles.reply}>{turn.reply}</p>}
                        {turn.proposal?.steps.length ? (
                          <ol className={styles.steps}>{turn.proposal.steps.map((step) => <li key={step}>{step}</li>)}</ol>
                        ) : null}
                        {turn.error && <p className={styles.turnError}>{turn.error}</p>}
                        <div className={styles.messageActions}>
                          {turn.reply && <button type="button" onClick={() => void copyText([turn.reply, ...(turn.proposal?.steps ?? [])].join('\n\n'))}>Copy</button>}
                          <button type="button" onClick={() => setDraft(turn.prompt)}>Retry</button>
                          <button type="button" onClick={() => { setSelectedTurnId(turn.id); setDrawerMode('evidence'); setDrawerOpen(true); }}>Evidence</button>
                        </div>
                      </>
                    )}
                  </article>
                </div>
              ))}
              <div ref={endRef} />
            </section>

            <div className={styles.composerDock}>
              {(requestError || healthError) && <p className={styles.errorBanner} role="alert">{requestError ?? healthError}</p>}
              <form className={styles.composer} onSubmit={submit}>
                <label htmlFor="apocrypha-message" className={styles.srOnly}>Message Apocrypha</label>
                <textarea
                  id="apocrypha-message"
                  ref={composerRef}
                  value={draft}
                  rows={2}
                  maxLength={MAX_OBJECTIVE_LENGTH}
                  placeholder={runtimeReady ? 'Message Apocrypha…' : 'Waiting for the relay…'}
                  disabled={!runtimeReady || working}
                  onChange={(event) => setDraft(event.target.value)}
                  onKeyDown={onComposerKeyDown}
                />
                <div className={styles.composerBar}>
                  <div className={styles.capabilityLine}>
                    <span>Text relay live</span>
                    <span>{visionReady ? 'Vision faculty online · browser attachment pending' : 'Vision unavailable'}</span>
                    <span>Effects admission-gated</span>
                  </div>
                  {working ? (
                    <button type="button" className={styles.stopButton} onClick={() => abortRef.current?.abort()}>Stop</button>
                  ) : (
                    <button type="submit" className={styles.sendButton} disabled={!runtimeReady || !draft.trim()}>Send <span aria-hidden="true">↑</span></button>
                  )}
                </div>
              </form>
              <p className={styles.disclosure}>Enter sends · Shift+Enter adds a line · history stays in this browser until you clear it</p>
            </div>
          </main>

          <aside className={`${styles.drawer} ${drawerOpen ? styles.drawerOpen : ''}`} aria-label="Conversation context">
            <header className={styles.drawerHeader}>
              <div><span>Context</span><strong>{selectedTurn ? 'Current turn' : 'Relay'}</strong></div>
              <button type="button" className={styles.mobileClose} onClick={() => setDrawerOpen(false)} aria-label="Close context">×</button>
            </header>
            <div className={styles.drawerTabs}>
              <button type="button" className={drawerMode === 'artifact' ? styles.activeTab : undefined} onClick={() => setDrawerMode('artifact')}>Artifact</button>
              <button type="button" className={drawerMode === 'evidence' ? styles.activeTab : undefined} onClick={() => setDrawerMode('evidence')}>Evidence</button>
            </div>
            {drawerMode === 'artifact' ? (
              <div className={styles.drawerBody}>
                <p className={styles.drawerKicker}>Selected proposal</p>
                {selectedTurn?.proposal ? (
                  <>
                    <h2>{selectedTurn.proposal.summary}</h2>
                    {selectedTurn.proposal.steps.length > 0 && <ol>{selectedTurn.proposal.steps.map((step) => <li key={step}>{step}</li>)}</ol>}
                    {selectedTurn.proposal.sourceRefs.length > 0 && (
                      <details><summary>Source references</summary><ul>{selectedTurn.proposal.sourceRefs.map((ref) => <li key={ref}>{ref}</li>)}</ul></details>
                    )}
                    {selectedTurn.proposal.countercase && <details><summary>Strongest countercase</summary><p>{selectedTurn.proposal.countercase}</p></details>}
                    {selectedTurn.proposal.falsifier && <details><summary>Falsifier</summary><p>{selectedTurn.proposal.falsifier}</p></details>}
                  </>
                ) : (
                  <div className={styles.emptyDrawer}><strong>No artifact yet</strong><p>A selected proposal will resolve here after a completed turn.</p></div>
                )}
              </div>
            ) : (
              <div className={styles.drawerBody}>
                <p className={styles.drawerKicker}>Observed receipt</p>
                {selectedTurn?.evidence ? (
                  <dl className={styles.receiptGrid}>
                    <div><dt>Cycle</dt><dd>{selectedTurn.evidence.status}</dd></div>
                    <div><dt>Observed</dt><dd>{selectedTurn.evidence.observedAt}</dd></div>
                    <div><dt>Latency</dt><dd>{selectedTurn.evidence.latencyMs.toLocaleString()} ms</dd></div>
                    <div><dt>HTTP</dt><dd>{selectedTurn.evidence.upstreamStatus}</dd></div>
                    <div><dt>Candidate</dt><dd>{shortDigest(selectedTurn.evidence.candidateDigest)}</dd></div>
                    <div><dt>Test</dt><dd>{shortDigest(selectedTurn.evidence.testDigest)}</dd></div>
                    <div><dt>Checkpoint</dt><dd>{shortDigest(selectedTurn.evidence.checkpointDigest)}</dd></div>
                    <div><dt>Faculty team</dt><dd>{shortDigest(selectedTurn.evidence.facultyTeamId)}</dd></div>
                    <div><dt>Auth</dt><dd>{selectedTurn.evidence.authMode ?? 'not observed'}</dd></div>
                    <div><dt>Registry</dt><dd>{shortDigest(selectedTurn.evidence.registryRef)}</dd></div>
                    <div><dt>Binding</dt><dd>{shortDigest(selectedTurn.evidence.bindingRef)}</dd></div>
                    <div><dt>Principal</dt><dd>{shortDigest(selectedTurn.evidence.principalRef)}</dd></div>
                  </dl>
                ) : (
                  <div className={styles.emptyDrawer}><strong>No final receipt</strong><p>Runtime observation and model report remain separate. A stopped or incomplete turn is never presented as accepted.</p></div>
                )}
                <p className={styles.epistemic}>Proposal text is model-reported. HTTP, test, identity, and checkpoint fields above are observed transport/runtime receipts.</p>
              </div>
            )}
          </aside>
          {(railOpen || drawerOpen) && <button type="button" className={styles.scrim} onClick={() => { setRailOpen(false); setDrawerOpen(false); }} aria-label="Close overlay" />}
        </div>
      )}
    </AdminLayout>
  );
};

export default Apex;
