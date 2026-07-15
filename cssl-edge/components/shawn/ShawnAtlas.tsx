import { useMemo, useState } from 'react';
import type {
  BridgeRecord,
  ChronologyEvent,
  EpisodeRecord,
  Lens,
  ReferenceRecord,
} from '@/lib/shawn/types';
import { atlasData } from '@/lib/shawn/atlas';
import { publicationBlockers, referenceBySlug, referenceCatalog } from '@/lib/shawn/catalog';
import { ReferenceDialogProvider, ReferenceLink } from './ReferenceSystem';
import styles from './Atlas.module.css';

const TRACKS: ReadonlyArray<{ id: ChronologyEvent['track']; label: string }> = [
  { id: 'life-context', label: 'Life / context' },
  { id: 'state-phenomenology', label: 'State / phenomenology' },
  { id: 'intellectual-artifact', label: 'Intellectual / artifact' },
];

const MODEL_AXES = [
  { claimId: 'claim-attractor', label: 'Generative identity' },
  { claimId: 'claim-method', label: 'Rotation method' },
  { claimId: 'claim-zeroes-discipline', label: 'Artifact discipline' },
  { claimId: 'claim-audience-translation', label: 'Translation' },
  { claimId: 'claim-voice-functional', label: 'Voice fidelity' },
  { claimId: 'claim-ontology-open', label: 'Open ontology' },
] as const;

function topicTitle(slug: string): string {
  return referenceBySlug(slug)?.title ?? slug.replace(/-/g, ' ');
}

function TopicLinks({ slugs }: { readonly slugs: readonly string[] }): JSX.Element | null {
  if (slugs.length === 0) return null;
  return (
    <div className={styles.topicLinks} aria-label="Related advanced topics">
      {slugs.map((slug) => (
        <ReferenceLink key={slug} slug={slug}>{topicTitle(slug)}</ReferenceLink>
      ))}
    </div>
  );
}

function SectionHeader({
  index,
  title,
  description,
}: {
  readonly index: string;
  readonly title: string;
  readonly description: string;
}): JSX.Element {
  return (
    <header className={styles.sectionHeader}>
      <span className={styles.sectionIndex}>{index}</span>
      <h2>{title}</h2>
      <p>{description}</p>
    </header>
  );
}

function ModelOverview(): JSX.Element | null {
  const [activeClaimId, setActiveClaimId] = useState<string>(MODEL_AXES[0].claimId);
  const activeClaim = atlasData.claims.find((claim) => claim.id === activeClaimId);
  if (!activeClaim) return null;

  const chronology = atlasData.chronology.filter((event) => event.claimIds.includes(activeClaim.id));
  const artifacts = atlasData.artifacts.filter((artifact) => artifact.claimIds.includes(activeClaim.id));
  const sources = activeClaim.sourceIds
    .map((sourceId) => atlasData.sourceRefs.find((source) => source.id === sourceId))
    .filter((source) => source !== undefined);

  return (
    <section className={`${styles.section} ${styles.modelOverview}`} id="model">
      <SectionHeader
        index="00 / current model"
        title="The portrait before the apparatus."
        description="This is the shortest honest path through the current inference report. Select an axis to see the claim, its epistemic status, the evidence path, the strongest countercase, and what would force revision."
      />
      <div className={styles.modelAxisSelector} aria-label="Current model axes">
        {MODEL_AXES.map((axis) => (
          <button
            type="button"
            key={axis.claimId}
            aria-pressed={axis.claimId === activeClaim.id}
            onClick={() => setActiveClaimId(axis.claimId)}
          >
            {axis.label}
          </button>
        ))}
      </div>
      <article className={styles.modelAxisPanel} aria-live="polite">
        <div className={styles.modelAxisClaim}>
          <span className={styles.stateBadge}>{activeClaim.truthState} · {activeClaim.lane} · {activeClaim.confidence}</span>
          <h3>{activeClaim.title}</h3>
          <p className={styles.modelStatement}>{activeClaim.wording}</p>
          <TopicLinks slugs={activeClaim.topicSlugs} />
        </div>
        <div className={styles.modelAxisAudit}>
          <div>
            <h4>Strongest countermodel</h4>
            <p>{activeClaim.countercase}</p>
          </div>
          <div>
            <h4>Revision condition</h4>
            <p>{activeClaim.falsifier}</p>
          </div>
        </div>
        <div className={styles.modelEvidencePath}>
          <h4>Trace this inference</h4>
          <div>
            {chronology.map((event) => <a href={`#${event.id}`} key={event.id}>Event · {event.period}</a>)}
            {artifacts.map((artifact) => <a href={`#${artifact.id}`} key={artifact.id}>Artifact · {artifact.title}</a>)}
            {sources.map((source) => <span key={source.id}>Source · {source.label}</span>)}
            {chronology.length + artifacts.length + sources.length === 0 ? <span>No public evidence path is attached.</span> : null}
          </div>
        </div>
      </article>
      <p className={styles.modelOverviewBoundary}>
        Six axes are a navigation projection, not six compartments of a person. The chronology, ordinary-life samples, state variables, artifacts, and contradictions below remain necessary to test whether this compression holds.
      </p>
    </section>
  );
}

