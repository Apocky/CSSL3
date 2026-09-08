import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';
import { useRouter } from 'next/router';
import { useEffect, useMemo, useState } from 'react';
import { PUBLIC_GLOSSARY_SYMBOLS, PUBLIC_GLOSSARY_TERMS, type PublicGlossaryTerm } from '../lib/public-glossary';
import { useToast } from '../components/ui/Feedback';
import styles from '../components/atlas/Words.module.css';

const EXAMPLES: Readonly<Record<string, string>> = {
  intention: '“I want to make room for rest” names a direction. Choosing an earlier bedtime is one possible action toward it.',
  sigil: 'A small mark you draw for courage can remind you of what you want to practice.',
  reflection: 'After a difficult conversation, ask: What did I feel? What did I assume? What would I change?',
  metaphor: '“I am carrying a mountain” describes a heavy burden without claiming there is a literal mountain on your back.',
  allegory: 'A story about a city that sells its shadows might explore what people surrender to belong.',
  myth: 'A story of a world rising from water can carry a culture’s ideas about beginnings and order.',
  omnoid: 'In the Omnoid writing, a boundary can also be a passage. That is part of the authored model, rather than a measurement of physical space.',
  consent: '“You may share this paragraph” gives permission for that paragraph, not the rest of a private notebook.',
  local: 'A spell saved in your browser stays on that device unless you export it.',
  provenance: 'A quotation with its author, book, and page number gives you a path back to its source.',
  permission: 'Allowing a website to use your microphone does not give it permission to use your camera.',
  account: 'You can sign out of a shared computer while keeping your account for next time.',
  alpha: 'An early game build may let you play one area while other areas are still being made.',
  cssl: 'CSSL is the name of a programming language, just as Python is the name of another.',
  cslv3: 'A note such as “question → evidence → decision” records a relationship in compact form.',
  api: 'A weather app can ask a weather service for a forecast through an API.',
  diagnostics: 'An error message that says which file could not open helps someone find the problem.',
  telemetry: 'With your chosen sharing settings, a page might send a timing measurement to help find slow loading.',
  runtime: 'A music player is running while it plays a song; its saved source files are a different thing.',
  'self-hosted': 'A person can run a website on their own server instead of using a hosted publishing service.',
};
const CREATIVE_TERMS: readonly PublicGlossaryTerm[] = [
  { id: 'intention', term: 'Intention', meaning: 'A purpose or direction you choose for an action. Naming an intention can help you decide what to do next.' },
  { id: 'sigil', term: 'Sigil', meaning: 'A drawn symbol used to represent an intention, name, or idea. Here, making one is a creative way to give a chosen meaning a visible form.' },
  { id: 'reflection', term: 'Reflection', meaning: 'Taking time to examine an experience, thought, or feeling. It can help you notice patterns and choose a response.' },
  { id: 'metaphor', term: 'Metaphor', meaning: 'Describing one thing through another to reveal a quality or feeling they share.' },
  { id: 'allegory', term: 'Allegory', meaning: 'A story whose people, places, or events also carry a connected layer of meaning, such as a moral or political argument.' },
  { id: 'myth', term: 'Myth', meaning: 'A traditional story that carries meaning about origins, sacred beings, or a community’s place in the world. The word does not simply mean a lie.' },
  { id: 'omnoid', term: 'Omnoid', meaning: 'The name of Shawn Apocky’s evolving authored cosmology, explored through ideas of wholes, boundaries, passages, and recursive relationships.' },
];
const RELATED: Readonly<Record<string, { href: string; label: string }>> = {
  intention: { href: '/spellcraft', label: 'Choose an intention' }, sigil: { href: '/sigils', label: 'Make a sigil' }, reflection: { href: '/spellcraft', label: 'Try a reflection prompt' },
  metaphor: { href: '/codex-apockalypsis', label: 'Explore the stories' }, allegory: { href: '/codex-apockalypsis', label: 'Explore the stories' }, myth: { href: '/codex-apockalypsis', label: 'Read Codex Apockalypsis' },
  omnoid: { href: '/omnoid-singularity', label: 'Read the Omnoid writing' }, consent: { href: '/principles', label: 'Read the principles' }, local: { href: '/spellbook', label: 'Open your spellbook' }, provenance: { href: '/akashic-records', label: 'Explore published writing' },
};
const FIRST = ['intention', 'sigil', 'reflection', 'metaphor', 'allegory', 'myth', 'consent', 'omnoid', 'local', 'provenance', 'permission', 'account'];
const ALL_TERMS = [...CREATIVE_TERMS, ...PUBLIC_GLOSSARY_TERMS.filter(term => !CREATIVE_TERMS.some(item => item.id === term.id))];

