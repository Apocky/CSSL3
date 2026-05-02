// cssl-edge · components/PrevNextNav.tsx
// Bottom-of-page sequential navigation. Uses lib/docs-content for ordering.

import { getDocNeighbors } from '@/lib/docs-content';

interface PrevNextNavProps {
  /** Slug of the current page · drives prev/next lookup. */
  slug: string;
}

const PrevNextNav = ({ slug }: PrevNextNavProps) => {
  const { prev, next } = getDocNeighbors(slug);
  const cellStyle = {
    display: 'block',
    padding: '1rem 1.2rem',
    background: 'rgba(20, 20, 30, 0.5)',
    border: '1px solid #1f1f2a',
    borderRadius: 6,
    fontSize: '0.85rem',
    color: '#cdd6e4',
    flex: '1 1 240px',
    minWidth: 0,
  };
  return (
    <nav
      style={{
        marginTop: '3rem',
        display: 'flex',
        gap: '0.9rem',
        flexWrap: 'wrap',
        justifyContent: 'space-between',
      }}
      aria-label="Previous and next docs page"
    >
      {prev !== null ? (
        <a href={`/docs/${prev.slug}`} style={cellStyle}>
          <div style={{ fontSize: '0.7rem', color: '#7a7a8c', letterSpacing: '0.08em' }}>← previous</div>
          <div style={{ fontWeight: 600, color: '#e6e6f0', marginTop: '0.25rem' }}>{prev.title}</div>
        </a>
      ) : (
        <span style={{ flex: '1 1 240px' }} />
      )}
      {next !== null ? (
        <a href={`/docs/${next.slug}`} style={{ ...cellStyle, textAlign: 'right' }}>
          <div style={{ fontSize: '0.7rem', color: '#7a7a8c', letterSpacing: '0.08em' }}>next →</div>
          <div style={{ fontWeight: 600, color: '#e6e6f0', marginTop: '0.25rem' }}>{next.title}</div>
        </a>
      ) : (
        <span style={{ flex: '1 1 240px' }} />
      )}
    </nav>
  );
};

export default PrevNextNav;