function InterpretiveContract(): JSX.Element {
  return (
    <section className={styles.section} id="contract" aria-labelledby="contract-title">
      <SectionHeader
        index="01 / contract"
        title="Read the report before collapsing the model."
        description="Experience, artifact validity, causation, ontology, and clinical interpretation are different questions. This atlas keeps them different long enough to test them."
      />
      <div className={styles.voiceBraid} aria-label="Exact authored position braided with analysis">
        {atlasData.voiceFragments.map((fragment) => (
          <article key={fragment.id}>
            <div className={styles.voiceSource}>SHAWN · EXACT CURRENT DIRECTIVE</div>
            <blockquote>{fragment.text}</blockquote>
            <div className={styles.voiceAnalysis}>
              <h3>Scholarly reading</h3>
              <p>{fragment.analysis}</p>
              <details className={styles.detailToggle}>
                <summary>Failure boundary</summary>
                <p>{fragment.boundary}</p>
              </details>
            </div>
          </article>
        ))}
      </div>
      <h3 id="contract-title" className={styles.label}>Interpretive contract</h3>
      <div className={styles.contractGrid}>
        {atlasData.interpretiveContract.map((item, index) => (
          <article className={styles.contractItem} key={item}>
            <span>{String(index + 1).padStart(2, '0')}</span>
            <p>{item}</p>
          </article>
        ))}
      </div>
    </section>
  );
}

function Chronology(): JSX.Element {
  const [activeTracks, setActiveTracks] = useState<ReadonlySet<ChronologyEvent['track']>>(
    () => new Set(TRACKS.map((track) => track.id)),
  );
  const visible = atlasData.chronology.filter((event) => activeTracks.has(event.track));

  const toggleTrack = (track: ChronologyEvent['track']): void => {
    setActiveTracks((previous) => {
      const next = new Set(previous);
      if (next.has(track)) {
        if (next.size > 1) next.delete(track);
      } else {
        next.add(track);
      }
      return next;
    });
  };

  return (
    <section className={styles.section} id="chronology">
      <SectionHeader
        index="02 / chronology"
        title="The event spine."
        description="Three synchronized tracks distinguish recorded context, lived state, and artifact development. Dates express the precision the source supports—not retrospective certainty."
      />
      <div className={styles.chronologyControls} aria-label="Chronology tracks">
        {TRACKS.map((track) => (
          <button
            type="button"
            className={styles.filterButton}
            key={track.id}
            aria-pressed={activeTracks.has(track.id)}
            onClick={() => toggleTrack(track.id)}
          >
            {track.label}
          </button>
        ))}
      </div>
      <div className={styles.trackCoverage} aria-label="Published chronology coverage">
        {TRACKS.map((track) => {
          const count = atlasData.chronology.filter((event) => event.track === track.id).length;
          return (
            <p key={track.id}>
              <strong>{track.label}</strong>
              <span>{count > 0 ? `${count} public event${count === 1 ? '' : 's'}` : 'Public events pending source and excerpt approval'}</span>
            </p>
          );
        })}
      </div>
      <div className={styles.chronology} aria-live="polite">
        {visible.length === 0 ? (
          <p className={styles.emptyState}>No published events currently occupy the selected tracks.</p>
        ) : null}
        {visible.map((event) => (
          <article
            className={`${styles.chronologyEvent} ${styles[`track_${event.track.replace(/-/g, '_')}`] ?? ''}`}
            id={event.id}
            key={event.id}
          >
            <time className={styles.eventPeriod}>{event.period}</time>
            <span className={styles.eventNode} aria-hidden="true" />
            <div className={styles.eventCard}>
              <span className={styles.trackLabel}>{event.track.replace(/-/g, ' ')}</span>
              <span className={styles.laneBadge}>{event.lane}</span>
              <span className={styles.stateBadge}>{event.truthState}</span>
              <h3>{event.title}</h3>
              <p>{event.summary}</p>
              <TopicLinks slugs={event.topicSlugs} />
            </div>
          </article>
        ))}
      </div>
    </section>
  );
}

