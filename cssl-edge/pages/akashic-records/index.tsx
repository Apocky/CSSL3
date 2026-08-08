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

  const sources = useMemo(() => uniqueSorted(records.map((record) => record.source)), [records]);
  const topics = useMemo(() => uniqueSorted(records.flatMap((record) => record.topics)), [records]);
  const years = useMemo(
    () => Array.from(new Set(records.map((record) => record.year))).sort((a, b) => b - a),
    [records],
  );
  const types = useMemo(() => uniqueSorted(records.map((record) => record.type)), [records]);

  const filteredRecords = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    return records.filter((record) => {
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
  }, [query, records, source, topic, type, year]);

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
        <title>Akashic Records — Works by Shawn Apocky</title>
        <meta
          name="description"
          content="A searchable public archive of Shawn Apocky’s approved non-draft Medium works."
        />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta property="og:title" content="Akashic Records — Works by Shawn Apocky" />
        <meta
          property="og:description"
          content="Search Shawn Apocky’s approved public Medium works by title or description, and browse them by year."
        />
        <meta property="og:type" content="website" />
        <meta property="og:url" content="https://www.apocky.com/akashic-records" />
        <link rel="canonical" href="https://www.apocky.com/akashic-records" />
      </Head>

      <main className={styles.root}>
        <section className={styles.hero} aria-labelledby="akashic-title">
          <div className={styles.heroCopy}>
            <p className={styles.eyebrow}>Public works archive · Shawn Apocky</p>
            <h1 id="akashic-title">Akashic Records</h1>
            <p className={styles.lede}>
              Read, search, compare, and follow recurring ideas across my
              approved non-draft Medium works. Future sources can join the
              archive only through the same explicit public-approval gate.
            </p>
            <p className={styles.publicationNote}>
              Every work here is a non-draft publication approved by the author for
              this public archive. Original off-site links are preserved when known,
              but some may no longer be available. For analysis and verification, view
              the <a href="/akashic-records/manifest.json">hash-sealed public catalog</a>.
            </p>
          </div>

          <dl className={styles.metrics} aria-label="Archive overview">
            <div>
              <dt>Works</dt>
              <dd>{records.length}</dd>
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
                <h2 id="explore-records-title">Find a work</h2>
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
              {filteredRecords.length === records.length
                ? `${records.length} ${records.length === 1 ? 'work' : 'works'}`
                : `${filteredRecords.length} of ${records.length} works`}
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
                      Read this work <span aria-hidden="true">→</span>
                    </Link>
                  </article>
                ))}
              </div>
            ) : (
              <div id="akashic-record-list" className={styles.emptyState}>
                <h3>No works match these filters.</h3>
                <p>Try a broader search, or clear the filters to see the whole archive.</p>
                <button type="button" onClick={clearFilters}>Show all works</button>
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
