import Link from 'next/link';
import { FormEvent, useMemo, useRef, useState } from 'react';

import {
  createOracleSeed,
  drawOracle,
  isHighStakesOracleQuestion,
  ORACLE_MAX_QUESTION_LENGTH,
  type OracleReading,
} from '../../lib/oracle';
import styles from '../../styles/SymbolicStudio.module.css';

const STARTERS = [
  'Should I make the smallest reversible move today?',
  'Should I ask for more information before deciding?',
  'Should I protect more time for this work?',
] as const;

export default function YesNoOracle(): JSX.Element {
  const [question, setQuestion] = useState('');
  const [reading, setReading] = useState<OracleReading | null>(null);
  const [history, setHistory] = useState<readonly OracleReading[]>([]);
  const [error, setError] = useState<{ readonly message: string; readonly code: string } | null>(null);
  const [copyState, setCopyState] = useState<'idle' | 'copied' | 'failed'>('idle');
  const [pending, setPending] = useState(false);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const resultRef = useRef<HTMLElement>(null);
  const remaining = ORACLE_MAX_QUESTION_LENGTH - question.length;

  const safetyNote = useMemo(() => {
    return isHighStakesOracleQuestion(question)
      ? 'This looks high-stakes. Use the signal only to uncover a question; rely on qualified help and real evidence for the decision.'
      : null;
  }, [question]);

  const reveal = (event?: FormEvent): void => {
    event?.preventDefault();
    setCopyState('idle');
    setPending(true);
    try {
      const next = drawOracle(question, createOracleSeed());
      setReading(next);
      setHistory((current) => [next, ...current].slice(0, 3));
      setError(null);
      window.requestAnimationFrame(() => resultRef.current?.focus());
    } catch (caught) {
      const code = caught instanceof Error ? caught.message : 'APX-ORACLE-UNKNOWN';
      setError({
        code: code === 'APX-ORACLE-QUESTION-REQUIRED'
          ? 'ORACLE_INPUT_REQUIRED'
          : code === 'APX-ORACLE-HIGH-STAKES-BLOCKED'
            ? 'ORACLE_HIGH_STAKES_BLOCKED'
            : code === 'APX-ORACLE-CRYPTO-UNAVAILABLE'
              ? 'ORACLE_CRYPTO_UNAVAILABLE'
              : 'SYMBOLIC_INPUT_LIMIT',
        message:
        code === 'APX-ORACLE-QUESTION-REQUIRED'
          ? 'Write one question first.'
          : code === 'APX-ORACLE-HIGH-STAKES-BLOCKED'
            ? 'This oracle does not answer medical, legal, financial, safety, self-harm, surveillance, coercion, or directed-harm decisions. Use qualified help and real evidence.'
            : code === 'APX-ORACLE-CRYPTO-UNAVAILABLE'
              ? 'This browser cannot generate a private signal. Try a current browser.'
              : 'Keep the question under 280 characters.',
      });
      inputRef.current?.focus();
    } finally {
      setPending(false);
    }
  };

  const copyReceipt = async (): Promise<void> => {
    if (!reading) return;
    try {
      await navigator.clipboard.writeText(
        `${reading.signal.toUpperCase()} — ${reading.question}\n${reading.counterweight}\nSeed: ${reading.seed}\nEngine: ${reading.version}\nReceipt: ${reading.receipt}`,
      );
      setCopyState('copied');
    } catch {
      setCopyState('failed');
    }
  };

  return (
    <div className={styles.toolGrid}>
      <form className={styles.workbench} onSubmit={reveal}>
        <div className={styles.panelHeader}>
          <span className={styles.statusDot} aria-hidden="true" />
          <span>FAST SIGNAL · SESSION ONLY</span>
        </div>
        <label className={styles.label} htmlFor="oracle-question">Ask one question that can be answered yes or no</label>
        <textarea
          ref={inputRef}
          id="oracle-question"
          className={styles.textarea}
          value={question}
          maxLength={ORACLE_MAX_QUESTION_LENGTH}
          onChange={(event) => {
            setQuestion(event.target.value);
            setError(null);
          }}
          placeholder="Should I…?"
          rows={4}
        />
        <div className={styles.inputMeta}>
          <span>{remaining} characters left</span>
          <span>Nothing is uploaded</span>
        </div>
        {safetyNote ? <p className={styles.warning} role="note">{safetyNote}</p> : null}
        {error ? <p className={styles.error} role="alert">{error.message} <code>{error.code}</code></p> : null}
        <button className={styles.primaryButton} type="submit" disabled={pending}>{pending ? 'Drawing…' : 'Reveal yes / no'}</button>
        <div className={styles.starters} aria-label="Example questions">
          {STARTERS.map((starter) => (
            <button key={starter} type="button" onClick={() => { setQuestion(starter); setError(null); inputRef.current?.focus(); }}>
              {starter}
            </button>
          ))}
        </div>
      </form>

      <section
        ref={resultRef}
        className={styles.resultPanel}
        tabIndex={-1}
        aria-live="polite"
        aria-label="Oracle result"
        data-signal={reading?.signal ?? 'waiting'}
      >
        {reading ? (
          <>
            <p className={styles.resultKicker}>THE SIGNAL SAYS</p>
            <strong className={styles.signal}>{reading.signal}</strong>
            <div className={styles.clarity}>
              <span>symbolic clarity</span>
              <div><i style={{ width: `${reading.clarity}%` }} /></div>
              <b>{reading.clarity}%</b>
            </div>
            <p className={styles.counterweight}>{reading.counterweight}</p>
            <p className={styles.nextQuestion}>{reading.nextQuestion}</p>
            <dl className={styles.oracleReceiptMeta} aria-label="Reproduction details">
              <div><dt>Seed</dt><dd><code>{reading.seed}</code></dd></div>
              <div><dt>Engine</dt><dd>{reading.version}</dd></div>
            </dl>
            <code className={styles.receipt}>{reading.receipt}</code>
            <div className={styles.buttonRow}>
              <button type="button" onClick={() => reveal()}>Draw again</button>
              <button type="button" onClick={copyReceipt}>{copyState === 'copied' ? 'Copied' : copyState === 'failed' ? 'Copy unavailable' : 'Copy receipt'}</button>
              <Link href="/spellcraft">Shape an intention →</Link>
              <a href="https://chaos-tarot.com/free-reading?source=apocky-oracle-result" target="_blank" rel="noopener noreferrer">Ask Chaos Tarot ↗</a>
            </div>
          </>
        ) : (
          <div className={styles.emptyResult}>
            <span aria-hidden="true">◇</span>
            <h2>One bit of friction.</h2>
            <p>The result is generated on this device from your question and a fresh seed. Treat it as a prompt that interrupts certainty—not a prediction or command.</p>
          </div>
        )}
      </section>

      {history.length > 1 ? (
        <section className={styles.sessionHistory} aria-labelledby="oracle-history-title">
          <div>
            <p className={styles.resultKicker}>CURRENT TAB</p>
            <h2 id="oracle-history-title">Recent signals</h2>
          </div>
          <ol>
            {history.map((item, index) => (
              <li key={`${item.receipt}-${index}`}>
                <b>{item.signal}</b><span>{item.question}</span><code>{item.receipt.split(':').at(-1)}</code>
              </li>
            ))}
          </ol>
        </section>
      ) : null}
    </div>
  );
}
