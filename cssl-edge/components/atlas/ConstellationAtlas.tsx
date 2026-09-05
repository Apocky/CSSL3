import Link from 'next/link';
import { useRouter } from 'next/router';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import RecoveryPanel from '../RecoveryPanel';
import {
  filterPublicGlossary,
} from '../../lib/public-glossary';
import {
  filterPublicSurfaceNodes,
  getPublicSurfaceNode,
  getPublicSurfaceRelations,
  PUBLIC_SURFACE_AVAILABILITY_LABELS,
  PUBLIC_SURFACE_AXES,
  PUBLIC_SURFACE_EDGES,
  PUBLIC_SURFACE_KIND_LABELS,
  PUBLIC_SURFACE_NODES,
  type PublicSurfaceAvailability,
  type PublicSurfaceAxis,
  type PublicSurfaceId,
  type PublicSurfaceKind,
  type PublicSurfaceNode,
} from '../../lib/public-surface-graph';
import { ATLAS_EMPTY_STATE } from '../../lib/public-ui-state';
import styles from './ConstellationAtlas.module.css';

type AtlasView = 'map' | 'matrix' | 'index' | 'dictionary';
type AxisFilter = PublicSurfaceAxis | 'all';
type KindFilter = PublicSurfaceKind | 'all';
type AvailabilityFilter = PublicSurfaceAvailability | 'all';

const VIEWS: readonly { readonly id: AtlasView; readonly label: string; readonly description: string }[] = [
  { id: 'index', label: 'Directory', description: 'Find a tool, a story, or an idea to use.' },
  { id: 'map', label: 'Map', description: 'See how the pages connect.' },
  { id: 'matrix', label: 'Compare', description: 'Compare the kinds of pages and how to use them.' },
  { id: 'dictionary', label: 'Definitions', description: 'Look up a word or symbol.' },
];

const KIND_OPTIONS = (Object.entries(PUBLIC_SURFACE_KIND_LABELS) as [PublicSurfaceKind, string][])
  .filter(([kind]) => PUBLIC_SURFACE_NODES.some((node) => node.kind === kind));
const AVAILABILITY_OPTIONS = (Object.entries(PUBLIC_SURFACE_AVAILABILITY_LABELS) as [PublicSurfaceAvailability, string][])
  .filter(([availability]) => PUBLIC_SURFACE_NODES.some((node) => node.availability === availability));

function firstQueryValue(value: string | readonly string[] | undefined): string | undefined {
  return Array.isArray(value) ? value[0] : value;
}

function isView(value: string | undefined): value is AtlasView {
  return VIEWS.some((view) => view.id === value);
}

function isAxis(value: string | undefined): value is PublicSurfaceAxis {
  return PUBLIC_SURFACE_AXES.some((axis) => axis === value);
}

function isKind(value: string | undefined): value is PublicSurfaceKind {
  return KIND_OPTIONS.some(([kind]) => kind === value);
}

function isAvailability(value: string | undefined): value is PublicSurfaceAvailability {
  return AVAILABILITY_OPTIONS.some(([availability]) => availability === value);
}

function DestinationLink({ node, className, labelClassName }: { node: PublicSurfaceNode; className?: string; labelClassName?: string }): JSX.Element {
  const contents = (
    <>
      {labelClassName ? <span className={labelClassName}>{node.action}</span> : node.action}
      <span aria-hidden="true"> {node.external ? '↗' : '→'}</span>
      {node.external ? <span className={styles.srOnly}> (opens in a new tab)</span> : null}
    </>
  );

  return node.external ? (
    <a className={className} href={node.href} target="_blank" rel="noopener noreferrer">{contents}</a>
  ) : (
    <Link className={className} href={node.href}>{contents}</Link>
  );
}

function AvailabilityBadge({ node }: { node: PublicSurfaceNode }): JSX.Element {
  return (
    <span className={styles.availability} data-availability={node.availability}>
      {PUBLIC_SURFACE_AVAILABILITY_LABELS[node.availability]}
    </span>
  );
}

