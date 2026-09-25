import { useEffect, useReducer, useRef, useState, type FormEvent, type KeyboardEvent, type ReactNode } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  ArrowUp, Check, CheckCheck, ChevronDown, CircleAlert, CircleCheck, CircleX, Copy,
  FileDiff, FileText, FolderOpen, History, Info, LoaderCircle, PanelRight, Plus,
  RefreshCw, Settings, ShieldCheck, Square, Terminal, WifiOff, X,
} from 'lucide-react';
import { attachTask, EMPTY_BOOTSTRAP, localIpc, type LocalBootstrap } from '../lib/local-ipc.ts';
import {
  EMPTY_WORK, isActive, reduceWork, WORK_EVENT, WORK_STREAM_EVENT,
  type ConsentDecision, type LocalSession, type Sampling, type StreamEnvelope, type ToolOutcome, type WorkEnvelope,
} from '../lib/local-work.ts';
import './Work.css';

const appIcon = new URL('../../../icons/icon.ico', import.meta.url).href;
const inspectors = [
  { id: 'files', label: 'Files / Changes', icon: FileDiff },
  { id: 'terminal', label: 'Terminal', icon: Terminal },
  { id: 'artifacts', label: 'Artifacts', icon: FileText },
  { id: 'context', label: 'Context', icon: Info },
] as const;
type Inspector = typeof inspectors[number]['id'];

function IconButton({ label, children, ...props }: {
  label: string; children: ReactNode;
} & Omit<React.ButtonHTMLAttributes<HTMLButtonElement>, 'children'>) {
  return <button type="button" className="work-icon" title={label} aria-label={label} {...props}>{children}</button>;
}

function Output({ text, label }: { text: string; label: string }) {
  const [limit, setLimit] = useState(12_000);
  return <div className="work-output">
    <pre aria-label={label}>{text.slice(0, limit)}</pre>
    {text.length > limit && <button type="button" onClick={() => setLimit(limit + 24_000)}>Show more ({text.length - limit} characters remaining)</button>}
  </div>;
}

function ToolResult({ tool }: { tool: ToolOutcome }) {
  const Status = tool.ok ? CircleCheck : CircleX;
  const [copied, setCopied] = useState(false);
  const [copyError, setCopyError] = useState('');
  async function copy() {
    try {
      await navigator.clipboard.writeText(tool.diff?.patch ?? tool.content ?? tool.error ?? '');
      setCopied(true);
      setCopyError('');
    } catch { setCopyError('Clipboard unavailable. Select the output to copy it.'); }
  }
  return <details className={`work-tool ${tool.ok ? 'success' : 'failure'}`}>
    <summary>
      <Status size={16} aria-hidden="true" />
      <span><strong>{tool.name}</strong><span>{tool.summary || (tool.denied ? 'Denied' : tool.ok ? 'Completed' : 'Failed')}</span></span>
      <small>{tool.ok ? 'Completed' : tool.denied ? 'Denied' : 'Failed'}</small>
      <ChevronDown size={14} aria-hidden="true" />
    </summary>
    <div className="work-tool-detail">
      <div className="work-output-heading">
        <span>{tool.diff?.path ?? tool.name}</span>
        {Number.isFinite(tool.elapsedMs) && <small>{(tool.elapsedMs / 1000).toFixed(2)}s</small>}
        <IconButton label={copied ? 'Copied' : 'Copy output'} onClick={() => void copy()}>{copied ? <Check size={16} /> : <Copy size={16} />}</IconButton>
      </div>
      {tool.error && <p className="work-error">{tool.error}</p>}
      {tool.diff && <><p className="work-diff-stat">+{tool.diff.added} / -{tool.diff.removed}</p><Output text={tool.diff.patch} label={`Diff for ${tool.diff.path}`} /></>}
      {tool.content && <Output text={tool.content} label={`Output from ${tool.name}`} />}
      {copyError && <p role="status">{copyError}</p>}
    </div>
  </details>;
}

