import Link from 'next/link';
import { type FormEvent, useMemo, useRef, useState } from 'react';
import { addLocalSpellbookEntry } from '../../lib/spellbook-local';
import { analyzeSpell, createSigil, createSpellbookEntry, HALOIC_VOCAB, type SigilArtifact, type SpellAnalysis, type VocabularyNamespace } from '../../lib/spellcraft';
import IntentPicker from './IntentPicker';
import { INTENT_PRESETS, selectedIntent } from './intent-presets';
import styles from '../../styles/SymbolicStudio.module.css';

const NAMESPACES: readonly VocabularyNamespace[] = ['prefix', 'root', 'suffix', 'verb', 'particle', 'power'];

export default function SpellComposer(): JSX.Element {
  const [input, setInput] = useState<string>(INTENT_PRESETS[0].source);
  const [analysis, setAnalysis] = useState<SpellAnalysis | null>(null);
  const [artifact, setArtifact] = useState<SigilArtifact | null>(null);
  const [namespace, setNamespace] = useState<VocabularyNamespace>('root');
  const [label, setLabel] = useState('');
  const [saveState, setSaveState] = useState<'idle' | 'saved' | 'failed'>('idle');
  const resultRef = useRef<HTMLElement>(null);
  const visibleVocabulary = useMemo(() => HALOIC_VOCAB.filter(entry => entry.namespace === namespace), [namespace]);
  const intent = selectedIntent(input);
  const changeInput = (source: string): void => { setInput(source); setAnalysis(null); setArtifact(null); setSaveState('idle'); };
  const compile = (event?: FormEvent): void => {
    event?.preventDefault();
    setAnalysis(analyzeSpell(input)); setArtifact(null); setSaveState('idle');
    window.requestAnimationFrame(() => { resultRef.current?.focus(); resultRef.current?.scrollIntoView({ block: 'start' }); });
  };
  const save = (): void => {
    if (!analysis || analysis.status !== 'valid') return;
    const entry = createSpellbookEntry(analysis, { label: label.trim() || intent?.label || analysis.interpretation.text.slice(0, 72), savedAt: new Date().toISOString() });
    try { setSaveState(addLocalSpellbookEntry(window.localStorage, entry).status === 'ready' ? 'saved' : 'failed'); } catch { setSaveState('failed'); }
  };
  const downloadArt = (): void => {
    if (!artifact) return;
    const href = URL.createObjectURL(new Blob([artifact.svg], { type: 'image/svg+xml;charset=utf-8' }));
    const anchor = document.createElement('a'); anchor.href = href; anchor.download = `apocky-sigil-${artifact.seedHash.slice(0, 12)}.svg`; anchor.click(); URL.revokeObjectURL(href);
  };
  return <div className={styles.composerLayout}>
    <form className={styles.workbench} onSubmit={compile}>
      <IntentPicker input={input} onChange={changeInput} id="spell" />
      <button className={styles.primaryButton} type="submit">Create my spell <span aria-hidden="true">→</span></button>
      <p className={styles.smallNote}>A creative prompt for reflection. Saved only when you choose.</p>
      <details className={styles.customize}>
        <summary>Browse the symbolic vocabulary</summary><p>Choose a word to add it to the symbolic editor.</p>
        <div className={styles.namespaceTabs} role="group" aria-label="Vocabulary category">{NAMESPACES.map(item => <button key={item} type="button" aria-pressed={namespace === item} onClick={() => setNamespace(item)}>{item}</button>)}</div>
        <ul className={styles.morphemeGrid}>{visibleVocabulary.map(entry => {
          const ambiguous = HALOIC_VOCAB.filter(candidate => candidate.lexeme === entry.lexeme).length > 1;
          const insertion = ambiguous ? `${entry.namespace}:${entry.lexeme}` : entry.lexeme;
          return <li key={entry.id}><button type="button" onClick={() => changeInput(`${input.trim()}${input.trim() ? ' ' : ''}${insertion}`)}><b>{insertion}</b><span>{entry.meaning}</span></button></li>;
        })}</ul>
      </details>
    </form>
    <section ref={resultRef} className={styles.analysisPanel} tabIndex={-1} aria-label="Your spell" aria-live="polite">
      {!analysis ? <div className={styles.emptyResult}><span aria-hidden="true">✧</span><h2>A few words to carry with you.</h2><p>Choose a focus, then create a reflection. You can keep it in your spellbook or turn it into a sigil.</p></div> : analysis.status === 'valid' ? <>
        <p className={styles.resultKicker}>Your reflection</p><h2 className={styles.resultTitle}>{intent?.label ?? 'Your chosen intention'}</h2>
        <blockquote className={styles.reflection}>{analysis.interpretation.text.charAt(0).toUpperCase() + analysis.interpretation.text.slice(1)}</blockquote>
        {intent ? <p className={styles.reflectionPrompt}>{intent.prompt}</p> : null}
        <div className={styles.savePanel}>
          <label htmlFor="spell-label">Give it a name <span>optional</span></label><input id="spell-label" className={styles.textInput} value={label} maxLength={80} onChange={event => setLabel(event.target.value)} placeholder={intent?.label ?? 'My reflection'} />
          <button type="button" onClick={save}>Save to spellbook</button>
          {saveState !== 'idle' ? <p role="status">{saveState === 'saved' ? 'Saved in this browser.' : 'This browser could not save it. Your reflection is still here.'}</p> : null}
        </div>
        <div className={styles.buttonRow}><button type="button" onClick={() => setArtifact(createSigil(analysis, { variant: 0 }))}>Make a sigil</button><Link href="/spellbook">Open spellbook →</Link></div>
        {artifact ? <div className={styles.inlineArtwork}><img src={`data:image/svg+xml;charset=utf-8,${encodeURIComponent(artifact.svg)}`} alt="A geometric sigil made from your reflection" width={512} height={512} /><button className={styles.quietButton} type="button" onClick={downloadArt}>Download image</button></div> : null}
        <details className={styles.customize}>
          <summary>How it works</summary><p>{analysis.interpretation.disclaimer}</p><p><strong>Symbolic words:</strong> <code>{analysis.input}</code></p>
          <p>Lexical confidence: {Math.round(analysis.confidence * 100)}%. This measures recognition of vocabulary, not the likelihood of an outcome.</p>
          <h3>Word meanings</h3><ol className={styles.technicalList}>{analysis.interpretation.trace.map((item, index) => <li key={`${item.term}-${index}`}><code>{item.term}</code> — {item.gloss}</li>)}</ol>
          <h3>Symbolic graph</h3><p>{analysis.compiled.operations.length} nodes · executable: no</p><ol className={styles.technicalList}>{analysis.compiled.operations.map(node => <li key={node.id}><code>{node.operation}</code> — {node.label}</li>)}</ol>
          <dl className={styles.technicalData}><div><dt>Engine</dt><dd>{analysis.receipt.engineVersion}</dd></div><div><dt>Vocabulary</dt><dd>{analysis.receipt.vocabularyVersion}</dd></div><div><dt>Program hash</dt><dd><code>{analysis.receipt.programHash}</code></dd></div></dl>
        </details>
      </> : <div className={styles.blockedResult}>
        <h2>Let’s adjust those words.</h2><p>One or more symbolic words could not be understood. Try a starting point, or edit the words below “Edit symbolic words.”</p>
        <ul>{analysis.issues.map((issue, index) => <li key={`${issue.code}-${index}`}>{issue.message}</li>)}</ul>
        <button className={styles.quietButton} type="button" onClick={() => changeInput(INTENT_PRESETS[0].source)}>Start with Clarity</button>
        <details className={styles.customize}><summary>Technical details</summary><code>{analysis.status === 'quarantined' ? 'SYMBOLIC_UNKNOWN_QUARANTINED' : 'SYMBOLIC_COMPILE_BLOCKED'}</code></details>
      </div>}
    </section>
  </div>;
}