function ReasoningChains(): JSX.Element {
  return (
    <section className={styles.section} id="method">
      <SectionHeader
        index="03 / method"
        title="How the field becomes an artifact."
        description="The method is not a mood-board of associations. It is a traceable sequence that must survive countermodels, implementation, and revision."
      />
      {atlasData.reasoningChains.map((chain, chainIndex) => (
        <details className={styles.chain} id={chain.id} key={chain.id} open={chainIndex === 0 ? true : undefined}>
          <summary className={styles.chainSummary}>
            <h3 className={styles.chainTitle}>{chain.title}</h3>
          </summary>
          <p className={styles.chainDescription}>{chain.summary}</p>
          <ol className={styles.chainSteps}>
            {chain.steps.map((step, index) => (
              <li key={step.id}>
                <span className={styles.chainStepNumber}>{String(index + 1).padStart(2, '0')} · {step.lane}</span>
                <h4>{step.label}</h4>
                <p>{step.description}</p>
                <TopicLinks slugs={step.topicSlugs} />
              </li>
            ))}
          </ol>
        </details>
      ))}
    </section>
  );
}

function VariableMatrix(): JSX.Element {
  return (
    <section className={styles.section} id="variables">
      <SectionHeader
        index="04 / validity"
        title="Variables, controls, and confounds."
        description="A condition is not made a symptom merely because it is unusual. It is classified by what was varied, measured, held, suspected, or left unknown."
      />
      <div className={styles.tableScroll}>
        <table className={styles.variableTable}>
          <caption className={styles.label}>Public longitudinal variable inventory</caption>
          <thead>
            <tr>
              <th scope="col">Variable</th>
              <th scope="col">Role</th>
              <th scope="col">Operational account</th>
              <th scope="col">Certainty</th>
            </tr>
          </thead>
          <tbody>
            {atlasData.variables.map((variable) => (
              <tr key={variable.id}>
                <td>{variable.label}</td>
                <td><span className={styles.rolePill}>{variable.role}</span></td>
                <td>{variable.description}</td>
                <td>{variable.certainty}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div className={styles.episodeGrid}>
        {atlasData.episodes.map((episode) => (
          <article className={styles.episodeCard} id={episode.id} key={episode.id}>
            <span className={styles.eyebrow}>{episode.period} · {episode.truthState}</span>
            <h3>{episode.title}</h3>
            <p>{episode.summary}</p>
            <dl className={styles.auditSteps}>
              <div><dt>Context</dt><dd>{episode.context}</dd></div>
              <div><dt>Observation</dt><dd>{episode.observation}</dd></div>
              <div><dt>Interpretation</dt><dd>{episode.interpretation}</dd></div>
              <div><dt>Method</dt><dd>{episode.method}</dd></div>
              <div><dt>Result</dt><dd>{episode.result}</dd></div>
              <div><dt>External check</dt><dd>{episode.externalCheck}</dd></div>
            </dl>
            <details className={styles.detailToggle}>
              <summary>Rival explanations and counterevidence</summary>
              <h4>Rival explanations</h4>
              <ul>{episode.rivalExplanations.map((item) => <li key={item}>{item}</li>)}</ul>
              <h4>Counterevidence</h4>
              <ul>{episode.counterevidence.map((item) => <li key={item}>{item}</li>)}</ul>
            </details>
            <TopicLinks slugs={episode.topicSlugs} />
          </article>
        ))}
      </div>
    </section>
  );
}

function ArtifactCases(): JSX.Element {
  const [selectedArtifactId, setSelectedArtifactId] = useState('artifact-atlas');
  const selectedArtifact = atlasData.artifacts.find((artifact) => artifact.id === selectedArtifactId) ?? atlasData.artifacts[0];
  const selectedEdges = atlasData.artifactLineage.filter(
    (edge) => edge.fromArtifactId === selectedArtifact?.id || edge.toArtifactId === selectedArtifact?.id,
  );

  return (
    <section className={styles.section} id="artifacts">
      <SectionHeader
        index="05 / artifacts"
        title="The work is part of the evidence."
        description="Each case follows the transformation from a question or experience into a durable object. Collaboration, failure boundaries, and unresolved tests remain visible."
      />
      {selectedArtifact ? (
        <div className={styles.lineageExplorer}>
          <div className={styles.artifactSelector} aria-label="Select an artifact lineage node">
            {atlasData.artifacts.map((artifact) => (
              <button
                type="button"
                key={`${artifact.id}-selector`}
                aria-pressed={artifact.id === selectedArtifact.id}
                onClick={() => setSelectedArtifactId(artifact.id)}
              >
                {artifact.title}
              </button>
            ))}
          </div>
          <article className={styles.lineagePanel} aria-live="polite">
            <span className={styles.eyebrow}>Selected lineage node</span>
            <h3>{selectedArtifact.title}</h3>
            <p>{selectedArtifact.thesis}</p>
            <div className={styles.lineageEdges}>
              {selectedEdges.length > 0 ? selectedEdges.map((edge) => {
                const outgoing = edge.fromArtifactId === selectedArtifact.id;
                const otherId = outgoing ? edge.toArtifactId : edge.fromArtifactId;
                const other = atlasData.artifacts.find((artifact) => artifact.id === otherId);
                return (
                  <button type="button" key={edge.id} onClick={() => setSelectedArtifactId(otherId)}>
                    <span>{outgoing ? 'to' : 'from'} · {edge.relation} · {edge.lane}</span>
                    <strong>{other?.title ?? otherId}</strong>
                    <small>{edge.description}</small>
                  </button>
                );
              }) : <p className={styles.emptyState}>No reviewed lineage edge is attached to this artifact yet.</p>}
            </div>
          </article>
        </div>
      ) : null}
      <div className={styles.artifactGrid}>
        {atlasData.artifacts.map((artifact) => (
          <article className={styles.artifactCard} id={artifact.id} key={artifact.id}>
            <span className={styles.artifactKind}>{artifact.kind} · {artifact.status} · {artifact.period}</span>
            <h3>{artifact.title}</h3>
            <p>{artifact.thesis}</p>
            <dl className={styles.artifactDetails}>
              <div><dt>Method</dt><dd>{artifact.method.join(' ')}</dd></div>
              <div><dt>Evidence</dt><dd>{artifact.evidence.join(' ')}</dd></div>
              <div><dt>Negative results</dt><dd>{artifact.negativeResults.join(' ')}</dd></div>
              <div><dt>Open questions</dt><dd>{artifact.openQuestions.join(' ')}</dd></div>
              <div><dt>Authorship boundary</dt><dd>{artifact.collaborationNote}</dd></div>
            </dl>
            <TopicLinks slugs={artifact.topicSlugs} />
          </article>
        ))}
      </div>
    </section>
  );
}

function LensRotation(): JSX.Element {
  const [activeId, setActiveId] = useState(atlasData.lenses[0]?.id ?? '');
  const [episodeId, setEpisodeId] = useState(atlasData.episodes[0]?.id ?? '');
  const active = atlasData.lenses.find((lens) => lens.id === activeId) ?? atlasData.lenses[0];
  const episode = atlasData.episodes.find((item) => item.id === episodeId) ?? atlasData.episodes[0];

  return (
    <section className={styles.section} id="lenses">
      <SectionHeader
        index="06 / rotation"
        title="One event. Multiple accountable lenses."
        description="Rotation is not indecision. Each lens must disclose what it can preserve, what it can test, and what it loses when used alone."
      />
      <label className={styles.lensEpisodePicker}>
        <span>Episode under rotation</span>
        <select value={episode?.id ?? ''} onChange={(event) => setEpisodeId(event.target.value)}>
          {atlasData.episodes.map((item) => <option value={item.id} key={item.id}>{item.title}</option>)}
        </select>
      </label>
      {active && episode ? (
        <div className={styles.lensLayout}>
          <div className={styles.lensSelector} aria-label="Interpretive lenses">
            {atlasData.lenses.map((lens) => (
              <button
                type="button"
                className={styles.lensButton}
                key={lens.id}
                aria-pressed={active.id === lens.id}
                onClick={() => setActiveId(lens.id)}
              >
                {lens.label}
              </button>
            ))}
          </div>
          <LensPanel lens={active} episode={episode} />
        </div>
      ) : null}
    </section>
  );
}

function LensPanel({ lens, episode }: { readonly lens: Lens; readonly episode: EpisodeRecord }): JSX.Element {
  const topics = Array.from(new Set([...episode.topicSlugs, ...lens.topicSlugs]));
  return (
    <article className={styles.lensPanel} aria-live="polite">
      <span className={styles.eyebrow}>Active lens · {episode.period}</span>
      <h3>{lens.label}</h3>
      <h4 className={styles.rotatedEpisode}>{episode.title}</h4>
      <p className={styles.lensQuestion}>{lens.question}</p>
      <div className={styles.lensColumns}>
        <div><h4>Record that survives rotation</h4><p>{episode.observation}</p></div>
        <div><h4>This lens preserves / exposes</h4><p>{lens.strength}</p></div>
        <div><h4>Candidate reading</h4><p>{episode.interpretation}</p></div>
        <div><h4>Cannot conclude from this lens</h4><p>{lens.limitation}</p></div>
      </div>
      <details className={styles.detailToggle}>
        <summary>Pressure-test this rotation</summary>
        <p><strong>Strongest rival:</strong> {episode.rivalExplanations[0] ?? 'No rival model recorded.'}</p>
        <p><strong>Counterevidence:</strong> {episode.counterevidence[0] ?? 'No counterevidence recorded.'}</p>
        <p><strong>External check:</strong> {episode.externalCheck}</p>
      </details>
      <TopicLinks slugs={topics} />
    </article>
  );
}

interface GraphNode {
  readonly label: string;
  readonly x: number;
  readonly y: number;
}

function graphProjection(bridges: readonly BridgeRecord[]): { readonly nodes: readonly GraphNode[]; readonly nodeMap: ReadonlyMap<string, GraphNode> } {
  const labels = Array.from(new Set(bridges.flatMap((bridge) => [bridge.from, bridge.to])));
  const nodes = labels.map((label, index) => {
    const angle = (Math.PI * 2 * index) / Math.max(labels.length, 1) - Math.PI / 2;
    return {
      label,
      x: 50 + Math.cos(angle) * 37,
      y: 50 + Math.sin(angle) * 35,
    };
  });
  return { nodes, nodeMap: new Map(nodes.map((node) => [node.label, node])) };
}

function BridgeNetwork(): JSX.Element {
  const projection = useMemo(() => graphProjection(atlasData.bridges), []);

  return (
    <section className={styles.section} id="bridges">
      <SectionHeader
        index="07 / bridges"
        title="Similarity is typed before it becomes authority."
        description="The constellation preserves declared adjacency. It does not preserve chronology, strength, or causality; those remain in the evidence cards below."
      />
      <figure className={styles.bridgeFigure}>
        <svg className={styles.bridgeSvg} viewBox="0 0 1000 500" role="img" aria-labelledby="bridge-map-title bridge-map-description">
          <title id="bridge-map-title">Cross-domain relationship projection</title>
          <desc id="bridge-map-description">A visual projection of the relationships listed in full beneath the graph.</desc>
          {atlasData.bridges.map((bridge) => {
            const from = projection.nodeMap.get(bridge.from);
            const to = projection.nodeMap.get(bridge.to);
            if (!from || !to) return null;
            return (
              <line
                key={bridge.id}
                x1={from.x * 10}
                y1={from.y * 5}
                x2={to.x * 10}
                y2={to.y * 5}
                data-lane={bridge.lane}
              >
                <title>{`${bridge.from} to ${bridge.to}: ${bridge.relationship}`}</title>
              </line>
            );
          })}
        </svg>
        {projection.nodes.map((node) => (
          <div
            className={styles.bridgeNode}
            style={{ left: `${node.x}%`, top: `${node.y}%` }}
            key={node.label}
            aria-hidden="true"
          >
            {node.label}
          </div>
        ))}
        <figcaption className={styles.bridgeCaption}>
          Solid lines indicate observed or self-reported relations; dashed lines remain inferred, proposed, disputed, or unknown. Read the cards for the actual claim and failure test.
        </figcaption>
      </figure>

      <div className={styles.bridgeList} aria-label="Accessible relationship list">
        {atlasData.bridges.map((bridge) => (
          <article className={styles.bridgeCard} id={bridge.id} key={bridge.id}>
            <span className={styles.relationBadge}>{bridge.relationship} · {bridge.lane} · {bridge.truthState}{bridge.quantumLane ? ` · ${bridge.quantumLane}` : ''}</span>
            <h3>{bridge.from} ↔ {bridge.to}</h3>
            <p>{bridge.statement}</p>
            <dl className={styles.claimBoundary}>
              <dt>Invariant proposed</dt><dd>{bridge.invariant}</dd>
              <dt>Prediction</dt><dd>{bridge.prediction}</dd>
              <dt>Negative-transfer test</dt><dd>{bridge.negativeTransferTest}</dd>
            </dl>
            <details className={styles.detailToggle}>
              <summary>Material differences</summary>
              <ul>{bridge.differences.map((item) => <li key={item}>{item}</li>)}</ul>
            </details>
            <TopicLinks slugs={bridge.topicSlugs} />
          </article>
        ))}
      </div>
    </section>
  );
}

function ReferenceNetwork(): JSX.Element {
  const [query, setQuery] = useState('');
  const [domain, setDomain] = useState('all');
  const domainOrder = useMemo(() => new Map<string, number>([
    ['mathematics', 0],
    ['physics', 1],
    ['geometry-topology', 2],
    ['computation-cognition', 3],
    ['modeling-methods', 4],
    ['games-simulation', 5],
    ['spirituality-esotericism', 6],
    ['philosophy-epistemology', 7],
    ['psychology-inquiry', 8],
    ['language-symbolism', 9],
    ['myth-theology-fiction', 10],
  ]), []);
  const topics = useMemo(
    () => [...referenceCatalog].sort((a, b) => (domainOrder.get(a.domain) ?? 99) - (domainOrder.get(b.domain) ?? 99) || a.title.localeCompare(b.title)),
    [domainOrder],
  );
  const domains = useMemo(() => Array.from(new Set(topics.map((topic) => topic.domain))), [topics]);
  const normalizedQuery = query.trim().toLowerCase();
  const visibleTopics = topics.filter((topic) => {
    if (domain !== 'all' && topic.domain !== domain) return false;
    if (!normalizedQuery) return true;
    return [topic.title, topic.orientation, topic.domain, ...topic.aliases]
      .some((value) => value.toLowerCase().includes(normalizedQuery));
  });

  return (
    <section className={styles.section} id="references">
      <SectionHeader
        index="08 / references"
        title="The atlas opens all the way down."
        description="Advanced terms are not prestige decoration. Each opens to prerequisites, the relevant proof or evidence mode, Shawn’s use, counterpositions, support boundaries, and primary references."
      />
      <div className={styles.referenceFilters}>
        <label>
          <span>Search the explanatory network</span>
          <input value={query} onChange={(event) => setQuery(event.target.value)} type="search" placeholder="e.g. causality, Tarot, zeta" />
        </label>
        <label>
          <span>Domain</span>
          <select value={domain} onChange={(event) => setDomain(event.target.value)}>
            <option value="all">All domains</option>
            {domains.map((item) => <option key={item} value={item}>{item.replace(/-/g, ' ')}</option>)}
          </select>
        </label>
        <p aria-live="polite">{visibleTopics.length} of {topics.length} reference nodes visible</p>
      </div>
      <div className={styles.referenceGrid}>
        {visibleTopics.map((reference: ReferenceRecord) => (
          <article className={styles.topicCard} key={reference.slug}>
            <div className={styles.topicMeta}>{reference.domain.replace(/-/g, ' ')} · {reference.role} · {reference.evidenceMode}</div>
            <h3><ReferenceLink slug={reference.slug}>{reference.title}</ReferenceLink></h3>
            <p>{reference.orientation}</p>
            <span className={styles.laneBadge}>{reference.evidence.label}</span>
          </article>
        ))}
        {visibleTopics.length === 0 ? <p className={styles.emptyState}>No reference node matches this field rotation.</p> : null}
      </div>
    </section>
  );
}

function RevisionLedger(): JSX.Element {
  const claims = [...atlasData.claims].sort((a, b) => {
    const rank = { FALSE: 0, OPEN: 1, TRUE: 2 } as const;
    return rank[a.truthState] - rank[b.truthState];
  });
  return (
    <section className={styles.section} id="revisions">
      <SectionHeader
        index="09 / revisions"
        title="A model earns trust by showing where it can break."
        description="TRUE, OPEN, and FALSE remain distinct. Confidence, countercase, and falsifier travel with every consequential claim."
      />
      <div className={styles.claimGrid}>
        {claims.map((claim) => (
          <article className={styles.claimCard} data-state={claim.truthState} id={claim.id} key={claim.id}>
            <span className={styles.stateBadge}>{claim.truthState} · {claim.lane} · {claim.confidence}</span>
            <h3>{claim.title}</h3>
            <p>{claim.wording}</p>
            <dl className={styles.claimBoundary}>
              <dt>Strongest countercase</dt><dd>{claim.countercase}</dd>
              <dt>Falsifier / revision trigger</dt><dd>{claim.falsifier}</dd>
            </dl>
            <TopicLinks slugs={claim.topicSlugs} />
          </article>
        ))}
      </div>
      <div className={styles.tableScroll}>
        <table className={styles.variableTable}>
          <caption className={styles.label}>Contradiction and supersession matrix</caption>
          <thead>
            <tr><th scope="col">Claim</th><th scope="col">Support</th><th scope="col">Counterevidence</th><th scope="col">Supersession</th></tr>
          </thead>
          <tbody>
            {claims.map((claim) => (
              <tr key={`${claim.id}-matrix`}>
                <td><a href={`#${claim.id}`}>{claim.title}</a></td>
                <td>
                  {claim.supportingCitationIds.length > 0
                    ? claim.supportingCitationIds.map((citationId) => {
                      const citation = atlasData.citations.find((item) => item.id === citationId);
                      return citation ? <ReferenceLink key={citation.id} slug={citation.referenceSlug}>{citation.relation}</ReferenceLink> : null;
                    })
                    : 'No public supporting citation'}
                </td>
                <td>
                  {claim.contradictingCitationIds.map((citationId) => {
                    const citation = atlasData.citations.find((item) => item.id === citationId);
                    return citation ? <ReferenceLink key={citation.id} slug={citation.referenceSlug}>{citation.relation}</ReferenceLink> : null;
                  })}
                  {(claim.counterevidenceSourceIds ?? []).map((sourceId) => {
                    const source = atlasData.sourceRefs.find((item) => item.id === sourceId);
                    return source ? <span className={styles.sourceEvidence} key={source.id}>{source.label}</span> : null;
                  })}
                  {claim.contradictingCitationIds.length === 0 && (claim.counterevidenceSourceIds ?? []).length === 0 ? claim.countercase : null}
                </td>
                <td>
                  {claim.supersedes.length > 0
                    ? claim.supersedes.map((claimId) => {
                      const predecessor = atlasData.claims.find((item) => item.id === claimId);
                      return predecessor ? <a key={claimId} href={`#${claimId}`}>{predecessor.title}</a> : claimId;
                    })
                    : 'No predecessor recorded'}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function ProvenanceLedger(): JSX.Element {
  const blockers = publicationBlockers();
  const unreadReferences = referenceCatalog.filter((reference) => !reference.fullRead).length;
  return (
    <section className={styles.section} id="provenance">
      <SectionHeader
        index="10 / provenance"
        title="The missing evidence is part of the model."
        description="The software scaffold and current source projection are implemented; the portrait denominator, external entailment audit, and ratification are not complete. Publication readiness remains a separate, inspectable gate."
      />
      <div className={styles.provenanceSummary}>
        <p><strong>{atlasData.sourceRefs.length}</strong><span>canonical public-safe source pointers</span></p>
        <p><strong>{unreadReferences}</strong><span>external full-text readings still required</span></p>
        <p><strong>{blockers.length}</strong><span>current publication blockers</span></p>
      </div>
      <div className={styles.tableScroll}>
        <table className={styles.variableTable}>
          <caption className={styles.label}>Canonical source and authorship ledger</caption>
          <thead>
            <tr><th scope="col">Source</th><th scope="col">Authorship</th><th scope="col">Lane</th><th scope="col">Read</th><th scope="col">Boundary</th></tr>
          </thead>
          <tbody>
            {atlasData.sourceRefs.map((source) => (
              <tr key={source.id}>
                <td>{source.label}</td>
                <td>{source.authorClass}</td>
                <td>{source.evidenceLane}</td>
                <td>{source.fullRead ? 'full' : 'pending'}</td>
                <td>{source.limitations.join(' ')}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <details className={styles.detailToggle}>
        <summary>Current release blockers and absent denominator</summary>
        <ul>
          <li>The three chronology tracks now contain source-complete public-safe projections, but the selected conversations are not a representative life denominator.</li>
          <li>Exact private-conversation quotations remain withheld; only the approved blueprint and whole-stack directives appear verbatim in this candidate.</li>
          <li>Novels, notes, games, Tarot, HALO, and broader ordinary-life counterexamples remain underrepresented pending complete source audits.</li>
          <li>External references are metadata- and link-checked, but full-text entailment review is not complete.</li>
          <li>The model remains candidate until Shawn ratifies claims, wording, and public excerpts.</li>
        </ul>
      </details>
    </section>
  );
}

function AtlasFooter(): JSX.Element {
  const publicSourceCount = atlasData.sourceRefs.filter((source) => source.privacy === 'public').length;
  return (
    <footer className={styles.footer}>
      <p className={styles.eyebrow}>Candidate model · version {atlasData.version} · updated {atlasData.updatedAt}</p>
      <p>
        <strong>This atlas is a correctable projection, not a diagnosis and not a claim of complete comprehension.</strong>{' '}
        It currently compiles {publicSourceCount} public-safe source records, {atlasData.claims.length} typed claims, {atlasData.bridges.length} cross-domain bridges, and {referenceCatalog.length} explanatory references. Restricted clinical narrative is not part of this public dataset.
      </p>
      <p>
        Geometry preserves named adjacency but loses strength, chronology, and causality. Repository timestamps prove repository state, not the origin date of an idea. Coauthorship and source limitations remain attached to the relevant records.
      </p>
    </footer>
  );
}

export default function ShawnAtlas(): JSX.Element {
  return (
    <ReferenceDialogProvider referenceBySlug={referenceBySlug}>
      <div className={styles.atlasRoot}>
        <a className={styles.skipLink} href="#atlas-content">Skip to atlas content</a>
        <header className={styles.masthead}>
          <div className={styles.mastheadInner}>
            <p className={styles.kicker}>SHAWN / APOCKY · LONGITUDINAL COGNITIVE &amp; EPISTEMIC ATLAS</p>
            <h1>Pattern into <span>instrument.</span></h1>
            <blockquote className={styles.thesis}>{atlasData.thesis}</blockquote>
            <div className={styles.mastheadMeta}>
              <span>STATUS / {atlasData.status}</span>
              <span>EVIDENCE / TYPED</span>
              <span>INTERPRETATION / PLURAL</span>
              <span>REVISION / EXPLICIT</span>
            </div>
            <div className={styles.atlasActions}>
              <button type="button" onClick={() => window.print()}>Print public atlas</button>
              <a href="/shawn/clinical">Open authenticated clinician view</a>
            </div>
          </div>
        </header>

        <nav className={styles.atlasNav} aria-label="Atlas sections">
          <ol>
            <li><a href="#model">Current model</a></li>
            <li><a href="#contract">Contract</a></li>
            <li><a href="#chronology">Chronology</a></li>
            <li><a href="#method">Method</a></li>
            <li><a href="#variables">Variables</a></li>
            <li><a href="#artifacts">Artifacts</a></li>
            <li><a href="#lenses">Lenses</a></li>
            <li><a href="#bridges">Bridges</a></li>
            <li><a href="#references">References</a></li>
            <li><a href="#revisions">Revisions</a></li>
            <li><a href="#provenance">Provenance</a></li>
          </ol>
        </nav>

        <main className={styles.content} id="atlas-content">
          <ModelOverview />
          <InterpretiveContract />
          <Chronology />
          <ReasoningChains />
          <VariableMatrix />
          <ArtifactCases />
          <LensRotation />
          <BridgeNetwork />
          <ReferenceNetwork />
          <RevisionLedger />
          <ProvenanceLedger />
        </main>
        <AtlasFooter />
      </div>
    </ReferenceDialogProvider>
  );
}
