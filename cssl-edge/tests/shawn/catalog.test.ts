import { atlasData } from '@/lib/shawn/atlas';
import katex from 'katex';
import { publicationBlockers, referenceBySlug, referenceCatalog, validateCatalog } from '@/lib/shawn/catalog';
import type { AtlasData, BridgeRecord, CitationRecord, ClaimRecord, ReferenceRecord } from '@/lib/shawn/types';

function assert(condition: boolean, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

export function testCatalogIsCompleteAndValid(): void {
  const errors = validateCatalog();
  assert(errors.length === 0, `catalog validation failed:\n${errors.join('\n')}`);
  assert(referenceCatalog.length >= 50, 'advanced topic denominator must remain broad');
  assert(atlasData.status === 'candidate', 'unratified model must remain candidate');
}

export function testEveryTopicResolvesAndExplainsItsBoundary(): void {
  for (const slug of atlasData.topicSlugs) {
    const record = referenceBySlug(slug);
    assert(record !== undefined, `missing topic reference: ${slug}`);
    assert(record.orientation.trim().length > 0, `${slug}: orientation`);
    assert(record.technical.trim().length > 0, `${slug}: technical account`);
    assert(record.evidence.summary.trim().length > 0, `${slug}: evidence account`);
    assert(record.supports.length > 0, `${slug}: supports`);
    assert(record.doesNotSupport.length > 0, `${slug}: doesNotSupport`);
    assert(record.counterpositions.length > 0, `${slug}: counterposition`);
    assert(record.revisionConditions.length > 0, `${slug}: revision condition`);
    assert(record.backlinks.length > 0, `${slug}: backlinks`);
    assert(record.urls.canonical.startsWith('https://'), `${slug}: authoritative HTTPS source`);
    assert(record.edition.trim().length > 0, `${slug}: edition`);
    assert(record.version.trim().length > 0, `${slug}: version`);
    assert(record.exactLocator.trim().length > 0, `${slug}: exact locator field`);
    for (const expression of record.mathExpressions) {
      katex.renderToString(expression.tex, {
        output: 'htmlAndMathml',
        throwOnError: true,
        trust: false,
      });
    }
  }
}

export function testAliasesResolveToCanonicalRecords(): void {
  assert(referenceBySlug('riemann-hypothesis')?.slug === 'zeta-zeros', 'RH alias');
  assert(referenceBySlug('quantum-chaos')?.slug === 'random-matrix-statistics', 'quantum-chaos alias');
  assert(referenceBySlug('tla-plus')?.slug === 'formal-methods', 'TLA+ alias');
  assert(referenceBySlug('n-of-1-method')?.slug === 'n-of-1-method', 'canonical slug');
  assert(referenceBySlug('not-a-topic') === undefined, 'unknown topic must not resolve');
}

export function testFullReadRequiresReviewReceipt(): void {
  const reviewed = referenceCatalog.find((record) => record.fullRead);
  assert(reviewed !== undefined, 'at least one reviewed reference fixture');
  assert(reviewed.reviewReceipt !== undefined, 'reviewed reference exposes a receipt');
  const invalid: ReferenceRecord = { ...reviewed, reviewReceipt: undefined };
  const errors = validateCatalog(referenceCatalog.map((record) => record.slug === invalid.slug ? invalid : record));
  assert(errors.some((error) => error.includes('fullRead requires a review receipt')), 'unreceipted fullRead must fail');
}

export function testProofRelationRejectsNonFormalEvidence(): void {
  const invalidCitation: CitationRecord = {
    id: 'cite-invalid-proof',
    referenceSlug: 'testimony',
    relation: 'proves',
    claimIds: ['claim-ontology-open'],
    supports: 'A deliberately invalid proof relation for the regression oracle.',
    doesNotSupport: 'Anything; this record must be rejected.',
  };
  const invalidAtlas: AtlasData = { ...atlasData, citations: [...atlasData.citations, invalidCitation] };
  const errors = validateCatalog(referenceCatalog, invalidAtlas);
  assert(errors.some((error) => error.includes('relation=proves requires a reviewed formal proof')), 'non-formal proof must fail');
}

export function testConsequentialClosedClaimRequiresHighGradeEvidence(): void {
  const original = atlasData.claims.find((claim) => claim.id === 'claim-attractor');
  assert(original !== undefined, 'claim-attractor fixture');
  const invalidClaim: ClaimRecord = { ...original, truthState: 'TRUE' };
  const invalidAtlas: AtlasData = {
    ...atlasData,
    claims: atlasData.claims.map((claim) => claim.id === invalidClaim.id ? invalidClaim : claim),
  };
  const errors = validateCatalog(referenceCatalog, invalidAtlas);
  assert(errors.some((error) => error.includes('consequential closed claim lacks full-read entailing R0/R1 evidence')), 'closed claim without entailing full-read R0/R1 must fail');
}

export function testAnalogyCannotCloseConsequentialClaim(): void {
  const original = atlasData.claims.find((claim) => claim.id === 'claim-attractor');
  assert(original !== undefined, 'claim-attractor fixture');
  const invalidCitation: CitationRecord = {
    id: 'cite-analogy-prestige-trap',
    referenceSlug: 'mcts',
    relation: 'analogizes',
    claimIds: [original.id],
    supports: 'Deliberate prestige-edge falsifier.',
    doesNotSupport: 'Identity or mechanism.',
  };
  const invalidClaim: ClaimRecord = {
    ...original,
    truthState: 'TRUE',
    supportingCitationIds: [...original.supportingCitationIds, invalidCitation.id],
  };
  const invalidAtlas: AtlasData = {
    ...atlasData,
    claims: atlasData.claims.map((claim) => claim.id === invalidClaim.id ? invalidClaim : claim),
    citations: [...atlasData.citations, invalidCitation],
  };
  const errors = validateCatalog(referenceCatalog, invalidAtlas);
  assert(errors.some((error) => error.includes('cannot be closed by analogy')), 'prestigious analogy cannot close claim');
}

export function testTextualEvidenceCannotSupportPhysicalQL0Claim(): void {
  const original = atlasData.claims.find((claim) => claim.id === 'claim-attractor');
  assert(original !== undefined, 'claim-attractor fixture');
  const invalidCitation: CitationRecord = {
    id: 'cite-textual-physical-trap',
    referenceSlug: 'qabbalah',
    relation: 'supports',
    claimIds: [original.id],
    supports: 'Deliberate cross-mode falsifier.',
    doesNotSupport: 'Any instrumented physical mechanism.',
  };
  const invalidClaim: ClaimRecord = {
    ...original,
    kind: 'physical-mechanism',
    quantumLane: 'QL0',
    supportingCitationIds: [...original.supportingCitationIds, invalidCitation.id],
  };
  const invalidAtlas: AtlasData = {
    ...atlasData,
    claims: atlasData.claims.map((claim) => claim.id === invalidClaim.id ? invalidClaim : claim),
    citations: [...atlasData.citations, invalidCitation],
  };
  const errors = validateCatalog(referenceCatalog, invalidAtlas);
  assert(errors.some((error) => error.includes('physical or QL0 claim requires empirical instrument evidence')), 'textual evidence cannot support QL0');
}

export function testQuantumAnalogyCannotBecomeIdentityOrFormalHomology(): void {
  const invalidBridge: BridgeRecord = {
    id: 'bridge-invalid-ql2',
    from: 'cognitive alternative',
    to: 'physical quantum state',
    relationship: 'IDENTITY',
    statement: 'Deliberately invalid QL2 promotion.',
    invariant: 'vocabulary only',
    differences: ['No physical measurement.'],
    lane: 'proposed',
    truthState: 'OPEN',
    prediction: 'None.',
    negativeTransferTest: 'Classical model suffices.',
    sourceIds: ['src-pattern-ontology'],
    topicSlugs: ['unitarity'],
    quantumLane: 'QL2',
  };
  const invalidAtlas: AtlasData = { ...atlasData, bridges: [...atlasData.bridges, invalidBridge] };
  const errors = validateCatalog(referenceCatalog, invalidAtlas);
  assert(errors.some((error) => error.includes('QL2 cannot support identity or formal homology')), 'QL2 promotion must fail');
}

export function testFictionalModelsCannotCloseAsTrue(): void {
  const original = atlasData.bridges.find((bridge) => bridge.relationship === 'FICTIONAL_MODEL');
  assert(original !== undefined, 'fictional bridge fixture');
  const invalidBridge: BridgeRecord = { ...original, truthState: 'TRUE' };
  const invalidAtlas: AtlasData = {
    ...atlasData,
    bridges: atlasData.bridges.map((bridge) => bridge.id === invalidBridge.id ? invalidBridge : bridge),
  };
  const errors = validateCatalog(referenceCatalog, invalidAtlas);
  assert(errors.some((error) => error.includes('fictional model cannot be closed as TRUE')), 'fictional truth promotion must fail');
}

export function testPublicProjectionHasNoRestrictedSourceOrLocalPath(): void {
  for (const source of atlasData.sourceRefs) {
    assert(source.privacy === 'public', `${source.id}: public privacy class`);
    assert(source.publicationApproved, `${source.id}: publication approval`);
  }
  const serialized = JSON.stringify({ atlasData, referenceCatalog });
  assert(!/(?:[A-Za-z]:\\|file:\/\/|\\Users\\|\/Users\/)/i.test(serialized), 'public data must not contain local filesystem paths');
  assert(!serialized.toLowerCase().includes('dose'), 'public data must not contain medication dosage language');
}

export function testAtlasCoversRequiredStructures(): void {
  assert(atlasData.chronology.length >= 5, 'chronology coverage');
  assert(atlasData.reasoningChains.some((chain) => chain.steps.length >= 8), 'full reasoning chain');
  assert(atlasData.episodes.length >= 2, 'episode coverage');
  assert(new Set(atlasData.variables.map((variable) => variable.role)).size >= 7, 'variable role coverage');
  assert(atlasData.artifacts.length >= 7, 'artifact coverage');
  assert(atlasData.bridges.some((bridge) => bridge.relationship === 'STRUCTURAL_ANALOGY'), 'structural analogy bridge');
  assert(atlasData.bridges.some((bridge) => bridge.relationship === 'FICTIONAL_MODEL'), 'fictional model bridge');
  assert(atlasData.lenses.length >= 10, 'lens coverage');
  assert(atlasData.lenses.some((lens) => lens.id === 'lens-quantum-literal'), 'QL0 lens');
  assert(atlasData.lenses.some((lens) => lens.id === 'lens-quantum-analogical'), 'QL2 lens');
}

export function testCandidateCannotMasqueradeAsPublicationReady(): void {
  const blockers = publicationBlockers();
  assert(blockers.some((item) => item.includes('model remains candidate')), 'candidate status blocks publication');
  assert(blockers.some((item) => item.includes('full-text review pending')), 'unread reference blocks publication');
}

export function runCatalogTests(): void {
  testCatalogIsCompleteAndValid();
  testEveryTopicResolvesAndExplainsItsBoundary();
  testAliasesResolveToCanonicalRecords();
  testFullReadRequiresReviewReceipt();
  testProofRelationRejectsNonFormalEvidence();
  testConsequentialClosedClaimRequiresHighGradeEvidence();
  testAnalogyCannotCloseConsequentialClaim();
  testTextualEvidenceCannotSupportPhysicalQL0Claim();
  testQuantumAnalogyCannotBecomeIdentityOrFormalHomology();
  testFictionalModelsCannotCloseAsTrue();
  testPublicProjectionHasNoRestrictedSourceOrLocalPath();
  testAtlasCoversRequiredStructures();
  testCandidateCannotMasqueradeAsPublicationReady();
}

runCatalogTests();
// eslint-disable-next-line no-console
console.log('shawn/catalog.test : OK · 13 tests passed');
