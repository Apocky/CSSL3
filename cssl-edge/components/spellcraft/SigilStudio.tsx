import Link from 'next/link';
import { FormEvent, useMemo, useState } from 'react';

import { addLocalSpellbookEntry } from '../../lib/spellbook-local';
import {
  analyzeSpell,
  createSigil,
  createSpellbookEntry,
  DEFAULT_SPELL_LIMITS,
  type SigilArtifact,
  type SpellAnalysis,
} from '../../lib/spellcraft';
import styles from '../../styles/SymbolicStudio.module.css';

export default function SigilStudio(): JSX.Element {
  const [input, setInput] = useState('ka-sol-el');
  const [variant, setVariant] = useState(0);
  const [analysis, setAnalysis] = useState<SpellAnalysis | null>(null);
  const [artifact, setArtifact] = useState<SigilArtifact | null>(null);
  const [saveState, setSaveState] = useState<'idle' | 'saved' | 'failed'>('idle');
  const imageSource = useMemo(() => artifact ? `data:image/svg+xml;charset=utf-8,${encodeURIComponent(artifact.svg)}` : '', [artifact]);

  const render = (event?: FormEvent, nextVariant = variant): void => {
    event?.preventDefault();
    const next = analyzeSpell(input);
    setAnalysis(next);
    setSaveState('idle');
    setArtifact(next.status === 'valid' ? createSigil(next, { variant: nextVariant }) : null);
  };

  const download = (): void => {
    if (!artifact) return;
    const href = URL.createObjectURL(new Blob([artifact.svg], { type: 'image/svg+xml;charset=utf-8' }));
    const anchor = document.createElement('a');
    anchor.href = href;
    anchor.download = `apocky-sigil-${artifact.seedHash.slice(0, 12)}.svg`;
    anchor.click();
    URL.revokeObjectURL(href);
  };

  const saveSource = (): void => {
    if (!analysis || analysis.status !== 'valid') return;
    const entry = createSpellbookEntry(analysis, { label: `Sigil ${artifact?.seedHash.slice(0, 8) ?? ''}`, savedAt: new Date().toISOString() });
    const result = addLocalSpellbookEntry(window.localStorage, entry);
    setSaveState(result.status === 'ready' ? 'saved' : 'failed');
  };

  return (
    <div className={styles.sigilLayout}>
      <form className={styles.workbench} onSubmit={render}>
        <div className={styles.panelHeader}><span className={styles.statusDot} aria-hidden="true" /><span>VISIBLE GEOMETRY · NO HIDDEN PAYLOAD</span></div>
        <label className={styles.label} htmlFor="sigil-source">Build from a validated symbolic form</label>
        <textarea id="sigil-source" className={styles.textarea} value={input} maxLength={DEFAULT_SPELL_LIMITS.maxInputChars} onChange={(event) => { setInput(event.target.value); setAnalysis(null); setArtifact(null); }} rows={4} spellCheck={false} />
        <label className={styles.rangeLabel} htmlFor="sigil-variant"><span>Visible variant</span><b>{variant}</b></label>
        <input id="sigil-variant" className={styles.range} type="range" min={0} max={255} value={variant} onChange={(event) => { const value = Number(event.target.value); setVariant(value); if (analysis?.status === 'valid') setArtifact(createSigil(analysis, { variant: value })); }} />
        <button className={styles.primaryButton} type="submit">Validate and render</button>
        {analysis && analysis.status !== 'valid' ? <div className={styles.warning} role="alert">The source is {analysis.status}. Resolve every compiler issue before geometry is created. <code>SIGIL_RENDER_BLOCKED</code></div> : null}
      </form>

      <section className={styles.sigilCanvas} aria-live="polite">
        {artifact ? (
          <>
            <img src={imageSource} alt="Generated geometric sigil: cyan perimeter, indigo rings, violet paths, and visible nodes." width={512} height={512} />
            <div className={styles.sigilMeta}>
              <p className={styles.resultKicker}>DETERMINISTIC ARTIFACT</p>
              <h2>Variant {artifact.variant}</h2>
              <code>{artifact.seedHash}</code>
              <ul><li>{artifact.semantics.path}</li><li>{artifact.semantics.rings}</li><li>{artifact.semantics.nodes}</li></ul>
              <p>{artifact.semantics.disclosure}</p>
            </div>
            <div className={styles.buttonRow}>
              <button type="button" onClick={download}>Download SVG</button>
              <button type="button" onClick={saveSource}>{saveState === 'saved' ? 'Saved locally' : saveState === 'failed' ? 'Save unavailable' : 'Save source to Spellbook'}</button>
              <Link href="/spellbook">Open Spellbook →</Link>
            </div>
          </>
        ) : <div className={styles.emptyResult}><span aria-hidden="true">✦</span><h2>Structure before ornament.</h2><p>The studio draws only after the language engine produces a valid, non-executable symbolic program. The exact same program and variant reproduce the same SVG.</p></div>}
      </section>
    </div>
  );
}
