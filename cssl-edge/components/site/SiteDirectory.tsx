import Link from 'next/link';
import { useState } from 'react';
import { DIRECTORY_GROUPS, directoryGroup, findDirectoryItems } from '../../lib/site-directory';
import type { PublicSurfaceNode } from '../../lib/public-surface-graph';
import styles from '../../styles/UsefulHub.module.css';

const FEATURED = ['codex-apockalypsis', 'apocrypha', 'atlas', 'chaos-tarot'];
const PANEL_TITLES: Record<string, string> = {
  'codex-apockalypsis': 'Codex Apockalypsis',
  apocrypha: 'Talk to Apocrypha',
  atlas: 'Explore Atlas',
  'chaos-tarot': 'Enter Chaos Tarot',
  words: 'Words & meanings',
  conversations: 'Thoughts & conversations',
  'akashic-records': 'Essays & writing',
  cslv3: 'Symbols & notation',
};
const PANEL_KICKERS: Record<string, string> = {
  'codex-apockalypsis': 'The Good Book',
  apocrypha: 'A conversation of your own',
  atlas: 'Find your next stop',
  'chaos-tarot': 'Bring a question',
};

function DestinationPanel({ node }: { node: PublicSurfaceNode }): JSX.Element {
  const contents = <>
    <p className={styles.panelKicker}>{PANEL_KICKERS[node.id] ?? directoryGroup(node)}</p>
    <h3>{PANEL_TITLES[node.id] ?? node.title}</h3>
    <p className={styles.panelDescription}>{node.summary}</p>
    <span className={styles.panelAction}>{node.id === 'codex-apockalypsis' ? 'Explore the Codex' : node.action}<span aria-hidden="true">{node.external ? ' ↗' : ' →'}</span></span>
    {node.external ? <span className="sr-only">Opens another website in a new tab.</span> : null}
  </>;
  return <article className={styles.destinationPanel} data-destination={node.id} data-featured={FEATURED.includes(node.id) ? 'true' : undefined}>
    {node.external
      ? <a className={styles.panelBody} href={node.href} target="_blank" rel="noopener noreferrer">{contents}</a>
      : <Link className={styles.panelBody} href={node.href}>{contents}</Link>}
    {node.id === 'codex-apockalypsis' ? <a className={styles.panelSecondary} href="/codex-apockalypsis/library/novel-volume-01-01-before-anyone-asked">Begin the opening chapter <span aria-hidden="true">→</span></a> : null}
  </article>;
}

export default function SiteDirectory(): JSX.Element {
  const [query, setQuery] = useState('');
  const nodes = findDirectoryItems(query);
  const featured = FEATURED.flatMap(id => nodes.filter(node => node.id === id));
  return <section className={styles.directory} id="everything" aria-labelledby="directory-title">
    <div className={styles.sectionHeading}>
      <h2 id="directory-title">Choose your next page.</h2>
      <label className={styles.search}><span className="sr-only">Find a tool, word, or idea</span><span aria-hidden="true">⌕</span><input type="search" placeholder="Find a page…" value={query} onChange={event => setQuery(event.target.value)} /></label>
    </div>
    <p className="sr-only" role="status">{nodes.length} {nodes.length === 1 ? 'destination' : 'destinations'} found</p>
    {nodes.length ? <>
      {featured.length ? <div className={styles.panelGrid} aria-label="Read, talk, and explore">{featured.map(node => <DestinationPanel key={node.id} node={node} />)}</div> : null}
      {DIRECTORY_GROUPS.map(group => {
        const items = nodes.filter(node => !FEATURED.includes(node.id) && directoryGroup(node) === group);
        return items.length ? <section className={styles.panelGroup} key={group} aria-label={group}>
          <h2>{group}</h2><div className={styles.panelGrid}>{items.map(node => <DestinationPanel key={node.id} node={node} />)}</div>
        </section> : null;
      })}
    </> : <div className={styles.empty}><p>No match for “{query}”. Try “sigil”, “story”, “tarot”, or “meaning”.</p><button type="button" onClick={() => setQuery('')}>Show every panel</button></div>}
  </section>;
}
