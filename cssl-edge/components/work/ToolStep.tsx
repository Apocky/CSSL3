import { useState } from 'react';
import { DiffView } from './DiffView';
import styles from './WorkConsole.module.css';
import type { ToolCallOutcome } from '@/lib/work/client';

function elapsed(ms: number): string {
  if (ms < 1_000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1_000).toFixed(1)}s`;
  return `${Math.floor(ms / 60_000)}m${String(Math.round((ms % 60_000) / 1_000)).padStart(2, '0')}s`;
}

export function ToolStep({ call, running }: { readonly call: ToolCallOutcome; readonly running?: boolean }): JSX.Element {
  const [open, setOpen] = useState(false);
  const tone = call.denied ? styles.stepDenied : call.ok ? '' : styles.stepFail;
  const body = call.ok ? call.content : (call.error ?? 'The step failed.');
  const hasBody = body.trim().length > 0;

  return (
    <div className={`${styles.step} ${tone}`}>
      <button
        type="button"
        className={styles.stepHead}
        onClick={() => setOpen((value) => !value)}
        aria-expanded={open}
        disabled={!hasBody && !call.diff}
      >
        <span className={styles.stepName} aria-hidden="true">{call.denied ? '⊘' : call.ok ? '⚙' : '✕'}</span>
        <span className={styles.stepName}>{call.name}</span>
        <span className={styles.stepSummary}>{call.summary}</span>
        {call.diff
          ? <span className={styles.diffStat}>
            <span className={styles.diffStatAdd}>+{call.diff.added}</span>{' '}
            <span className={styles.diffStatDel}>−{call.diff.removed}</span>
          </span>
          : null}
        <span className={styles.stepTime}>{running ? 'running…' : elapsed(call.elapsedMs)}</span>
      </button>
      {open && call.diff ? <DiffView patch={call.diff.patch} /> : null}
      {open && hasBody ? <pre className={styles.stepBody}>{body}</pre> : null}
    </div>
  );
}
