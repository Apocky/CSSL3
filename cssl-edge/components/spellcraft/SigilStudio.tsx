import Link from 'next/link';
import { type FormEvent, useMemo, useRef, useState } from 'react';
import { addLocalSpellbookEntry } from '../../lib/spellbook-local';
import { analyzeSpell, createSigil, createSpellbookEntry, type SigilArtifact, type SpellAnalysis } from '../../lib/spellcraft';
import IntentPicker from './IntentPicker';
import { INTENT_PRESETS, selectedIntent } from './intent-presets';
import styles from '../../styles/SymbolicStudio.module.css';

export default function SigilStudio(): JSX.Element {
  const [input, setInput] = useState<string>(INTENT_PRESETS[0].source);
  const [variant, setVariant] = useState(0);
  const [analysis, setAnalysis] = useState<SpellAnalysis | null>(null);
  const [artifact, setArtifact] = useState<SigilArtifact | null>(null);
  const [saveState, setSaveState] = useState<'idle' | 'saved' | 'failed'>('idle');
  const resultRef = useRef<HTMLElement>(null);
  const intent = selectedIntent(input);
  const imageSource = useMemo(() => artifact ? `data:image/svg+xml;charset=utf-8,${encodeURIComponent(artifact.svg)}` : '', [artifact]);
  const changeInput = (source: string): void => { setInput(source); setAnalysis(null); setArtifact(null); setSaveState('idle'); };
  const render = (event?: FormEvent): void => {
    event?.preventDefault(); const next = analyzeSpell(input); setAnalysis(next); setSaveState('idle');
    setArtifact(next.status === 'valid' ? createSigil(next, { variant }) : null);
    window.requestAnimationFrame(() => { resultRef.current?.focus(); resultRef.current?.scrollIntoView({ block: 'start' }); });
  };
  const download = (): void => {
    if (!artifact) return;
    const href = URL.createObjectURL(new Blob([artifact.svg], { type: 'image/svg+xml;charset=utf-8' }));
    const anchor = document.createElement('a'); anchor.href = href; anchor.download = `apocky-sigil-${artifact.seedHash.slice(0, 12)}.svg`; anchor.click(); URL.revokeObjectURL(href);
  };
  const saveSource = (): void => {
    if (!analysis || analysis.status !== 'valid') return;
    const entry = createSpellbookEntry(analysis, { label: intent?.label ?? `Sigil ${artifact?.seedHash.slice(0, 8) ?? ''}`, savedAt: new Date().toISOString() });
    try { setSaveState(addLocalSpellbookEntry(window.localStorage, entry).status === 'ready' ? 'saved' : 'failed'); } catch { setSaveState('failed'); }
  };
  return <div className={styles.sigilLayout}>
    <form className={styles.workbench} onSubmit={render}>
      <IntentPicker input={input} onChange={changeInput} id="sigil" />
      <button className={styles.primaryButton} type="submit">Make my sigil <span aria-hidden="true">→</span></button>
      <p className={styles.smallNote}>An original mark for your chosen meaning. Created in this browser.</p>
    </form>
    <section ref={resultRef} className={styles.sigilCanvas} tabIndex={-1} aria-label="Your sigil" aria-live="polite">
      {artifact ? <>
        <div className={styles.artworkHeading}><div><p className={styles.resultKicker}>Your sigil</p><h2>{intent?.label ?? 'Your chosen intention'}</h2></div><span>Variation {artifact.variant + 1}</span></div>
        <img src={imageSource} alt="Generated geometric sigil: cyan perimeter, indigo rings, violet paths, and visible nodes." width={512} height={512} />
        <div className={styles.buttonRow}><button type="button" onClick={download}>Download image</button><button type="button" onClick={saveSource}>Save to spellbook</button></div>
        {saveState !== 'idle' ? <p className={styles.smallNote} role="status">{saveState === 'saved' ? 'The words behind your sigil are saved in this browser. Download the image to keep this variation.' : 'This browser could not save it. You can still download the image.'}</p> : null}
        <label className={styles.rangeLabel} htmlFor="sigil-variant"><span>Try another shape</span><b>{variant + 1} / 256</b></label>
        <input id="sigil-variant" className={styles.range} type="range" min={0} max={255} value={variant} onChange={event => { const value = Number(event.target.value); setVariant(value); if (analysis?.status === 'valid') setArtifact(createSigil(analysis, { variant: value })); }} />
        {intent ? <p className={styles.reflectionPrompt}>{intent.prompt}</p> : null}
        <details className={styles.customize}>
          <summary>How it works</summary><p>This is symbolic art for reflection. The same words and variation produce the same image. Downloaded images are SVG files that stay sharp at different sizes.</p>
          <p><strong>Symbolic words:</strong> <code>{input}</code></p><p>{analysis?.status === 'valid' ? analysis.interpretation.text : ''}</p>
          <ul><li>{artifact.semantics.path}</li><li>{artifact.semantics.rings}</li><li>{artifact.semantics.nodes}</li></ul><p>{artifact.semantics.disclosure}</p><code className={styles.longCode}>{artifact.seedHash}</code>
          <p>Saving keeps the words in your spellbook. Download the image to preserve this particular variation.</p>
        </details>
        <Link className={styles.textLink} href="/spellbook">Open your spellbook →</Link>
      </> : analysis && analysis.status !== 'valid' ? <div className={styles.blockedResult}>
        <h2>Let’s adjust those words.</h2><p>Choose a starting point or fix the symbolic words before making a sigil.</p><ul>{analysis.issues.map((issue, index) => <li key={`${issue.code}-${index}`}>{issue.message}</li>)}</ul>
        <button className={styles.quietButton} type="button" onClick={() => changeInput(INTENT_PRESETS[0].source)}>Start with Clarity</button>
        <details className={styles.customize}><summary>Technical details</summary><code>SIGIL_RENDER_BLOCKED</code></details>
      </div> : <div className={styles.emptyResult}><span aria-hidden="true">✦</span><h2>A meaning, made visible.</h2><p>Choose what matters to you. Make a sigil, try different shapes, and download the one you want to keep.</p></div>}
    </section>
  </div>;
}