function PhaseStatus({ phase }: { phase: string }) {
  const labels: Record<string, string> = {
    queued: 'Queued', thinking: 'Thinking', awaiting_consent: 'Approval needed', tool: 'Running tool', writing: 'Writing',
    done: 'Completed', failed: 'Failed', cancelled: 'Cancelled',
  };
  return <span className={`work-phase phase-${phase}`}>{labels[phase] ?? phase}</span>;
}

function message(error: unknown): string {
  return typeof error === 'string' ? error : error instanceof Error ? error.message : 'The local request could not be completed.';
}

export function Work() {
  const [bootstrap, setBootstrap] = useState<LocalBootstrap>(EMPTY_BOOTSTRAP);
  const [work, dispatch] = useReducer(reduceWork, EMPTY_WORK);
  const [draft, setDraft] = useState('');
  const [rootPath, setRootPath] = useState('');
  const [preset, setPreset] = useState('');
  const [sampling, setSampling] = useState<Sampling>({});
  const [inspector, setInspector] = useState<Inspector>('files');
  const [inspectorOpen, setInspectorOpen] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [historyQuery, setHistoryQuery] = useState('');
  const [taskTitle, setTaskTitle] = useState('');
  const [pending, setPending] = useState<string[]>(['bootstrap']);
  const [uncertain, setUncertain] = useState(false);
  const pendingRef = useRef(new Set(['bootstrap']));
  const selected = useRef<LocalSession | null>(null);
  const selectionVersion = useRef(0);
  const mounted = useRef(true);
  const streamEnd = useRef<HTMLDivElement>(null);
  const thread = useRef<HTMLDivElement>(null);
  const follow = useRef(true);
  const composer = useRef<HTMLTextAreaElement>(null);
  const historyDialog = useRef<HTMLDialogElement>(null);
  const settingsDialog = useRef<HTMLDialogElement>(null);
  const last = work.turns.at(-1);
  const active = isActive(last);
  const switching = pending.includes('select');
  const sending = pending.includes('send');
  const loading = pending.includes('bootstrap');
  const allTools = work.turns.flatMap((turn) => turn.toolCalls);
  const roots = bootstrap.workspace?.roots ?? [];
  const focusedRoot = roots.find((root) => root.path === rootPath);
  const selectedPreset = bootstrap.health?.presets?.find((item) => item.id === preset);
  const effectiveSampling = { ...(selectedPreset?.profile ?? bootstrap.health?.sampling ?? {}), ...sampling };
  const draftBytes = new TextEncoder().encode(draft.trim()).length;
  const blocked = !bootstrap.online ? 'Local host offline' : loading || switching ? 'Loading task' :
    uncertain ? 'Reload this task to reconcile the last send' : active || sending ? 'Task is running' :
      work.session && work.connection !== 'connected' ? 'Waiting for the task stream' : draftBytes > 256 * 1024 ? 'Task exceeds 256 KB' : '';

  function pendingAction(name: string, value: boolean) {
    if (value) pendingRef.current.add(name); else pendingRef.current.delete(name);
    if (mounted.current) setPending([...pendingRef.current]);
  }

  async function act(name: string, job: () => Promise<void>) {
    if (pendingRef.current.has(name)) return;
    pendingAction(name, true);
    try { await job(); }
    catch (error) { if (mounted.current) dispatch({ type: 'notice', message: message(error) }); }
    finally { pendingAction(name, false); }
  }

  function accept(snapshot: LocalSession) {
    selected.current = snapshot;
    dispatch({ type: 'open', snapshot });
    setTaskTitle(snapshot.session.title);
    setUncertain(false);
    setBootstrap((current) => ({ ...current, sessions: [snapshot.session, ...current.sessions.filter((item) => item.id !== snapshot.session.id)] }));
  }

  async function openTask(id?: string) {
    const version = ++selectionVersion.current;
    const snapshot = id ? await localIpc.openSession(id) : await localIpc.newSession('Untitled task');
    if (!mounted.current || selectionVersion.current !== version) return;
    await attachTask(localIpc, snapshot, accept);
    setHistoryOpen(false);
    composer.current?.focus();
  }

  async function refresh(reopen = true) {
    const result = await localIpc.bootstrap();
    if (!mounted.current) return;
    setBootstrap(result);
    if (result.online && reopen && selected.current) await openTask(selected.current.session.id);
  }

  useEffect(() => {
    mounted.current = true;
    let disposed = false;
    const stops: UnlistenFn[] = [];
    async function start() {
      if (!isTauri()) {
        setBootstrap({ ...EMPTY_BOOTSTRAP, notice: 'Native bridge unavailable. Open the Apocrypha Desktop executable to connect to the local Work host.' });
        pendingAction('bootstrap', false);
        return;
      }
      try {
        for (const [event, handler] of [
          [WORK_EVENT, (payload: unknown) => dispatch({ type: 'event', envelope: payload as WorkEnvelope })],
          [WORK_STREAM_EVENT, (payload: unknown) => dispatch({ type: 'stream', envelope: payload as StreamEnvelope })],
        ] as const) {
          const stop = await listen(event, (incoming) => { if (!disposed) handler(incoming.payload); });
          if (disposed) { stop(); return; }
          stops.push(stop);
        }
        const result = await localIpc.bootstrap();
        if (disposed) return;
        setBootstrap(result);
      } catch (error) {
        if (!disposed) setBootstrap({ ...EMPTY_BOOTSTRAP, notice: message(error) });
      } finally { if (!disposed) pendingAction('bootstrap', false); }
    }
    void start();
    return () => {
      disposed = true;
      mounted.current = false;
      selectionVersion.current += 1;
      stops.forEach((stop) => stop());
      if (selected.current) void localIpc.detach(selected.current.epoch).catch(() => {});
    };
  }, []);

  useEffect(() => {
    if (historyOpen && !historyDialog.current?.open) historyDialog.current?.showModal();
    if (!historyOpen && historyDialog.current?.open) historyDialog.current.close();
  }, [historyOpen]);

  useEffect(() => {
    if (settingsOpen && !settingsDialog.current?.open) settingsDialog.current?.showModal();
    if (!settingsOpen && settingsDialog.current?.open) settingsDialog.current.close();
  }, [settingsOpen]);

  useEffect(() => {
    if (follow.current) streamEnd.current?.scrollIntoView({ block: 'end' });
  }, [work.lastSeq, work.epoch, inspectorOpen]);

  async function submit(event?: FormEvent) {
    event?.preventDefault();
    if (blocked || !draft.trim() || pendingRef.current.has('send')) return;
    const content = draft.trim();
    await act('send', async () => {
      let session = selected.current;
      if (!session) {
        session = await localIpc.newSession(content.slice(0, 80));
        await attachTask(localIpc, session, accept);
      }
      const prompt = focusedRoot ? `Workspace: ${focusedRoot.label} (${focusedRoot.path})\n\n${content}` : content;
      try {
        await localIpc.send(session.session.id, prompt, preset || undefined, Object.keys(sampling).length ? sampling : undefined);
        setDraft('');
        follow.current = true;
      } catch (error) {
        setUncertain(true);
        throw error;
      }
    });
  }

  function decide(decision: ConsentDecision) {
    const request = work.consent;
    if (!work.session || !request || work.connection !== 'connected') return;
    void act('consent', async () => {
      const response = await localIpc.consent(work.session!.id, work.epoch, request.id, decision);
      if (!response.resolved) dispatch({ type: 'notice', message: 'That approval is no longer pending. Reload this task.' });
    });
  }

  function tabsKey(event: KeyboardEvent<HTMLButtonElement>, index: number) {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? inspectors.length - 1 :
      (index + (event.key === 'ArrowRight' ? 1 : -1) + inspectors.length) % inspectors.length;
    setInspector(inspectors[next]!.id);
    event.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>('[role="tab"]')[next]?.focus();
  }

  function inspect() {
    if (inspector === 'files') {
      const tools = allTools.filter((tool) => tool.diff || /file|directory|search|grep|read|list/i.test(tool.name));
      return <>
        {tools.length ? tools.map((tool, index) => <ToolResult key={`${tool.id}-${index}`} tool={tool} />) : <p className="work-empty-detail">No file results in this task.</p>}
        <p className="work-unavailable">Direct file browsing and undo endpoints are unavailable.</p>
      </>;
    }
    if (inspector === 'terminal') {
      const tools = allTools.filter((tool) => /shell|command|terminal|exec/i.test(tool.name));
      return <>
        {tools.length ? tools.map((tool, index) => <ToolResult key={`${tool.id}-${index}`} tool={tool} />) : <p className="work-empty-detail">No command results in this task.</p>}
        <p className="work-unavailable">Interactive terminal endpoint unavailable.</p>
      </>;
    }
    if (inspector === 'artifacts') return <p className="work-unavailable">Artifact listing and export endpoints are unavailable. File paths and patches are retained in Files / Changes.</p>;
    return <>
      <h3>Workspace</h3>
      {roots.length ? roots.map((root) => <div className="work-root" key={root.path}><strong>{root.label}</strong><code>{root.path}</code><small>{root.writable ? 'Writable' : 'Read only'}</small></div>) : <p className="work-unavailable">Workspace roots unavailable.</p>}
      <h3>Host Policy</h3><Output text={JSON.stringify(bootstrap.health?.policy ?? { status: 'unavailable' }, null, 2)} label="Host policy" />
      <h3>Tools</h3>
      {bootstrap.workspace?.tools.map((tool) => <details className="work-capability" key={tool.name}><summary>{tool.name}<small>{tool.risk}</small></summary><p>{tool.description}</p></details>)}
      <h3>MCP</h3><Output text={JSON.stringify(bootstrap.health?.mcp ?? { status: 'unavailable' }, null, 2)} label="MCP availability" />
      <p className="work-unavailable">Federated context details are not exposed by this host API.</p>
    </>;
  }

  return <main className={`workbench${inspectorOpen ? ' inspector-open' : ''}`}>
    <header className="work-topbar">
      <div className="work-brand"><img src={appIcon} alt="" width="25" height="25" /><strong>Apocrypha</strong></div>
      <IconButton label="Task history" onClick={() => setHistoryOpen(true)}><History size={18} /></IconButton>
      <div className="work-selectors">
        <label className="work-workspace-select"><FolderOpen size={16} aria-hidden="true" /><select aria-label="Task workspace focus" value={rootPath} disabled={!roots.length || active || sending} onChange={(event) => setRootPath(event.target.value)}>
          <option value="">All configured roots</option>{roots.map((root) => <option key={root.path} value={root.path}>{root.label}{root.writable ? '' : ' (read only)'}</option>)}
        </select></label>
        <select aria-label="Current task" value={work.session?.id ?? ''} disabled={!bootstrap.online || switching || sending} onChange={(event) => { if (event.target.value) void act('select', () => openTask(event.target.value)); }}>
          <option value="">New task</option>{bootstrap.sessions.map((session) => <option key={session.id} value={session.id}>{session.title || 'Untitled task'}</option>)}
        </select>
      </div>
      <IconButton label="New task" disabled={!bootstrap.online || switching || sending} onClick={() => void act('select', () => openTask())}><Plus size={18} /></IconButton>
      <div className={`work-host-status ${bootstrap.online ? bootstrap.health?.status === 'ok' ? 'healthy' : 'degraded' : 'offline'}`} role="status">
        {loading ? <LoaderCircle size={14} className="work-spin" /> : bootstrap.online ? <span className="work-status-dot" /> : <WifiOff size={14} />}
        <span>{loading ? 'Connecting' : bootstrap.online ? bootstrap.health?.status === 'ok' ? 'Local host ready' : 'Local host degraded' : 'Local host offline'}</span>
      </div>
      <IconButton label="Reconnect and reload task" disabled={loading || switching || sending || !isTauri()} onClick={() => void act('bootstrap', () => refresh())}><RefreshCw size={17} className={loading ? 'work-spin' : ''} /></IconButton>
      <IconButton label={inspectorOpen ? 'Close inspector' : 'Open inspector'} aria-expanded={inspectorOpen} onClick={() => setInspectorOpen(!inspectorOpen)}><PanelRight size={18} /></IconButton>
      <IconButton label="Task settings" onClick={() => setSettingsOpen(true)}><Settings size={18} /></IconButton>
    </header>

    {(bootstrap.notice || work.notice) && <div className="work-notice" role="status"><CircleAlert size={17} aria-hidden="true" /><span>{work.notice || bootstrap.notice}</span></div>}

    <div className="work-body">
      <section className="work-task" aria-label="Task thread and composer">
        <div className="work-thread" ref={thread} onScroll={() => { const element = thread.current; if (element) follow.current = element.scrollHeight - element.scrollTop - element.clientHeight < 120; }} tabIndex={0} aria-label="Task history and results">
          {!work.turns.length && <div className="work-empty">
            <h1>{work.session?.title || 'New task'}</h1>
            <p>{loading ? 'Connecting to the local host...' : bootstrap.online ? 'No turns yet.' : 'Waiting for the local Work host.'}</p>
            {bootstrap.host && <code>{bootstrap.host.endpoint}</code>}
          </div>}
          {work.turns.map((turn) => <article className="work-turn" key={turn.id}>
            <div className="work-prompt"><h2>Task</h2><Output text={turn.prompt} label="Submitted task" /></div>
            <div className="work-answer-heading"><strong>Apocrypha</strong><PhaseStatus phase={turn.phase} /></div>
            {turn.toolCalls.length > 0 && <div className="work-tools">{turn.toolCalls.map((tool, index) => <ToolResult key={`${tool.id}-${index}`} tool={tool} />)}</div>}
            {turn.output && <Output text={turn.output} label="Apocrypha response" />}
            {turn.error && <p role="alert" className="work-error">{turn.error}</p>}
            {turn.usage && <p className="work-usage">
              {typeof turn.usage.totalTokens === 'number' && <span>{turn.usage.totalTokens.toLocaleString()} tokens</span>}
              {typeof turn.usage.elapsedS === 'number' && <span>{turn.usage.elapsedS.toFixed(1)}s</span>}
            </p>}
          </article>)}
          {work.pendingTool && <div className="work-running" role="status"><LoaderCircle className="work-spin" size={16} /><span>{work.pendingTool.name}</span></div>}
          {work.consent && <section className="work-consent" aria-labelledby="consent-title">
            <header><ShieldCheck size={20} aria-hidden="true" /><h2 id="consent-title">Approval required</h2><span>{work.consent.risk}</span></header>
            <strong>{work.consent.tool}</strong><p>{work.consent.summary}</p>
            <Output text={work.consent.detail} label="Exact action awaiting approval" />
            <div className="work-consent-actions">
              <button type="button" disabled={pending.includes('consent') || work.connection !== 'connected'} onClick={() => decide('deny')}><X size={16} />Deny</button>
              <button type="button" disabled={pending.includes('consent') || work.connection !== 'connected'} onClick={() => decide('allow_session')} title={`Allow ${work.consent.tool} for this session`}><CheckCheck size={16} />Allow tool for session</button>
              <button type="button" className="work-primary" disabled={pending.includes('consent') || work.connection !== 'connected'} onClick={() => decide('allow')}><Check size={16} />Allow once</button>
            </div>
          </section>}
          <div ref={streamEnd} />
        </div>

        <form className="work-composer" onSubmit={(event) => void submit(event)}>
          <textarea ref={composer} aria-label="Task prompt" placeholder="Describe a coding task..." rows={3} value={draft} onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => { if (event.key === 'Enter' && (event.ctrlKey || event.metaKey) && !event.nativeEvent.isComposing) { event.preventDefault(); void submit(); } }} />
          <div className="work-composer-controls">
            <label><span className="work-sr">Sampling preset</span><select aria-label="Sampling preset" value={preset} disabled={sending} onChange={(event) => { setPreset(event.target.value); setSampling({}); }}>
              <option value="">Host defaults</option>{bootstrap.health?.presets?.map((item) => <option key={item.id} value={item.id}>{item.label}</option>)}
            </select></label>
            <span className="work-compose-status" role="status">{blocked || (work.session ? `${work.connection} / local` : 'Local task')}</span>
            <IconButton label="Stop current task" className="work-icon work-stop" disabled={!bootstrap.online || !work.session || pending.includes('cancel')} onClick={() => void act('cancel', async () => {
              const response = await localIpc.cancel(work.session!.id);
              dispatch({ type: 'notice', message: response.cancelled ? 'Cancellation requested. Waiting for the host result.' : 'No running turn was cancelled. Reload the task to confirm its state.' });
            })}><Square size={16} /></IconButton>
            <button className="work-send" type="submit" disabled={!!blocked || !draft.trim()} title="Send task (Ctrl+Enter)" aria-label="Send task">{sending ? <LoaderCircle size={18} className="work-spin" /> : <ArrowUp size={19} />}</button>
          </div>
        </form>
      </section>

      {inspectorOpen && <aside className="work-inspector" aria-label="Task inspector">
        <div className="work-inspector-heading"><h2>Inspector</h2><IconButton label="Close inspector" onClick={() => setInspectorOpen(false)}><X size={18} /></IconButton></div>
        <div className="work-tabs" role="tablist" aria-label="Inspector views">{inspectors.map((item, index) => <button key={item.id} id={`tab-${item.id}`} type="button" role="tab" aria-selected={inspector === item.id} aria-controls="inspector-panel" tabIndex={inspector === item.id ? 0 : -1} onKeyDown={(event) => tabsKey(event, index)} onClick={() => setInspector(item.id)} title={item.label}><item.icon size={15} /><span>{item.label}</span></button>)}</div>
        <div className="work-inspector-content" id="inspector-panel" role="tabpanel" aria-labelledby={`tab-${inspector}`} tabIndex={0}>{inspect()}</div>
      </aside>}
    </div>

    <footer className="work-statusbar"><span>{bootstrap.health?.service ?? 'apocrypha-work'}</span><span>{typeof bootstrap.health?.engine.alias === 'string' ? bootstrap.health.engine.alias : 'Engine unavailable'}</span><span>{work.session ? `Event ${work.lastSeq}` : 'No task selected'}</span></footer>

    <dialog ref={historyDialog} className="work-dialog work-history" onClose={() => setHistoryOpen(false)}>
      <header><h2>Tasks</h2><IconButton label="Close task history" onClick={() => setHistoryOpen(false)}><X size={18} /></IconButton></header>
      <div className="work-history-controls"><input type="search" aria-label="Search task history" placeholder="Find a task" value={historyQuery} onChange={(event) => setHistoryQuery(event.target.value)} /><IconButton label="Create new task" disabled={!bootstrap.online || switching || sending} onClick={() => void act('select', () => openTask())}><Plus size={18} /></IconButton></div>
      <div className="work-history-list">{bootstrap.sessions.filter((session) => session.title.toLowerCase().includes(historyQuery.toLowerCase())).map((session) => <button type="button" key={session.id} aria-current={session.id === work.session?.id ? 'true' : undefined} disabled={switching || sending || !bootstrap.online} onClick={() => void act('select', () => openTask(session.id))}><strong>{session.title || 'Untitled task'}</strong><small>{session.lastActiveAt}</small></button>)}
        {!bootstrap.sessions.length && <p className="work-empty-detail">{bootstrap.online ? 'No saved tasks.' : 'Task history unavailable while offline.'}</p>}
      </div>
      <button type="button" disabled={loading || !isTauri()} onClick={() => void act('bootstrap', () => refresh(false))}><RefreshCw size={16} />Refresh history</button>
    </dialog>

    <dialog ref={settingsDialog} className="work-dialog work-settings" onClose={() => setSettingsOpen(false)}>
      <header><h2>Task settings</h2><IconButton label="Close settings" onClick={() => setSettingsOpen(false)}><X size={18} /></IconButton></header>
      <div className="work-settings-content">
        {work.session && <form className="work-rename" onSubmit={(event) => { event.preventDefault(); void act('rename', async () => {
          await localIpc.rename(work.session!.id, taskTitle);
          await refresh(false);
          setSettingsOpen(false);
        }); }}><label htmlFor="work-task-title">Task title</label><div><input id="work-task-title" value={taskTitle} maxLength={256} onChange={(event) => setTaskTitle(event.target.value)} /><button type="submit" disabled={!taskTitle.trim() || pending.includes('rename') || !bootstrap.online}>Rename</button></div></form>}
        <h3>Sampling</h3>
        <label htmlFor="settings-preset">Preset</label><select id="settings-preset" value={preset} onChange={(event) => { setPreset(event.target.value); setSampling({}); }}><option value="">Host defaults</option>{bootstrap.health?.presets?.map((item) => <option key={item.id} value={item.id}>{item.label}</option>)}</select>
        <div className="work-sampling-grid">{[
          ['temperature', 'Temperature', 0, 2, 0.05], ['topP', 'Top P', 0, 1, 0.01], ['topK', 'Top K', 0, 200, 1],
          ['minP', 'Min P', 0, 1, 0.01], ['repeatPenalty', 'Repeat penalty', 0.5, 2, 0.05],
        ].map(([key, label, min, max, step]) => <label key={key} htmlFor={`dial-${key}`}>{label}<input id={`dial-${key}`} type="number" min={min} max={max} step={step} disabled={!bootstrap.online} value={typeof effectiveSampling[String(key)] === 'number' ? Number(effectiveSampling[String(key)]) : ''} onChange={(event) => {
          const value = event.target.valueAsNumber;
          if (Number.isFinite(value) && value >= Number(min) && value <= Number(max)) setSampling((current) => ({ ...current, [key!]: value }));
        }} /></label>)}</div>
        <button type="button" onClick={() => setSampling({})} disabled={!Object.keys(sampling).length}><RefreshCw size={15} />Reset overrides</button>
        <details><summary>Effective request settings</summary><Output text={JSON.stringify(effectiveSampling, null, 2)} label="Effective sampling settings" /></details>
        <h3>Local Connection</h3>
        <dl className="work-connection"><dt>Endpoint</dt><dd>{bootstrap.host?.endpoint ?? 'Unavailable'}</dd><dt>State directory</dt><dd>{bootstrap.host?.state_dir ?? 'Unavailable'}</dd><dt>Service</dt><dd>{bootstrap.health?.service ?? 'Unavailable'}</dd></dl>
        <details><summary>Engine status</summary><Output text={JSON.stringify(bootstrap.health?.engine ?? { status: 'unavailable' }, null, 2)} label="Reported engine status" /></details>
        <p className="work-unavailable">Host launch, model lifecycle, phone pairing, and package updates are not controlled by this client.</p>
      </div>
    </dialog>
  </main>;
}