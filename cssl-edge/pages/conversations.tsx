import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';
import { useEffect, useMemo, useState } from 'react';

import {
  CONVERSATION_ARCHIVE_FACTS,
  CONVERSATION_CONSTELLATIONS,
  type ConversationConstellation,
  type ConversationProvider,
  type ConversationTheme,
} from '@/lib/conversation-constellations';
import {
  validatePublicConversationBrowseManifest,
  type ConversationCorpusBrowseManifest,
  type ConversationCorpusBrowseRecord,
  type CorpusProvider,
} from '@/lib/conversation-corpus';
import { SUPPORT_LINKS } from '@/lib/support-links';
import styles from '@/styles/Conversations.module.css';

type Lens = 'plain' | 'dialogue' | 'allegory' | 'critical';
type ProviderFilter = 'All' | ConversationProvider;
type ThemeFilter = 'All' | ConversationTheme;

const LENSES: readonly { readonly id: Lens; readonly label: string; readonly help: string }[] = [
  { id: 'plain', label: 'Plain language', help: 'What the exchange explored, without specialist language.' },
  { id: 'dialogue', label: 'Human + AI', help: 'Separate paraphrases of each side of the exchange.' },
  { id: 'allegory', label: 'Allegory', help: 'The same relationship carried by a compact image or story.' },
  { id: 'critical', label: 'Critical lens', help: 'Limits, counterweights, omissions, and evidence boundaries.' },
];

const PROVIDERS: readonly ProviderFilter[] = ['All', 'ChatGPT', 'Claude', 'Codex'];
const THEMES: readonly ThemeFilter[] = [
  'All',
  'Spiritual life',
  'Myth and meaning',
  'Consciousness',
  'Divination',
  'Creative practice',
  'Sovereignty',
  'Ordinary magic',
  'Building Apocky',
];

const TOPIC_SIGNALS = [
  ['Spirituality & Mysticism', CONVERSATION_ARCHIVE_FACTS.chatGptSpiritualityMysticism],
  ['Philosophy & Psychology', CONVERSATION_ARCHIVE_FACTS.chatGptPhilosophyPsychology],
  ['Relationships & Personal', CONVERSATION_ARCHIVE_FACTS.chatGptRelationshipsPersonal],
  ['Creative Writing & Worldbuilding', CONVERSATION_ARCHIVE_FACTS.chatGptCreativeWritingWorldbuilding],
  ['Mythology & Folklore', CONVERSATION_ARCHIVE_FACTS.chatGptMythologyFolklore],
  ['Religion & Sacred Texts', CONVERSATION_ARCHIVE_FACTS.chatGptReligionSacredTexts],
  ['Numerology & Astrology', CONVERSATION_ARCHIVE_FACTS.chatGptNumerologyAstrology],
] as const;

const QUICK_PATHS: readonly {
  readonly label: string;
  readonly theme: ThemeFilter;
  readonly lens: Lens;
  readonly description: string;
}[] = [
  { label: 'Something spiritual', theme: 'Spiritual life', lens: 'plain', description: 'Lived meaning with its reality boundary intact.' },
  { label: 'A useful metaphor', theme: 'Myth and meaning', lens: 'allegory', description: 'Enter through story instead of jargon.' },
  { label: 'Human–AI disagreement', theme: 'Sovereignty', lens: 'critical', description: 'See corrections, limits, and who supplied the insight.' },
  { label: 'How the system connects', theme: 'Building Apocky', lens: 'dialogue', description: 'Move from an idea to a designed mechanism.' },
];

function formatDate(value: string): string {
  const date = new Date(`${value}T00:00:00Z`);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat('en-US', { month: 'short', day: 'numeric', year: 'numeric', timeZone: 'UTC' }).format(date);
}

