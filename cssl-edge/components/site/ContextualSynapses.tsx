import Link from 'next/link';
import { useMemo } from 'react';

import {
  findPublicSurfaceNodeForPath,
  getPublicSurfaceNode,
  getPublicSurfaceRelations,
  type PublicSurfaceRelation,
} from '../../lib/public-surface-graph';

const PRIORITY = ['codex-apockalypsis', 'words', 'conversations', 'sigils', 'spellcraft', 'akashic-records', 'clearing', 'quests', 'chaos-tarot'] as const;

function rank(relation: PublicSurfaceRelation): number {
  const index = PRIORITY.indexOf(relation.neighbor.id as typeof PRIORITY[number]);
  return index === -1 ? PRIORITY.length : index;
}

export default function ContextualSynapses({ pathname }: { pathname: string }): JSX.Element {
  const isHome = pathname === '/' || pathname === '/download/apocrypha';
  const current = isHome ? undefined : (findPublicSurfaceNodeForPath(pathname) ?? getPublicSurfaceNode('home'));
  const relations = useMemo(() => {
    if (!current) return [];
    return [...getPublicSurfaceRelations(current.id)]
      .filter((relation) => relation.neighbor.id !== 'oracle')
      .sort((left, right) => rank(left) - rank(right))
      .slice(0, 2);
  }, [current]);

  if (isHome || !current || relations.length === 0) return <></>;

  return (
    <aside className="apx-synapses" aria-labelledby="apx-synapses-title">
      <div className="apx-synapses-head">
        <div>
          <p>Keep exploring</p>
          <h2 id="apx-synapses-title">You might like these.</h2>
        </div>
        <Link href="/atlas">Browse everything →</Link>
      </div>
      <nav className="apx-synapse-list" aria-label={`Destinations connected to ${current.title}`}>
        {relations.map((relation) => {
          const item = relation.neighbor;
          const contents = (
            <>
              <span className="apx-synapse-dot" aria-hidden="true" />
              <span><strong>{item.shortTitle}</strong><small>{item.summary}</small></span>
              <i aria-hidden="true">{item.external ? '↗' : '→'}</i>
            </>
          );
          return item.external ? (
            <a key={`${relation.source}-${relation.target}`} href={item.href} target="_blank" rel="noopener noreferrer">{contents}</a>
          ) : (
            <Link key={`${relation.source}-${relation.target}`} href={item.href}>{contents}</Link>
          );
        })}
      </nav>
    </aside>
  );
}
