import type { NextPage } from 'next';
import Link from 'next/link';
import { useRouter } from 'next/router';
import { useEffect, useMemo, useState } from 'react';

import AdminLayout from '../../components/AdminLayout';
import { authFetch } from '../../lib/browser-auth';

type ToolId = 'build' | 'repair' | 'tests' | 'refactor' | 'docs';

interface ToolPreset {
  id: ToolId;
  label: string;
  description: string;
  instruction: string;
}

interface JsonObject {
  [key: string]: unknown;
}

interface CodeProjection {
  kind?: unknown;
  error?: unknown;
  observed?: {
    receipt?: {
      latency_ms?: unknown;
      upstream_status?: unknown;
      effect_scope_ref?: unknown;
    };
    runtime?: JsonObject;
    test?: JsonObject | null;
  };
  generated?: {
    proposal_digest?: unknown;
    requested_allowed_paths?: unknown;
    faculty_attempts?: unknown;
  };
}

interface RollbackProjection {
  kind?: unknown;
  error?: unknown;
  observed?: {
    receipt?: unknown;
    runtime?: unknown;
  };
}

const TOOLS: readonly ToolPreset[] = [
  {
    id: 'build',
    label: 'Build',
    description: 'Create or extend a feature.',
    instruction: 'Build the requested feature within the exact allowed files.',
  },
  {
    id: 'repair',
    label: 'Repair',
    description: 'Diagnose and fix a defect.',
    instruction: 'Diagnose the described defect, implement the smallest complete repair, and preserve unrelated behavior.',
  },
  {
    id: 'tests',
    label: 'Tests',
    description: 'Generate meaningful coverage.',
    instruction: 'Add or strengthen tests for the requested behavior without weakening existing assertions.',
  },
  {
    id: 'refactor',
    label: 'Refactor',
    description: 'Improve structure safely.',
    instruction: 'Refactor the named code while preserving behavior and proving the result with tests.',
  },
  {
    id: 'docs',
    label: 'Docs',
    description: 'Generate exact documentation.',
    instruction: 'Write source-faithful documentation for the requested surface and do not invent unsupported behavior.',
  },
];

const SHA256_RE = /^[0-9a-f]{64}$/;
const BROWSER_DEADLINE_MS = 250_000;