function CoordinateList({ node, compact = false }: { node: PublicSurfaceNode; compact?: boolean }): JSX.Element {
  return (
    <dl className={compact ? styles.coordinateListCompact : styles.coordinateList}>
      {PUBLIC_SURFACE_AXES.map((axis) => (
        <div key={axis}>
          <dt>{axis}</dt>
          <dd>{node.coordinates[axis]}</dd>
        </div>
      ))}
    </dl>
  );
}

interface PositionedNode {
  readonly node: PublicSurfaceNode;
  readonly x: number;
  readonly y: number;
}

function positionNodes(nodes: readonly PublicSurfaceNode[]): readonly PositionedNode[] {
  if (nodes.length === 1 && nodes[0]) return [{ node: nodes[0], x: 480, y: 320 }];

  const center = nodes.find((node) => node.id === 'atlas');
  const orbit = nodes.filter((node) => node.id !== center?.id);
  const positioned: PositionedNode[] = center ? [{ node: center, x: 480, y: 320 }] : [];

  const addRing = (
    ring: readonly PublicSurfaceNode[],
    radiusX: number,
    radiusY: number,
    phase = 0,
  ): void => {
    ring.forEach((node, index) => {
      const angle = (-Math.PI / 2) + phase + ((Math.PI * 2 * index) / Math.max(ring.length, 1));
      // Keep SVG attributes byte-stable across Node and browser Math implementations.
      const roundCoordinate = (value: number): number => Math.round(value * 1_000) / 1_000;
      positioned.push({
        node,
        x: roundCoordinate(480 + (Math.cos(angle) * radiusX)),
        y: roundCoordinate(320 + (Math.sin(angle) * radiusY)),
      });
    });
  };

  if (orbit.length <= 16) {
    addRing(orbit, 340, 235);
  } else {
    const innerCount = Math.round(orbit.length / 3);
    const inner = orbit.slice(0, innerCount);
    const outer = orbit.slice(innerCount);
    addRing(inner, 160, 125);
    addRing(outer, 350, 235, Math.PI / Math.max(outer.length, 1));
  }

  return positioned;
}

