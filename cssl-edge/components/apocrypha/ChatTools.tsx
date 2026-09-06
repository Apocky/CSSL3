import { useEffect, useId, useMemo, useRef, useState, type FormEvent } from 'react';
import { ACTIONS, buildIntent, INTENT_PRESETS, selectedIntent, THEMES } from '../spellcraft/intent-presets';
import { createOracleSeed, ORACLE_MAX_QUESTION_LENGTH } from '../../lib/oracle';
import { createChatOracleResult, createChatToolResult, findSymbolicMeanings, meaningForMessage, type ChatCreationTool, type ChatOracleResult, type ChatToolResult } from '../../lib/apocrypha/chat-tools';
import styles from './ChatTools.module.css';

type Tool = ChatCreationTool | 'meanings' | 'oracle';
interface ChatToolsProps { readonly onInsert: (text: string) => void | boolean; readonly disabled?: boolean }

export default function ChatTools({ onInsert, disabled = false }: ChatToolsProps): JSX.Element {
  const id = useId();
  const [tool, setTool] = useState<Tool>('sigil');
  const [source, setSource] = useState<string>(INTENT_PRESETS[0].source);
  const [action, setAction] = useState('create');
  const [theme, setTheme] = useState('ken');
  const [query, setQuery] = useState('');
  const [question, setQuestion] = useState('');
  const [oracle, setOracle] = useState<ChatOracleResult | null>(null);
  const [result, setResult] = useState<ChatToolResult | null>(null);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const selected = selectedIntent(source);
  const meanings = useMemo(() => findSymbolicMeanings(query), [query]);
  const visibleResult = result?.kind === tool ? result : null;
  const resultRegion = useRef<HTMLElement | null>(null);
  useEffect(() => {
    if (!result && !oracle) return;
    const region = resultRegion.current;
    if (!region || region.closest('[hidden]')) return;
    region.focus({ preventScroll: true });
    region.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  }, [result, oracle]);

  const updateSource = (next: string): void => {
    setSource(next); setResult(null); setError(''); setNotice('');
  };
  const create = (kind: ChatCreationTool, variant = 0): void => {
    const outcome = createChatToolResult(source, kind, variant);
    setNotice('');
    if (outcome.ok) { setResult(outcome.result); setError(''); }
    else { setResult(null); setError(outcome.error); }
  };
  const submit = (event: FormEvent): void => {
    event.preventDefault();
    if (tool === 'sigil' || tool === 'reflection') create(tool);
  };
  const revealOracle = (event: FormEvent): void => {
    event.preventDefault(); setNotice('');
    try {
      const outcome = createChatOracleResult(question, createOracleSeed());
      if (outcome.ok) { setOracle(outcome.result); setError(''); }
      else { setOracle(null); setError(outcome.error); }
    } catch {
      setOracle(null); setError('This browser cannot make a private reading. Try a current browser.');
    }
  };
  const insert = (text: string): void => {
    if (disabled) return;
    try {
      const accepted = onInsert(text);
      setNotice(accepted === false
        ? 'This result will not fit in your current message. Shorten the draft or copy the result.'
        : 'Added to your message. Review it before sending.');
    }
    catch { setNotice('This could not be added. Make room in your draft and try again.'); }
  };
  const copy = async (text: string): Promise<void> => {
    try { await navigator.clipboard.writeText(text); setNotice('Copied.'); }
    catch { setNotice('Copy is unavailable here. You can select the result and copy it.'); }
  };
  const download = (value: ChatToolResult | ChatOracleResult): void => {
    const image = value.kind === 'sigil' && value.svg;
    const content = image || value.message;
    let href = '';
    let anchor: HTMLAnchorElement | null = null;
    try {
      href = URL.createObjectURL(new Blob([content], { type: image ? 'image/svg+xml;charset=utf-8' : 'text/plain;charset=utf-8' }));
      anchor = document.createElement('a');
      anchor.href = href; anchor.download = image ? 'apocky-sigil.svg' : value.kind === 'oracle' ? 'apocky-oracle.txt' : 'apocky-reflection.txt';
      document.body.appendChild(anchor); anchor.click();
      setNotice(image ? 'Your image download is ready.' : 'Your text download is ready.');
    } catch { setNotice('This download could not be prepared. Try copying the meaning instead.'); }
    finally {
      anchor?.remove();
      if (href) window.setTimeout(() => URL.revokeObjectURL(href), 1_000);
    }
  };

  return <section className={styles.tools} aria-label="Make something in this conversation">
    <div className={styles.intro}><p>A symbol, a reflection, or a word to think with.</p></div>
    <div className={styles.choices} role="group" aria-label="Choose a creation tool">
      {([{ id: 'sigil', mark: '✧', label: 'Sigil' }, { id: 'reflection', mark: '✎', label: 'Reflection' }, { id: 'meanings', mark: 'Aa', label: 'Word meanings' }, { id: 'oracle', mark: '☾', label: 'Oracle' }] as const).map(item =>
        <button key={item.id} type="button" aria-pressed={tool === item.id} onClick={() => { setTool(item.id); setError(''); setNotice(''); }}>
          <span aria-hidden="true">{item.mark}</span>{item.label}
        </button>)}
    </div>
    <p className={styles.privacy}>Created in this browser.</p>

    {tool === 'meanings' ? <div className={styles.meanings}>
      <label htmlFor={id + '-meaning'}>Find a symbolic word</label>
      <input id={id + '-meaning'} type="search" value={query} maxLength={80} onChange={event => { setQuery(event.target.value); setNotice(''); }} placeholder="Try knowledge, growth, or ken" />
      <p className={styles.hint}>Explore Apocky’s symbolic vocabulary.</p>
      <ul>{meanings.map(entry => <li key={entry.id}>
        <div><strong>{entry.lexeme}</strong><p>{entry.meaning}</p></div>
        <button type="button" disabled={disabled} onClick={() => insert(meaningForMessage(entry))} aria-label={'Add meaning of ' + entry.lexeme + ' to message'}>Add to message</button>
      </li>)}</ul>
      {meanings.length === 0 ? <p role="status">No matching word. Try another meaning.</p> : null}
    </div> : tool === 'oracle' ? <>
      <form className={styles.form} onSubmit={revealOracle}>
        <label htmlFor={id + '-question'}>Ask a yes-or-no question
          <textarea id={id + '-question'} value={question} maxLength={ORACLE_MAX_QUESTION_LENGTH} rows={3}
            onChange={event => { setQuestion(event.target.value); setOracle(null); setError(''); setNotice(''); }}
            placeholder="Should I try a small creative experiment today?" />
        </label>
        <p className={styles.hint}>A chance-drawn prompt for small decisions. Notice how you feel about the answer.</p>
        <button className={styles.primary} type="submit">Draw a reflection <span aria-hidden="true">→</span></button>
      </form>
      {error ? <p className={styles.error} role="alert">{error}</p> : null}
      {oracle ? <article ref={resultRegion} tabIndex={-1} className={styles.result} aria-label="Your oracle reflection" aria-live="polite">
        <p className={styles.oracleQuestion}>{oracle.question}</p>
        <strong className={styles.oracleSignal}>{oracle.signal}</strong>
        <blockquote>{oracle.counterweight}</blockquote><p className={styles.prompt}>{oracle.nextQuestion}</p>
        <div className={styles.actions}>
          <button className={styles.primary} type="button" disabled={disabled} onClick={() => insert(oracle.message)}>Add to message</button>
          <button type="button" onClick={() => { void copy(oracle.message); }}>Copy reflection</button>
          <button type="button" onClick={() => download(oracle)}>Download text</button>
        </div>
        <p className={styles.hint}>A reflection prompt, not a prediction. Your choices remain yours.</p>
      </article> : null}
    </> : <>
      <form className={styles.form} onSubmit={submit}>
        <fieldset><legend>Choose a focus</legend><div className={styles.presets}>
          {INTENT_PRESETS.map(preset => <button type="button" key={preset.label} aria-pressed={selected?.label === preset.label} onClick={() => updateSource(preset.source)}>{preset.label}</button>)}
        </div></fieldset>
        <details className={styles.customize}><summary>Make it your own</summary>
          <div className={styles.selects}>
            <label htmlFor={id + '-action'}>I want to<select id={id + '-action'} value={action} onChange={event => setAction(event.target.value)}>{ACTIONS.map(item => <option value={item.value} key={item.value}>{item.label}</option>)}</select></label>
            <label htmlFor={id + '-theme'}>Focus on<select id={id + '-theme'} value={theme} onChange={event => setTheme(event.target.value)}>{THEMES.map(item => <option value={item.lexeme} key={item.id}>{item.meaning}</option>)}</select></label>
          </div>
          <button className={styles.quiet} type="button" onClick={() => updateSource(buildIntent(action, theme))}>Use this combination</button>
          <label htmlFor={id + '-source'}>Symbolic words<textarea id={id + '-source'} value={source} maxLength={512} rows={2} spellCheck={false} onChange={event => updateSource(event.target.value)} /></label>
          <p className={styles.hint}>Choose meanings above, or edit words you know from the vocabulary.</p>
        </details>
        <button className={styles.primary} type="submit">{tool === 'sigil' ? 'Make my sigil' : 'Make my reflection'} <span aria-hidden="true">→</span></button>
      </form>
      {error ? <p className={styles.error} role="alert">{error}</p> : null}
      {visibleResult ? <article ref={resultRegion} tabIndex={-1} className={styles.result} aria-label={tool === 'sigil' ? 'Your sigil' : 'Your reflection'} aria-live="polite">
        <div className={styles.resultHeading}><h4>{visibleResult.title}</h4>{visibleResult.svg ? <span>Shape {(visibleResult.variant ?? 0) + 1}</span> : null}</div>
        {visibleResult.svg ? <img className={styles.art} src={'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(visibleResult.svg)} alt={'Geometric sigil for ' + visibleResult.title.toLowerCase()} width={512} height={512} /> : null}
        <blockquote>{visibleResult.meaning}</blockquote><p className={styles.prompt}>{visibleResult.prompt}</p>
        <div className={styles.actions}>
          <button className={styles.primary} type="button" disabled={disabled} onClick={() => insert(visibleResult.message)}>Add to message</button>
          <button type="button" onClick={() => { void copy(visibleResult.message); }}>Copy meaning</button>
          <button type="button" onClick={() => download(visibleResult)}>{visibleResult.svg ? 'Download image' : 'Download text'}</button>
          {visibleResult.svg ? <button type="button" onClick={() => create('sigil', ((visibleResult.variant ?? 0) + 1) % 256)}>Try another shape</button> : null}
        </div>
        <p className={styles.hint}>{visibleResult.svg ? 'The image stays here. Add its meaning to your message, or download the image to keep it.' : 'A prompt for reflection. Add it to your message when you want to explore it together.'}</p>
      </article> : <p className={styles.hint}>{tool === 'sigil' ? 'Choose a meaning, then make an image you can keep.' : 'Choose a meaning, then make a reflection to carry with you.'}</p>}
    </>}
    <p className={styles.notice} role="status" aria-live="polite">{notice}</p>
  </section>;
}