function lensContent(record: ConversationConstellation, lens: Lens): JSX.Element {
  if (lens === 'dialogue') {
    return (
      <div className={styles.dialogue}>
        <div className={styles.turn} data-role="human">
          <span>Shawn · faithful paraphrase</span>
          <p>{record.humanParaphrase}</p>
        </div>
        <div className={styles.turn} data-role="ai">
          <span>{record.provider} · faithful paraphrase</span>
          <p>{record.aiParaphrase}</p>
        </div>
      </div>
    );
  }
  if (lens === 'allegory') return <blockquote className={styles.allegory}>{record.allegory}</blockquote>;
  if (lens === 'critical') {
    return (
      <div className={styles.critical}>
        <p><strong>Boundary.</strong> {record.criticalNote}</p>
        <p><strong>Not carried into public.</strong> {record.omissions}</p>
      </div>
    );
  }
  return <p className={styles.plain}>{record.plain}</p>;
}

const Conversations: NextPage = () => {
  const [lens, setLens] = useState<Lens>('plain');
  const [provider, setProvider] = useState<ProviderFilter>('All');
  const [theme, setTheme] = useState<ThemeFilter>('All');
  const [query, setQuery] = useState('');
  const [corpus, setCorpus] = useState<ConversationCorpusBrowseManifest | null>(null);
  const [corpusError, setCorpusError] = useState('');
  const [corpusQuery, setCorpusQuery] = useState('');
  const [corpusProvider, setCorpusProvider] = useState<'All' | CorpusProvider>('All');
  const [corpusCategory, setCorpusCategory] = useState('All');
  const [corpusTheme, setCorpusTheme] = useState('All');
  const [corpusVisible, setCorpusVisible] = useState(30);
  const koFi = SUPPORT_LINKS.find((link) => link.name === 'Ko-fi');

  useEffect(() => {
    const controller = new AbortController();
    fetch('/conversation-corpus/browse.v1.json', { signal: controller.signal })
      .then((response) => {
        if (!response.ok) throw new Error(`Corpus index returned ${response.status}`);
        return response.json() as Promise<ConversationCorpusBrowseManifest>;
      })
      .then((value) => {
        validatePublicConversationBrowseManifest(value);
        setCorpus(value);
      })
      .catch((error: unknown) => {
        if (error instanceof DOMException && error.name === 'AbortError') return;
        setCorpusError(error instanceof Error ? error.message : 'Unable to load the complete corpus index');
      });
    return () => controller.abort();
  }, []);

  const filtered = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    return CONVERSATION_CONSTELLATIONS.filter((record) => {
      const searchable = [
        record.title,
        record.provider,
        record.plain,
        record.humanParaphrase,
        record.aiParaphrase,
        record.allegory,
        record.criticalNote,
        ...record.themes,
      ].join('\n').toLocaleLowerCase();
      return (
        (provider === 'All' || record.provider === provider)
        && (theme === 'All' || record.themes.includes(theme))
        && (needle.length === 0 || searchable.includes(needle))
      );
    });
  }, [provider, query, theme]);

  const corpusCategories = useMemo(() => ['All', ...new Set(corpus?.records.map((record) => record.category) ?? [])].sort(), [corpus]);
  const corpusThemes = useMemo(() => ['All', ...new Set(corpus?.records.flatMap((record) => record.themes) ?? [])].sort(), [corpus]);
  const corpusFiltered = useMemo(() => {
    const needle = corpusQuery.trim().toLocaleLowerCase();
    return (corpus?.records ?? []).filter((record) => {
      const searchable = [record.title, record.provider, record.category, record.excerpt, ...record.themes]
        .join('\n').toLocaleLowerCase();
      return (corpusProvider === 'All' || record.provider === corpusProvider)
        && (corpusCategory === 'All' || record.category === corpusCategory)
        && (corpusTheme === 'All' || record.themes.includes(corpusTheme))
        && (needle.length === 0 || searchable.includes(needle));
    });
  }, [corpus, corpusCategory, corpusProvider, corpusQuery, corpusTheme]);

  useEffect(() => setCorpusVisible(30), [corpusCategory, corpusProvider, corpusQuery, corpusTheme]);

  const selectPath = (path: (typeof QUICK_PATHS)[number]): void => {
    setQuery('');
    setProvider('All');
    setTheme(path.theme);
    setLens(path.lens);
    const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    document.getElementById('conversation-explorer')?.scrollIntoView({ behavior: reduceMotion ? 'auto' : 'smooth', block: 'start' });
  };

  const clear = (): void => {
    setQuery('');
    setProvider('All');
    setTheme('All');
  };

  const jsonLd = {
    '@context': 'https://schema.org',
    '@type': 'CollectionPage',
    name: 'Conversation Constellations',
    description: 'A privacy-safe map of Shawn Apocky conversations: public editorial constellations, aggregate export facts, and full bodies only after explicit privacy and rights review.',
    url: 'https://www.apocky.com/conversations',
    isPartOf: { '@type': 'WebSite', name: 'Apocky', url: 'https://www.apocky.com' },
    mainEntity: {
      '@type': 'ItemList',
      numberOfItems: CONVERSATION_CONSTELLATIONS.length,
      itemListElement: CONVERSATION_CONSTELLATIONS.map((record, index) => ({
        '@type': 'ListItem',
        position: index + 1,
        name: record.title,
        url: `https://www.apocky.com/conversations#${record.id}`,
      })),
    },
  };

  return (
    <>
      <Head>
        <title>Conversation Constellations · Shawn Apocky with ChatGPT, Claude, and Codex</title>
        <meta name="description" content="Explore short readings on soul, myth, creativity, and sovereignty. Search ideas, compare voices, and follow their sources." />
        <meta name="keywords" content="Shawn Apocky conversations, AI conversations, ChatGPT spirituality, Claude philosophy, consciousness, mythology, divination, allegory, human AI dialogue" />
        <meta name="robots" content="index,follow,max-image-preview:large" />
        <meta name="theme-color" content="#000000" />
        <meta property="og:type" content="website" />
        <meta property="og:site_name" content="Apocky" />
        <meta property="og:title" content="Conversation Constellations" />
        <meta property="og:description" content="The personal, spiritual, playful, and difficult ideas—not just the implementation logs." />
        <meta property="og:url" content="https://www.apocky.com/conversations" />
        <meta name="twitter:card" content="summary_large_image" />
        <link rel="canonical" href="https://www.apocky.com/conversations" />
        <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(jsonLd) }} />
      </Head>

      <main className={styles.page}>
        <div className={styles.wrap}>
          <header className={styles.hero}>
            <div>
              <p className={styles.eyebrow}>Conversations · Shawn Apocky</p>
              <h1>Ideas worth <em>thinking with.</em></h1>
            </div>
            <div className={styles.heroCopy}>
              <p>
                Short readings drawn from conversations about soul, myth, creativity, and the difficult art of being yourself.
              </p>
              <div className={styles.heroActions}>
                <Link className={styles.readingLink} href="/codex-apockalypsis">Codex Apockalypsis →</Link>
                <Link className={styles.readingLink} href="/akashic-records">Essays →</Link>
              </div>
            </div>
          </header>

          <section className={styles.explorer} id="conversation-explorer" aria-label="Find an idea">
            <form className={styles.controls} onSubmit={(event) => event.preventDefault()}>
              <label className={styles.searchField}>
                <span>Search ideas</span>
                <input
                  type="search"
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder="Try soul, myth, ordinary, reality…"
                  autoComplete="off"
                  aria-controls="conversation-results"
                />
              </label>
              <label>
                <span>Theme</span>
                <select value={theme} onChange={(event) => setTheme(event.target.value as ThemeFilter)}>
                  {THEMES.map((value) => <option key={value}>{value}</option>)}
                </select>
              </label>
              <button type="button" onClick={clear} disabled={query.length === 0 && provider === 'All' && theme === 'All'}>Clear</button>
            </form>

            <details className={styles.readingOptions}>
              <summary>Voices &amp; reading styles</summary>
              <label>
                <span>Provider</span>
                <select value={provider} onChange={(event) => setProvider(event.target.value as ProviderFilter)}>
                  {PROVIDERS.map((value) => <option key={value}>{value}</option>)}
                </select>
              </label>
            <fieldset className={styles.lensSwitch}>
              <legend>Reading lens</legend>
              <div>
                {LENSES.map((option) => (
                  <button
                    key={option.id}
                    type="button"
                    aria-pressed={lens === option.id}
                    onClick={() => setLens(option.id)}
                    title={option.help}
                  >
                    {option.label}
                  </button>
                ))}
              </div>
              <p>{LENSES.find((option) => option.id === lens)?.help}</p>
            </fieldset>
            </details>

            <p className={styles.resultCount} role="status" aria-live="polite">
              {filtered.length} of {CONVERSATION_CONSTELLATIONS.length} ideas
            </p>

            {filtered.length > 0 ? (
              <div className={styles.results} id="conversation-results">
                {filtered.map((record) => (
                  <article className={styles.conversationCard} id={record.id} key={record.id}>
                    <div className={styles.cardMeta}>
                      <span data-provider={record.provider.toLowerCase()}>{record.provider}</span>
                      <time dateTime={record.recordedAt}>{formatDate(record.recordedAt)}</time>
                      <span>{record.tone}</span>
                    </div>
                    <h3>{record.title}</h3>
                    <div className={styles.lensContent} data-lens={lens}>
                      {lensContent(record, lens)}
                    </div>
                    <ul className={styles.themeList} aria-label="Themes">
                      {record.themes.map((value) => <li key={value}>{value}</li>)}
                    </ul>
                    <details className={styles.provenance}>
                      <summary>Sources and attribution</summary>
                      <dl>
                        <div><dt>Provider</dt><dd>{record.provider}</dd></div>
                        <div><dt>Source reference</dt><dd><code>{record.sourceReference}</code></dd></div>
                        <div><dt>Archive fingerprint</dt><dd><code>{record.sourceFingerprint}</code></dd></div>
                        <div><dt>Public form</dt><dd>Faithful editorial paraphrase · no raw private transcript</dd></div>
                      </dl>
                      {record.sourceHref ? <Link href={record.sourceHref}>Read the approved public transcript →</Link> : null}
                    </details>
                  </article>
                ))}
              </div>
            ) : (
              <div className={styles.emptyState} id="conversation-results">
                <h3>No ideas match yet.</h3>
                <p>Clear the filters or try a broader theme.</p>
                <button type="button" onClick={clear}>Show every idea</button>
              </div>
            )}
          </section>

          <section className={styles.quickSection} aria-labelledby="quick-title">
            <div className={styles.sectionHeading}>
              <div>
                <p className={styles.eyebrow}>Quick paths</p>
                <h2 id="quick-title">What do you want from the archive?</h2>
              </div>
              <p>Choose a human question. The filters and reading lens will move with you.</p>
            </div>
            <div className={styles.quickGrid}>
              {QUICK_PATHS.map((path) => (
                <button key={path.label} type="button" onClick={() => selectPath(path)}>
                  <strong>{path.label}</strong>
                  <span>{path.description}</span>
                  <span aria-hidden="true">Open path →</span>
                </button>
              ))}
            </div>
          </section>

          <details className={styles.archiveDetails}>
            <summary>About the conversations and their sources</summary>
            <p>The complete export is indexed locally; full bodies stay private until each record passes explicit privacy and publication-rights review.</p>
          <section className={styles.truthPanel} aria-labelledby="truth-title">
            <div>
              <p className={styles.eyebrow}>What changed</p>
              <h2 id="truth-title">The whole corpus is counted. Publication remains chosen.</h2>
            </div>
            <p>
              The earlier Akashic snapshot published {CONVERSATION_ARCHIVE_FACTS.publicMediumWorks} authored works and only{' '}
              {CONVERSATION_ARCHIVE_FACTS.publicCodexConversations} selected Codex conversations across{' '}
              {CONVERSATION_ARCHIVE_FACTS.publicCodexTranscriptChunks} transcript records. The local index now counts all{' '}
              {CONVERSATION_ARCHIVE_FACTS.localChatGptConversations.toLocaleString('en-US')} ChatGPT and{' '}
              {CONVERSATION_ARCHIVE_FACTS.localClaudeConversations} Claude conversation records without converting
              automated redaction into consent. Selection decides what is featured; explicit review decides what may be public.
            </p>
          </section>

          <section className={styles.archiveMap} aria-labelledby="map-title">
            <div className={styles.sectionHeading}>
              <div>
                <p className={styles.eyebrow}>Archive map · local denominator</p>
                <h2 id="map-title">The whole export has shape.</h2>
              </div>
              <p>Every record remains represented in the local denominator. Public reading begins only after privacy, rights, and editorial review.</p>
            </div>
            <div className={styles.mapGrid}>
              <article className={styles.metricCard}>
                <span>ChatGPT export</span>
                <strong>{CONVERSATION_ARCHIVE_FACTS.localChatGptConversations.toLocaleString('en-US')}</strong>
                <p>conversations · categorized for discovery</p>
              </article>
              <article className={styles.metricCard}>
                <span>Claude export</span>
                <strong>{CONVERSATION_ARCHIVE_FACTS.localClaudeConversations}</strong>
                <p>conversations · {CONVERSATION_ARCHIVE_FACTS.localAnthropicDuplicateDelivery} also present in a duplicate Anthropic delivery</p>
              </article>
              <article className={styles.metricCard}>
                <span>Public Codex selection</span>
                <strong>{CONVERSATION_ARCHIVE_FACTS.publicCodexConversations}</strong>
                <p>approved conversations · narrow technical bias now disclosed</p>
              </article>
              <article className={`${styles.metricCard} ${styles.metricCardAccent}`}>
                <span>Publication gate</span>
                <strong>0 / 1,386</strong>
                <p>full bodies public · all current records held for review</p>
              </article>
            </div>
            <div className={styles.signalChart} aria-label="ChatGPT export category counts">
              {TOPIC_SIGNALS.map(([label, count]) => (
                <div className={styles.signalRow} key={label}>
                  <span>{label}</span>
                  <div className={styles.signalTrack} aria-hidden="true">
                    <i style={{ width: `${(count / CONVERSATION_ARCHIVE_FACTS.chatGptSpiritualityMysticism) * 100}%` }} />
                  </div>
                  <strong>{count}</strong>
                </div>
              ))}
            </div>
            <details className={styles.methodNote}>
              <summary>What “the whole corpus” includes—and what it still excludes</summary>
              <p>
                All 1,386 export records and 19,479 visible human/assistant turns are represented by aggregate counts in
                the local review queue. No unreviewed title, excerpt, signal, or body is copied into this public reading
                room. Hidden reasoning, tool payloads, private attachments, credentials, third-party privacy, and
                publication rights remain separate review boundaries—not problems a regex can certify away.
              </p>
            </details>
          </section>

          <section className={styles.corpusSection} id="full-corpus" aria-labelledby="corpus-title">
            <div className={styles.sectionHeading}>
              <div>
                <p className={styles.eyebrow}>Approved conversation library</p>
                <h2 id="corpus-title">Review before reach.</h2>
              </div>
              <p>The public index stays light and fail-closed. Only records with explicit approval can appear here, load through the API, or enter search engines.</p>
            </div>

            {corpus ? (
              <div className={styles.corpusFacts} aria-label="Corpus totals">
                <span><strong>{corpus.counts.uniqueConversations.toLocaleString('en-US')}</strong> locally indexed</span>
                <span><strong>{corpus.counts.messages.toLocaleString('en-US')}</strong> local visible turns</span>
                <span><strong>{corpus.counts.publiclyApprovedConversations.toLocaleString('en-US')}</strong> public bodies</span>
                <span><strong>{corpus.counts.reviewHeldConversations.toLocaleString('en-US')}</strong> held for review</span>
              </div>
            ) : null}

            {corpus && corpus.records.length === 0 ? (
              <div className={styles.reviewHold} role="status">
                <p className={styles.eyebrow}>Privacy + rights boundary</p>
                <h3>The body library is deliberately closed today.</h3>
                <p>Automated screening found useful candidates, but it cannot grant consent or publication rights. Records will appear here only after human review; the twelve ideas above are available now.</p>
                <a href="#conversation-explorer">Return to the ideas ↑</a>
              </div>
            ) : <form className={styles.corpusControls} onSubmit={(event) => event.preventDefault()}>
              <label className={styles.searchField}><span>Search title, signal, or idea</span><input type="search" value={corpusQuery} onChange={(event) => setCorpusQuery(event.target.value)} placeholder="Try consciousness, ritual, code, relationship…" autoComplete="off" /></label>
              <label><span>Provider</span><select value={corpusProvider} onChange={(event) => setCorpusProvider(event.target.value as 'All' | CorpusProvider)}><option>All</option><option>ChatGPT</option><option>Claude</option></select></label>
              <label><span>Category</span><select value={corpusCategory} onChange={(event) => setCorpusCategory(event.target.value)}>{corpusCategories.map((value) => <option key={value}>{value}</option>)}</select></label>
              <label><span>Theme</span><select value={corpusTheme} onChange={(event) => setCorpusTheme(event.target.value)}>{corpusThemes.map((value) => <option key={value}>{value}</option>)}</select></label>
              <button type="button" disabled={corpusQuery.length === 0 && corpusProvider === 'All' && corpusCategory === 'All' && corpusTheme === 'All'} onClick={() => { setCorpusQuery(''); setCorpusProvider('All'); setCorpusCategory('All'); setCorpusTheme('All'); }}>Clear</button>
            </form>}

            {corpusError ? <div className={styles.corpusError} role="alert"><strong>The complete index did not load.</strong><span>{corpusError}</span></div> : null}
            {!corpus && !corpusError ? <div className={styles.corpusLoading} role="status">Connecting the archive…</div> : null}
            {corpus && corpus.records.length > 0 ? (
              <>
                <p className={styles.resultCount} role="status" aria-live="polite">{corpusFiltered.length.toLocaleString('en-US')} matching {corpusFiltered.length === 1 ? 'record' : 'records'} · showing {Math.min(corpusVisible, corpusFiltered.length).toLocaleString('en-US')}</p>
                <div className={styles.corpusGrid}>
                  {corpusFiltered.slice(0, corpusVisible).map((record: ConversationCorpusBrowseRecord) => (
                    <article className={styles.corpusCard} key={record.id}>
                      <div className={styles.cardMeta}><span data-provider={record.provider.toLowerCase()}>{record.provider}</span><time dateTime={record.createdAt}>{formatDate(record.createdAt)}</time><span>{record.messageCount} {record.messageCount === 1 ? 'turn' : 'turns'}</span></div>
                      <p className={styles.corpusRealm}>{record.loreRealm} · {record.loreArtifact}</p>
                      <h3><Link href={record.href}>{record.title}</Link></h3>
                      <p>{record.excerpt || 'No visible dialogue body was present in this export record.'}</p>
                      <ul className={styles.themeList}>{record.themes.map((value) => <li key={value}>{value}</li>)}</ul>
                      <div className={styles.corpusFooter}><span>{record.category}</span>{record.contentWarningCount > 0 ? <span>{record.contentWarningCount} content notice{record.contentWarningCount === 1 ? '' : 's'}</span> : <span>Open reading</span>}<Link href={record.href}>Read + interpret →</Link></div>
                    </article>
                  ))}
                </div>
                {corpusVisible < corpusFiltered.length ? <button className={styles.loadMore} type="button" onClick={() => setCorpusVisible((value) => value + 30)}>Show {Math.min(30, corpusFiltered.length - corpusVisible)} more {corpusFiltered.length - corpusVisible === 1 ? 'conversation' : 'conversations'}</button> : null}
              </>
            ) : null}
          </section>

          </details>

          <section className={styles.cta} aria-labelledby="cta-title">
            <div>
              <p className={styles.eyebrow}>More to read</p>
              <h2 id="cta-title">Keep exploring.</h2>
              <p>
                Read an essay, begin the Good Book, or support the writing if it gives you something useful.
              </p>
            </div>
            <div className={styles.ctaActions}>
              <Link className={styles.primary} href="/membership">Become a member →</Link>
              {koFi ? <a className={styles.secondary} href={koFi.href} target="_blank" rel="noopener noreferrer">Fund the next release on Ko-fi ↗</a> : null}
              <a className={styles.secondary} href="https://chaos-tarot.com/free-reading?source=apocky-conversations" target="_blank" rel="noopener noreferrer">Ask Chaos Tarot ↗</a>
            </div>
          </section>
        </div>
      </main>
    </>
  );
};

export default Conversations;
