import { useEffect, useRef } from 'react';
import styles from './WorkConsole.module.css';
import type { ConsentDecision, RiskTier } from '@/lib/work/client';

export interface ConsentPrompt {
  readonly id: string;
  readonly tool: string;
  readonly risk: RiskTier;
  readonly summary: string;
  readonly detail: string;
}

const RISK_WORD: Record<RiskTier, string> = {
  read: 'Read',
  write: 'Change a file',
  execute: 'Run a command',
};

export function ConsentCard({
  prompt, onDecide, busy,
}: {
  readonly prompt: ConsentPrompt;
  readonly onDecide: (decision: ConsentDecision) => void;
  readonly busy: boolean;
}): JSX.Element {
  const first = useRef<HTMLButtonElement | null>(null);
  // The agent is blocked until this is answered, so put focus on the decision rather than making
  // the operator hunt for it after scrolling.
  useEffect(() => { first.current?.focus(); }, [prompt.id]);

  return (
    <section className={styles.consent} aria-label="Approval needed" aria-live="assertive">
      <div className={styles.consentHead}>
        <span className={styles.consentTitle}>{RISK_WORD[prompt.risk]}</span>
        <span className={styles.stepTime}>{prompt.tool}</span>
      </div>
      <p className={styles.consentSummary}>{prompt.summary}</p>
      {prompt.detail.trim() ? <pre className={styles.consentDetail}>{prompt.detail}</pre> : null}
      <div className={styles.consentRow}>
        <button ref={first} type="button" className={styles.allow} disabled={busy} onClick={() => onDecide('allow')}>
          Allow once
        </button>
        <button type="button" className={styles.always} disabled={busy} onClick={() => onDecide('allow_session')}>
          Allow {prompt.tool} for this task
        </button>
        <button type="button" className={styles.deny} disabled={busy} onClick={() => onDecide('deny')}>
          Decline
        </button>
      </div>
    </section>
  );
}