const Words: NextPage = () => {
  const router = useRouter();
  const notify = useToast();
  const [query, setQuery] = useState('');
  const [symbolsOpen, setSymbolsOpen] = useState(false);
  const [moreOpen, setMoreOpen] = useState(false);
  useEffect(() => {
    if (!router.isReady) return;
    const initial = router.query.q;
    if (typeof initial === 'string') setQuery(initial);
    if (router.asPath.includes('#symbols') || router.asPath.includes('#symbol-')) setSymbolsOpen(true);
    const hash = router.asPath.split('#')[1];
    if (hash === 'technical-terms' || (hash && ALL_TERMS.some(term => term.id === hash && !FIRST.includes(term.id)))) setMoreOpen(true);
  }, [router.isReady, router.query.q, router.asPath]);
  useEffect(() => {
    const hash = router.asPath.split('#')[1];
    if (!router.isReady || !hash) return;
    const frame = window.requestAnimationFrame(() => { document.getElementById(hash)?.scrollIntoView({ block: 'start' }); });
    return () => window.cancelAnimationFrame(frame);
  }, [router.isReady, router.asPath, moreOpen, symbolsOpen]);
  const normalized = query.trim().toLowerCase();
  const terms = useMemo(() => ALL_TERMS.filter(term => `${term.term} ${term.meaning} ${EXAMPLES[term.id] ?? ''}`.toLowerCase().includes(normalized)).sort((a, b) => {
    const rank = (id: string): number => FIRST.includes(id) ? FIRST.indexOf(id) : FIRST.length;
    return rank(a.id) - rank(b.id) || a.term.localeCompare(b.term);
  }), [normalized]);
  const visibleTerms = normalized ? terms : terms.filter(term => FIRST.includes(term.id));
  const moreTerms = normalized ? [] : terms.filter(term => !FIRST.includes(term.id));
  const symbols = PUBLIC_GLOSSARY_SYMBOLS.filter(item => `${item.symbol} ${item.meaning}`.toLowerCase().includes(normalized));
  const copy = async (term: string, meaning: string): Promise<void> => {
    try { await navigator.clipboard.writeText(`${term}: ${meaning}`); notify('Definition copied.'); }
    catch { notify('Copying is unavailable. You can select the definition and copy it yourself.'); }
  };
  const definitions = (items: readonly PublicGlossaryTerm[]): JSX.Element => <dl className={styles.definitions}>{items.map(({ id, term, meaning }) => <div className={styles.definition} id={id} key={id}>
    <dt>{term}</dt><dd><p>{meaning}</p>{EXAMPLES[id] ? <p className={styles.example}><strong>For example</strong>{EXAMPLES[id]}</p> : null}<div className={styles.definitionActions}>{RELATED[id] ? <Link href={RELATED[id].href}>{RELATED[id].label} →</Link> : null}<button type="button" className={styles.copy} onClick={() => { void copy(term, meaning); }}>Copy definition</button></div></dd>
  </div>)}</dl>;
  return <>
    <Head><title>Words and meanings · Apocky</title><meta name="description" content="Search plain-language definitions, find practical examples, and make sense of the words used on Apocky." /><meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" /><link rel="canonical" href="https://www.apocky.com/words" /></Head>
    <main className={styles.page}>
      <header className={styles.header}><p className={styles.eyebrow}>Words</p><h1>Words and meanings.</h1><p>A clear definition. An example you can use. A little less guesswork.</p></header>
      <form className={styles.search} onSubmit={event => event.preventDefault()} role="search">
        <label htmlFor="word-search">What does it mean?</label><div><input id="word-search" type="search" value={query} onChange={event => setQuery(event.target.value)} placeholder="Try sigil, metaphor, or consent…" autoComplete="off" />{query ? <button type="button" onClick={() => setQuery('')}>Clear</button> : null}</div>
      </form>
      <div className={styles.suggestions} role="group" aria-label="Try a definition">{['Sigil', 'Metaphor', 'Consent', 'Omnoid'].map(word => <button key={word} type="button" onClick={() => setQuery(word)}>{word}</button>)}</div>
      <p className={styles.count} role="status">{normalized ? `${terms.length} matching ${terms.length === 1 ? 'definition' : 'definitions'}` : 'For making, thinking, and everyday choices'}{normalized && symbols.length ? ` · ${symbols.length} ${symbols.length === 1 ? 'symbol' : 'symbols'} below` : ''}</p>
      <section aria-label="Word definitions">
        {definitions(visibleTerms)}
        {terms.length === 0 ? <div className={styles.empty}><h2>No matching words yet.</h2><p>Try a shorter word or clear the search to browse every definition.</p><button type="button" onClick={() => setQuery('')}>See all definitions</button></div> : null}
      </section>
      {moreTerms.length ? <details id="technical-terms" className={styles.symbols} open={moreOpen} onToggle={event => setMoreOpen(event.currentTarget.open)}><summary>Project and technical terms <span>{moreTerms.length}</span></summary><p>Programming, software, and shorthand used in the project notes. These definitions are also included in search.</p>{definitions(moreTerms)}</details> : null}
      {symbols.length > 0 ? <details id="symbols" className={styles.symbols} open={symbolsOpen} onToggle={event => setSymbolsOpen(event.currentTarget.open)}><summary>Symbols used in the technical notes <span>{symbols.length}</span></summary><p>A reference for the shorthand you may meet in project notes.</p><dl>{symbols.map(({ id, symbol, meaning }) => <div id={`symbol-${id}`} key={id}><dt>{symbol}</dt><dd>{meaning}</dd></div>)}</dl></details> : null}
    </main>
  </>;
};
export default Words;
