import type { GetStaticProps, NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';
import { useMemo, useState } from 'react';

import {
  getAkashicRecordSummaries,
  type AkashicRecordSummary,
} from '@/lib/akashic-records';
import styles from '@/styles/AkashicRecords.module.css';

interface AkashicRecordsIndexProps {
  records: AkashicRecordSummary[];
}

const ALL = 'all';

function uniqueSorted(values: readonly string[]): string[] {
  return Array.from(new Set(values.filter(Boolean))).sort((a, b) => a.localeCompare(b));
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

const AkashicRecordsIndex: NextPage<AkashicRecordsIndexProps> = ({ records }) => {
  const [query, setQuery] = useState('');
  const [source, setSource] = useState(ALL);
  const [topic, setTopic] = useState(ALL);
  const [year, setYear] = useState(ALL);
  const [type, setType] = useState(ALL);

  const archiveEntries = useMemo(() => {
    const primaryConversationRecords = new Map<string, AkashicRecordSummary>();
    for (const record of records) {
      if (record.conversationId === undefined) continue;
      const current = primaryConversationRecords.get(record.conversationId);
      if (current === undefined || (record.part ?? 1) < (current.part ?? 1)) {
        primaryConversationRecords.set(record.conversationId, record);
      }
    }
    return records.filter((record) => {
      if (record.conversationId === undefined) return true;
      return primaryConversationRecords.get(record.conversationId)?.slug === record.slug;
    });
  }, [records]);
  const sources = useMemo(() => uniqueSorted(archiveEntries.map((record) => record.source)), [archiveEntries]);
  const topics = useMemo(() => uniqueSorted(archiveEntries.flatMap((record) => record.topics)), [archiveEntries]);
  const years = useMemo(
    () => Array.from(new Set(archiveEntries.map((record) => record.year))).sort((a, b) => b - a),
    [archiveEntries],
  );
  const types = useMemo(() => uniqueSorted(archiveEntries.map((record) => record.type)), [archiveEntries]);
  const conversationCount = archiveEntries.filter((record) => record.conversationId !== undefined).length;
  const workCount = archiveEntries.length - conversationCount;

  const filteredRecords = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    return archiveEntries.filter((record) => {
      const searchable = [
        record.title,
        record.excerpt,
      ].join('\n').toLocaleLowerCase();
      return (
        (needle.length === 0 || searchable.includes(needle)) &&
        (source === ALL || record.source === source) &&
        (topic === ALL || record.topics.includes(topic)) &&
        (year === ALL || record.year === Number(year)) &&
        (type === ALL || record.type === type)
      );
    });
  }, [archiveEntries, query, source, topic, type, year]);

  const activeFilters = query.trim().length > 0 || [source, topic, year, type].some((value) => value !== ALL);
  const yearRange = years.length > 0
    ? years.length === 1
      ? String(years[0])
      : `${years[years.length - 1]}–${years[0]}`
    : '—';

  const clearFilters = (): void => {
    setQuery('');
    setSource(ALL);
    setTopic(ALL);
    setYear(ALL);
    setType(ALL);
  };

  return (
    <>
      <Head>
        <title>Akashic Records — Public Works and Conversations</title>
        <meta
          name="description"
          content="A searchable archive of Shawn Apocky’s explicitly approved public works and Codex conversations."
        />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta property="og:title" content="Akashic Records — Public Works and Conversations" />
        <meta
          property="og:description"
          content="Search Shawn Apocky’s approved public works and Codex conversations by title, description, source, type, or year."
        />
        <meta property="og:type" content="website" />
        <meta property="og:url" content="https://www.apocky.com/akashic-records" />
        <link rel="canonical" href="https://www.apocky.com/akashic-records" />
      </Head>

      <main className={styles.root}>
        <section className={styles.hero} aria-labelledby="akashic-title">
          <div className={styles.heroCopy}>
            <p className={styles.eyebrow}>Public archive · Shawn Apocky</p>
            <h1 id="akashic-title">Akashic Records</h1>
            <p className={styles.lede}>
              Read, search, compare, and follow recurring ideas across my approved
              public works and conversations. Each source enters through an explicit
              public-approval gate and a reproducible, hash-sealed projection.
            </p>
            <p className={styles.publicationNote}>
              Medium works are approved non-draft publications. Codex conversations are
              approved, public-safe transcript projections; redactions are counted and
              their projection fingerprints are published. For verification, view the{' '}
              <a href="/akashic-records/manifest.json">hash-sealed public catalog</a>.
            </p>
          </div>

          <dl className={styles.metrics} aria-label="Archive overview">
            <div>
              <dt>Works</dt>
              <dd>{workCount}</dd>
            </div>
            <div>
              <dt>Conversations</dt>
              <dd>{conversationCount}</dd>
            </div>
            <div>
              <dt>Years</dt>
              <dd>{yearRange}</dd>
            </div>
            {topics.length > 0 ? (
              <div>
                <dt>Topics</dt>
                <dd>{topics.length}</dd>
              </div>
            ) : null}
          </dl>
        </section>

        <section className={styles.explorer} aria-labelledby="explore-records-title">
          <form className={styles.filters} onSubmit={(event) => event.preventDefault()}>
            <div className={styles.filterHeading}>
              <div>
                <p className={styles.eyebrow}>Explore the archive</p>
                <h2 id="explore-records-title">Find a record</h2>
              </div>
              <button
                className={styles.clearButton}
                type="button"
                onClick={clearFilters}
                disabled={!activeFilters}
              >
                Clear
              </button>
            </div>

            <label className={styles.field}>
              <span>Search</span>
              <input
                type="search"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Titles and descriptions…"
                autoComplete="off"
                aria-controls="akashic-record-list"
              />
            </label>

            <div className={styles.filterGrid}>
              {sources.length > 1 ? (
                <label className={styles.field}>
                  <span>Source</span>
                  <select value={source} onChange={(event) => setSource(event.target.value)}>
                    <option value={ALL}>All sources</option>
                    {sources.map((value) => <option key={value} value={value}>{value}</option>)}
                  </select>
                </label>
              ) : null}

              {topics.length > 0 ? (
                <label className={styles.field}>
                  <span>Topic</span>
                  <select value={topic} onChange={(event) => setTopic(event.target.value)}>
                    <option value={ALL}>All topics</option>
                    {topics.map((value) => <option key={value} value={value}>{value}</option>)}
                  </select>
                </label>
              ) : null}

              <label className={styles.field}>
                <span>Year</span>
                <select value={year} onChange={(event) => setYear(event.target.value)}>
                  <option value={ALL}>All years</option>
                  {years.map((value) => <option key={value} value={String(value)}>{value}</option>)}
                </select>
              </label>

              {types.length > 1 ? (
                <label className={styles.field}>
                  <span>Type</span>
                  <select value={type} onChange={(event) => setType(event.target.value)}>
                    <option value={ALL}>All types</option>
                    {types.map((value) => <option key={value} value={value}>{value}</option>)}
                  </select>
                </label>
              ) : null}
            </div>
          </form>

          <div className={styles.results}>
            <p className={styles.resultCount} role="status" aria-live="polite" aria-atomic="true">
              {filteredRecords.length === archiveEntries.length
                ? `${archiveEntries.length} ${archiveEntries.length === 1 ? 'entry' : 'entries'}`
                : `${filteredRecords.length} of ${archiveEntries.length} entries`}
            </p>

            {filteredRecords.length > 0 ? (
              <div id="akashic-record-list" className={styles.recordList}>
                {filteredRecords.map((record) => (
                  <article key={record.slug} className={styles.recordCard}>
                    <div className={styles.recordMeta}>
                      <span>{record.source}</span>
                      <span aria-hidden="true">·</span>
                      <time dateTime={record.publishedAt}>{formatDate(record.publishedAt)}</time>
                      <span aria-hidden="true">·</span>
                      <span>{record.type}</span>
                      {(record.parts ?? 1) > 1 ? (
                        <><span aria-hidden="true">·</span><span>{record.parts} parts</span></>
                      ) : null}
                      {record.publicationState === 'withheld' ? (
                        <><span aria-hidden="true">·</span><span>Transcript withheld</span></>
                      ) : null}
                    </div>
                    <h3>
                      <Link href={`/akashic-records/${record.slug}`}>{record.title}</Link>
                    </h3>
                    <p>{record.excerpt}</p>
                    {record.topics.length > 0 ? (
                      <ul className={styles.topicList} aria-label="Topics">
                        {record.topics.map((value) => <li key={value}>{value}</li>)}
                      </ul>
                    ) : null}
                    <Link className={styles.readLink} href={`/akashic-records/${record.slug}`}>
                      {record.publicationState === 'withheld'
                        ? 'View withheld record'
                        : record.type === 'Conversation transcript' ? 'Read transcript' : 'Read this work'}{' '}
                      <span aria-hidden="true">→</span>
                    </Link>
                  </article>
                ))}
              </div>
            ) : (
              <div id="akashic-record-list" className={styles.emptyState}>
                <h3>No records match these filters.</h3>
                <p>Try a broader search, or clear the filters to see the whole archive.</p>
                <button type="button" onClick={clearFilters}>Show all records</button>
              </div>
            )}
          </div>
        </section>
      </main>
    </>
  );
};

export const getStaticProps: GetStaticProps<AkashicRecordsIndexProps> = () => ({
  props: { records: [...getAkashicRecordSummaries()] },
});

export default AkashicRecordsIndex;
