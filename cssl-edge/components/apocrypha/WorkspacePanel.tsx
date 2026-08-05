import { useEffect, useMemo, useRef, useState } from 'react';

import type { SiteAccessState } from '@/components/hub/SiteSession';
import styles from '@/styles/PublicApocrypha.module.css';

export type WorkspaceMode = 'general' | 'code' | 'analyze' | 'write' | 'explain';

interface WorkspacePanelProps {
  open: boolean;
  access: SiteAccessState;
  artifacts: Array<Record<string, unknown>>;
  jobs: Array<Record<string, unknown>>;
  artifactTotal: number;
  jobTotal: number;
  artifactsTruncated: boolean;
  jobsTruncated: boolean;
  activeJobCount: number;
  cancellingJobId: string | null;
  onClose: () => void;
  onPrepare: (mode: WorkspaceMode, prompt: string) => void;
  onCancelJob: (jobId: string) => void;
}

type WorkspaceTab = 'create' | 'artifacts' | 'activity';

interface Starter {
  label: string;
  title: string;
  description: string;
  mode: WorkspaceMode;
  prompt: string;
}

const CONTENT_STARTERS: readonly Starter[] = [
  {
    label: 'Document',
    title: 'Draft a finished document',
    description: 'Structure, write, and revise a source-aware deliverable.',
    mode: 'write',
    prompt: 'Create a polished document from this brief. Preserve source claims, mark unknowns, and finish with a publication-ready draft:\n',
  },
  {
    label: 'Research',
    title: 'Build an evidence brief',
    description: 'Separate observed facts, inferences, countercases, and gaps.',
    mode: 'analyze',
    prompt: 'Build an evidence brief for this question. Separate observed facts, inferences, strongest countercase, confidence, and the next discriminating test:\n',
  },
  {
    label: 'Campaign',
    title: 'Shape a content campaign',
    description: 'Create one coherent message across useful formats.',
    mode: 'write',
    prompt: 'Create a cohesive content campaign from this brief. Include the core narrative, audience, channel-specific drafts, reusable visual direction, and release checklist:\n',
  },
  {
    label: 'Explain',
    title: 'Make a complex idea clear',
    description: 'Turn dense material into a precise, usable explanation.',
    mode: 'explain',
    prompt: 'Turn this into a clear, precise explanation. Keep the important distinctions, show how the parts connect, and include a concrete example:\n',
  },
];

