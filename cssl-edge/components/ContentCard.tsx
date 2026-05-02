// cssl-edge · components/ContentCard.tsx
// W12-6 · UGC-Discover-Browse · single content-package card
//
// Sovereignty-UX:
//   - Author-revocability glyph (⊘) shown on every card
//   - Clicking glyph routes to /docs/sovereign-cap (revoke flow)
//   - "Why am I seeing this?" tooltip on every card with rationale
//   - NO engagement-tracking · NO scroll-depth · NO time-on-page
//
// Sawyer-style: status-pill from LUT (no string-cmp at render).

import type { ContentItem } from '@/lib/content-fetch';
import { STATUS_PILL, displayAuthor, timeAgo } from '@/lib/content-fetch';

interface ContentCardProps {
  item: ContentItem;
  /** When true, renders the rationale-tooltip ("why am I seeing this?"). */
  showRationale?: boolean;
  /** Optional explicit href override (defaults to /content/<slug>). */
  href?: string;
}

const ContentCard = ({ item, showRationale = false, href }: ContentCardProps) => {
  const pill = STATUS_PILL[item.status];
  const target = href ?? `/content/${encodeURIComponent(item.slug)}`;
  const ratingDisplay =
    item.rating_summary.total_ratings > 0
      ? `${item.rating_summary.mean_score.toFixed(1)} ★ · ${item.rating_summary.total_ratings}`
      : '— ratings';

  return (
    <article
      className="content-card"
      style={{
        position: 'relative',
        padding: '1.1rem 1.15rem',
        background: 'rgba(20, 20, 30, 0.55)',
        border: '1px solid #1f1f2a',
        borderRadius: 8,
        display: 'flex',
        flexDirection: 'column',
        minHeight: 240,
        transition: 'border-color 150ms, background 150ms',
      }}
    >
      {/* status-pill · top-right */}
      <div
        aria-label={`status ${pill.label}`}
        style={{
          position: 'absolute',
          top: 10,
          right: 10,
          padding: '0.15rem 0.5rem',
          background: pill.bg,
          color: pill.color,
          fontSize: '0.66rem',
          letterSpacing: '0.08em',
          borderRadius: 999,
          textTransform: 'uppercase',
          fontWeight: 600,
        }}
      >
        <span aria-hidden="true">{pill.glyph}</span>{' '}
        {pill.label}
      </div>

      {/* thumbnail-or-glyph */}
      <div
        aria-hidden="true"
        style={{
          width: '100%',
          height: 80,
          marginBottom: '0.75rem',
          background: item.thumbnail_url
            ? `center/cover no-repeat url(${item.thumbnail_url})`
            : 'linear-gradient(135deg, rgba(192,132,252,0.08) 0%, rgba(125,211,252,0.06) 100%)',
          border: '1px solid #14141d',
          borderRadius: 4,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          fontSize: '1.4rem',
          color: '#3a3a4a',
        }}
      >
        {!item.thumbnail_url && '⟨ § ⟩'}
      </div>

      {/* title · clickable */}
      <a
        href={target}
        style={{
          fontSize: '1rem',
          fontWeight: 600,
          color: '#e6e6f0',
          marginBottom: '0.35rem',
          lineHeight: 1.3,
          textDecoration: 'none',
        }}
      >
        {item.title}
      </a>

      {/* blurb · 2-line clamp */}
      <p
        style={{
          fontSize: '0.82rem',
          color: '#a0a0b0',
          margin: '0 0 0.65rem',
          lineHeight: 1.45,
          display: '-webkit-box',
          WebkitLineClamp: 2,
          WebkitBoxOrient: 'vertical',
          overflow: 'hidden',
        }}
      >
        {item.blurb}
      </p>

      {/* tags · max-3 + rest counter */}
      {item.tags.length > 0 && (
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: '0.3rem', marginBottom: '0.65rem' }}>
          {item.tags.slice(0, 3).map((tag) => (
            <span
              key={tag}
              style={{
                fontSize: '0.7rem',
                padding: '0.1rem 0.45rem',
                background: 'rgba(125,211,252,0.08)',
                color: '#7dd3fc',
                borderRadius: 3,
              }}
            >
              {tag}
            </span>
          ))}
          {item.tags.length > 3 && (
            <span style={{ fontSize: '0.7rem', color: '#5a5a6a' }}>
              +{item.tags.length - 3}
            </span>
          )}
        </div>
      )}

      {/* footer · author + revocability + rating + time */}
      <div
        style={{
          marginTop: 'auto',
          display: 'flex',
          flexWrap: 'wrap',
          alignItems: 'center',
          justifyContent: 'space-between',
          gap: '0.4rem',
          fontSize: '0.72rem',
          color: '#7a7a8c',
          paddingTop: '0.6rem',
          borderTop: '1px solid #14141d',
        }}
      >
        <span style={{ display: 'inline-flex', alignItems: 'center', gap: '0.3rem' }}>
          <span style={{ color: '#a0a0b0' }}>{displayAuthor(item)}</span>
          <a
            href="/docs/sovereign-cap"
            aria-label="author revocable-pubkey · cap-revoke flow"
            title="Σ-mask : author cap is revocable · click for revoke flow"
            style={{
              color: '#c084fc',
              fontSize: '0.85em',
              padding: '0 0.2rem',
              textDecoration: 'none',
            }}
          >
            ⊘
          </a>
        </span>
        <span style={{ display: 'inline-flex', alignItems: 'center', gap: '0.5rem' }}>
          <span>{ratingDisplay}</span>
          <span aria-hidden="true">·</span>
          <span>{timeAgo(item.published_at)}</span>
        </span>
      </div>

      {/* rationale · "why am I seeing this?" */}
      {showRationale && item.rationale && (
        <details
          style={{
            marginTop: '0.5rem',
            fontSize: '0.7rem',
            color: '#7a7a8c',
            borderTop: '1px dashed #1a1a26',
            paddingTop: '0.5rem',
          }}
        >
          <summary
            style={{
              cursor: 'pointer',
              color: '#a78bfa',
              listStyle: 'none',
            }}
          >
            ◐ why am I seeing this?
          </summary>
          <p style={{ margin: '0.4rem 0 0', lineHeight: 1.5 }}>
            <strong style={{ color: '#c084fc' }}>{item.rationale.kind}</strong>{' '}
            {item.rationale.kan_axis ? (
              <span style={{ color: '#7dd3fc' }}>· axis={item.rationale.kan_axis}</span>
            ) : null}
            <br />
            {item.rationale.explanation}
          </p>
        </details>
      )}
    </article>
  );
};

export default ContentCard;