function MapView({
  nodes,
  selected,
  onSelect,
}: {
  nodes: readonly PublicSurfaceNode[];
  selected: PublicSurfaceNode;
  onSelect: (id: PublicSurfaceId) => void;
}): JSX.Element {
  const mapStageRef = useRef<HTMLDivElement>(null);
  const positioned = positionNodes(nodes);
  const positions = new Map(positioned.map((item) => [item.node.id, item]));
  const visibleEdges = PUBLIC_SURFACE_EDGES.flatMap((edge) => {
    const source = positions.get(edge.source);
    const target = positions.get(edge.target);
    return source && target ? [{ edge, source, target }] : [];
  });
  const visibleNodeIds = new Set(nodes.map((node) => node.id));
  const relations = getPublicSurfaceRelations(selected.id).filter((relation) => visibleNodeIds.has(relation.neighbor.id));

  useEffect(() => {
    const stage = mapStageRef.current;
    if (!stage || stage.scrollWidth <= stage.clientWidth) return undefined;
    const frame = window.requestAnimationFrame(() => {
      stage.scrollLeft = Math.max(0, (stage.scrollWidth - stage.clientWidth) / 2);
    });
    return () => window.cancelAnimationFrame(frame);
  }, [nodes.length]);

  return (
    <div className={styles.mapLayout}>
      <div className={styles.mapColumn}>
        <div ref={mapStageRef} className={styles.mapStage} data-testid="constellation-map">
          <svg
            className={styles.mapSvg}
            viewBox="0 0 960 640"
            role="group"
            aria-labelledby="constellation-title constellation-description"
          >
            <title id="constellation-title">Public Apocky constellation</title>
            <desc id="constellation-description">
              An interactive map of the same public destinations and explicit relationships listed below it.
            </desc>
            <defs>
              <radialGradient id="atlas-node-fill">
                <stop offset="0%" stopColor="#6f7cff" stopOpacity="0.72" />
                <stop offset="100%" stopColor="#17152b" stopOpacity="0.98" />
              </radialGradient>
            </defs>
            <g aria-hidden="true">
              {visibleEdges.map(({ edge, source, target }) => {
                const active = edge.source === selected.id || edge.target === selected.id;
                return (
                  <line
                    key={`${edge.source}-${edge.target}`}
                    className={active ? styles.edgeActive : styles.edge}
                    x1={source.x}
                    y1={source.y}
                    x2={target.x}
                    y2={target.y}
                  />
                );
              })}
            </g>
            {positioned.map(({ node, x, y }) => (
              <g
                key={node.id}
                className={styles.mapNode}
                data-selected={node.id === selected.id ? 'true' : undefined}
                data-external={node.external ? 'true' : undefined}
                data-testid={`atlas-map-node-${node.id}`}
                transform={`translate(${x} ${y})`}
                role="button"
                tabIndex={0}
                aria-label={`Select ${node.title}`}
                aria-pressed={node.id === selected.id}
                onClick={() => onSelect(node.id)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' || event.key === ' ') {
                    event.preventDefault();
                    onSelect(node.id);
                  }
                }}
              >
                <circle className={styles.nodeHalo} r="40" />
                <circle className={styles.nodeCore} r={node.id === 'atlas' ? 29 : 24} />
                <text className={styles.nodeLabel} y={node.id === 'atlas' ? 54 : 49} textAnchor="middle" aria-hidden="true">
                  {node.shortTitle}
                </text>
              </g>
            ))}
          </svg>
        </div>
        <p className={styles.mapPanHint}>Swipe or drag the starfield to scan it. Every destination is also listed below.</p>
      </div>

      <article className={styles.detailCard} aria-live="polite" aria-atomic="true" data-testid="atlas-selection">
        <p className={styles.eyebrow}>{selected.eyebrow}</p>
        <h2>{selected.title}</h2>
        <AvailabilityBadge node={selected} />
        <p className={styles.detailSummary}>{selected.summary}</p>
        <CoordinateList node={selected} />

        <section className={styles.relations} aria-labelledby="selected-relations-title">
          <h3 id="selected-relations-title">Visible relationships ({relations.length})</h3>
          <ul>
            {relations.map((relation) => (
              <li key={`${relation.source}-${relation.target}`}>
                <button type="button" onClick={() => onSelect(relation.neighbor.id)}>
                  {relation.neighbor.title}
                </button>
                <span>{relation.statement}</span>
              </li>
            ))}
            {relations.length === 0 ? <li><span>No related destinations are visible under the current filters.</span></li> : null}
          </ul>
        </section>

        {selected.external ? (
          <p className={styles.externalNotice}>This link leaves apocky.com. Nothing is sent there unless you choose the handoff.</p>
        ) : null}
        <DestinationLink node={selected} className={styles.primaryLink} />
      </article>

      {/* Sibling of the stage so the grid can place it under the map on wide screens and after the detail card on narrow ones. */}
      <div className={styles.mapKey}>
        <p className={styles.microcopy}>Readable map key</p>
        <ul aria-label="Atlas destinations">
          {nodes.map((node) => (
            <li key={node.id} data-selected={node.id === selected.id ? 'true' : undefined}>
              <button
                type="button"
                onClick={() => onSelect(node.id)}
                aria-pressed={node.id === selected.id}
                data-testid={`atlas-node-${node.id}`}
              >
                <span>{node.title}</span>
                <small>{PUBLIC_SURFACE_KIND_LABELS[node.kind]}</small>
              </button>
              <DestinationLink node={node} className={styles.mapKeyLink} labelClassName={styles.mapKeyLinkText} />
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}

function IndexView({ nodes }: { nodes: readonly PublicSurfaceNode[] }): JSX.Element {
  const priority: readonly string[] = ['sigils', 'spellcraft', 'apocrypha', 'codex-apockalypsis', 'words', 'akashic-records', 'spellbook', 'conversations', 'chaos-tarot'];
  const ordered = [...nodes].sort((left, right) => {
    const rank = (id: string): number => priority.includes(id) ? priority.indexOf(id) : priority.length;
    return rank(left.id) - rank(right.id) || left.title.localeCompare(right.title);
  });
  return (
    <ul className={styles.indexGrid} aria-label="Matching public destinations">
      {ordered.map((node) => (
        <li key={node.id}>
          <article className={styles.indexCard} data-testid={`atlas-index-node-${node.id}`}>
            <div className={styles.cardHeading}>
              <div>
                <h2>{node.title}</h2>
              </div>
              <span className={styles.kind}>{PUBLIC_SURFACE_KIND_LABELS[node.kind]}</span>
            </div>
            <p>{node.summary}</p>
            {node.availability !== 'public' ? <AvailabilityBadge node={node} /> : null}
            <DestinationLink node={node} className={styles.cardLink} />
          </article>
        </li>
      ))}
    </ul>
  );
}

function MatrixView({ nodes }: { nodes: readonly PublicSurfaceNode[] }): JSX.Element {
  const kinds = KIND_OPTIONS.filter(([kind]) => nodes.some((node) => node.kind === kind));
  const states = AVAILABILITY_OPTIONS.filter(([state]) => nodes.some((node) => node.availability === state));

  return (
    <div className={styles.matrixWrap}>
      <table className={styles.matrix} aria-label="Public destinations by kind and access state">
        <caption>Compare kinds of pages and how you can use them.</caption>
        <thead>
          <tr>
            <th scope="col">Kind ↓ / access →</th>
            {states.map(([state, label]) => <th scope="col" key={state}>{label}</th>)}
          </tr>
        </thead>
        <tbody>
          {kinds.map(([kind, label]) => (
            <tr key={kind}>
              <th scope="row">{label}</th>
              {states.map(([state]) => {
                const occupants = nodes.filter((node) => node.kind === kind && node.availability === state);
                return (
                  <td key={state} data-count={occupants.length}>
                    {occupants.length > 0 ? (
                      <ul>
                        {occupants.map((node) => (
                          <li key={node.id}>
                            {node.external ? (
                              <a href={node.href} target="_blank" rel="noopener noreferrer">{node.shortTitle}<span className={styles.srOnly}> (opens in a new tab)</span></a>
                            ) : (
                              <Link href={node.href}>{node.shortTitle}</Link>
                            )}
                            <span>{node.axes.join(' · ')}</span>
                          </li>
                        ))}
                      </ul>
                    ) : <span className={styles.matrixEmpty} aria-label="No matching destinations">—</span>}
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function DictionaryView({ query, onReset }: { query: string; onReset: () => void }): JSX.Element {
  const glossary = useMemo(() => filterPublicGlossary(query), [query]);
  const count = glossary.terms.length + glossary.symbols.length;

  if (count === 0) {
    return <RecoveryPanel state={ATLAS_EMPTY_STATE} onReset={onReset} resetLabel="Clear dictionary search" className={styles.recovery} />;
  }

  return (
    <div className={styles.dictionary}>
      <p className={styles.resultStatus} role="status" aria-live="polite">
        {count} {count === 1 ? 'definition' : 'definitions'} shown
      </p>
      {glossary.terms.length > 0 ? (
        <section aria-labelledby="atlas-terms-title">
          <div className={styles.sectionHeading}>
            <div>
              <p className={styles.eyebrow}>Plain language</p>
              <h2 id="atlas-terms-title">Words and abbreviations</h2>
            </div>
            <Link href="/words#technical-terms">Open the standalone reference <span aria-hidden="true">→</span></Link>
          </div>
          <dl className={styles.definitionGrid}>
            {glossary.terms.map(({ id, term, meaning }) => (
              <div key={id}>
                <dt>{term}</dt>
                <dd>{meaning}</dd>
              </div>
            ))}
          </dl>
        </section>
      ) : null}

      {glossary.symbols.length > 0 ? (
        <section aria-labelledby="atlas-symbols-title">
          <div className={styles.sectionHeading}>
            <div>
              <p className={styles.eyebrow}>Notation key</p>
              <h2 id="atlas-symbols-title">Symbols</h2>
            </div>
          </div>
          <dl className={styles.symbolGrid}>
            {glossary.symbols.map(({ id, symbol, meaning }) => (
              <div key={id}>
                <dt>{symbol}</dt>
                <dd>{meaning}</dd>
              </div>
            ))}
          </dl>
        </section>
      ) : null}
    </div>
  );
}

export default function ConstellationAtlas(): JSX.Element {
  const router = useRouter();
  const [view, setView] = useState<AtlasView>('index');
  const [query, setQuery] = useState('');
  const [axis, setAxis] = useState<AxisFilter>('all');
  const [kind, setKind] = useState<KindFilter>('all');
  const [availability, setAvailability] = useState<AvailabilityFilter>('all');
  const [selectedId, setSelectedId] = useState<PublicSurfaceId>('atlas');

  useEffect(() => {
    if (!router.isReady) return;
    const urlView = firstQueryValue(router.query.view);
    const urlQuery = firstQueryValue(router.query.q);
    const urlAxis = firstQueryValue(router.query.axis);
    const urlKind = firstQueryValue(router.query.kind);
    const urlAvailability = firstQueryValue(router.query.state);
    const urlNode = firstQueryValue(router.query.node);

    // The URL is the shareable source of truth. Missing or invalid values
    // restore the documented defaults instead of leaving state behind from a
    // previous shallow navigation to this same page.
    setView(isView(urlView) ? urlView : 'index');
    setQuery(typeof urlQuery === 'string' ? urlQuery : '');
    setAxis(isAxis(urlAxis) ? urlAxis : 'all');
    setKind(isKind(urlKind) ? urlKind : 'all');
    setAvailability(isAvailability(urlAvailability) ? urlAvailability : 'all');
    const selected = urlNode ? getPublicSurfaceNode(urlNode) : undefined;
    setSelectedId(selected?.id ?? 'atlas');
  }, [router.isReady, router.query]);

  const replaceUrl = useCallback((next: {
    readonly view?: AtlasView;
    readonly query?: string;
    readonly axis?: AxisFilter;
    readonly kind?: KindFilter;
    readonly availability?: AvailabilityFilter;
    readonly selectedId?: PublicSurfaceId;
  }) => {
    const nextView = next.view ?? view;
    const nextQuery = next.query ?? query;
    const nextAxis = next.axis ?? axis;
    const nextKind = next.kind ?? kind;
    const nextAvailability = next.availability ?? availability;
    const nextSelectedId = next.selectedId ?? selectedId;
    const urlQuery: Record<string, string> = {};

    if (nextView !== 'index') urlQuery.view = nextView;
    if (nextQuery.trim()) urlQuery.q = nextQuery.trim();
    if (nextAxis !== 'all') urlQuery.axis = nextAxis;
    if (nextKind !== 'all') urlQuery.kind = nextKind;
    if (nextAvailability !== 'all') urlQuery.state = nextAvailability;
    if (nextSelectedId !== 'atlas') urlQuery.node = nextSelectedId;

    void router.replace({ pathname: '/atlas', query: urlQuery }, undefined, { shallow: true, scroll: false });
  }, [availability, axis, kind, query, router, selectedId, view]);

  const filteredNodes = useMemo(() => filterPublicSurfaceNodes({ query, axis, kind, availability }), [availability, axis, kind, query]);
  const selectedNode = filteredNodes.find((node) => node.id === selectedId) ?? filteredNodes[0];
  const graphResultCount = filteredNodes.length;
  const dictionaryResultCount = useMemo(() => {
    const result = filterPublicGlossary(query);
    return result.terms.length + result.symbols.length;
  }, [query]);

  const resetFilters = useCallback(() => {
    setQuery('');
    setAxis('all');
    setKind('all');
    setAvailability('all');
    setSelectedId('atlas');
    replaceUrl({ query: '', axis: 'all', kind: 'all', availability: 'all', selectedId: 'atlas' });
  }, [replaceUrl]);

  const selectNode = useCallback((id: PublicSurfaceId) => {
    setSelectedId(id);
    replaceUrl({ selectedId: id });
  }, [replaceUrl]);

  const showEmptyGraph = view !== 'dictionary' && graphResultCount === 0;

  return (
    <main className={styles.root}>
      <header className={styles.directoryHeader}><p className={styles.directoryEyebrow}>All of Apocky</p><h1>Find something useful.</h1><p>Tools to try, words to understand, and ideas worth following.</p></header>
      <section className={styles.explorer} aria-label="Find a page">
        <form className={styles.directorySearch} onSubmit={event => event.preventDefault()} aria-label="Filter the Atlas">
          <label className={styles.searchField}><span>{view === 'dictionary' ? 'Search definitions' : 'What are you looking for?'}</span><input type="search" value={query} placeholder={view === 'dictionary' ? 'A word or its meaning…' : 'Try sigils, stories, memory…'} onChange={event => { const nextQuery = event.currentTarget.value; setQuery(nextQuery); replaceUrl({ query: nextQuery }); }} /></label>
          {view !== 'dictionary' ? <details className={styles.additionalFilters}><summary>More filters</summary><div className={styles.filterGrid}>
            <label><span>Kind of page</span><select value={kind} onChange={event => { const value = event.currentTarget.value as KindFilter; setKind(value); replaceUrl({ kind: value }); }}><option value="all">Everything</option>{KIND_OPTIONS.map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
            <label><span>How you can use it</span><select value={availability} onChange={event => { const value = event.currentTarget.value as AvailabilityFilter; setAvailability(value); replaceUrl({ availability: value }); }}><option value="all">Any</option>{AVAILABILITY_OPTIONS.map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
            <label><span>Related to</span><select value={axis} onChange={event => { const value = event.currentTarget.value as AxisFilter; setAxis(value); replaceUrl({ axis: value }); }}><option value="all">Any subject</option>{PUBLIC_SURFACE_AXES.map(item => <option key={item} value={item}>{item}</option>)}</select></label>
          </div></details> : null}
        </form>
        <div className={styles.directoryToolbar}>
          <div className={styles.viewSwitch} role="group" aria-label="Atlas view">{VIEWS.map(item => <button key={item.id} type="button" aria-pressed={view === item.id} onClick={() => { setView(item.id); replaceUrl({ view: item.id }); }}>{item.label}</button>)}</div>
          <div className={styles.filterFooter}><p className={styles.resultStatus} role="status" aria-live="polite" aria-atomic="true">{view === 'dictionary' ? dictionaryResultCount + (dictionaryResultCount === 1 ? ' definition' : ' definitions') : graphResultCount + (graphResultCount === 1 ? ' place to go' : ' places to go')}</p>{query || axis !== 'all' || kind !== 'all' || availability !== 'all' ? <button type="button" className={styles.resetButton} onClick={resetFilters}>Clear filters</button> : null}</div>
        </div>
        <noscript><p className={styles.noScript}>You can follow every directory link without JavaScript. Turn it on to search and change views.</p></noscript>
        <div className={styles.viewPanel} data-view={view}>
          {showEmptyGraph ? <div className={styles.emptyDirectory} role="status"><h2>No pages found.</h2><p>Try a shorter word or clear the filters to see everything.</p><button type="button" className={styles.resetButton} onClick={resetFilters}>Clear filters</button></div> : null}
          {!showEmptyGraph && view === 'map' && selectedNode ? <MapView nodes={filteredNodes} selected={selectedNode} onSelect={selectNode} /> : null}
          {!showEmptyGraph && view === 'matrix' ? <MatrixView nodes={filteredNodes} /> : null}
          {!showEmptyGraph && view === 'index' ? <IndexView nodes={filteredNodes} /> : null}
          {view === 'dictionary' ? <DictionaryView query={query} onReset={resetFilters} /> : null}
        </div>
        {view === 'map' ? <details className={styles.additionalFilters}><summary>About this map</summary><p>Lines show links between published pages. A connection here does not mean two ideas are equivalent or that one causes the other.</p></details> : null}
      </section>
    </main>
  );
}
