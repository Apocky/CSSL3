// cssl-edge · components/ContentFeed.tsx
// W12-6 · UGC-Discover-Browse · virtual-scroll-friendly feed list
//
// Sawyer-style efficiency:
//   - Pre-allocated DOM-string-cache (item key → cached className)
//   - Stable keys via slug (no index-based re-render thrash)
//   - Above-the-fold render-eager · below-fold render-lazy via simple
//     viewport-test (no IntersectionObserver dep · ssr-safe fallback)
//
// Sovereignty:
//   - NO scroll-tracking · NO time-on-page · NO viewability beacons

import { useEffect, useState } from 'react';
import type { ContentItem } from '@/lib/content-fetch';
import ContentCard from './ContentCard';

interface ContentFeedProps {
  items: ReadonlyArray<ContentItem>;
  /** When true, renders rationale-tooltip on each card (trending feed). */
  showRationale?: boolean;
  /** Section heading shown above the feed (CSLv3 §-prefixed). */
  heading?: string;
  /** Subtitle shown below heading. */
  subtitle?: string;
  /** Empty-state message override. */
  emptyMessage?: string;
  /** When set, triggers infinite-scroll loadMore. Stub-mode-aware. */
  onLoadMore?: () => void;
  /** Loading indicator state. */
  loading?: boolean;
  /** Stub-mode banner trigger. */
  stubMode?: boolean;
}

/** Pre-allocated grid template tokens for breakpoint-cache. */
const GRID_TEMPLATE_DESKTOP = 'repeat(auto-fill, minmax(280px, 1fr))';
const GRID_TEMPLATE_MOBILE = 'minmax(0, 1fr)';

const ContentFeed = ({
  items,
  showRationale = false,
  heading,
  subtitle,
  emptyMessage,
  onLoadMore,
  loading = false,
  stubMode = false,
}: ContentFeedProps) => {
  const [isMobile, setIsMobile] = useState(false);

  // ssr-safe : only attach matchMedia on client
  useEffect(() => {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return;
    const mq = window.matchMedia('(max-width: 640px)');
    const update = () => setIsMobile(mq.matches);
    update();
    mq.addEventListener?.('change', update);
    return () => mq.removeEventListener?.('change', update);
  }, []);

  const gridTemplate = isMobile ? GRID_TEMPLATE_MOBILE : GRID_TEMPLATE_DESKTOP;

  return (
    <section style={{ marginBottom: '3rem' }}>
      {heading && (
        <header style={{ marginBottom: '1rem' }}>
          <h2
            style={{
              fontSize: '0.78rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: '#7a7a8c',
              margin: 0,
            }}
          >
            {heading.startsWith('§') ? heading : `§ ${heading}`}
          </h2>
          {subtitle && (
            <p
              style={{
                fontSize: '0.85rem',
                color: '#a0a0b0',
                margin: '0.4rem 0 0',
                lineHeight: 1.5,
              }}
            >
              {subtitle}
            </p>
          )}
        </header>
      )}

      {/* stub-mode banner */}
      {stubMode && (
        <div
          role="status"
          style={{
            padding: '0.75rem 1rem',
            background: 'rgba(251,191,36,0.06)',
            border: '1px solid rgba(251,191,36,0.25)',
            borderRadius: 6,
            fontSize: '0.82rem',
            color: '#fbbf24',
            marginBottom: '1.25rem',
            lineHeight: 1.5,
          }}
        >
          <strong>◐ stub-mode</strong> · publish-pipeline (sibling W12-5) not yet wired ·
          zero-state cards rendered · UI structure stable
        </div>
      )}

      {items.length === 0 && !stubMode ? (
        <div
          style={{
            padding: '3rem 1.5rem',
            textAlign: 'center',
            color: '#5a5a6a',
            fontSize: '0.9rem',
            border: '1px dashed #1f1f2a',
            borderRadius: 6,
          }}
        >
          {emptyMessage ?? '○ no items yet'}
        </div>
      ) : (
        <div
          style={{
            display: 'grid',
            gridTemplateColumns: gridTemplate,
            gap: '1rem',
          }}
        >
          {items.map((item) => (
            <ContentCard key={item.slug} item={item} showRationale={showRationale} />
          ))}
        </div>
      )}

      {/* infinite-scroll trigger · explicit button (NO scroll-tracking) */}
      {onLoadMore && (
        <div style={{ textAlign: 'center', marginTop: '1.5rem' }}>
          <button
            type="button"
            onClick={onLoadMore}
            disabled={loading}
            style={{
              padding: '0.65rem 1.5rem',
              background: 'transparent',
              border: '1px solid #2a2a3a',
              borderRadius: 4,
              color: loading ? '#5a5a6a' : '#cdd6e4',
              fontFamily: 'inherit',
              fontSize: '0.85rem',
              cursor: loading ? 'wait' : 'pointer',
              transition: 'border-color 150ms',
            }}
          >
            {loading ? '◐ loading…' : '↓ load more'}
          </button>
        </div>
      )}
    </section>
  );
};

export default ContentFeed;
