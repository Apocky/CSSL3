import type { GetStaticPaths, GetStaticProps, NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';
import type { ReactNode } from 'react';

import {
  findAkashicRecord,
  getAkashicRecordSummaries,
  type AkashicBlock,
  type AkashicLink,
  type AkashicRecord,
  type AkashicRecordSummary,
} from '@/lib/akashic-records';
import styles from '@/styles/AkashicRecords.module.css';

interface RecordNeighbor {
  slug: string;
  title: string;
}

type AkashicRecordPageRecord = Omit<AkashicRecord, 'body'>;

interface AkashicRecordPageProps {
  record: AkashicRecordPageRecord;
  previous: RecordNeighbor | null;
  next: RecordNeighbor | null;
}

function formatDate(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat('en-US', {
    day: 'numeric',
    month: 'long',
    timeZone: 'UTC',
    year: 'numeric',
  }).format(date);
}

function originalLinkLabel(status: AkashicRecord['sourceUrlStatus']): string {
  if (status === 'dead') return 'Original source link (may be unavailable)';
  if (status === 'live') return 'Read the original source';
  return 'Original source link (availability unverified)';
}

function recordNeighbor(record: AkashicRecordSummary | undefined): RecordNeighbor | null {
  return record === undefined ? null : { slug: record.slug, title: record.title };
}

function safeExternalHref(value: string | undefined): string | undefined {
  if (value === undefined) return undefined;
  try {
    const parsed = new URL(value);
    if (!['http:', 'https:'].includes(parsed.protocol)) return undefined;
    if (parsed.username.length > 0 || parsed.password.length > 0) return undefined;
    return parsed.toString();
  } catch {
    return undefined;
  }
}

function renderLinkedText(text: string, links: readonly AkashicLink[] | undefined): ReactNode[] {
  if (links === undefined || links.length === 0) return [text];

  const nodes: ReactNode[] = [];
  let cursor = 0;
  const orderedLinks = [...links].sort((left, right) => left.start - right.start);
  for (const link of orderedLinks) {
    const safeHref = safeExternalHref(link.href);
    const valid = (
      safeHref !== undefined &&
      link.start >= cursor &&
      link.end > link.start &&
      link.end <= text.length &&
      text.slice(link.start, link.end) === link.text
    );
    if (!valid) continue;
    if (link.start > cursor) nodes.push(text.slice(cursor, link.start));
    nodes.push(
      <a key={`${link.start}-${link.end}-${link.href}`} href={safeHref} target="_blank" rel="noopener noreferrer">
        {link.text}<span className={styles.externalMark} aria-hidden="true"> ↗</span><span className={styles.srOnly}> (opens in new tab)</span>
      </a>,
    );
    cursor = link.end;
  }
  if (cursor < text.length) nodes.push(text.slice(cursor));
  return nodes;
}

function renderRecordBlock(block: AkashicBlock, index: number): JSX.Element {
  const key = `${block.kind}-${index}`;
  switch (block.kind) {
    case 'paragraph':
      return <p key={key}>{renderLinkedText(block.text, block.links)}</p>;
    case 'heading':
      return block.level === 2
        ? <h2 key={key}>{renderLinkedText(block.text, block.links)}</h2>
        : <h3 key={key}>{renderLinkedText(block.text, block.links)}</h3>;
    case 'blockquote':
      return <blockquote key={key}><p>{renderLinkedText(block.text, block.links)}</p></blockquote>;
    case 'list': {
      const items = block.items.map((item, itemIndex) => <li key={`${key}-${itemIndex}`}>{item}</li>);
      return block.ordered ? <ol key={key}>{items}</ol> : <ul key={key}>{items}</ul>;
    }
    case 'pre':
      return <pre key={key} tabIndex={0}><code>{block.text}</code></pre>;
    case 'figure':
      return (
        <figure key={key} className={styles.omittedBlock}>
          <div className={styles.omittedLabel}>Image omitted from archive copy</div>
          {block.alt !== undefined ? <p><strong>Image description:</strong> {block.alt}</p> : null}
          {block.caption !== undefined ? <figcaption>{block.caption}</figcaption> : null}
        </figure>
      );
    case 'embed': {
      const embedHref = safeExternalHref(block.href);
      return (
        <aside key={key} className={styles.omittedBlock} aria-label="Omitted embedded media">
          <div className={styles.omittedLabel}>
            {block.provider !== undefined ? `${block.provider} embed omitted from archive copy` : 'Embedded media omitted from archive copy'}
          </div>
          {embedHref !== undefined ? (
            <a href={embedHref} target="_blank" rel="noopener noreferrer">
              Open the original embedded item <span aria-hidden="true">↗</span><span className={styles.srOnly}> (opens in new tab)</span>
            </a>
          ) : null}
        </aside>
      );
    }
    case 'linkCard': {
      const cardHref = safeExternalHref(block.href);
      return (
        <div key={key} className={styles.linkCard}>
          {cardHref !== undefined ? (
            <a href={cardHref} target="_blank" rel="noopener noreferrer">
              <span>{block.text}</span>
              <span className={styles.linkCardAction}>Open linked item <span aria-hidden="true">↗</span></span>
              <span className={styles.srOnly}> (opens in new tab)</span>
            </a>
          ) : <span>{block.text}</span>}
        </div>
      );
    }
    case 'divider':
      return <hr key={key} />;
  }
  const unreachable: never = block;
  throw new Error(`Unsupported Akashic block: ${String(unreachable)}`);
}

const AkashicRecordPage: NextPage<AkashicRecordPageProps> = ({ record, previous, next }) => {
  const localUrl = `https://www.apocky.com/akashic-records/${record.slug}`;
  const pageTitle = `${record.title} · Akashic Records`;
  const sourceUrl = safeExternalHref(record.sourceUrl);

  return (
    <>
      <Head>
        <title>{pageTitle}</title>
        <meta name="description" content={record.excerpt} />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta property="og:title" content={record.title} />
        <meta property="og:description" content={record.excerpt} />
        <meta property="og:type" content="article" />
        <meta property="og:url" content={localUrl} />
        <meta property="article:published_time" content={record.publishedAt} />
        <link rel="canonical" href={localUrl} />
      </Head>

      <main className={`${styles.root} ${styles.detailRoot}`}>
        <article className={styles.reader}>
          <Link className={styles.backLink} href="/akashic-records">
            <span aria-hidden="true">←</span> All Akashic Records
          </Link>

          <header className={styles.readerHeader}>
            <p className={styles.eyebrow}>Akashic Records · {record.type}</p>
            <h1>{record.title}</h1>
            <p className={styles.readerExcerpt}>{record.excerpt}</p>

            <dl className={styles.readerMeta}>
              <div>
                <dt>Published</dt>
                <dd><time dateTime={record.publishedAt}>{formatDate(record.publishedAt)}</time></dd>
              </div>
              <div>
                <dt>Source</dt>
                <dd>{record.source}</dd>
              </div>
              <div>
                <dt>Type</dt>
                <dd>{record.type}</dd>
              </div>
            </dl>

            {record.topics.length > 0 ? (
              <ul className={styles.topicList} aria-label="Topics">
                {record.topics.map((topic) => <li key={topic}>{topic}</li>)}
              </ul>
            ) : null}

            {sourceUrl !== undefined ? (
              <div className={styles.originalSource}>
                <a href={sourceUrl} target="_blank" rel="noopener noreferrer">
                  {originalLinkLabel(record.sourceUrlStatus)} <span aria-hidden="true">↗</span><span className={styles.srOnly}> (opens in new tab)</span>
                </a>
                <p>This archive copy remains readable even if the off-site source has moved or disappeared.</p>
              </div>
            ) : null}
          </header>

          <div className={styles.readerBody}>
            {record.blocks.map(renderRecordBlock)}
          </div>

          <aside className={styles.provenance} aria-labelledby="record-provenance-title">
            <p className={styles.eyebrow}>Provenance</p>
            <h2 id="record-provenance-title">About this archive copy</h2>
            <dl>
              <div>
                <dt>Publication state</dt>
                <dd>Author-approved public non-draft work</dd>
              </div>
              <div>
                <dt>Archive source</dt>
                <dd>{record.source}</dd>
              </div>
              {record.updatedAt !== undefined ? (
                <div>
                  <dt>Archive/import date</dt>
                  <dd><time dateTime={record.updatedAt}>{formatDate(record.updatedAt)}</time></dd>
                </div>
              ) : null}
              <div>
                <dt>Source fingerprint</dt>
                <dd><code>{record.sourceSha256}</code></dd>
              </div>
              <div>
                <dt>Stable archive address</dt>
                <dd><a href={localUrl}>{localUrl}</a></dd>
              </div>
            </dl>
          </aside>

          <nav className={styles.adjacent} aria-label="Nearby works">
            {previous !== null ? (
              <Link href={`/akashic-records/${previous.slug}`}>
                <span>Previous work</span>
                <strong><span aria-hidden="true">←</span> {previous.title}</strong>
              </Link>
            ) : <span />}
            {next !== null ? (
              <Link href={`/akashic-records/${next.slug}`}>
                <span>Next work</span>
                <strong>{next.title} <span aria-hidden="true">→</span></strong>
              </Link>
            ) : <span />}
          </nav>
        </article>
      </main>
    </>
  );
};

export const getStaticPaths: GetStaticPaths = () => ({
  paths: getAkashicRecordSummaries().map((record) => ({ params: { slug: record.slug } })),
  fallback: false,
});

export const getStaticProps: GetStaticProps<AkashicRecordPageProps> = (context) => {
  const slug = typeof context.params?.['slug'] === 'string' ? context.params['slug'] : '';
  const record = findAkashicRecord(slug);
  if (record === undefined) return { notFound: true };

  const summaries = getAkashicRecordSummaries();
  const index = summaries.findIndex((candidate) => candidate.slug === slug);
  const { body: _readbackBody, ...pageRecord } = record;
  return {
    props: {
      record: pageRecord,
      previous: recordNeighbor(summaries[index - 1]),
      next: recordNeighbor(summaries[index + 1]),
    },
  };
};

export default AkashicRecordPage;
