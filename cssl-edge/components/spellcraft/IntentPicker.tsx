import { useState } from 'react';
import { DEFAULT_SPELL_LIMITS } from '../../lib/spellcraft';
import { INTENT_PRESETS, THEMES, ACTIONS, selectedIntent, buildIntent } from './intent-presets';
import styles from '../../styles/SymbolicStudio.module.css';

export default function IntentPicker({ input, onChange, id }: { input: string; onChange: (source: string) => void; id: string }): JSX.Element {
  const [action, setAction] = useState('create');
  const [theme, setTheme] = useState('ken');
  const selected = selectedIntent(input);
  return <>
    <fieldset className={styles.intentFieldset}>
      <legend>What would you like to focus on?</legend>
      <div className={styles.intentPresets}>{INTENT_PRESETS.map(preset => <button type="button" key={preset.label} aria-pressed={selected?.label === preset.label} onClick={() => onChange(preset.source)}><strong>{preset.label}</strong><span>{preset.meaning}</span></button>)}</div>
    </fieldset>
    <details className={styles.customize}>
      <summary>Choose your own combination</summary><p>Choose two meanings from Apocky’s symbolic vocabulary, then use the combination.</p>
      <div className={styles.meaningChoices}>
        <label htmlFor={`${id}-action`}>I want to<select id={`${id}-action`} className={styles.select} value={action} onChange={event => setAction(event.target.value)}>{ACTIONS.map(item => <option key={item.value} value={item.value}>{item.label}</option>)}</select></label>
        <label htmlFor={`${id}-meaning`}>Focus on<select id={`${id}-meaning`} className={styles.select} value={theme} onChange={event => setTheme(event.target.value)}>{THEMES.map(item => <option key={item.id} value={item.lexeme}>{item.meaning}</option>)}</select></label>
      </div>
      <button type="button" onClick={() => onChange(buildIntent(action, theme))}>Use this combination</button>
    </details>
    <details className={styles.customize}>
      <summary>Edit symbolic words</summary><p>For people who know the vocabulary. This editor reads symbolic words, rather than translating everyday sentences.</p>
      <label className={styles.sourceLabel} htmlFor={`${id}-source`}>Symbolic words</label>
      <textarea id={`${id}-source`} className={styles.textarea} value={input} maxLength={DEFAULT_SPELL_LIMITS.maxInputChars} onChange={event => onChange(event.target.value)} rows={3} spellCheck={false} />
      <small>{input.length} / {DEFAULT_SPELL_LIMITS.maxInputChars} characters</small>
    </details>
  </>;
}
