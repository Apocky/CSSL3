import Link from 'next/link';
import { useMemo } from 'react';

import {
  findPublicSurfaceNodeForPath,
  getPublicSurfaceNode,
  getPublicSurfaceRelations,
  type PublicSurfaceRelation,
} from '../../lib/public-surface-graph';

const PRIORITY = ['chaos-tarot', 'oracle', 'spellcraft', 'sigils', 'atlas', 'membership', 'quests', 'akashic-records', 'clearing', 'status'] as const;

function rank(relation: PublicSurfaceRelation): number {
  const index = PRIORITY.indexOf(relation.neighbor.id as typeof PRIORITY[number]);
  return index === -1 ? PRIORITY.length : index;
}

export default function ContextualSynapses({ pathname }: { pathname: string }): JSX.Element {
  const current = findPublicSurfaceNodeForPath(pathname) ?? getPublicSurfaceNode('home');
  const relations = useMemo(() => {
    if (!current) return [];
    return [...getPublicSurfaceRelations(current.id)].sort((left, right) => rank(left) - rank(right)).slice(0, 5);
  }, [current]);

  if (!current || relations.length === 0) return <></>;

  return (
    <aside className="apx-synapses" aria-labelledby="apx-synapses-title">
      <div className="apx-synapses-head">
        <div>
          <p>CONTEXTUAL SYNAPSES</p>
          <h2 id="apx-synapses-title">From {current.shortTitle}, continue through…</h2>
        </div>
        <Link href="/atlas">Open the full map →</Link>
      </div>
      <nav className="apx-synapse-list" aria-label={`Destinations connected to ${current.title}`}>
        {relations.map((relation) => {
          const item = relation.neighbor;
          const contents = (
            <>
              <span className="apx-synapse-dot" aria-hidden="true" />
              <span><strong>{item.shortTitle}</strong><small>{relation.statement}</small></span>
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