function isObject(value: unknown): value is JsonObject {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function stringValue(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function digestValue(value: unknown): string | null {
  return typeof value === 'string' && SHA256_RE.test(value) ? value : null;
}

function parsePaths(raw: string): string[] {
  return raw
    .split(/[\r\n,]+/)
    .map((value) => value.trim())
    .filter(Boolean)
    .sort();
}

function shortDigest(value: unknown): string {
  const digest = digestValue(value);
  return digest ? `${digest.slice(0, 12)}…` : '—';
}

function safeError(body: { error?: unknown }, status: number): string {
  return stringValue(body.error) ?? `The coding run failed (HTTP ${status}).`;
}

const Coder: NextPage = () => {
  const router = useRouter();
  const [toolId, setToolId] = useState<ToolId>('build');
  const [task, setTask] = useState('');
  const [pathInput, setPathInput] = useState('');
  const [running, setRunning] = useState(false);
  const [rollingBack, setRollingBack] = useState(false);
  const [result, setResult] = useState<CodeProjection | null>(null);
  const [error, setError] = useState<string | null>(null);
  const selectedTool = TOOLS.find((tool) => tool.id === toolId) ?? TOOLS[0]!;
  const paths = useMemo(() => parsePaths(pathInput), [pathInput]);
  const duplicatePath = useMemo(() => new Set(paths).size !== paths.length, [paths]);
  const runtime = result?.observed?.runtime && isObject(result.observed.runtime)
    ? result.observed.runtime
    : null;
  const test = result?.observed?.test && isObject(result.observed.test)
    ? result.observed.test
    : null;
  const state = runtime ? stringValue(runtime.state) : null;
  const promotionDigest = runtime ? digestValue(runtime.promotion_event_digest) : null;
  const attempts = Array.isArray(result?.generated?.faculty_attempts)
    ? result.generated.faculty_attempts.filter(isObject)
    : [];
  const receiptPaths = Array.isArray(result?.generated?.requested_allowed_paths)
    ? result.generated.requested_allowed_paths.filter((value): value is string => typeof value === 'string')
    : [];
  const canRun = !running
    && !rollingBack
    && task.trim().length > 0
    && paths.length > 0
    && paths.length <= 32
    && !duplicatePath;

  useEffect(() => {
    if (!router.isReady) return;
    const requested = Array.isArray(router.query.tool) ? router.query.tool[0] : router.query.tool;
    if (TOOLS.some((tool) => tool.id === requested)) setToolId(requested as ToolId);
  }, [router.isReady, router.query.tool]);

  async function run(): Promise<void> {
    if (!canRun) return;
    const objective = `${selectedTool.instruction}\n\n${task.trim()}`;
    setRunning(true);
    setError(null);
    setResult(null);
    const controller = new AbortController();
    const deadline = setTimeout(() => controller.abort(), BROWSER_DEADLINE_MS);
    try {
      const response = await authFetch('/api/admin/apocv4/code', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        cache: 'no-store',
        signal: controller.signal,
        body: JSON.stringify({
          objective,
          allowed_paths: paths,
          confirm_apply: true,
        }),
      });
      const body = await response.json() as CodeProjection;
      if (!response.ok) throw new Error(safeError(body, response.status));
      if (body.kind !== 'code' || !body.observed || !body.generated) {
        throw new Error('The runtime returned an invalid coding receipt.');
      }
      setResult(body);
    } catch (cause) {
      setError(cause instanceof DOMException && cause.name === 'AbortError'
        ? 'The bounded coding run exceeded its browser deadline. It was not retried automatically.'
        : cause instanceof Error ? cause.message : String(cause));
    } finally {
      clearTimeout(deadline);
      setRunning(false);
    }
  }

  async function rollback(): Promise<void> {
    if (!promotionDigest || rollingBack || running) return;
    setRollingBack(true);
    setError(null);
    try {
      const response = await authFetch('/api/admin/apocv4/code/rollback', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        cache: 'no-store',
        body: JSON.stringify({
          promotion_event_digest: promotionDigest,
          confirm_rollback: true,
        }),
      });
      const body = await response.json() as RollbackProjection;
      if (!response.ok) throw new Error(safeError(body, response.status));
      const rollbackRuntime = body.observed?.runtime;
      if (
        body.kind !== 'rollback'
        || !isObject(body.observed?.receipt)
        || !isObject(rollbackRuntime)
        || rollbackRuntime.state !== 'ROLLED_BACK'
        || rollbackRuntime.promotion_event_digest !== promotionDigest
        || !digestValue(rollbackRuntime.rollback_event_digest)
        || !digestValue(rollbackRuntime.journal_tip_digest)
      ) {
        throw new Error('The runtime returned an invalid rollback receipt.');
      }
      setResult((current) => current && current.observed?.runtime
        ? {
          ...current,
          observed: {
            ...current.observed,
            runtime: {
              ...current.observed.runtime,
              state: rollbackRuntime.state,
              rollback_event_digest: rollbackRuntime.rollback_event_digest,
              rollback_journal_tip_digest: rollbackRuntime.journal_tip_digest,
            },
          },
        }
        : current);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setRollingBack(false);
    }
  }

  return (
    <AdminLayout title="Agent workspace" hideHeading immersive>
      <div className="agent-shell">
        <header className="agent-topbar">
          <div>
            <span className="agent-mark">APOCRYPHA</span>
            <strong>Agent workspace</strong>
          </div>
          <nav aria-label="Workspace views">
            <Link href="/admin/chat">Chat</Link>
            <span aria-current="page">Code</span>
            <Link href="/admin/apex">Runtime</Link>
            <Link href="/admin/logs">Logs</Link>
          </nav>
        </header>

        <main className="agent-main">
          <aside className="agent-tools" aria-label="Generative tools">
            <p className="section-label">Generative tools</p>
            {TOOLS.map((tool) => (
              <button
                key={tool.id}
                type="button"
                className={tool.id === toolId ? 'active' : ''}
                aria-pressed={tool.id === toolId}
                onClick={() => setToolId(tool.id)}
                disabled={running || rollingBack}
              >
                <strong>{tool.label}</strong>
                <span>{tool.description}</span>
              </button>
            ))}
            <div className="boundary-card">
              <strong>Governed effect</strong>
              <span>Exact file scope</span>
              <span>Isolated execution</span>
              <span>Tests before promotion</span>
              <span>One-shot rollback</span>
            </div>
          </aside>

          <section className="agent-task">
            <div className="task-heading">
              <div>
                <p className="section-label">{selectedTool.label} task</p>
                <h1>Tell Apocrypha what to change.</h1>
              </div>
              <span className={`state-badge state-${(state ?? (running ? 'RUNNING' : 'READY')).toLowerCase()}`}>
                {running ? 'RUNNING' : state ?? 'READY'}
              </span>
            </div>

            <label htmlFor="coder-task">Task</label>
            <textarea
              id="coder-task"
              className="task-input"
              value={task}
              onChange={(event) => setTask(event.target.value)}
              placeholder="Describe the feature, defect, refactor, tests, or documentation you want."
              rows={8}
              disabled={running || rollingBack}
            />

            <label htmlFor="coder-paths">Allowed files · one exact repository-relative path per line</label>
            <textarea
              id="coder-paths"
              className="path-input"
              value={pathInput}
              onChange={(event) => setPathInput(event.target.value)}
              placeholder={'src/apocv4/example.py\ntests/test_example.py'}
              rows={4}
              spellCheck={false}
              disabled={running || rollingBack}
            />
            <div className="scope-line">
              <span>{paths.length}/32 files</span>
              <span>Mode adds: {selectedTool.instruction}</span>
            </div>
            {duplicatePath && <p className="form-error">Each allowed path must be unique.</p>}
            {error && <p className="run-error" role="alert">{error}</p>}

            <div className="run-actions">
              <button type="button" className="run-button" onClick={() => { void run(); }} disabled={!canRun}>
                {running ? 'Generating, testing and applying…' : 'Generate, test & apply'}
              </button>
              <span>No automatic retry. Changes apply only after isolated tests pass.</span>
            </div>
          </section>

          <aside className="agent-receipt" aria-label="Coding run receipt">
            <p className="section-label">Run receipt</p>
            {!result && !running && (
              <div className="receipt-empty">
                <strong>No run yet</strong>
                <span>The admitted scope, test result, effect state, and rollback receipt will appear here.</span>
              </div>
            )}
            {running && (
              <p className="run-progress" aria-live="polite">
                Coding run in progress. Exact stage and effect details arrive with the final receipt.
              </p>
            )}
            {result && runtime && (
              <>
                <div className="receipt-state">
                  <span>Effect state</span>
                  <strong>{state}</strong>
                </div>
                <dl className="receipt-grid">
                  <div><dt>Edge time</dt><dd>{String(result.observed?.receipt?.latency_ms ?? '—')} ms</dd></div>
                  <div><dt>Files</dt><dd>{receiptPaths.length}</dd></div>
                  <div><dt>Proposal</dt><dd>{shortDigest(result.generated?.proposal_digest)}</dd></div>
                  <div><dt>Frame</dt><dd>{shortDigest(runtime.frame_digest)}</dd></div>
                  <div><dt>Delta</dt><dd>{shortDigest(isObject(runtime.isolated_outcome) ? runtime.isolated_outcome.delta_digest : null)}</dd></div>
                  <div><dt>{state === 'ROLLED_BACK' ? 'Rollback' : 'Event'}</dt><dd>{shortDigest(state === 'ROLLED_BACK' ? runtime.rollback_event_digest : runtime.terminal_event_digest)}</dd></div>
                </dl>

                <section className="receipt-section">
                  <h2>Allowed file scope</h2>
                  <ul className="artifact-list">
                    {receiptPaths.map((path) => <li key={path}><code>{path}</code></li>)}
                  </ul>
                </section>

                <section className="receipt-section">
                  <h2>Faculty</h2>
                  {attempts.map((attempt, index) => (
                    <div className="attempt" key={`${String(attempt.faculty_identity_digest)}-${index}`}>
                      <span>{String(attempt.status ?? 'UNKNOWN')}</span>
                      <code>{shortDigest(attempt.faculty_identity_digest)}</code>
                    </div>
                  ))}
                </section>

                <section className="receipt-section">
                  <h2>Isolated test</h2>
                  {test ? (
                    <dl className="test-grid">
                      <div><dt>Passed</dt><dd>{test.passed === true ? 'YES' : 'NO'}</dd></div>
                      <div><dt>Exit</dt><dd>{String(test.exit_code ?? '—')}</dd></div>
                      <div><dt>Time</dt><dd>{String(test.elapsed_ms ?? '—')} ms</dd></div>
                      <div><dt>Receipt</dt><dd>{shortDigest(test.receipt_digest)}</dd></div>
                    </dl>
                  ) : <p className="muted">No isolated test receipt was returned for this terminal state.</p>}
                </section>

                {promotionDigest && state === 'PROMOTED' && (
                  <button
                    type="button"
                    className="rollback-button"
                    onClick={() => { void rollback(); }}
                    disabled={rollingBack || running}
                  >
                    {rollingBack ? 'Rolling back…' : 'Rollback this change'}
                  </button>
                )}
              </>
            )}
          </aside>
        </main>
      </div>

      <style jsx>{`
        .agent-shell { min-height:100dvh; background:#090b10; color:#edf0f7; font-family:Inter,ui-sans-serif,system-ui,sans-serif; }
        .agent-topbar { min-height:62px; display:flex; align-items:center; justify-content:space-between; gap:24px; padding:0 clamp(16px,3vw,34px); border-bottom:1px solid #242936; background:rgba(11,13,19,.94); }
        .agent-topbar>div { display:flex; align-items:baseline; gap:12px; }
        .agent-mark,.section-label { color:#9b8cff; font:750 .66rem/1.2 ui-monospace,monospace; letter-spacing:.16em; text-transform:uppercase; }
        .agent-topbar nav { display:flex; align-items:center; gap:8px; }
        .agent-topbar nav :global(a),.agent-topbar nav span { padding:8px 11px; border-radius:8px; color:#969cab; font-size:.82rem; }
        .agent-topbar nav span { color:#f2f4f8; background:#202431; }
        .agent-main { display:grid; grid-template-columns:220px minmax(360px,1fr) minmax(300px,390px); min-height:calc(100dvh - 62px); }
        .agent-tools,.agent-receipt { padding:24px 18px; background:#0d1017; }
        .agent-tools { border-right:1px solid #222733; }
        .agent-receipt { border-left:1px solid #222733; overflow-y:auto; max-height:calc(100dvh - 62px); }
        .section-label { margin:0 0 14px; }
        .agent-tools>button { width:100%; display:grid; gap:4px; margin:0 0 7px; padding:12px; border:1px solid transparent; border-radius:10px; text-align:left; color:#c8cdd8; background:transparent; cursor:pointer; }
        .agent-tools>button:hover,.agent-tools>button.active { border-color:#363c4c; background:#171b25; }
        .agent-tools>button.active strong { color:#fff; }
        .agent-tools>button span { color:#767e8e; font-size:.73rem; line-height:1.4; }
        .boundary-card { display:grid; gap:7px; margin-top:24px; padding:14px; border:1px solid #28303d; border-radius:11px; color:#87909e; font:600 .7rem/1.35 ui-monospace,monospace; background:#10141c; }
        .boundary-card strong { color:#dce1ea; margin-bottom:3px; }
        .agent-task { padding:clamp(24px,4vw,56px); overflow-y:auto; }
        .task-heading { display:flex; align-items:flex-start; justify-content:space-between; gap:24px; margin-bottom:30px; }
        h1 { margin:7px 0 0; font-size:clamp(1.5rem,3vw,2.35rem); letter-spacing:-.035em; }
        .state-badge { padding:7px 9px; border:1px solid #343b4a; border-radius:999px; color:#9aa2b2; font:700 .65rem/1 ui-monospace,monospace; }
        .state-promoted { color:#73e2ae; border-color:#285b45; background:#10271d; }
        .state-rolled_back,.state-execution_rolled_back,.state-promotion_aborted { color:#ffca79; border-color:#654d27; background:#2b2111; }
        label { display:block; margin:0 0 8px; color:#a8afbd; font-size:.78rem; font-weight:700; }
        textarea { width:100%; box-sizing:border-box; border:1px solid #303746; border-radius:12px; color:#f0f2f7; background:#11151e; outline:none; resize:vertical; }
        textarea:focus { border-color:#8170db; box-shadow:0 0 0 3px rgba(129,112,219,.14); }
        .task-input { padding:16px; font:inherit; line-height:1.6; }
        .path-input { margin-top:20px; padding:13px 15px; color:#c8d7ee; font:500 .8rem/1.55 ui-monospace,monospace; }
        .scope-line { display:flex; justify-content:space-between; gap:16px; margin-top:8px; color:#697181; font-size:.68rem; }
        .form-error,.run-error { padding:10px 12px; border-radius:9px; color:#ffb4bb; background:#2b151b; border:1px solid #5b2d36; font-size:.8rem; }
        .run-actions { display:flex; align-items:center; gap:15px; margin-top:24px; }
        .run-actions span { color:#71798a; font-size:.72rem; }
        .run-button { min-height:48px; padding:0 18px; border:0; border-radius:11px; color:#100f17; background:linear-gradient(135deg,#ffc671,#b5a2ff); font-weight:800; cursor:pointer; }
        button:disabled { opacity:.5; cursor:not-allowed; }
        .receipt-empty { display:grid; gap:7px; margin-top:40px; color:#727a89; line-height:1.55; font-size:.82rem; }
        .receipt-empty strong { color:#d7dbe4; }
        .run-progress { margin:28px 0; padding:14px; border:1px solid #2d3341; border-radius:10px; color:#b7adcf; background:#11151e; font-size:.78rem; line-height:1.55; }
        .receipt-state { display:flex; align-items:center; justify-content:space-between; margin:4px 0 18px; padding:14px; border:1px solid #29313e; border-radius:11px; background:#11161f; }
        .receipt-state span { color:#7e8796; font-size:.72rem; }
        .receipt-state strong { font:750 .78rem/1 ui-monospace,monospace; }
        .receipt-grid,.test-grid { display:grid; grid-template-columns:1fr 1fr; gap:1px; overflow:hidden; border:1px solid #252c38; border-radius:10px; background:#252c38; }
        .receipt-grid div,.test-grid div { padding:10px; background:#10141c; min-width:0; }
        dt { color:#6e7685; font-size:.64rem; text-transform:uppercase; letter-spacing:.08em; }
        dd { margin:4px 0 0; overflow:hidden; color:#c9ced8; font:650 .72rem/1.4 ui-monospace,monospace; text-overflow:ellipsis; }
        .receipt-section { margin-top:22px; }
        .receipt-section h2 { margin:0 0 9px; color:#9da5b3; font-size:.72rem; text-transform:uppercase; letter-spacing:.09em; }
        .artifact-list { display:grid; gap:5px; margin:0; padding:0; list-style:none; }
        .artifact-list li { padding:8px 10px; border:1px solid #262d39; border-radius:8px; overflow:hidden; color:#c8d7ee; background:#10141b; text-overflow:ellipsis; }
        .artifact-list code { font-size:.7rem; }
        .attempt { display:flex; align-items:center; justify-content:space-between; gap:10px; padding:8px 0; border-bottom:1px solid #202631; color:#8ad8b2; font-size:.68rem; }
        .attempt code { color:#6f7786; }
        .muted { color:#747c8b; font-size:.76rem; line-height:1.5; }
        .rollback-button { width:100%; min-height:44px; margin-top:24px; border:1px solid #6b3a43; border-radius:10px; color:#ffbdc3; background:#2a151b; cursor:pointer; font-weight:750; }
        @media (max-width:1050px) { .agent-main { grid-template-columns:180px 1fr; } .agent-receipt { grid-column:1/-1; border-left:0; border-top:1px solid #222733; max-height:none; } }
        @media (max-width:700px) {
          .agent-topbar { align-items:flex-start; flex-direction:column; padding:12px; gap:10px; }
          .agent-topbar nav { width:100%; overflow-x:auto; }
          .agent-main { display:block; min-height:0; }
          .agent-tools { display:flex; gap:7px; overflow-x:auto; border-right:0; border-bottom:1px solid #222733; padding:12px; }
          .agent-tools .section-label,.boundary-card { display:none; }
          .agent-tools>button { min-width:112px; margin:0; padding:10px; }
          .agent-task { padding:24px 14px; }
          .task-heading { margin-bottom:22px; }
          .scope-line,.run-actions { align-items:flex-start; flex-direction:column; }
          .run-button { width:100%; }
          .agent-receipt { padding:22px 14px; }
        }
        @media (prefers-reduced-motion:reduce) { * { scroll-behavior:auto !important; transition:none !important; } }
      `}</style>
    </AdminLayout>
  );
};

export default Coder;
