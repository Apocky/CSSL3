// § hub.index : ∀ public.destination → one dense row
//
// This was 33 cards at 236px minimum, which made the front page 6,812px — seven and a half
// screens to find out what exists here. A directory whose job is "show me everything" cannot be
// something you scroll for seven screens. So every destination is now one row: name, what it is,
// what you do there. Same information, no destination privileged over another, roughly a third
// of the height.

import Link from 'next/link';
import { useState } from 'react';
import { DIRECTORY_GROUPS, directoryGroup, findDirectoryItems } from '../../lib/site-directory';
import type { PublicSurfaceNode } from '../../lib/public-surface-graph';
import styles from '../../styles/UsefulHub.module.css';

// Not "better" destinations — the four most people arrive wanting. They lead, then everything
// else follows in the same shape.
const FEATURED = ['codex-apockalypsis', 'apocrypha', 'atlas', 'chaos-tarot'];

// A deeper entry worth one click from home rather than two.
const DIRECT_CHAPTER = '/codex-apockalypsis/library/novel-volume-01-01-before-anyone-asked';

function DestinationRow({ node }: { node: PublicSurfaceNode }): JSX.Element {
  const contents = <>
    <span className={styles.rowName}>{node.title}</span>
    <span className={styles.rowSummary}>{node.summary}</span>
    <span className={styles.rowAction}>
      {node.action}<span aria-hidden="true">{node.external ? ' ↗' : ' →'}</span>
    </span>
    {node.external ? <span className="sr-only">Opens another website in a new tab.</span> : null}
  </>;
  return <li className={styles.row} data-destination={node.id}>
    {node.external
      ? <a className={styles.rowLink} href={node.href} target="_blank" rel="noopener noreferrer">{contents}</a>
      : <Link className={styles.rowLink} href={node.href}>{contents}</Link>}
    {node.id === 'codex-apockalypsis'
      ? <a className={styles.rowExtra} href={DIRECT_CHAPTER}>Begin the opening chapter <span aria-hidden="true">→</span></a>
      : null}
  </li>;
}

export default function SiteDirectory(): JSX.Element {
  const [query, setQuery] = useState('');
  const nodes = findDirectoryItems(query);
  const featured = FEATURED.flatMap(id => nodes.filter(node => node.id === id));
  return <section className={styles.directory} id="everything" aria-labelledby="directory-title">
    <div className={styles.sectionHeading}>
      <h2 id="directory-title">Everything here.</h2>
      <label className={styles.search}>
        <span className="sr-only">Find a tool, word, or idea</span>
        <span aria-hidden="true">⌕</span>
        <input type="search" placeholder="Find a page…" value={query} onChange={event => setQuery(event.target.value)} />
      </label>
    </div>
    {/* Visible, not only announced: the count is how you know a search actually narrowed it. */}
    <p className={styles.count} role="status">
      {nodes.length} {nodes.length === 1 ? 'place' : 'places'}{query ? ` matching “${query}”` : ''}
    </p>
    {nodes.length ? <>
      {featured.length ? <section className={styles.group} aria-label="Start here">
        <h3>Start here</h3>
        <ul className={styles.rows}>{featured.map(node => <DestinationRow key={node.id} node={node} />)}</ul>
      </section> : null}
      {DIRECTORY_GROUPS.map(group => {
        const items = nodes.filter(node => !FEATURED.includes(node.id) && directoryGroup(node) === group);
        return items.length ? <section className={styles.group} key={group} aria-label={group}>
          <h3>{group}</h3>
          <ul className={styles.rows}>{items.map(node => <DestinationRow key={node.id} node={node} />)}</ul>
        </section> : null;
      })}
    </> : <div className={styles.empty}>
      <p>No match for “{query}”. Try “sigil”, “story”, “tarot”, or “meaning”.</p>
      <button type="button" onClick={() => setQuery('')}>Show everything</button>
    </div>}
  </section>;
}
