import Link from 'next/link';
import { FormEvent, useMemo, useRef, useState } from 'react';

import { addLocalSpellbookEntry } from '../../lib/spellbook-local';
import {
  analyzeSpell,
  createSpellbookEntry,
  DEFAULT_SPELL_LIMITS,
  HALOIC_VOCAB,
  type SpellAnalysis,
  type VocabularyNamespace,
} from '../../lib/spellcraft';
import styles from '../../styles/SymbolicStudio.module.css';

const EXAMPLES = [
  ['Illuminate', 'ka-sol-el'],
  ['Hold a boundary', 'nau zur'],
  ['Conditional', 'ki shan root:rad, ya sol um verb:alg'],
] as const;

const NAMESPACES: readonly VocabularyNamespace[] = ['prefix', 'root', 'suffix', 'verb', 'particle', 'power'];

export default function SpellComposer(): JSX.Element {
  const [input, setInput] = useState('ka-sol-el');
  const [analysis, setAnalysis] = useState<SpellAnalysis | null>(null);
  const [namespace, setNamespace] = useState<VocabularyNamespace>('root');
  const [label, setLabel] = useState('');
  const [saveState, setSaveState] = useState<'idle' | 'saved' | 'failed'>('idle');
  const resultRef = useRef<HTMLElement>(null);

  const visibleVocabulary = useMemo(
    () => HALOIC_VOCAB.filter((entry) => entry.namespace === namespace),
    [namespace],
  );

  const compile = (event?: FormEvent): void => {
    event?.preventDefault();
    const next = analyzeSpell(input);
    setAnalysis(next);
    setSaveState('idle');
    window.requestAnimationFrame(() => resultRef.current?.focus());
  };

  const insert = (value: string): void => {
    setInput((current) => `${current.trim()}${current.trim() ? ' ' : ''}${value}`);
    setAnalysis(null);
    setSaveState('idle');
  };

  const save = (): void => {
    if (!analysis || analysis.status !== 'valid') return;
    const entry = createSpellbookEntry(analysis, {
      label: label || analysis.interpretation.text.slice(0, 72),
      savedAt: new Date().toISOString(),
    });
    const result = addLocalSpellbookEntry(window.localStorage, entry);
    setSaveState(result.status === 'ready' ? 'saved' : 'failed');
  };

  return (
    <div className={styles.composerLayout}>
      <form className={styles.workbench} onSubmit={compile}>
        <div className={styles.panelHeader}><span className={styles.statusDot} aria-hidden="true" /><span>ENGINE 1.0 · SYMBOLIC ONLY</span></div>
        <label className={styles.label} htmlFor="spell-source">Compose a symbolic intention</label>
        <textarea
          id="spell-source"
          className={styles.textarea}
          value={input}
          maxLength={DEFAULT_SPELL_LIMITS.maxInputChars}
          onChange={(event) => { setInput(event.target.value); setAnalysis(null); setSaveState('idle'); }}
          rows={5}
          spellCheck={false}
        />
        <div className={styles.inputMeta}><span>{input.length} / {DEFAULT_SPELL_LIMITS.maxInputChars}</span><span>Compile writes nothing</span></div>
        <div className={styles.exampleRow} aria-label="Example spells">
          {EXAMPLES.map(([name, value]) => <button key={value} type="button" onClick={() => { setInput(value); setAnalysis(null); }}>{name}</button>)}
        </div>
        <button className={styles.primaryButton} type="submit">Compile and interpret</button>

        <section className={styles.vocabulary} aria-labelledby="vocabulary-title">
          <div className={styles.vocabularyHeading}>
            <h2 id="vocabulary-title">Vocabulary deck</h2>
            <span>{HALOIC_VOCAB.length} versioned forms</span>
          </div>
          <div className={styles.namespaceTabs} role="group" aria-label="Vocabulary category">
            {NAMESPACES.map((item) => <button key={item} type="button" aria-pressed={namespace === item} onClick={() => setNamespace(item)}>{item}</button>)}
          </div>
          <ul className={styles.morphemeGrid}>
            {visibleVocabulary.map((entry) => {
              const ambiguous = HALOIC_VOCAB.filter((candidate) => candidate.lexeme === entry.lexeme).length > 1;
              const insertion = ambiguous ? `${entry.namespace}:${entry.lexeme}` : entry.lexeme;
              return (
                <li key={entry.id}>
                  <button type="button" onClick={() => insert(insertion)}>
                    <b>{insertion}</b><span>{entry.meaning}</span>
                  </button>
                </li>
              );
            })}
          </ul>
        </section>
      </form>

      <section ref={resultRef} className={styles.analysisPanel} tabIndex={-1} aria-live="polite">
        {!analysis ? (
          <div className={styles.emptyResult}><span aria-hidden="true">⌘</span><h2>Readable machinery.</h2><p>Compile to see the parse, English interpretation, immutable symbolic graph, uncertainty, and provenance. Unknown or ambiguous forms stop before compilation.</p></div>
        ) : (
          <>
            <div className={styles.analysisHeader}>
              <div><p className={styles.resultKicker}>COMPILER VERDICT</p><h2>{analysis.status}</h2></div>
              <span data-status={analysis.status}>{Math.round(analysis.confidence * 100)}% lexical confidence</span>
            </div>

            {analysis.issues.length > 0 ? (
              <ul className={styles.issueList} aria-label="Compiler messages">
                {analysis.issues.map((issue, index) => <li key={`${issue.code}-${index}`} data-severity={issue.severity}><code>{issue.code}</code><span>{issue.message}</span></li>)}
              </ul>
            ) : null}

            {analysis.status === 'valid' ? (
              <>
                <div className={styles.interpretation}><span>Plain-language reflection</span><p>{analysis.interpretation.text}</p><small>{analysis.interpretation.disclaimer}</small></div>
                <section className={styles.trace} aria-labelledby="trace-title">
                  <h3 id="trace-title">Morpheme trace</h3>
                  <ol>{analysis.interpretation.trace.map((item, index) => <li key={`${item.term}-${index}`}><b>{item.term}</b><span>{item.gloss}</span><em>{Math.round(item.confidence * 100)}%</em></li>)}</ol>
                </section>
                <section className={styles.graphPreview} aria-labelledby="graph-title">
                  <div><h3 id="graph-title">Symbolic graph</h3><span>{analysis.compiled.operations.length} immutable nodes · executable: no</span></div>
                  <ol>{analysis.compiled.operations.map((node) => <li key={node.id}><code>{node.id}</code><b>{node.operation.replace('symbolic.', '')}</b><span>{node.label}</span></li>)}</ol>
                </section>
                <details className={styles.receiptDetails}><summary>Provenance receipt</summary><dl><div><dt>Engine</dt><dd>{analysis.receipt.engineVersion}</dd></div><div><dt>Vocabulary</dt><dd>{analysis.receipt.vocabularyVersion}</dd></div><div><dt>Program hash</dt><dd><code>{analysis.receipt.programHash}</code></dd></div><div><dt>Authority</dt><dd>{analysis.receipt.authority}</dd></div></dl></details>

                <div className={styles.savePanel}>
                  <label htmlFor="spell-label">Spellbook label <span>optional</span></label>
                  <input id="spell-label" className={styles.textInput} value={label} maxLength={80} onChange={(event) => setLabel(event.target.value)} placeholder="Name this working" />
                  <button type="button" onClick={save}>{saveState === 'saved' ? 'Saved to this browser' : saveState === 'failed' ? 'Local save unavailable' : 'Save explicitly'}</button>
                </div>
                <div className={styles.buttonRow}>
                  <Link href="/sigils">Render a sigil →</Link>
                  <Link href="/spellbook">Open Spellbook →</Link>
                  <a href="https://chaos-tarot.com/free-reading?source=apocky-spellcraft-result" target="_blank" rel="noopener noreferrer">Ask Chaos Tarot ↗</a>
                </div>
              </>
            ) : (
              <div className={styles.blockedResult}>
                <h3>{analysis.status === 'quarantined' ? 'Meaning remains unresolved.' : 'Compilation stopped safely.'}</h3>
                <p>{analysis.status === 'quarantined' ? 'Qualify an ambiguous term as namespace:lexeme or replace an unknown form. Nothing was compiled or saved.' : 'Correct the marked input. No symbolic plan, sigil, storage write, or external action occurred.'}</p>
                <code>{analysis.status === 'quarantined' ? 'SYMBOLIC_UNKNOWN_QUARANTINED' : 'SYMBOLIC_COMPILE_BLOCKED'}</code>
              </div>
            )}
          </>
        )}
      </section>
    </div>
  );
}