const DRAWER_MEDIA_QUERY = '(max-width: 1100px)';
const FOCUSABLE_SELECTOR = [
  'a[href]',
  'button:not([disabled])',
  'input:not([disabled])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(',');

function stringValue(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function numericValue(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function artifactText(artifact: Record<string, unknown>): string {
  const content = artifact.content;
  if (typeof content === 'string') return content;
  try {
    return JSON.stringify(content, null, 2) ?? '[Empty artifact]';
  } catch {
    return '[This artifact cannot be represented as text.]';
  }
}

function jobReceipt(job: Record<string, unknown>, state: string): { label: string; value: string } {
  const outputDigest = stringValue(job.output_digest);
  const errorDigest = stringValue(job.error_digest);
  const requestDigest = stringValue(job.request_digest);
  const terminalFailure = ['FAILED', 'CANCELLED', 'INTERRUPTED'].includes(state.toUpperCase());
  const digest = terminalFailure
    ? errorDigest ?? outputDigest ?? requestDigest
    : outputDigest ?? requestDigest ?? errorDigest;
  if (digest) {
    return {
      label: digest === errorDigest ? 'Error receipt' : digest === outputDigest ? 'Output receipt' : 'Request receipt',
      value: digest,
    };
  }
  const reason = stringValue(job.reason_code) ?? stringValue(job.error_class);
  if (reason) return { label: 'Reason', value: reason };
  return {
    label: 'Receipt',
    value: terminalFailure ? 'No terminal receipt recorded' : 'Awaiting completion',
  };
}

function artifactKey(artifact: Record<string, unknown>, index: number): string {
  return stringValue(artifact.artifact_id) ?? `artifact-${index}`;
}

function safeFilename(value: string): string {
  const normalized = value
    .normalize('NFKD')
    .replace(/[^a-zA-Z0-9._-]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 80);
  return normalized || 'apocky-artifact';
}

function downloadArtifact(artifact: Record<string, unknown>): void {
  const title = stringValue(artifact.title) ?? 'apocky-artifact';
  const kind = (stringValue(artifact.kind) ?? '').toLowerCase();
  const content = artifact.content;
  const structured = typeof content !== 'string';
  const extension = structured || kind.includes('json') ? 'json' : kind.includes('markdown') ? 'md' : 'txt';
  const mime = extension === 'json' ? 'application/json' : 'text/plain;charset=utf-8';
  const url = URL.createObjectURL(new Blob([artifactText(artifact)], { type: mime }));
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = `${safeFilename(title)}.${extension}`;
  anchor.click();
  setTimeout(() => URL.revokeObjectURL(url), 0);
}

export function WorkspacePanel({
  open,
  access,
  artifacts,
  jobs,
  artifactTotal,
  jobTotal,
  artifactsTruncated,
  jobsTruncated,
  activeJobCount,
  cancellingJobId,
  onClose,
  onPrepare,
  onCancelJob,
}: WorkspacePanelProps): JSX.Element {
  const [tab, setTab] = useState<WorkspaceTab>(artifacts.length > 0 ? 'artifacts' : 'create');
  const [selectedArtifactId, setSelectedArtifactId] = useState<string | null>(null);
  const [copiedArtifactId, setCopiedArtifactId] = useState<string | null>(null);
  const [drawer, setDrawer] = useState(false);
  const panelRef = useRef<HTMLElement>(null);
  const closeButtonRef = useRef<HTMLButtonElement>(null);
  const returnFocusRef = useRef<HTMLElement | null>(null);
  const authenticated = access === 'member' || access === 'owner';

  useEffect(() => {
    if (activeJobCount > 0) setTab('activity');
  }, [activeJobCount]);

  useEffect(() => {
    const media = window.matchMedia(DRAWER_MEDIA_QUERY);
    const sync = () => setDrawer(media.matches);
    sync();
    media.addEventListener('change', sync);
    return () => media.removeEventListener('change', sync);
  }, []);

  useEffect(() => {
    if (!drawer || !open || !panelRef.current) return undefined;
    const panel = panelRef.current;
    returnFocusRef.current = document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
    const priorOverflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    const focusPanel = window.requestAnimationFrame(() => {
      (closeButtonRef.current ?? panel).focus();
    });
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        onClose();
        return;
      }
      if (event.key !== 'Tab') return;
      const focusable = Array.from(panel.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR))
        .filter((element) => element.tabIndex >= 0);
      if (focusable.length === 0) {
        event.preventDefault();
        panel.focus();
        return;
      }
      const first = focusable[0]!;
      const last = focusable[focusable.length - 1]!;
      const active = document.activeElement;
      if (event.shiftKey && (active === first || !panel.contains(active))) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && (active === last || !panel.contains(active))) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener('keydown', onKeyDown, true);
    return () => {
      window.cancelAnimationFrame(focusPanel);
      document.removeEventListener('keydown', onKeyDown, true);
      document.body.style.overflow = priorOverflow;
      const returnTarget = returnFocusRef.current;
      returnFocusRef.current = null;
      window.requestAnimationFrame(() => {
        if (returnTarget?.isConnected) returnTarget.focus();
      });
    };
  }, [drawer, onClose, open]);

  useEffect(() => {
    const selectedStillExists = artifacts.some(
      (artifact, index) => artifactKey(artifact, index) === selectedArtifactId,
    );
    if (!selectedStillExists) {
      const lastIndex = artifacts.length - 1;
      setSelectedArtifactId(lastIndex >= 0 ? artifactKey(artifacts[lastIndex]!, lastIndex) : null);
    }
  }, [artifacts, selectedArtifactId]);

  const selectedArtifact = useMemo(() => artifacts.find(
    (artifact, index) => artifactKey(artifact, index) === selectedArtifactId,
  ) ?? null, [artifacts, selectedArtifactId]);

  const prepareStarter = (starter: Starter) => {
    if (!authenticated) return;
    onPrepare(starter.mode, starter.prompt);
  };

  const copyArtifact = async (artifact: Record<string, unknown>) => {
    const id = stringValue(artifact.artifact_id) ?? 'artifact';
    try {
      await navigator.clipboard.writeText(artifactText(artifact));
      setCopiedArtifactId(id);
      window.setTimeout(() => setCopiedArtifactId((current) => current === id ? null : current), 1_600);
    } catch {
      setCopiedArtifactId(null);
    }
  };

  return (
    <>
      <button
        type="button"
        className={styles.workspacePanelScrim}
        data-open={open}
        aria-label="Close workspace"
        tabIndex={-1}
        onClick={onClose}
      />
      <aside
        ref={panelRef}
        id="apocrypha-workspace-panel"
        className={styles.workspacePanel}
        data-open={open}
        data-drawer={drawer}
        role={drawer ? 'dialog' : undefined}
        aria-modal={drawer && open ? true : undefined}
        aria-hidden={drawer && !open ? true : undefined}
        aria-label="Apocrypha workspace"
        tabIndex={drawer ? -1 : undefined}
      >
        <header className={styles.workspacePanelHeader}>
          <div>
            <span>Workspace</span>
            <strong>Create, inspect, continue</strong>
          </div>
          <button ref={closeButtonRef} type="button" className={styles.workspacePanelClose} onClick={onClose} aria-label="Close workspace">×</button>
        </header>

        <div className={styles.workspaceTabs} role="tablist" aria-label="Workspace views">
          <button type="button" role="tab" aria-selected={tab === 'create'} onClick={() => setTab('create')}>Create</button>
          <button type="button" role="tab" aria-selected={tab === 'artifacts'} onClick={() => setTab('artifacts')}>
            Artifacts <span>{artifacts.length === artifactTotal ? artifactTotal : `${artifacts.length}/${artifactTotal}`}</span>
          </button>
          <button type="button" role="tab" aria-selected={tab === 'activity'} onClick={() => setTab('activity')}>
            Activity <span>{jobs.length === jobTotal ? jobTotal : `${jobs.length}/${jobTotal}`}</span>
          </button>
        </div>

        <div className={styles.workspacePanelBody} tabIndex={0} aria-label="Workspace content">
          {tab === 'create' && (
            <section className={styles.workspaceCreate} aria-label="Content starters">
              <div className={styles.workspaceIntro}>
                <span>From brief to working draft</span>
                <h2>What are we making?</h2>
                <p>Choose a starting shape, then revise the prepared brief in conversation. Nothing runs until you send it.</p>
              </div>
              <div className={styles.starterGrid}>
                {CONTENT_STARTERS.map((starter) => (
                  <button
                    key={starter.label}
                    type="button"
                    onClick={() => prepareStarter(starter)}
                    disabled={!authenticated}
                  >
                    <span>{starter.label}</span>
                    <strong>{starter.title}</strong>
                    <small>{starter.description}</small>
                    <b aria-hidden="true">↗</b>
                  </button>
                ))}
                {access === 'owner' && (
                  <button
                    type="button"
                    onClick={() => onPrepare(
                      'code',
                      'Build and verify this interactive experience. First inspect the existing implementation, then use only the exact paths I approve:\n',
                    )}
                  >
                    <span>Interactive</span>
                    <strong>Build a working experience</strong>
                    <small>Governed code generation with exact paths, tests, receipts, and rollback.</small>
                    <b aria-hidden="true">↗</b>
                  </button>
                )}
              </div>
              {!authenticated && (
                <p className={styles.workspaceNotice}>Sign in to prepare a draft. No request is sent from this panel.</p>
              )}
              <div className={styles.capabilityLedger} aria-label="Current workspace capabilities">
                <div><span>Conversation</span><strong>{authenticated ? 'Ready' : 'Sign-in required'}</strong></div>
                <div><span>Files</span><strong>Text context</strong></div>
                <div><span>Background work</span><strong>{activeJobCount > 0 ? `${activeJobCount} active` : 'Observed here'}</strong></div>
                <div><span>External changes</span><strong>{access === 'owner' ? 'Exact approval' : 'Not granted'}</strong></div>
              </div>
            </section>
          )}

          {tab === 'artifacts' && (
            <section className={styles.artifactWorkspace} aria-label="Conversation artifacts">
              {(artifactsTruncated || artifactTotal > artifacts.length) && (
                <p className={styles.workspaceProjectionNotice} role="status">
                  Showing {artifacts.length} of {artifactTotal} saved artifacts from the bounded runtime projection.
                </p>
              )}
              {artifacts.length === 0 ? (
                <div className={styles.workspaceEmpty}>
                  <span aria-hidden="true">◇</span>
                  <strong>{artifactTotal > 0 ? 'No artifacts visible in this projection' : 'No saved artifacts in this conversation'}</strong>
                  <p>{artifactTotal > 0
                    ? 'The runtime reports saved artifacts outside this bounded response. Refreshing the conversation may expose a newer projection.'
                    : 'Runtime-created results appear here with their exact content and provenance. Ordinary replies stay in the conversation.'}</p>
                  <button type="button" onClick={() => setTab('create')}>Start a content draft</button>
                </div>
              ) : (
                <>
                  <div className={styles.artifactList} aria-label="Saved artifact list">
                    {artifacts.map((artifact, index) => {
                      const id = artifactKey(artifact, index);
                      const title = stringValue(artifact.title) ?? stringValue(artifact.kind) ?? 'Conversation artifact';
                      return (
                        <button
                          type="button"
                          key={id}
                          aria-pressed={id === selectedArtifactId}
                          onClick={() => setSelectedArtifactId(id)}
                        >
                          <span aria-hidden="true">◇</span>
                          <span><strong>{title}</strong><small>{stringValue(artifact.kind) ?? 'artifact'}</small></span>
                        </button>
                      );
                    })}
                  </div>
                  {selectedArtifact && (
                    <article className={styles.artifactPreview}>
                      <header>
                        <div>
                          <span>{stringValue(selectedArtifact.kind) ?? 'Artifact'}</span>
                          <h2>{stringValue(selectedArtifact.title) ?? 'Conversation artifact'}</h2>
                        </div>
                        <div>
                          <button type="button" onClick={() => { void copyArtifact(selectedArtifact); }}>
                            {copiedArtifactId === stringValue(selectedArtifact.artifact_id) ? 'Copied' : 'Copy'}
                          </button>
                          <button type="button" onClick={() => downloadArtifact(selectedArtifact)}>Download</button>
                        </div>
                      </header>
                      <pre>{artifactText(selectedArtifact)}</pre>
                      <dl>
                        <div><dt>Bytes</dt><dd>{(numericValue(selectedArtifact.content_bytes) ?? artifactText(selectedArtifact).length).toLocaleString()}</dd></div>
                        <div><dt>Content receipt</dt><dd>{stringValue(selectedArtifact.content_digest) ?? 'Unavailable'}</dd></div>
                        <div><dt>Event receipt</dt><dd>{stringValue(selectedArtifact.event_digest) ?? 'Unavailable'}</dd></div>
                      </dl>
                      {Array.isArray(selectedArtifact.refs) && selectedArtifact.refs.length > 0 && (
                        <details>
                          <summary>References · {selectedArtifact.refs.length}</summary>
                          <ul>{selectedArtifact.refs.map((ref, index) => <li key={`${String(ref)}-${index}`}>{String(ref)}</li>)}</ul>
                        </details>
                      )}
                    </article>
                  )}
                </>
              )}
            </section>
          )}

          {tab === 'activity' && (
            <section className={styles.activityWorkspace} aria-label="Observed background activity">
              <div className={styles.workspaceIntro}>
                <span>Durable run state</span>
                <h2>{activeJobCount > 0 ? `${activeJobCount} active` : 'At rest'}</h2>
                <p>This view reports runtime-recorded work. Controls that are not genuinely wired are omitted.</p>
              </div>
              {(jobsTruncated || jobTotal > jobs.length) && (
                <p className={styles.workspaceProjectionNotice} role="status">
                  Showing {jobs.length} of {jobTotal} recorded jobs from the bounded runtime projection.
                </p>
              )}
              {jobs.length === 0 ? (
                <div className={styles.workspaceEmpty}>
                  <span aria-hidden="true">○</span>
                  <strong>{jobTotal > 0 ? 'No jobs visible in this projection' : 'No background work recorded'}</strong>
                  <p>{jobTotal > 0
                    ? 'The runtime reports durable jobs outside this bounded response.'
                    : 'Agentic tasks will appear here only when the runtime returns a durable job record.'}</p>
                </div>
              ) : (
                <div className={styles.activityList}>
                  {[...jobs].reverse().map((job, index) => {
                    const state = stringValue(job.state) ?? 'RECORDED';
                    const jobId = stringValue(job.job_id);
                    const actionId = stringValue(job.action_id) ?? 'Background task';
                    const action = actionId === 'objective.proposal_council.v1' ? 'Proposal council' : actionId;
                    const receipt = jobReceipt(job, state);
                    const argumentsValue = job.arguments !== null && typeof job.arguments === 'object' && !Array.isArray(job.arguments)
                      ? job.arguments as Record<string, unknown>
                      : null;
                    const objective = stringValue(argumentsValue?.objective);
                    const active = ['QUEUED', 'RUNNING', 'CANCEL_REQUESTED'].includes(state);
                    return (
                      <article key={jobId ?? `job-${index}`}>
                        <header><strong>{action}</strong><span data-state={state.toLowerCase()}>{state}</span></header>
                        {objective && <p className={styles.jobObjective}>{objective}</p>}
                        <dl>
                          <div><dt>Attempt</dt><dd>{numericValue(job.attempt) ?? 0}</dd></div>
                          <div><dt>Artifacts</dt><dd>{Array.isArray(job.artifact_ids) ? job.artifact_ids.length : 0}</dd></div>
                          <div><dt>{receipt.label}</dt><dd>{receipt.value}</dd></div>
                        </dl>
                        {stringValue(job.reason_code) && <p>{stringValue(job.reason_code)}</p>}
                        {active && jobId && (
                          <button
                            type="button"
                            className={styles.jobCancelButton}
                            onClick={() => onCancelJob(jobId)}
                            disabled={cancellingJobId !== null}
                          >
                            {cancellingJobId === jobId ? 'Cancelling…' : 'Cancel background work'}
                          </button>
                        )}
                      </article>
                    );
                  })}
                </div>
              )}
            </section>
          )}
        </div>

        <footer className={styles.workspacePanelFooter}>
          <span><i data-active={activeJobCount > 0} />{activeJobCount > 0 ? `${activeJobCount} active` : 'Runtime at rest'}</span>
          <span>Training off</span>
        </footer>
      </aside>
    </>
  );
}
