import Link from 'next/link';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { ConsentCard } from './ConsentCard';
import { EngineControl } from './EngineControl';
import { ToolStep } from './ToolStep';
import styles from './WorkConsole.module.css';
import {
  WorkLaneOffline,
  streamSession,
  workApi,
  type ConsentDecision,
  type WorkHealth,
  type WorkSessionSummary,
  type WorkTurnRecord,
} from '@/lib/work/client';
import { IDLE, phaseLabel, reduceLive, type LiveState } from '@/lib/work/live';

function when(iso: string): string {
  const delta = Date.now() - new Date(iso).getTime();
  if (delta < 60_000) return 'just now';
  if (delta < 3_600_000) return `${Math.floor(delta / 60_000)}m ago`;
  if (delta < 86_400_000) return `${Math.floor(delta / 3_600_000)}h ago`;
  return new Date(iso).toLocaleDateString();
}

export default function WorkConsole(): JSX.Element {
  const [health, setHealth] = useState<WorkHealth | null>(null);
  const [offline, setOffline] = useState<{ detail: string; hint: string } | null>(null);
  const [sessions, setSessions] = useState<WorkSessionSummary[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [turns, setTurns] = useState<WorkTurnRecord[]>([]);
  const [live, setLive] = useState<LiveState>(IDLE);
  const [draft, setDraft] = useState('');
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState('');
  const foot = useRef<HTMLDivElement | null>(null);

  const running = !live.terminal;

  const refreshHealth = useCallback(async () => {
    try {
      setHealth(await workApi.health());
      setOffline(null);
    } catch (error) {
      if (error instanceof WorkLaneOffline) setOffline({ detail: error.message, hint: error.hint });
      else setNotice(error instanceof Error ? error.message : 'Could not reach the Work lane.');
      setHealth(null);
    }
  }, []);

  useEffect(() => {
    void refreshHealth();
    const timer = setInterval(() => { void refreshHealth(); }, 20_000);
    return () => clearInterval(timer);
  }, [refreshHealth]);

  useEffect(() => {
    if (offline) return;
    void workApi.listSessions().then((list) => {
      setSessions(list);
      setActiveId((current) => current ?? list[0]?.id ?? null);
    }).catch(() => undefined);
  }, [offline]);

  useEffect(() => {
    if (!activeId) { setTurns([]); setLive(IDLE); return; }
    let live_ = true;
    setLive(IDLE);
    void workApi.loadSession(activeId).then((record) => {
      if (live_) setTurns(record.turns);
    }).catch(() => undefined);
    const controller = new AbortController();
    streamSession(activeId, (event) => {
      setLive((state) => reduceLive(state, event));
    }, () => undefined, controller.signal);
    return () => { live_ = false; controller.abort(); };
  }, [activeId]);

  useEffect(() => {
    foot.current?.scrollIntoView({ block: 'end' });
  }, [live.answer, live.steps.length, live.consent?.id, turns.length]);

  // A finished turn moves from the live view into the persisted history, so reload it once the
  // server has had a moment to write the record rather than duplicating it in both places.
  useEffect(() => {
    if (!activeId || !live.terminal || live.prompt === null) return;
    const timer = setTimeout(() => {
      void workApi.loadSession(activeId).then((record) => {
        setTurns(record.turns);
        setLive(IDLE);
      }).catch(() => undefined);
      void workApi.listSessions().then(setSessions).catch(() => undefined);
    }, 600);
    return () => clearTimeout(timer);
  }, [activeId, live.terminal, live.prompt]);

  const newTask = useCallback(async () => {
    try {
      const session = await workApi.createSession('Untitled task');
      setSessions((list) => [session, ...list]);
      setActiveId(session.id);
      setNotice('');
    } catch (error) {
      setNotice(error instanceof Error ? error.message : 'Could not start a task.');
    }
  }, []);

  const submit = useCallback(async () => {
    const prompt = draft.trim();
    if (!prompt || busy || running) return;
    setBusy(true);
    setNotice('');
    try {
      let target = activeId;
      if (!target) {
        const session = await workApi.createSession(prompt);
        setSessions((list) => [session, ...list]);
        setActiveId(session.id);
        target = session.id;
      }
      await workApi.submit(target, prompt);
      setDraft('');
    } catch (error) {
      setNotice(error instanceof Error ? error.message : 'The task could not be started.');
    } finally {
      setBusy(false);
    }
  }, [activeId, busy, draft, running]);

  const decide = useCallback(async (decision: ConsentDecision) => {
    const request = live.consent;
    if (!request) return;
    setBusy(true);
    try {
      await workApi.consent(request.id, decision);
    } catch (error) {
      setNotice(error instanceof Error ? error.message : 'That decision did not reach the agent.');
    } finally {
      setBusy(false);
    }
  }, [live.consent]);

  const stop = useCallback(async () => {
    if (!activeId) return;
    try { await workApi.cancel(activeId); } catch { setNotice('Could not stop the task.'); }
  }, [activeId]);

  const engineDot = offline || !health ? styles.dotBad : running ? styles.dotBusy : health.engine.healthy ? styles.dotOk : styles.dotBad;
  const engineLabel = offline ? 'service offline'
    : !health ? 'checking…'
      : !health.engine.healthy ? `engine down — ${health.engine.detail ?? 'no answer'}`
        : health.arbiter?.resident === 'chat'
          ? `${health.engine.model} · chat model loaded — switch for coding strength`
          : `${health.engine.model} · ${phaseLabel(live)}`;

  const roots = useMemo(() => health?.workspace ?? [], [health]);

  return (
    <div className={styles.shell}>
      <header className={styles.bar}>
        <Link href="/apocrypha" className={styles.brand} aria-label="Back to Apocrypha chat">
          Apocrypha Work<span>local coding agent &middot; back to chat</span>
        </Link>
        <div className={styles.status}><span className={`${styles.dot} ${engineDot}`} />{engineLabel}</div>
        {health?.arbiter ? <EngineControl arbiter={health.arbiter} onChange={(next) => setHealth((h) => (h ? { ...h, arbiter: next } : h))} onNotice={setNotice} /> : null}
        <div className={styles.spacer} />
        {health?.policy.shell === false ? <div className={styles.status}>commands off</div> : null}
        {live.usage?.totalTokens
          ? <div className={styles.status}>{live.usage.totalTokens} tok{live.usage.elapsedS ? ` · ${live.usage.elapsedS.toFixed(1)}s` : ''}</div>
          : null}
      </header>

      <aside className={styles.side}>
        <div>
          <h2 className={styles.sideHead}>Tasks</h2>
          <button type="button" className={styles.newTask} onClick={() => { void newTask(); }}>+ New task</button>
          <ul className={styles.sessionList}>
            {sessions.map((session) => (
              <li key={session.id} className={styles.sessionItem}>
                <button
                  type="button"
                  className={`${styles.sessionButton} ${session.id === activeId ? styles.sessionActive : ''}`}
                  onClick={() => setActiveId(session.id)}
                >
                  {session.title}
                  <span className={styles.sessionWhen}>{when(session.lastActiveAt)}</span>
                </button>
              </li>
            ))}
          </ul>
        </div>
        <div>
          <h2 className={styles.sideHead}>Workspace</h2>
          <ul className={styles.rootList}>
            {roots.map((root) => (
              <li key={root.label} className={styles.root}>
                {root.label}
                {root.writable ? null : <span className={styles.rootRo}>READ-ONLY</span>}
              </li>
            ))}
            {roots.length === 0 ? <li className={styles.root}>—</li> : null}
          </ul>
        </div>
      </aside>

      <main className={styles.main}>
        <div className={styles.transcript}>
          {offline ? (
            <div className={styles.empty}>
              <h2>The Work lane is not running here</h2>
              <p>{offline.detail}</p>
              <p>{offline.hint}</p>
            </div>
          ) : turns.length === 0 && live.prompt === null ? (
            <div className={styles.empty}>
              <h2>Give it a job</h2>
              <p>This lane reads and edits files on your machine and runs commands you approve. It keeps its own sessions, separate from Chat — they only share the engine.</p>
              <p><code>fix the failing test in lib/oracle.ts</code></p>
            </div>
          ) : null}

          {turns.map((turn) => (
            <article key={turn.id}>
              <div className={styles.task}><span className={styles.taskLabel}>Task</span>{turn.prompt}</div>
              {turn.toolCalls.map((call, index) => <ToolStep key={`${call.id}-${index}`} call={call} />)}
              {turn.output ? <div className={styles.answer}>{turn.output}</div> : null}
              {turn.error ? <div className={`${styles.notice} ${styles.noticeBad}`}>{turn.error}</div> : null}
            </article>
          ))}

          {live.prompt !== null ? (
            <article>
              <div className={styles.task}><span className={styles.taskLabel}>Task</span>{live.prompt}</div>
              {live.steps.map((call, index) => <ToolStep key={`${call.id}-${index}`} call={call} />)}
              {live.pendingStep ? <ToolStep call={live.pendingStep} running /> : null}
              {live.consent ? <ConsentCard prompt={live.consent} onDecide={(decision) => { void decide(decision); }} busy={busy} /> : null}
              {live.answer ? <div className={styles.answer}>{live.answer}{running ? <span className={styles.cursor}>&nbsp;</span> : null}</div> : null}
              {live.error ? <div className={`${styles.notice} ${styles.noticeBad}`}>{live.error}</div> : null}
            </article>
          ) : null}

          {notice ? <div className={styles.notice} role="status">{notice}</div> : null}
          <div ref={foot} />
        </div>

        <form
          className={styles.composer}
          onSubmit={(event) => { event.preventDefault(); void submit(); }}
        >
          <div className={styles.composerRow}>
            <textarea
              value={draft}
              onChange={(event) => setDraft(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) { event.preventDefault(); void submit(); }
              }}
              placeholder={running ? 'Working… stop the task to send another.' : 'Describe the job. Name files if you know them.'}
              disabled={running || offline !== null}
              aria-label="Task for the agent"
            />
            {running
              ? <button type="button" className={styles.stop} onClick={() => { void stop(); }}>Stop</button>
              : <button type="submit" className={styles.send} disabled={!draft.trim() || busy || offline !== null}>Run</button>}
          </div>
          <p className={styles.hint}>
            Ctrl+Enter to run · reads happen automatically · writes and commands ask first
            {health ? ` · budget ${health.policy.max_iterations} steps` : ''}
          </p>
        </form>
      </main>
    </div>
  );
}
