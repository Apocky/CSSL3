// cssl-edge · pages/content/search.tsx
// W12-6 · /content/search?q=<>&tags=<> · privacy-respecting full-text + tag search
// Sovereignty:
//   - NO query-logging client-side (analytics)
//   - NO autocomplete-typeahead (avoids per-keystroke fingerprinting)
//   - submit-on-explicit-action (button or Enter)
//   - debounce-free · no rate-limit-fingerprinting

import type { NextPage } from 'next';
import { useRouter } from 'next/router';
import Head from 'next/head';
import { useEffect, useState, type FormEvent } from 'react';
import ContentFeed from '@/components/ContentFeed';
import { ContentNav, ContentFooter, contentLandingCSS } from './index';
import {
  fetchContentSearch,
  STUB_LIST_RESPONSE,
  type ContentItem,
} from '@/lib/content-fetch';

const ContentSearch: NextPage = () => {
  const router = useRouter();
  const queryFromUrl = typeof router.query.q === 'string' ? router.query.q : '';
  const tagsFromUrl =
    typeof router.query.tags === 'string'
      ? router.query.tags.split(',').filter((t) => t.length > 0)
      : [];

  const [query, setQuery] = useState(queryFromUrl);
  const [tagInput, setTagInput] = useState(tagsFromUrl.join(','));
  const [results, setResults] = useState<ReadonlyArray<ContentItem>>([]);
  const [stubMode, setStubMode] = useState(false);
  const [loading, setLoading] = useState(false);
  const [searched, setSearched] = useState(false);

  // Auto-execute when URL has ?q=<> on mount (deep-linked share)
  useEffect(() => {
    if (queryFromUrl.length > 0) {
      setQuery(queryFromUrl);
      void executeSearch(queryFromUrl, tagsFromUrl);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [queryFromUrl, router.query.tags]);

  const executeSearch = async (q: string, tags: ReadonlyArray<string>) => {
    if (q.trim().length === 0 && tags.length === 0) return;
    setLoading(true);
    setSearched(true);
    const res = await fetchContentSearch(q.trim(), tags);
    setStubMode(res.stub_mode);
    setResults(res.data?.items ?? (res.stub_mode ? STUB_LIST_RESPONSE.items : []));
    setLoading(false);
  };

  const onSubmit = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    const tags = tagInput
      .split(',')
      .map((t) => t.trim())
      .filter((t) => t.length > 0);
    // Update URL for shareability (replace not push · no history-pollution)
    const params = new URLSearchParams();
    if (query.trim().length > 0) params.set('q', query.trim());
    if (tags.length > 0) params.set('tags', tags.join(','));
    void router.replace(`/content/search?${params.toString()}`, undefined, { shallow: true });
    void executeSearch(query, tags);
  };

  return (
    <>
      <Head>
        <title>§ Search · Content · Apocky</title>
        <meta
          name="description"
          content="Search content packages by full-text + tags · ¬ query-logging · ¬ autocomplete-typeahead · privacy-respecting"
        />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta name="theme-color" content="#0a0a0f" />
        <link rel="canonical" href="https://apocky.com/content/search" />
        <style>{contentLandingCSS}</style>
      </Head>
      <main className="content-shell">
        <ContentNav active="search" />
        <header style={{ marginBottom: '2rem' }}>
          <h1 className="content-h1" style={{ fontSize: 'clamp(1.5rem, 4vw, 2.4rem)' }}>
            § Search · privacy-respecting
          </h1>
          <p className="content-blurb">
            Full-text + tag search · ¬ query-logging · ¬ autocomplete-typeahead ·
            ¬ keystroke-fingerprinting · explicit-submit only.
          </p>
        </header>

        <form onSubmit={onSubmit} style={{ marginBottom: '2rem' }}>
          <div style={{ display: 'flex', flexDirection: 'column', gap: '0.75rem' }}>
            <label
              style={{
                fontSize: '0.78rem',
                color: '#7a7a8c',
                textTransform: 'uppercase',
                letterSpacing: '0.1em',
              }}
            >
              query
              <input
                type="search"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="title · author · description-text…"
                spellCheck="false"
                autoComplete="off"
                style={{
                  display: 'block',
                  width: '100%',
                  marginTop: '0.4rem',
                  padding: '0.6rem 0.85rem',
                  background: '#0f0f17',
                  border: '1px solid #2a2a3a',
                  borderRadius: 4,
                  color: '#e6e6f0',
                  fontFamily: 'inherit',
                  fontSize: '0.9rem',
                }}
              />
            </label>
            <label
              style={{
                fontSize: '0.78rem',
                color: '#7a7a8c',
                textTransform: 'uppercase',
                letterSpacing: '0.1em',
              }}
            >
              tags · comma-separated
              <input
                type="text"
                value={tagInput}
                onChange={(e) => setTagInput(e.target.value)}
                placeholder="cosmetic, ambient, alchemy"
                spellCheck="false"
                autoComplete="off"
                style={{
                  display: 'block',
                  width: '100%',
                  marginTop: '0.4rem',
                  padding: '0.6rem 0.85rem',
                  background: '#0f0f17',
                  border: '1px solid #2a2a3a',
                  borderRadius: 4,
                  color: '#e6e6f0',
                  fontFamily: 'inherit',
                  fontSize: '0.9rem',
                }}
              />
            </label>
            <button
              type="submit"
              disabled={loading}
              style={{
                padding: '0.7rem 1.5rem',
                background: 'linear-gradient(135deg, #c084fc 0%, #7dd3fc 100%)',
                color: '#0a0a0f',
                fontWeight: 600,
                border: 'none',
                borderRadius: 4,
                fontSize: '0.9rem',
                fontFamily: 'inherit',
                cursor: loading ? 'wait' : 'pointer',
                alignSelf: 'flex-start',
              }}
            >
              {loading ? '◐ searching…' : '⌕ search'}
            </button>
          </div>
        </form>

        {searched && !loading && (
          <ContentFeed
            heading={`§ Results · ${results.length}`}
            items={results}
            stubMode={stubMode}
            emptyMessage="○ no matches · try broader tags or different query"
          />
        )}
        {!searched && (
          <p style={{ color: '#5a5a6a', fontSize: '0.85rem', textAlign: 'center', padding: '2rem 0' }}>
            ⌕ enter a query above to search
          </p>
        )}

        <ContentFooter />
      </main>
    </>
  );
};

export default ContentSearch;
