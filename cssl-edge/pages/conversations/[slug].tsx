import type { GetStaticPaths, GetStaticProps, NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';
import { useEffect, useState } from 'react';

import type {
  ConversationCorpusPageResponse,
  ConversationCorpusSummary,
  CorpusBranch,
  CorpusMessage,
} from '@/lib/conversation-corpus';
import { getBundledPublicConversationManifest } from '@/lib/server/conversation-corpus-manifest';
import { SUPPORT_LINKS } from '@/lib/support-links';
import styles from '@/styles/ConversationReader.module.css';

type Lens = 'dialogue' | 'signals' | 'lore' | 'connections';
type RoleFilter = 'all' | 'user' | 'assistant';
type BranchFilter = 'all' | CorpusBranch;

interface ConversationReaderProps {
  readonly summary: ConversationCorpusSummary;
}

const PAGE_SIZE = 24;

function formatDate(value: string): string {
  const date = new Date(`${value}T00:00:00Z`);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat('en-US', { dateStyle: 'medium', timeZone: 'UTC' }).format(date);
}

const ConversationReader: NextPage<ConversationReaderProps> = ({ summary }) => {
  const [lens, setLens] = useState<Lens>('dialogue');
  const [acknowledged, setAcknowledged] = useState(summary.contentWarnings.length === 0);
  const [record, setRecord] = useState<ConversationCorpusSummary | null>(null);
  const [messages, setMessages] = useState<readonly CorpusMessage[]>([]);
  const [messageTotal, setMessageTotal] = useState(0);
  const [nextOffset, setNextOffset] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const [loadError, setLoadError] = useState('');
  const [query, setQuery] = useState('');
  const [role, setRole] = useState<RoleFilter>('all');
  const [branch, setBranch] = useState<BranchFilter>('all');
  const [expanded, setExpanded] = useState<ReadonlySet<number>>(new Set());
  const [retryNonce, setRetryNonce] = useState(0);
  const koFi = SUPPORT_LINKS.find((link) => link.name === 'Ko-fi');

  useEffect(() => {
    if (!acknowledged) return;
    const controller = new AbortController();
    const timer = window.setTimeout(() => {
      setLoading(true);
      setLoadError('');
      const params = new URLSearchParams({ ack: '1', offset: '0', limit: String(PAGE_SIZE), role, branch });
      if (query.trim()) params.set('q', query.trim());
      fetch(`/api/conversation-corpus/${summary.id}?${params}`, { signal: controller.signal })
        .then(async (response) => {
          if (!response.ok) {
            const body = await response.json().catch(() => null) as { error?: { message?: string } } | null;
            throw new Error(body?.error?.message ?? `Archive page returned ${response.status}`);
          }
          return response.json() as Promise<ConversationCorpusPageResponse>;
        })
        .then((value) => {
          if (value.record.id !== summary.id || value.record.projectionSha256 !== summary.projectionSha256) throw new Error('Archive page failed its identity check');
          setRecord(value.record);
          setMessages(value.messages);
          setMessageTotal(value.page.total);
          setNextOffset(value.page.nextOffset);
        })
        .catch((error: unknown) => {
          if (error instanceof DOMException && error.name === 'AbortError') return;
          setLoadError(error instanceof Error ? error.message : 'Unable to load this dialogue');
        })
        .finally(() => setLoading(false));
    }, query.trim() ? 180 : 0);
    return () => { window.clearTimeout(timer); controller.abort(); };
  }, [acknowledged, branch, query, retryNonce, role, summary.id, summary.projectionSha256]);

  const loadMore = async (): Promise<void> => {
    if (nextOffset === null || loading) return;
    setLoading(true);
    setLoadError('');
    try {
      const params = new URLSearchParams({ ack: '1', offset: String(nextOffset), limit: String(PAGE_SIZE), role, branch });
      if (query.trim()) params.set('q', query.trim());
      const response = await fetch(`/api/conversation-corpus/${summary.id}?${params}`);
      if (!response.ok) throw new Error(`Archive page returned ${response.status}`);
      const value = await response.json() as ConversationCorpusPageResponse;
      setMessages((current) => [...current, ...value.messages]);
      setMessageTotal(value.page.total);
      setNextOffset(value.page.nextOffset);
    } catch (error: unknown) {
      setLoadError(error instanceof Error ? error.message : 'Unable to load more turns');
    } finally {
      setLoading(false);
    }
  };

  const toggleExpanded = (sequence: number): void => {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(sequence)) next.delete(sequence);
      else next.add(sequence);
      return next;
    });
  };

  const canonical = `https://www.apocky.com/conversations/${summary.slug}`;
  const active = record ?? summary;
  const description = summary.excerpt || `A ${summary.provider} conversation in the Apocky public archive.`;
  const jsonLd = {
    '@context': 'https://schema.org',
    '@type': 'CreativeWork',
    name: summary.title,
    description,
    url: canonical,
    dateCreated: summary.createdAt === 'unknown' ? undefined : summary.createdAt,
    author: { '@type': 'Person', name: 'Shawn Apocky' },
    isPartOf: { '@type': 'CollectionPage', name: 'Conversation Constellations', url: 'https://www.apocky.com/conversations' },
    keywords: [summary.category, ...summary.themes].join(', '),
  };

  return (
    <>
      <Head>
        <title>{summary.title} · Conversation Constellations</title>
        <meta name="description" content={description} />
        <meta name="robots" content={summary.indexable ? 'index,follow,max-image-preview:large' : 'noindex,follow'} />
        <meta name="theme-color" content="#000000" />
        <meta property="og:type" content="article" />
        <meta property="og:title" content={summary.title} />
        <meta property="og:description" content={description} />
        <meta property="og:url" content={canonical} />
        <link rel="canonical" href={canonical} />
        {summary.indexable ? <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(jsonLd) }} /> : null}
      </Head>

      <main className={styles.page}>
        <div className={styles.wrap}>
          <nav className={styles.crumbs} aria-label="Breadcrumb">
            <Link href="/conversations">Conversation ideas</Link><span aria-hidden="true">/</span><span>{summary.provider}</span>
          </nav>

          <header className={styles.hero}>
            <div className={styles.meta}>
              <span data-provider={summary.provider.toLowerCase()}>{summary.provider}</span>
              <time dateTime={summary.createdAt}>{formatDate(summary.createdAt)}</time>
              <span>{summary.messageCount} visible turns</span>
              {summary.alternateMessageCount > 0 ? <span>{summary.alternateMessageCount} alternate</span> : null}
            </div>
            <p className={styles.realm}>{summary.lore.realm} · {summary.lore.artifact}</p>
            <h1>{summary.title}</h1>
            <p className={styles.intro}>{summary.excerpt || 'This export record contains no visible human or assistant text body.'}</p>
            <ul className={styles.themes} aria-label="Themes">
              {summary.themes.map((theme) => <li key={theme}>{theme}</li>)}
            </ul>
          </header>

          <details className={styles.truthStrip}>
            <summary>Sources and publication details</summary>
            <p><strong>Complete means complete visible dialogue.</strong> Hidden model reasoning, tools, system prompts, private attachments, credentials, contact data, payment data, and likely third-party copyrighted payloads are not conversation bodies and are excluded or redacted.</p>
            <details>
              <summary>Inspect lineage</summary>
              <dl>
                <div><dt>Public reference</dt><dd><code>{summary.sourceReference}</code></dd></div>
                <div><dt>Source-record SHA-256</dt><dd><code>{summary.sourceFingerprint}</code></dd></div>
                <div><dt>Public projection SHA-256</dt><dd><code>{summary.projectionSha256}</code></dd></div>
                <div><dt>Category source</dt><dd>{summary.categoryProvenance}</dd></div>
                <div><dt>Redactions</dt><dd>{summary.redactionCount}</dd></div>
              </dl>
            </details>
          </details>

          <div className={styles.lensBar} role="group" aria-label="Conversation reading mode">
            {([
              ['dialogue', 'Full dialogue'],
              ['signals', 'Key ideas'],
              ['lore', 'Lore fragment'],
              ['connections', 'Connections'],
            ] as const).map(([value, label]) => (
              <button key={value} type="button" aria-pressed={lens === value} onClick={() => setLens(value)}>{label}</button>
            ))}
          </div>

          {lens === 'dialogue' ? (
            <section className={styles.panel} aria-labelledby="dialogue-title">
              <div className={styles.panelHeading}>
                <div><p className={styles.eyebrow}>Read the conversation</p><h2 id="dialogue-title">The reviewed conversation</h2></div>
                {acknowledged ? <a href={summary.bodyHref} download>Download JSON ↓</a> : null}
              </div>

              {!acknowledged ? (
                <div className={styles.interlock} role="note">
                  <p className={styles.eyebrow}>Content notice</p>
                  <h3>Choose before opening this record.</h3>
                  <p>This privacy- and rights-reviewed dialogue carries: {summary.contentWarnings.join(', ')}. The notice prevents accidental exposure; it is not a claim that difficult experiences should be hidden.</p>
                  <button type="button" onClick={() => setAcknowledged(true)}>I understand · load the dialogue</button>
                  <Link href="/conversations">Return to conversation ideas</Link>
                </div>
              ) : record === null && loadError.length === 0 ? (
                <div className={styles.loading} role="status">Opening the conversation…</div>
              ) : loadError.length > 0 ? (
                <div className={styles.error} role="alert"><strong>The conversation did not load.</strong><span>{loadError}</span><button type="button" onClick={() => { setLoadError(''); setAcknowledged(false); }}>Try again</button></div>
              ) : record?.bodyState === 'absent-in-export' ? (
                <div className={styles.empty}><h3>No readable messages are available.</h3><p>This record has no readable messages. Return to the ideas to keep exploring.</p></div>
              ) : (
                <>
                  <form className={styles.filters} onSubmit={(event) => event.preventDefault()}>
                    <label><span>Search this dialogue</span><input type="search" value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Find a word or idea…" /></label>
                    <label><span>Voice</span><select value={role} onChange={(event) => setRole(event.target.value as RoleFilter)}><option value="all">Both voices</option><option value="user">Shawn</option><option value="assistant">{summary.provider}</option></select></label>
                    {summary.alternateMessageCount > 0 ? <label><span>Branch</span><select value={branch} onChange={(event) => setBranch(event.target.value as BranchFilter)}><option value="all">All branches</option><option value="primary">Primary path</option><option value="alternate">Alternates</option></select></label> : null}
                  </form>
                  <p className={styles.resultCount} role="status">{messageTotal} matching turns · showing {messages.length}</p>
                  <div className={styles.messages}>
                    {messages.map((message) => {
                      const isLong = message.text.length > 5_000;
                      const isExpanded = expanded.has(message.sequence);
                      return (
                        <article className={styles.message} data-role={message.role} key={`${message.sequence}-${message.branch}`}>
                          <header><strong>{message.role === 'user' ? 'Shawn' : summary.provider}</strong><span>Turn {message.sequence}</span>{message.branch === 'alternate' ? <span>Alternate branch</span> : null}</header>
                          <p>{isLong && !isExpanded ? `${message.text.slice(0, 2_400)}…` : message.text}</p>
                          {isLong ? <button type="button" onClick={() => toggleExpanded(message.sequence)}>{isExpanded ? 'Collapse long turn' : `Read all ${message.text.length.toLocaleString('en-US')} characters`}</button> : null}
                        </article>
                      );
                    })}
                  </div>
                  {nextOffset !== null ? <button className={styles.more} type="button" onClick={() => void loadMore()}>Show {Math.min(PAGE_SIZE, messageTotal - messages.length)} more turns</button> : null}
                </>
              )}
            </section>
          ) : null}

          {lens === 'signals' ? (
            <section className={styles.panel} aria-labelledby="signals-title">
              <div className={styles.panelHeading}><div><p className={styles.eyebrow}>Navigation layer</p><h2 id="signals-title">Ideas from the exchange</h2></div><span className={styles.score}>{summary.qualityScore}/100 feature score</span></div>
              <p className={styles.boundary}>{summary.distillation.evidenceBoundary}</p>
              <div className={styles.signalGrid}>
                <article><span>Human signal</span><p>{summary.humanSignal || 'No human text body was present.'}</p></article>
                <article><span>AI signal</span><p>{summary.aiSignal || 'No assistant text body was present.'}</p></article>
                {summary.distillation.correctionSignal ? <article><span>Correction / resistance</span><p>{summary.distillation.correctionSignal}</p></article> : null}
                <article><span>Dialogue arc</span><p>{summary.distillation.arc}</p></article>
              </div>
              {summary.distillation.questions.length > 0 ? <div className={styles.questions}><h3>Questions carried by the exchange</h3><ul>{summary.distillation.questions.map((question, index) => <li key={`${question.slice(0, 24)}-${index}`}>{question}</li>)}</ul></div> : null}
              <details className={styles.quality}><summary>Why it received this score</summary><ul>{summary.selectionReasons.map((reason) => <li key={reason}>{reason}</li>)}</ul><dl>{Object.entries(summary.qualityDimensions).map(([key, value]) => <div key={key}><dt>{key.replace(/([A-Z])/g, ' $1')}</dt><dd>{value}</dd></div>)}</dl></details>
            </section>
          ) : null}

          {lens === 'lore' ? (
            <section className={`${styles.panel} ${styles.lorePanel}`} aria-labelledby="lore-title">
              <p className={styles.eyebrow}>Original Apocky lore · editorial allegory</p>
              <h2 id="lore-title">{summary.lore.fragmentTitle}</h2>
              <blockquote>{summary.lore.fragment}</blockquote>
              <p className={styles.echo}><span>Archive echo · sanitized human excerpt</span>{summary.lore.invocation}</p>
              <p>{summary.lore.reading}</p>
              <div className={styles.loreRule}>Lore helps you enter the archive. It does not overwrite the dialogue, convert metaphor into fact, or speak as Shawn.</div>
            </section>
          ) : null}

          {lens === 'connections' ? (
            <section className={styles.panel} aria-labelledby="connections-title">
              <div className={styles.panelHeading}><div><p className={styles.eyebrow}>Multidimensional index</p><h2 id="connections-title">Follow the neighboring shards</h2></div></div>
              <div className={styles.connectionGrid}>{summary.connections.map((connection) => <Link href={connection.href} key={connection.id}><span>{connection.provider}</span><h3>{connection.title}</h3><p>{connection.reason}</p><strong>Open conversation →</strong></Link>)}</div>
            </section>
          ) : null}

          <aside className={styles.cta}>
            <div><p className={styles.eyebrow}>Keep the archive alive</p><h2>Fund the next interpretation layer.</h2><p>Approved bodies are free to read. Membership and support fund careful review, connections, tools, cards, and new public interfaces.</p></div>
            <div><Link href="/membership">Become a member →</Link>{koFi ? <a href={koFi.href} target="_blank" rel="noopener noreferrer">Support on Ko-fi ↗</a> : null}<a href="https://chaos-tarot.com/free-reading?source=apocky-conversation" target="_blank" rel="noopener noreferrer">Ask Chaos Tarot ↗</a></div>
          </aside>
        </div>
      </main>
    </>
  );
};

export const getStaticPaths: GetStaticPaths = async () => ({ paths: [], fallback: 'blocking' });

export const getStaticProps: GetStaticProps<ConversationReaderProps> = async ({ params }) => {
  const slug = typeof params?.slug === 'string' ? params.slug : '';
  const manifest = getBundledPublicConversationManifest();
  const summary = manifest.records.find((record) => record.slug === slug && record.editorialReviewState === 'approved');
  if (!summary) return { notFound: true, revalidate: false };
  return { props: { summary }, revalidate: false };
};

export default ConversationReader;
