import { atlasData } from './atlas';
import type {
  AtlasData,
  EvidenceAccount,
  EvidenceMode,
  ReferenceBacklink,
  ReferenceIdentifier,
  ReferenceRecord,
  ReferenceReviewReceipt,
  ReferenceRole,
} from './types';

type Domain = ReferenceRecord['domain'];

interface TopicSeed {
  readonly slug: string;
  readonly aliases?: readonly string[];
  readonly title: string;
  readonly domain: Domain;
  readonly mode: EvidenceMode;
  readonly role: ReferenceRole;
  readonly creators: readonly string[];
  readonly edition?: string;
  readonly version?: string;
  readonly translation?: string;
  readonly date: string;
  readonly publisher: string;
  readonly url: string;
  readonly identifiers?: readonly ReferenceIdentifier[];
  readonly openAccess?: string;
  readonly archive?: string;
  readonly locator?: string;
  readonly fullRead?: boolean;
  readonly reviewReceipt?: ReferenceReviewReceipt;
  readonly contentHash?: string;
  readonly license?: string;
  readonly math?: readonly { readonly tex: string; readonly label: string }[];
  readonly authority: string;
  readonly orientation: string;
  readonly prerequisites?: readonly string[];
  readonly technical: string;
  readonly shawnUse: string;
  readonly supports: string;
  readonly boundary: string;
  readonly counter: string;
  readonly revision: string;
}

const reviewed = '2026-07-15';

const curatedReviewReceipts: Readonly<Record<string, ReferenceReviewReceipt>> = {
  'zeta-zeros': {
    id: 'ref-audit-20260715-zeta-zeros-v1', reviewedAt: reviewed, reviewer: 'OpenAI Codex', method: 'section-complete-web-review',
    scope: 'The DLMF sections actually used by this atlas entry, not the DLMF as a whole.', sourceVersion: 'DLMF 1.2.7 (2026-06-15)',
    coverage: '§25.10 in full; §25.16(i) in full, including equations 25.16.1–25.16.4.',
    sourceSnapshots: [
      { label: 'DLMF §25.10 HTML', sha256: '315b4e9010ea9731381288c0f0ff4cc344574829650a2b3e8d1639be8367b065' },
      { label: 'DLMF §25.16 HTML', sha256: 'bf13c1822856d95c2212df2c5bc0faff9185c041100f4e18ba22757f74dcb9a4' },
    ],
    limitations: ['The review covers the cited sections, not every DLMF dependency or the complete literature on RH.'],
  },
  'random-matrix-statistics': {
    id: 'ref-audit-20260715-montgomery-pair-correlation-v1', reviewedAt: reviewed, reviewer: 'OpenAI Codex', method: 'browser-ocr-line-review',
    scope: 'Montgomery’s complete 1973 paper as reproduced by a full-text OCR mirror, checked against the version-of-record identity.', sourceVersion: 'Proceedings of Symposia in Pure Mathematics 24 (1973), pp. 181–193',
    coverage: 'Complete paper, lines 1–839 of the audited browser transcript; theorem, corollaries, conjecture, §§1–4, and references.',
    sourceSnapshots: [],
    limitations: ['The mirror’s OCR degrades some equations; formula boundaries were therefore checked conservatively and no local content hash is asserted.'],
  },
  'wieferich-primes': {
    id: 'ref-audit-20260715-wieferich-search-v1', reviewedAt: reviewed, reviewer: 'OpenAI Codex', method: 'page-complete-pdf-text-review',
    scope: 'Crandall, Dilcher, and Pomerance’s complete published search paper.', sourceVersion: 'Mathematics of Computation 66.217 (1997), pp. 433–449',
    coverage: 'All 17 PDF pages; definition, §1–§5 algorithms, error checks, results, heuristics, and references.',
    sourceSnapshots: [{ label: 'Author-hosted paper PDF', sha256: 'f1248405d19b4a5279ba54e56e41e9baced0e78a91775e14de7272dedfef34b3' }],
    limitations: ['The reported search bound is historical to the paper and is not treated as the current known bound.'],
  },
  'prime-races': {
    id: 'ref-audit-20260715-prime-races-v1', reviewedAt: reviewed, reviewer: 'OpenAI Codex', method: 'page-complete-pdf-text-review',
    scope: 'Rubinstein and Sarnak’s complete 1994 paper on Chebyshev bias.', sourceVersion: 'Experimental Mathematics 3.3 (1994), pp. 173–197',
    coverage: 'All 25 PDF pages; Theorems 1.1–1.6, §§2–5, numerical methods, generalizations, and references.',
    sourceSnapshots: [{ label: 'Project Euclid PDF', sha256: '756191a53a8aa141eaabf333ca28993c432a0e83b21b633c954b2467db0ad190' }],
    limitations: ['OCR damages some symbols; hypotheses, density convention, theorem boundaries, and reported numerical error bounds were checked from surrounding text.'],
  },
  'compactified-time': {
    id: 'ref-audit-20260715-deutsch-ctc-v1', reviewedAt: reviewed, reviewer: 'OpenAI Codex', method: 'page-complete-pdf-text-review',
    scope: 'Deutsch’s complete 1991 article, reviewed as a finite-dimensional CTC consistency model rather than as evidence for global time compactification.', sourceVersion: 'Physical Review D 44.10 (1991), pp. 3197–3217',
    coverage: 'All pages 3197–3217; all sections; equations (1)–(59); figure captions; summary; acknowledgments; references [1]–[33].',
    sourceSnapshots: [
      { label: 'Publisher-formatted article PDF', sha256: 'be18bf9ec690f051da39ed4f8175187c29196ed4da81a32acd173ef4eb3f7271' },
      { label: 'Complete extracted text', sha256: '9aef08b9c5ed60891ebde9ddf1144100234877e36f18b41ff806ec9bf3c7e43d' },
    ],
    limitations: ['Two-column extraction degrades equation typography and figures; the fixed-point theorem was read semantically, but this is not a line-by-line visual equation-verification receipt.'],
  },
  'n-of-1-method': {
    id: 'ref-audit-20260715-cent-n-of-1-v1', reviewedAt: reviewed, reviewer: 'OpenAI Codex', method: 'browser-ocr-line-review',
    scope: 'The complete CENT 2015 statement together with its 2016 correction; the uncorrected author upload alone is not the canonical composite.', sourceVersion: 'BMJ 2015;350:h1738, corrected by BMJ 2016;355:i5381',
    coverage: 'Statement PDF pp. 1–6 / audited browser-text lines 88–1923, including the complete checklist, figures, declarations, and references; correction read in full.',
    sourceSnapshots: [],
    limitations: ['Lawful PDF endpoints returned HTTP 403, so no local content hash is asserted; browser text was checked contiguously and the author upload predates the mandatory item 4c correction.'],
  },
  salience: {
    id: 'ref-audit-20260715-salience-network-v1', reviewedAt: reviewed, reviewer: 'OpenAI Codex', method: 'section-complete-web-review',
    scope: 'The complete pinned PMC main-article HTML for Seeley et al. (2007), excluding supplements and underlying data.', sourceVersion: 'Journal of Neuroscience 27.9 (2007), pp. 2349–2356; PMCID PMC2680293',
    coverage: 'Metadata, abstract, introduction, methods, results, discussion, Figures 1–3 and captions, footnote, and 64 references.',
    sourceSnapshots: [{ label: 'PMC2680293 main-article HTML', sha256: 'fe6078b6b14fe571ccb4af0b1b25f8059fa48644c91a1f9cb1907a9ab8dd31ce' }],
    limitations: ['Supplemental Figures 1–4, Supplemental Tables 1–4, raw imaging data, and independent replications were not present in the pinned artifact and remain unaudited.'],
  },
  'thermal-time': {
    id: 'ref-audit-20260715-thermal-time-v1', reviewedAt: reviewed, reviewer: 'OpenAI Codex', method: 'page-complete-pdf-text-review',
    scope: 'Connes and Rovelli’s complete arXiv manuscript proposing the thermal-time hypothesis.', sourceVersion: 'arXiv:gr-qc/9406019v1 (1994)',
    coverage: 'All 25 PDF pages; §§1–5, equations (1)–(57), notes, and references.',
    sourceSnapshots: [{ label: 'arXiv v1 PDF', sha256: '82c43e2a66c3964cd1a1b286a9006691b6bff49651969c96ccfd5e5a1a292dd6' }],
    limitations: ['The PDF renders a later local typesetting date; the audited bibliographic version remains the 1994 arXiv v1 submission.'],
  },
};

const curatedContentHashes: Readonly<Record<string, string>> = {
  'wieferich-primes': 'sha256:f1248405d19b4a5279ba54e56e41e9baced0e78a91775e14de7272dedfef34b3',
  'prime-races': 'sha256:756191a53a8aa141eaabf333ca28993c432a0e83b21b633c954b2467db0ad190',
  'compactified-time': 'sha256:be18bf9ec690f051da39ed4f8175187c29196ed4da81a32acd173ef4eb3f7271',
  salience: 'sha256:fe6078b6b14fe571ccb4af0b1b25f8059fa48644c91a1f9cb1907a9ab8dd31ce',
  'thermal-time': 'sha256:82c43e2a66c3964cd1a1b286a9006691b6bff49651969c96ccfd5e5a1a292dd6',
};

const curatedMath: Readonly<Record<string, readonly { readonly tex: string; readonly label: string }[]>> = {
  'zeta-zeros': [{
    tex: '\\zeta(s)=0,\\ 0<\\operatorname{Re}(s)<1\\;\\Longrightarrow\\;\\operatorname{Re}(s)=\\tfrac12',
    label: 'The Riemann Hypothesis, displayed as a conjectural implication',
  }],
  'wieferich-primes': [{
    tex: '2^{p-1}\\equiv 1\\pmod{p^2}',
    label: 'The base-two Wieferich congruence',
  }],
  unitarity: [{
    tex: 'U(t)=e^{-iHt},\\qquad U(t)^\\dagger U(t)=I',
    label: 'Unitary evolution generated by a self-adjoint Hamiltonian under the stated domain conditions',
  }],
  'euler-characteristic': [{
    tex: '\\chi(X)=\\sum_k(-1)^k\\operatorname{rank}H_k(X)',
    label: 'Euler characteristic expressed through homology ranks',
  }],
  'replicator-dynamics': [{
    tex: '\\dot{x}_i=x_i\\left((Ax)_i-x^{\\mathsf T}Ax\\right)',
    label: 'The standard replicator equation',
  }],
};

const curatedLocators: Readonly<Record<string, string>> = {
  'zeta-zeros': 'DLMF §§25.10(i)–(ii), 25.16(i), equations 25.16.1–25.16.4',
  'random-matrix-statistics': 'pp. 181–193; theorem and corollaries, conjecture, §§1–4',
  'wieferich-primes': 'pp. 433–449; definition and §§1–5',
  'prime-races': 'pp. 173–197; Theorems 1.1–1.6 and §§2–5; GRH/GSH hypotheses are load-bearing',
  'compactified-time': 'pp. 3197–3217; especially equations (15)–(18), with downstream consequences and unresolved physical status in Discussion',
  'n-of-1-method': 'CENT 2015 statement, PDF pp. 1–6 and checklist items 1a–25; apply the 2016 correction to item 4c',
  salience: 'pp. 2349–2356; complete main article, Figures 1–3; cohorts and analyses in Methods; state/trait and switching limits in Discussion',
  'thermal-time': 'arXiv:gr-qc/9406019v1, §§1–5; especially equations (8), (20)–(26), (44), and (48)–(57)',
};

function evidenceLabel(mode: EvidenceMode): EvidenceAccount['label'] {
  switch (mode) {
    case 'formal': return 'Formal treatment';
    case 'computational': return 'Computational method';
    case 'empirical': return 'Empirical evidence';
    case 'textual': return 'Primary-text attestation';
    case 'philosophical': return 'Philosophical argument';
    case 'normative': return 'Normative framework';
    case 'phenomenological':
    case 'interpretive': return 'Interpretive lineage';
  }
}

function topicBacklinks(slugs: readonly string[]): readonly ReferenceBacklink[] {
  const links: ReferenceBacklink[] = [];
  const add = (kind: ReferenceBacklink['kind'], id: string, label: string): void => {
    const key = `${kind}:${id}`;
    if (!links.some((link) => `${link.kind}:${link.id}` === key)) links.push({ kind, id, label });
  };
  const has = (topics: readonly string[]): boolean => topics.some((topic) => slugs.includes(topic));
  for (const claim of atlasData.claims) if (has(claim.topicSlugs)) add('claim', claim.id, claim.title);
  for (const episode of atlasData.episodes) if (has(episode.topicSlugs)) add('episode', episode.id, episode.title);
  for (const bridge of atlasData.bridges) if (has(bridge.topicSlugs)) add('bridge', bridge.id, `${bridge.from} ↔ ${bridge.to}`);
  for (const artifact of atlasData.artifacts) if (has(artifact.topicSlugs)) add('artifact', artifact.id, artifact.title);
  for (const event of atlasData.chronology) if (has(event.topicSlugs)) add('chronology', event.id, event.title);
  return links;
}

function identifiersFor(url: string): readonly ReferenceIdentifier[] {
  if (url.startsWith('https://doi.org/')) return [{ scheme: 'DOI', value: url.slice('https://doi.org/'.length) }];
  const arxiv = /^https:\/\/arxiv\.org\/abs\/(.+)$/.exec(url);
  if (arxiv?.[1]) return [{ scheme: 'arXiv', value: arxiv[1] }];
  const w3c = /^https:\/\/www\.w3\.org\/TR\/([^/]+)\/?$/.exec(url);
  if (w3c?.[1]) return [{ scheme: 'W3C', value: w3c[1] }];
  return [];
}

function reference(seed: TopicSeed): ReferenceRecord {
  const aliases = (seed.aliases ?? []).filter(
    (alias) => !(seed.slug === 'compactified-time' && alias === 'thermal-time'),
  );
  const slugs = [seed.slug, ...aliases];
  const reviewReceipt = seed.reviewReceipt ?? curatedReviewReceipts[seed.slug];
  return {
    slug: seed.slug,
    aliases,
    title: seed.title,
    domain: seed.domain,
    creators: seed.creators,
    edition: seed.edition ?? 'Canonical web or version-of-record edition',
    version: seed.version ?? (seed.date.includes('current') ? 'Living page at access date' : seed.date),
    ...(seed.translation ? { translation: seed.translation } : {}),
    date: seed.date,
    publisher: seed.publisher,
    language: 'en',
    exactLocator: seed.locator ?? curatedLocators[seed.slug] ?? 'Work as a whole; section-level locator pending full-text review.',
    identifiers: [...identifiersFor(seed.url), ...(seed.identifiers ?? [])],
    urls: {
      canonical: seed.url,
      ...(seed.openAccess ? { openAccess: seed.openAccess } : {}),
      ...(seed.archive ? { archive: seed.archive } : {}),
    },
    accessed: reviewed,
    lastVerified: reviewed,
    ...(seed.license ? { license: seed.license } : {}),
    ...((seed.contentHash ?? curatedContentHashes[seed.slug]) ? { contentHash: seed.contentHash ?? curatedContentHashes[seed.slug] } : {}),
    fullRead: seed.fullRead ?? reviewReceipt !== undefined,
    ...(reviewReceipt ? { reviewReceipt } : {}),
    displayCitation: `${seed.creators.join(', ')}. ${seed.title}. ${seed.publisher}, ${seed.date}. ${seed.url}`,
    evidenceMode: seed.mode,
    role: seed.slug === 'gnosticism' ? 'R4' : seed.role,
    authorityScope: seed.authority,
    limitations: [seed.boundary],
    privacy: 'public',
    orientation: seed.orientation,
    prerequisites: seed.prerequisites ?? [],
    technical: seed.technical,
    mathExpressions: seed.math ?? curatedMath[seed.slug] ?? [],
    evidence: {
      label: evidenceLabel(seed.mode),
      summary: seed.supports,
      steps: [
        'Open the canonical source and locate the statement under discussion.',
        'Check definitions, hypotheses, version, translation, and measured scope.',
        'Compare the atlas claim with the stated limitation and counterposition.',
      ],
    },
    shawnUse: seed.shawnUse,
    supports: [seed.supports],
    doesNotSupport: [seed.boundary],
    counterpositions: [seed.counter],
    revisionConditions: [seed.revision],
    citationIds: atlasData.citations
      .filter((citation) => slugs.includes(citation.referenceSlug))
      .map((citation) => citation.id),
    backlinks: topicBacklinks(slugs),
  };
}

const seeds: readonly TopicSeed[] = [
  {
    slug: 'zeta-zeros', aliases: ['explicit-formula', 'riemann-hypothesis', 'hilbert-polya'], title: 'Riemann zeta zeros, the explicit formula, and RH', domain: 'mathematics', mode: 'formal', role: 'R2', creators: ['NIST Digital Library of Mathematical Functions'], date: 'current reference', publisher: 'National Institute of Standards and Technology', url: 'https://dlmf.nist.gov/25.10', openAccess: 'https://dlmf.nist.gov/25.16', version: 'DLMF 1.2.7 (2026-06-15)', authority: 'Reviewed DLMF §§25.10 and 25.16(i): definitions and established analytic results concerning zeta zeros, the explicit formula, PNT, and RH-equivalent error bounds; conjectures remain conjectures.', orientation: 'The nontrivial zeros of the zeta function enter explicit formulas that connect analytic behavior to the distribution of primes.', prerequisites: ['complex analysis', 'prime counting functions'], technical: 'DLMF §25.16(i), especially equation 25.16.2, expresses ψ(x) using the nontrivial zeros; §25.10 states the critical-strip symmetries and RH. Hilbert–Pólya is a conjectural operator program, not an operator supplied by these sections.', shawnUse: 'ZEROES uses the zeros as a spectral-resolution instance in its loop–obligation–defect grammar.', supports: 'The cited explicit formula, known zero symmetries, and the precise open status of RH.', boundary: 'DLMF §25.10 alone does not supply the explicit formula; the cited sections do not prove RH, produce a Hilbert–Pólya operator, or identify zeta zeros with Wieferich primes or cyclic-time states.', counter: 'A shared spectral vocabulary may be mathematically accurate while the proposed cross-domain dictionary remains non-predictive.', revision: 'Revise if a cited theorem is misstated, its hypotheses are omitted, or a claimed operator construction is not peer-verifiable.',
  },
  {
    slug: 'random-matrix-statistics', aliases: ['gue-cue-statistics', 'quantum-chaos', 'symmetry-classes'], title: 'Montgomery pair correlation and the GUE conjecture', domain: 'mathematics', mode: 'formal', role: 'R0', creators: ['Hugh L. Montgomery'], date: '1973', publisher: 'Proceedings of Symposia in Pure Mathematics', url: 'https://doi.org/10.1090/pspum/024/9944', version: 'Volume 24 (1973), pp. 181–193', authority: 'Original RH-conditional pair-correlation theorem under restricted Fourier support, together with Montgomery’s explicitly conjectural extension.', orientation: 'Montgomery found that a restricted correlation statistic for zeta zeros has the form later recognized by Dyson as matching complex Hermitian random matrices; the broader agreement remains conjectural in this paper.', prerequisites: ['zeta zeros', 'spectral statistics', 'probability'], technical: 'The paper assumes RH. It proves the stated asymptotic for the form factor only in the restricted range 0≤α<1 (uniformly away from 1), then conjectures F(α)≈1 for α≥1 and the corresponding full pair-correlation law. Dyson’s GUE observation motivates a spectral analogy; no Hilbert–Pólya operator is constructed.', shawnUse: 'ZEROES tests whether unitarity and time-reversal structure provide a disciplined rather than merely verbal bridge.', supports: 'The restricted RH-conditional theorem, the clearly labeled broader conjecture, and the historical GUE comparison recorded in the paper.', boundary: 'It does not prove the unrestricted GUE law, construct an operator, establish causal identity, or make every chaotic system a model of the zeta zeros.', counter: 'Universality can erase mechanism: distinct systems share local statistics precisely because details are forgotten.', revision: 'Revise if RH, Fourier-support restrictions, scaling, ensemble, or the theorem–conjecture boundary is omitted.',
  },
  {
    slug: 'wieferich-primes', aliases: ['fermat-quotient'], title: 'Wieferich primes and Fermat-quotient closure', domain: 'mathematics', mode: 'computational', role: 'R0', creators: ['Richard Crandall', 'Karl Dilcher', 'Carl Pomerance'], date: '1997', publisher: 'Mathematics of Computation', url: 'https://doi.org/10.1090/S0025-5718-97-00791-6', openAccess: 'https://math.dartmouth.edu/~carlp/PDF/paper111', version: 'Volume 66, number 217 (1997), pp. 433–449', authority: 'Primary computational mathematics source for the definition, algorithms, internal error checks, and search result in its published historical scope.', orientation: 'A base-2 Wieferich prime satisfies a stronger congruence than Fermat\'s little theorem ordinarily guarantees.', prerequisites: ['modular arithmetic', 'prime numbers'], technical: 'For prime p, the Fermat quotient q_p(2)=(2^(p-1)-1)/p is divisible by p exactly when 2^(p-1)≡1 mod p². The paper implements segmented search algorithms with independent machine segments and fatal-error checks. This is an arithmetic congruence, not a Riemann-zero condition.', shawnUse: 'ZEROES treats vanishing of the Fermat-quotient defect as one exact instance of unusually deep loop closure.', supports: 'The congruence definition, classical FLT connection, the paper’s algorithms, and its 1997 search report.', boundary: 'The paper’s bound is historical rather than current; its 1/p frequency model is heuristic, and the word “zero” does not identify these primes with zeta zeros.', counter: 'The closure language may restate the congruence elegantly without yielding new arithmetic.', revision: 'Revise when a current bound is independently audited or when a stated heuristic or independence assumption is presented as theorem.',
  },
  {
    slug: 'prime-races', aliases: ['chebyshev-bias'], title: 'Prime number races and Chebyshev bias', domain: 'mathematics', mode: 'formal', role: 'R0', creators: ['Michael Rubinstein', 'Peter Sarnak'], date: '1994', publisher: 'Experimental Mathematics', url: 'https://doi.org/10.1080/10586458.1994.10504289', openAccess: 'https://projecteuclid.org/journals/experimental-mathematics/volume-3/issue-3/Chebyshevs-bias/em/1048515870.pdf', version: 'Volume 3, number 3 (1994), pp. 173–197', authority: 'Primary analysis of limiting logarithmic distributions under GRH and of density and product-formula consequences under the stronger Grand Simplicity Hypothesis.', orientation: 'Specified residue classes can lead prime-counting races for highly nonuniform proportions of logarithmic time.', prerequisites: ['Dirichlet characters', 'GRH', 'linear independence hypothesis'], technical: 'Under GRH, Theorem 1.1 gives a limiting distribution for normalized prime-race error vectors. GSH—the rational linear independence of relevant zero ordinates—supports the product formula, smooth density, strict race probabilities, and symmetry results. Section 4 computes examples with explicit numerical approximation bounds.', shawnUse: 'The negative-space memo uses the bias as a measured asymmetry that a seam grammar should explain rather than hide.', supports: 'Conditional statements about specified races, explicitly using logarithmic distribution and the paper’s GRH/GSH assumptions.', boundary: 'It does not establish an unconditional universal preferred side, guarantee ordinary natural density, or license extrapolation from a vivid finite range.', counter: 'A finite range can mislead, and the strongest numerical probabilities depend on hypotheses not presently proved.', revision: 'Revise any displayed probability if hypotheses, modulus, residue classes, normalization, or density convention differ from the source.',
  },
  {
    slug: 'arithmetic-topology', aliases: ['knots-and-primes'], title: 'Arithmetic topology: knots and primes', domain: 'mathematics', mode: 'formal', role: 'R2', creators: ['Masanori Morishita'], date: '2012', publisher: 'Springer', url: 'https://doi.org/10.1007/978-1-4471-2158-9', authority: 'Research monograph developing precise analogies between three-manifold topology and number fields.', orientation: 'Arithmetic topology compares primes in number fields with knots in three-manifolds through rigorously defined invariants.', prerequisites: ['algebraic number theory', 'knot theory'], technical: 'The analogy relates linking phenomena, reciprocity symbols, fundamental groups, and ramification through established theorems and conjectural programs.', shawnUse: 'It supplies a theorem-grade precedent for asking when a loop analogy is formal rather than decorative.', supports: 'Specific dictionary entries proved within arithmetic topology.', boundary: 'It does not validate arbitrary prime–knot metaphors or the broader ZEROES transfer program.', counter: 'A successful analogy in one formal setting does not license transport to unrelated settings without a defined functor or test.', revision: 'Revise when a claimed correspondence lacks the theorem, hypotheses, or invariant named in the source.',
  },
  {
    slug: 'cpt-symmetry', aliases: ['charge-parity-time'], title: 'CPT symmetry and its assumptions', domain: 'physics', mode: 'formal', role: 'R2', creators: ['O. W. Greenberg'], date: '2015', publisher: 'Reports on Progress in Physics', url: 'https://doi.org/10.1088/0034-4885/78/10/106001', authority: 'Scholarly review of CPT, Lorentz invariance, and the assumptions connecting them.', orientation: 'CPT is a theorem of relativistic quantum field theory under defined structural assumptions, not a numerological combination of three letters.', prerequisites: ['special relativity', 'quantum field theory'], technical: 'Locality, Lorentz covariance, spectrum/positivity conditions, and standard spin–statistics structure matter to CPT statements; modifying the substrate changes which theorem applies.', shawnUse: 'The initial ZEROES question asks what must change before a genuinely new discrete operation can exist.', supports: 'The theorem\'s assumptions and the distinction between spacetime and internal transformations.', boundary: 'It does not establish the physical reality of compactified time or a fourth operation in nature.', counter: 'A group-theoretic operation on a hypothetical topology may be mathematically valid yet physically idle.', revision: 'Revise if the site states CPT without its assumptions or confuses experimental tests with the theorem itself.',
  },
  {
    slug: 'unitarity', aliases: ['self-adjoint-evolution'], title: 'Unitarity and quantum evolution', domain: 'physics', mode: 'formal', role: 'R2', creators: ['Stanford Encyclopedia of Philosophy'], date: '2023', publisher: 'Stanford University', url: 'https://plato.stanford.edu/entries/qm/', authority: 'Scholarly account of quantum theory\'s mathematical and interpretive foundations.', orientation: 'Unitary evolution preserves inner products and total probability in a closed quantum system.', prerequisites: ['Hilbert spaces', 'linear operators'], technical: 'For self-adjoint H, U(t)=exp(-iHt) is unitary under appropriate domain conditions. Self-adjointness, boundedness questions, and open-system dynamics must not be collapsed.', shawnUse: 'ZEROES tracks where unitarity is theorem, conjectural operator motivation, or merely analogy.', supports: 'The standard formal role of unitary time evolution.', boundary: 'It does not prove that the zeta zeros are eigenvalues of a self-adjoint operator or that cognition uses quantum dynamics.', counter: 'Open systems and effective descriptions need not evolve unitarily on the reduced state space.', revision: 'Revise if an operator\'s domain, self-adjointness, or physical system is unspecified.',
  },
  {
    slug: 'compactified-time', aliases: ['deutsch-ctc-consistency', 'ctc-density-operator-fixed-point', 'fixed-points'], title: 'Deutsch closed-timelike-curve consistency model', domain: 'physics', mode: 'formal', role: 'R0', creators: ['David Deutsch'], date: '1991', publisher: 'Physical Review D', url: 'https://doi.org/10.1103/PhysRevD.44.3197', openAccess: 'https://journals.aps.org/prd/abstract/10.1103/PhysRevD.44.3197', archive: 'https://ui.adsabs.harvard.edu/abs/1991PhRvD..44.3197D/abstract', version: 'Volume 44, number 10 (1991), pp. 3197–3217', authority: 'Primary source for a conditional finite-dimensional quantum-information model near closed timelike lines; it does not derive CTC existence or global time topology.', orientation: 'Given a chronology-violating interaction structure, Deutsch replaces contradictory pure-state histories with a density operator that is a fixed point of the induced CTC channel.', prerequisites: ['quantum channels', 'spacetime causal structure'], technical: 'For chronology-respecting input ρ₁, CTC state ρ₂, and finite-dimensional unitary U, equation (15) requires Tr₁[U(ρ₁⊗ρ₂)U†]=ρ₂. Equations (16)–(18) define the induced channel and prove at least one fixed point by Cesàro averaging, compactness, and continuity. The local interaction remains unitary while the external input-output map may be nonlinear and nonunitary; fixed points need not be unique, and maximum-entropy selection is conjectural.', shawnUse: 'ZEROES compares this precise density-operator fixed-point structure with arithmetic closure defects at QL1/QL2; it does not treat the paper as evidence for compactified or cyclic physical time.', supports: 'The finite-dimensional fixed-point theorem and conditional consequences within Deutsch’s stipulated CTC model.', boundary: 'It does not establish physical CTC existence, global compactification t∼t+τ, periodic fields, cyclic cosmology, thermal/KMS time, a half-turn operator K, unique fixed points, or a QL0 mechanism.', counter: 'Conditional mathematical consistency is not physical realizability: backreaction, chronology protection, quantum gravity, or a different microscopic state law may forbid the stipulated interaction, and nonuniqueness requires an unproved selection rule.', revision: 'Revise if the finite-dimensional assumptions or equation (15) are misstated, maximum entropy is presented as proved, or compactified, thermal, cosmological, and CTC time are promoted to identity.',
  },
  {
    slug: 'manifolds', aliases: [], title: 'Manifolds', domain: 'geometry-topology', mode: 'formal', role: 'R2', creators: ['Encyclopedia of Mathematics editorial board'], date: 'current reference', publisher: 'European Mathematical Society', url: 'https://encyclopediaofmath.org/wiki/Manifold', authority: 'Standard mathematical definition and overview.', orientation: 'A manifold is a space locally modeled on Euclidean space, with additional structures specified separately.', prerequisites: ['topological spaces', 'coordinate charts'], technical: 'Dimension, topology, differentiability, metric, orientation, boundary, and causal structure are independent data; naming a “14-dimensional manifold” supplies almost none of them.', shawnUse: 'The atlas uses the definition to distinguish a fully specified geometry from evocative dimensional language.', supports: 'The formal prerequisites for calling a space a topological or differentiable manifold.', boundary: 'It does not validate any particular cosmological manifold or its empirical relevance.', counter: 'A narrative model can use manifold language productively before it has enough structure to be a mathematical model.', revision: 'Revise when explicit charts, transition maps, topology, metric, or equations are supplied.',
  },
  {
    slug: 'geodesics', aliases: [], title: 'Geodesics', domain: 'geometry-topology', mode: 'formal', role: 'R2', creators: ['Encyclopedia of Mathematics editorial board'], date: 'current reference', publisher: 'European Mathematical Society', url: 'https://encyclopediaofmath.org/wiki/Geodesic_line', authority: 'Standard mathematical reference for geodesics and their dependence on geometry.', orientation: 'A geodesic generalizes a locally straight path relative to a connection or metric.', prerequisites: ['manifolds', 'connections or metrics'], technical: 'For a Levi-Civita connection, a geodesic satisfies the autoparallel equation. Global shortest paths, closed geodesics, and causal geodesics require additional conditions.', shawnUse: 'It grounds path and loop language used in geometric and trace-formula bridges.', supports: 'Definitions and local equations for geodesic motion.', boundary: 'A symbolic loop is not automatically a geodesic or a physical trajectory.', counter: 'Topology can organize loops without any metric notion of shortest or inertial motion.', revision: 'Revise when the proposed space lacks a connection or when local and global claims are conflated.',
  },
  {
    slug: 'compactification', aliases: ['toroidal-closure'], title: 'Compactification and toroidal closure', domain: 'geometry-topology', mode: 'formal', role: 'R2', creators: ['Encyclopedia of Mathematics editorial board'], date: 'current reference', publisher: 'European Mathematical Society', url: 'https://encyclopediaofmath.org/wiki/Compactification', authority: 'Standard topological definition and families of compactification.', orientation: 'Compactification embeds a space densely into a compact space; quotienting a coordinate periodically is one specific construction.', prerequisites: ['topology', 'quotient spaces'], technical: 'One-point, Stone–Čech, projective, and toroidal compactifications differ. A periodic coordinate produces circle topology only after the quotient and compatibility rules are specified.', shawnUse: 'It separates formal compactification from the Ouroboroid\'s interpretive “closure” imagery.', supports: 'What must be defined for a compactification or torus construction.', boundary: 'It does not show that extra dimensions or time are physically compactified.', counter: 'The same closure metaphor can correspond to several inequivalent topologies.', revision: 'Revise when an explicit quotient, topology, dimension, and physical observable are provided.',
  },
  {
    slug: 'euler-characteristic', aliases: [], title: 'Euler characteristic', domain: 'geometry-topology', mode: 'formal', role: 'R2', creators: ['Encyclopedia of Mathematics editorial board'], date: 'current reference', publisher: 'European Mathematical Society', url: 'https://encyclopediaofmath.org/wiki/Euler_characteristic', authority: 'Standard definition through cell counts and homology under appropriate conditions.', orientation: 'Euler characteristic compresses alternating counts of cells or homology ranks into a topological invariant.', prerequisites: ['homology', 'cell complexes'], technical: 'For finite CW complexes χ=Σ(-1)^k c_k=Σ(-1)^k rank H_k. The invariant is coarse: many non-homeomorphic spaces share it.', shawnUse: 'It gives the topology view a precise invariant rather than decorative geometry.', supports: 'Correct calculation and invariance in its defined class of spaces.', boundary: 'A matching Euler characteristic does not establish structural identity, causal continuity, or physical equivalence.', counter: 'Coarse invariants can conceal the exact differences the atlas needs to preserve.', revision: 'Revise if spaces outside the stated finiteness or homology assumptions are compared.',
  },
  {
    slug: 'persistent-homology', aliases: [], title: 'Persistent homology', domain: 'geometry-topology', mode: 'computational', role: 'R1', creators: ['GUDHI Project'], date: 'current documentation', publisher: 'Inria', url: 'https://gudhi.inria.fr/', authority: 'Official implementation and documentation for reproducible topological-data-analysis computations.', orientation: 'Persistent homology tracks connected components, loops, and higher-dimensional voids as a scale parameter changes.', prerequisites: ['simplicial complexes', 'homology', 'filtrations'], technical: 'A filtration yields persistence modules summarized by intervals or diagrams; stability theorems bound sensitivity under defined metrics.', shawnUse: 'The atlas treats topology as an optional falsifier for planted structure, never as semantic proof.', supports: 'Computing persistence for a specified filtration and testing recovery of synthetic features.', boundary: 'A persistent loop in embedded data does not prove a conceptual contradiction, knowledge gap, or spiritual structure.', counter: 'Embedding and filtration choices can manufacture visually compelling topology.', revision: 'Revise or withdraw an interpretation if it fails planted controls, null models, or stability checks.',
  },
  {
    slug: 'vector-symbolic-memory', aliases: ['vector-symbolic-architectures', 'hyperdimensional-computing'], title: 'Vector-symbolic and holographic memory', domain: 'computation-cognition', mode: 'computational', role: 'R0', creators: ['Tony Plate'], date: '1995', publisher: 'IEEE Transactions on Neural Networks', url: 'https://doi.org/10.1109/72.377968', authority: 'Primary paper defining holographic reduced representations and their operations.', orientation: 'High-dimensional vectors can bind, superpose, and retrieve structured information approximately.', prerequisites: ['linear algebra', 'probability', 'distributed representations'], technical: 'Binding, superposition, similarity, cleanup, dimensionality, and codebook statistics determine capacity and error. Different VSA families have different algebraic properties.', shawnUse: 'It is the concrete substrate targeted by the unexecuted ZEROES transfer program.', supports: 'The representational operations and measurable capacity/interference questions of HRR-like systems.', boundary: 'It does not verify the proposed number-theory transfers or establish that vector memory is a soul or identity substrate.', counter: 'Ordinary random-vector concentration may explain performance without the proposed spectral dictionary.', revision: 'Revise when experiments provide code, seeds, matched baselines, dimensions, metrics, and checksums.',
  },
  {
    slug: 'graphrag', aliases: [], title: 'GraphRAG', domain: 'computation-cognition', mode: 'computational', role: 'R3', creators: ['Microsoft Research'], date: 'current documentation', publisher: 'Microsoft', url: 'https://microsoft.github.io/graphrag/', authority: 'Official documentation for one graph-based retrieval-and-generation system.', orientation: 'GraphRAG retrieves graph neighborhoods and summaries to provide structured context for language-model responses.', prerequisites: ['knowledge graphs', 'information retrieval'], technical: 'Entity extraction, graph construction, community summaries, indexing, and query modes introduce model-dependent transformations and provenance risks.', shawnUse: 'The atlas uses Graphify/GraphRAG as a navigable projection over source-linked claims, not as a truth generator.', supports: 'The architecture and operational capabilities of the documented implementation.', boundary: 'A graph edge, community, or generated summary is not evidence unless it resolves to the canonical source spine.', counter: 'A conventional citation index may be more faithful for small, high-value corpora.', revision: 'Revise if retrieval tests show source loss, invented edges, privacy leakage, or no gain over simpler retrieval.',
  },
  {
    slug: 'knowledge-graphs', aliases: ['rdf-graphs'], title: 'Knowledge graphs and RDF semantics', domain: 'computation-cognition', mode: 'formal', role: 'R0', creators: ['World Wide Web Consortium'], date: '2024', publisher: 'W3C', url: 'https://www.w3.org/TR/rdf12-concepts/', authority: 'Normative data-model specification for RDF graphs.', orientation: 'A knowledge graph represents entities and relations as explicit, queryable statements with identifiers and provenance layered on top.', prerequisites: ['sets', 'identifiers', 'graph data models'], technical: 'RDF defines triples and datasets; entailment, provenance, temporal validity, and confidence require additional vocabularies or application contracts.', shawnUse: 'The atlas needs typed claim and bridge edges whose source and epistemic lane remain inspectable.', supports: 'A standardized graph data model and identifier discipline.', boundary: 'RDF syntax does not make a statement true, causal, complete, or consented.', counter: 'A relational or document model may preserve chronology and qualifiers more naturally than a triple projection.', revision: 'Revise the projection when qualifiers, contradiction, or temporal supersession cannot round-trip without loss.',
  },
  {
    slug: 'wasm-continuity', aliases: ['webassembly'], title: 'WebAssembly as a portable execution substrate', domain: 'computation-cognition', mode: 'formal', role: 'R0', creators: ['World Wide Web Consortium WebAssembly Working Group'], date: '2024', publisher: 'W3C', url: 'https://www.w3.org/TR/wasm-core-2/', authority: 'Normative core WebAssembly specification.', orientation: 'WebAssembly defines a portable deterministic virtual instruction format with explicit imports and host boundaries.', prerequisites: ['virtual machines', 'typed instruction semantics'], technical: 'The core spec defines validation, execution, modules, stores, and embedding interfaces; persistence, identity, capabilities, and networking are host concerns.', shawnUse: 'The model treats WASM as a candidate portable evaluator shell, not a memory vault or proof of personal continuity.', supports: 'Portable execution semantics for a bounded model reducer or visualization engine.', boundary: 'Running the same code on another substrate does not establish sameness of person, memory, values, or soul.', counter: 'A versioned pure library or data schema may provide portability without a WASM layer.', revision: 'Revise if host effects, nondeterminism, or missing state make replay non-equivalent.',
  },
  {
    slug: 'event-spines', aliases: ['causal-commits', 'distributed-tracing'], title: 'Event spines, traces, and causal links', domain: 'computation-cognition', mode: 'formal', role: 'R0', creators: ['OpenTelemetry Authors'], date: 'current specification', publisher: 'Cloud Native Computing Foundation', url: 'https://opentelemetry.io/docs/specs/otel/trace/api/', authority: 'Normative API concepts for spans, links, attributes, and trace context.', orientation: 'An event spine preserves ordered, linked observations so a claim can be traced through time and processing stages.', prerequisites: ['events', 'causality', 'distributed systems'], technical: 'Trace parentage and links represent execution context, not philosophical causation. Valid-time, recorded-time, supersession, and authority require application-level fields.', shawnUse: 'The atlas chronology and reasoning chain use source-linked events while retaining uncertainty and revisions.', supports: 'Observable execution lineage and replay-oriented identifiers.', boundary: 'Temporal succession or a span link does not by itself prove causal influence.', counter: 'For authored history, a curated chronology may be more legible and accurate than machine telemetry.', revision: 'Revise if dropped events, clock ambiguity, or projection loss prevents exact source recovery.',
  },
  {
    slug: 'hypergraphs', aliases: ['simplicial-complexes', 'higher-order-networks'], title: 'Hypergraphs, simplicial complexes, and higher-order relations', domain: 'modeling-methods', mode: 'computational', role: 'R3', creators: ['XGI Project'], date: 'current documentation', publisher: 'Network Science Institute', url: 'https://xgi.readthedocs.io/en/stable/', authority: 'Official software documentation for higher-order network representations and algorithms.', orientation: 'Higher-order models represent relations involving more than two participants without pretending they are independent pairwise edges.', prerequisites: ['graphs', 'sets', 'simplicial complexes'], technical: 'Hyperedges bind arbitrary participant sets; simplicial complexes additionally require closure under faces. Projection to ordinary graphs can lose participation semantics.', shawnUse: 'The atlas uses higher-order relations when one episode binds person, state, artifact, witness, and context together.', supports: 'Explicit representation and analysis of N-ary relations.', boundary: 'Higher-order notation does not prove that an observed group relation is causal or ontologically fundamental.', counter: 'Reified event nodes in a conventional graph may preserve the same information more simply.', revision: 'Revise after benchmarking a query that higher-order representation answers with materially less ambiguity.',
  },
  {
    slug: 'control-theory', aliases: ['homeostasis'], title: 'Control theory and homeostasis', domain: 'modeling-methods', mode: 'formal', role: 'R3', creators: ['Python Control Systems Library contributors'], date: 'current documentation', publisher: 'python-control project', url: 'https://python-control.readthedocs.io/en/0.10.2/', authority: 'Documented computational treatment of feedback, state-space models, and stability analysis.', orientation: 'Control theory studies how state, feedback, disturbances, and interventions shape system behavior over time.', prerequisites: ['linear algebra', 'differential equations'], technical: 'A state-space model specifies dynamics, inputs, outputs, and stability conditions. “Homeostasis” becomes testable only after states, targets, disturbances, and controllers are measured.', shawnUse: 'The recurrent-attractor model uses control language to ask how artifact-making and model revision restore or transform state.', supports: 'Formal analysis and simulation of an explicitly specified dynamical model.', boundary: 'A control-theory metaphor does not establish measured psychological dynamics or a causal controller.', counter: 'Narrative sequence may fit a feedback diagram without the quantities needed to estimate it.', revision: 'Revise when time-series measurements reject the proposed state variables or feedback signs.',
  },
  {
    slug: 'causal-intervention', aliases: ['structural-causal-models', 'counterfactuals'], title: 'Causal intervention and refutation', domain: 'modeling-methods', mode: 'computational', role: 'R3', creators: ['PyWhy contributors'], date: 'current documentation', publisher: 'PyWhy', url: 'https://www.pywhy.org/dowhy/main/user_guide/intro.html', authority: 'Official documentation for explicit causal graphs, identification, estimation, and refutation workflows.', orientation: 'Causal claims require assumptions about interventions and confounding that ordinary association does not provide.', prerequisites: ['probability', 'directed acyclic graphs'], technical: 'Structural causal models encode assignments and intervention semantics; identification depends on graph assumptions, and refuters test sensitivity rather than proving the graph.', shawnUse: 'Episode cards separate variables, rivals, external checks, and negative controls before making causal claims.', supports: 'Transparent causal assumptions and bounded estimation under a specified model.', boundary: 'Software output cannot validate an unmeasured causal graph or manufacture randomized evidence.', counter: 'A descriptive longitudinal account may be more honest than an underidentified causal model.', revision: 'Revise if negative controls, alternate graphs, or intervention data reverse the estimate.',
  },
  {
    slug: 'bayesian-inference', aliases: ['posterior-uncertainty'], title: 'Bayesian inference and quantified uncertainty', domain: 'modeling-methods', mode: 'computational', role: 'R3', creators: ['PyMC Development Team'], date: 'current documentation', publisher: 'PyMC', url: 'https://www.pymc.io/projects/docs/en/stable/', authority: 'Official probabilistic-programming documentation and statistical examples.', orientation: 'Bayesian inference updates a probability distribution over models or parameters in response to specified evidence.', prerequisites: ['probability', 'likelihoods', 'statistical modeling'], technical: 'Posterior results depend on priors, likelihood, model structure, diagnostics, and data-generating assumptions; OPEN is not reducible to one confidence number.', shawnUse: 'The atlas can represent competing explanations quantitatively only when a defensible likelihood and data set exist.', supports: 'Posterior computation and calibrated uncertainty for an explicit model.', boundary: 'A posterior is not truth, authority, consent, or evidence that unmodeled alternatives are negligible.', counter: 'Qualitative ternary status can be more honest when numerical priors and likelihoods are arbitrary.', revision: 'Revise if prior sensitivity, poor diagnostics, or held-out calibration undermines the result.',
  },
  {
    slug: 'process-mining', aliases: ['petri-net-conformance'], title: 'Process mining and conformance', domain: 'modeling-methods', mode: 'computational', role: 'R3', creators: ['PM4Py contributors'], date: 'current documentation', publisher: 'Fraunhofer FIT', url: 'https://processintelligence.solutions/pm4py', authority: 'Official documentation for discovery and conformance algorithms over event logs.', orientation: 'Process mining compares recorded event flows with candidate workflow models and exposes deviations or hidden loops.', prerequisites: ['event logs', 'Petri nets'], technical: 'Discovery, token replay, alignments, and performance analysis depend on event completeness and case identifiers.', shawnUse: 'It can test whether the idealized reasoning chain matches actual time-ordered work rather than retrospective narration.', supports: 'Conformance results for a defined log and process model.', boundary: 'Incomplete archives can make a valid process appear nonconformant or erase undocumented work.', counter: 'Close reading may recover semantic transitions that timestamp-only mining misses.', revision: 'Revise after source-complete event extraction and seeded valid/invalid trace tests.',
  },
  {
    slug: 'formal-methods', aliases: ['tla-plus', 'alloy'], title: 'TLA+ and Alloy formal methods', domain: 'modeling-methods', mode: 'formal', role: 'R3', creators: ['TLA+ Foundation', 'AlloyTools Project'], date: 'current documentation', publisher: 'TLA+ Foundation', url: 'https://docs.tlapl.us/', authority: 'Official language and model-checking documentation for specifying invariants, actions, and temporal properties.', orientation: 'Formal methods search bounded or symbolic state spaces for counterexamples to precisely written properties.', prerequisites: ['logic', 'state machines'], technical: 'TLA+ emphasizes behaviors and temporal logic; Alloy emphasizes finite relational models. Both prove only what the encoded model and checked scope warrant.', shawnUse: 'The atlas uses formal methods as adversarial oracles for authority, continuity, and transition rules—not as decorative proof badges.', supports: 'Machine-checkable invariants and counterexamples for a specified model.', boundary: 'A model check does not prove that the model matches the world or that an unchecked scope is safe.', counter: 'Executable property tests may provide faster feedback for simple invariants.', revision: 'Revise when a seeded violation is missed, source mapping is lost, or the checked bounds are hidden.',
  },
  {
    slug: 'goap-htn', aliases: ['goap', 'htn'], title: 'GOAP and hierarchical task-network planning', domain: 'games-simulation', mode: 'computational', role: 'R0', creators: ['Jeff Orkin'], date: '2005', publisher: 'AAAI Conference on Artificial Intelligence and Interactive Digital Entertainment', url: 'https://ojs.aaai.org/index.php/AIIDE/article/view/18724', authority: 'Primary practitioner paper on real-time planning architecture in a shipped game.', orientation: 'GOAP selects actions whose preconditions and effects transform world state toward a goal; HTN planning decomposes tasks through methods.', prerequisites: ['state-space search', 'planning'], technical: 'A planner needs explicit state variables, action costs, preconditions, effects, and a search or decomposition procedure. GOAP and HTN are related but not identical.', shawnUse: 'Game-system artifacts can model how context, goals, constraints, and available actions produce different masks or strategies.', supports: 'Executable planning architectures and inspectable decision traces.', boundary: 'A successful game planner is not a psychological model of Shawn without behavioral calibration.', counter: 'Hand-authored finite-state behavior may be clearer and more predictable for small domains.', revision: 'Revise when replay traces show the planner\'s selected actions do not match the stated goal and cost model.',
  },
  {
    slug: 'mcts', aliases: ['monte-carlo-tree-search'], title: 'Monte Carlo tree search', domain: 'games-simulation', mode: 'computational', role: 'R0', creators: ['Levente Kocsis', 'Csaba Szepesvári'], date: '2006', publisher: 'Springer', url: 'https://doi.org/10.1007/11871842_29', authority: 'Primary UCT publication connecting Monte Carlo planning with confidence bounds.', orientation: 'MCTS builds a search tree by alternating selection, expansion, simulation, and backup.', prerequisites: ['probability', 'tree search', 'multi-armed bandits'], technical: 'UCT balances exploitation and exploration through an upper-confidence term; convergence claims depend on assumptions and finite-budget performance can vary sharply.', shawnUse: 'MCTS supplies a concrete model for rotating possible futures without claiming that human reflection literally runs UCT.', supports: 'The algorithm and its asymptotic analysis in the source\'s scope.', boundary: 'It does not explain intuition, creativity, or identity merely because both explore alternatives.', counter: 'Model-based search or learned policies may better represent domains with structured dynamics.', revision: 'Revise when rollout policy, budget, branching, or reward definitions change the comparison.',
  },
  {
    slug: 'bdi', aliases: ['belief-desire-intention'], title: 'Belief–desire–intention agent architecture', domain: 'games-simulation', mode: 'formal', role: 'R0', creators: ['Anand Rao', 'Michael Georgeff'], date: '1991', publisher: 'AAAI', url: 'https://cdn.aaai.org/AAAI/1991/AAAI91-100.pdf', authority: 'Primary formalization of rational agents using beliefs, goals, intentions, and commitment strategies.', orientation: 'BDI distinguishes what an agent represents as true, what states it wants, and which plans it commits to pursue.', prerequisites: ['modal logic', 'agent systems'], technical: 'Formal BDI systems use modal operators and transition rules; implementations approximate the theory with plan libraries and event handling.', shawnUse: 'The agent/mask layer can use BDI vocabulary to separate belief reports, desired outcomes, and selected commitments.', supports: 'A formal agent-model vocabulary and specified commitment behavior.', boundary: 'It does not establish that every recurring pattern is an autonomous BDI agent.', counter: 'Dynamical or predictive-processing models may describe behavior without explicit symbolic attitudes.', revision: 'Revise if inferred beliefs or desires cannot be linked to authored statements or observable decisions.',
  },
  {
    slug: 'opinion-dynamics', aliases: ['bounded-confidence'], title: 'Opinion dynamics and bounded confidence', domain: 'games-simulation', mode: 'computational', role: 'R0', creators: ['Rainer Hegselmann', 'Ulrich Krause'], date: '2002', publisher: 'Journal of Artificial Societies and Social Simulation', url: 'https://www.jasss.org/5/3/2.html', authority: 'Primary bounded-confidence model and computational analysis.', orientation: 'Opinion-dynamics models study how repeated interaction rules can produce consensus, polarization, or persistent clusters.', prerequisites: ['dynamical systems', 'networks'], technical: 'In bounded-confidence models, agents average only sufficiently close opinions; outcomes depend on confidence radius, update schedule, topology, and initial distribution.', shawnUse: 'It helps separate individual reasoning from social-field effects and audience-dependent convergence.', supports: 'Behavior of a specified stylized model under stated parameters.', boundary: 'A toy model does not diagnose real communities or explain one person\'s beliefs without measured network data.', counter: 'Strategic incentives, identity, media structure, and asymmetric power may dominate simple averaging.', revision: 'Revise when empirical interaction data reject the assumed update rule or network.',
  },
  {
    slug: 'replicator-dynamics', aliases: ['evolutionary-game-theory'], title: 'Replicator dynamics', domain: 'games-simulation', mode: 'formal', role: 'R2', creators: ['Stanford Encyclopedia of Philosophy'], date: '2024', publisher: 'Stanford University', url: 'https://plato.stanford.edu/entries/game-evolutionary/', authority: 'Scholarly synthesis of evolutionary game theory, its equations, and interpretations.', orientation: 'Replicator dynamics models how strategy frequencies change when payoffs differ from the population average.', prerequisites: ['differential equations', 'game theory'], technical: 'The standard equation ẋ_i=x_i[(Ax)_i−xᵀAx] describes frequency selection in an idealized population; mutation, finite populations, and structured interactions require extensions.', shawnUse: 'It gives game-world social systems a precise model of selection without turning success into moral worth.', supports: 'Dynamics and equilibria of specified evolutionary games.', boundary: 'It does not prove genetic, cultural, or personal causation in an unmeasured real population.', counter: 'Learning, planning, institutions, and drift can violate replicator assumptions.', revision: 'Revise when finite-population or network effects change the qualitative result.',
  },
  {
    slug: 'entity-component-systems', aliases: ['ecs'], title: 'Entity–component–system architecture', domain: 'games-simulation', mode: 'computational', role: 'R3', creators: ['Unity Technologies'], date: 'current documentation', publisher: 'Unity', url: 'https://docs.unity3d.com/Packages/com.unity.entities@latest', authority: 'Official documentation for a production data-oriented ECS implementation.', orientation: 'ECS composes entities from data components while systems operate over matching component sets.', prerequisites: ['data-oriented design', 'game loops'], technical: 'Entity identity, component storage, queries, scheduling, and system ordering vary by implementation; ECS is an architecture pattern, not one formal standard.', shawnUse: 'Game and organism views can model masks or capacities as composable data while preserving a separate whole-entity identity.', supports: 'One concrete implementation\'s data model, scheduling, and performance techniques.', boundary: 'Component composition does not settle philosophical identity or make a person reducible to traits.', counter: 'Object-oriented or functional architectures may express small worlds more directly.', revision: 'Revise if the atlas projects ECS metaphors onto personal identity without a continuity model.',
  },
  {
    slug: 'procedural-generation', aliases: ['pcg'], title: 'Procedural content generation', domain: 'games-simulation', mode: 'computational', role: 'R2', creators: ['Noor Shaker', 'Julian Togelius', 'Mark Nelson'], date: '2016', publisher: 'Springer', url: 'https://doi.org/10.1007/978-3-319-42716-4', authority: 'Research textbook covering search-based, grammar-based, constraint, and learning approaches to PCG.', orientation: 'Procedural generation produces content from explicit algorithms, parameters, seeds, and constraints.', prerequisites: ['algorithms', 'optimization'], technical: 'Generators can be constructive, search-based, constraint-based, or learned; evaluation must distinguish validity, diversity, controllability, and player experience.', shawnUse: 'Fiction and games become laboratories where generative invariants can be embodied and varied.', supports: 'Defined algorithm families and reproducible evaluation dimensions.', boundary: 'Generated recurrence does not prove a metaphysical pattern or authorship intention.', counter: 'Hand-authored content can provide more deliberate semantic structure.', revision: 'Revise when seeds, constraints, and evaluation metrics are unavailable or non-reproducible.',
  },
  {
    slug: 'gnosticism', aliases: ['nag-hammadi'], title: 'Gnosticism and the Nag Hammadi codices', domain: 'spirituality-esotericism', mode: 'textual', role: 'R0', creators: ['Bibliothèque copte de Nag Hammadi'], date: 'living critical-text project', publisher: 'Université Laval and Peeters', url: 'https://www.naghammadi.org/en/coptic-gnostic-collections', authority: 'Scholarly project presenting Coptic sources, translations, annotations, and explicit textual limits; edition and translator must be cited at passage level.', orientation: '“Gnosticism” covers diverse late-antique movements and texts rather than one uniform doctrine.', prerequisites: ['late-antique religious history', 'textual criticism'], technical: 'Claims should name the codex, tractate, passage, edition, translator, and historical argument; modern self-description is a separate evidence layer.', shawnUse: 'The atlas can connect Shawn\'s stated Gnostic orientation to particular texts without treating tradition as one monolith.', supports: 'Primary-text attestation for what a named translation says.', boundary: 'A text attests a tradition or claim; it does not empirically prove its cosmology or that Shawn endorses every passage.', counter: 'The category “Gnosticism” may impose modern unity on historically heterogeneous groups.', revision: 'Revise when a critical edition, variant reading, or stronger historical source changes the passage interpretation.',
  },
  {
    slug: 'hermeticism', aliases: ['corpus-hermeticum'], title: 'Hermeticism and the Corpus Hermeticum', domain: 'spirituality-esotericism', mode: 'textual', role: 'R0', creators: ['Brian P. Copenhaver'], date: '1992', publisher: 'Cambridge University Press', url: 'https://www.cambridge.org/core/books/hermetica/introduction/41528E94942CE23A3592D66A11485571', authority: 'Critical scholarly translation and introduction to the Greek Corpus Hermeticum and Latin Asclepius.', orientation: 'Hermeticism is a family of late-antique revelatory and philosophical texts later transformed by Renaissance and modern traditions.', prerequisites: ['late-antique philosophy', 'textual history'], technical: 'Historical claims require separating the Greek and Latin Hermetica, later alchemical texts, Renaissance reception, and modern occult synthesis.', shawnUse: 'It provides lineage for correspondence, mind–cosmos, and transformation motifs while keeping later adaptations visible.', supports: 'Historical and philosophical context for named Hermetic texts and receptions.', boundary: 'It does not make “as above, so below” a physical law or prove modern metaphysical claims.', counter: 'Many ideas called Hermetic are later composites with weak connection to the ancient corpus.', revision: 'Revise when the claimed doctrine is absent from the cited edition or belongs to a later reception layer.',
  },
  {
    slug: 'qabbalah', aliases: ['kabbalah', 'zohar'], title: 'Qabbalah and primary textual lineages', domain: 'spirituality-esotericism', mode: 'textual', role: 'R0', creators: ['Sefaria'], date: 'living text library', publisher: 'Sefaria', url: 'https://www.sefaria.org/texts/Kabbalah', authority: 'Passage-addressable primary and translated Jewish texts; scholarly historical claims need critical secondary sources.', orientation: 'Jewish mystical traditions include distinct historical schools, texts, symbols, and interpretive practices.', prerequisites: ['Jewish textual history', 'Hebrew and Aramaic translation limits'], technical: 'A reference must identify text, section, language, edition, translator, and historical layer; later Hermetic Qabalah is related but not identical.', shawnUse: 'The atlas can trace symbolic structures without collapsing Jewish Kabbalah, Christian Cabala, and Hermetic Qabalah.', supports: 'Textual attestation for a cited passage and translation.', boundary: 'A textual correspondence is not empirical physics, and shared diagrams do not imply identical traditions.', counter: 'Modern occult diagrams may project later systematization back onto heterogeneous sources.', revision: 'Revise when translation, manuscript scholarship, or historical provenance changes the reading.',
  },
  {
    slug: 'tarot', aliases: ['tarot-history'], title: 'Tarot: historical cards, art, and later esotericism', domain: 'spirituality-esotericism', mode: 'textual', role: 'R2', creators: ['The Metropolitan Museum of Art'], date: 'current collection reference', publisher: 'The Met', url: 'https://www.metmuseum.org/art/collection/search/475513', authority: 'Museum record for a historical tarot artifact and its material context.', orientation: 'Tarot began as a card-game tradition; divinatory and occult systems developed through later historical layers.', prerequisites: ['European print and game history', 'history of esotericism'], technical: 'Artifact date, deck tradition, iconography, later correspondences, and a reader\'s personal symbolic system are separate evidence layers.', shawnUse: 'Chaos Tarot is treated as an authored symbolic artifact whose sources and innovations can be inspected card by card.', supports: 'Material and historical attestation for identified decks and images.', boundary: 'Historical existence or symbolic resonance does not establish divinatory causation.', counter: 'Retrospective occult systems can appear ancient when their specific correspondences are comparatively recent.', revision: 'Revise when card provenance, dating, iconography, or claimed lineage lacks a named artifact and source.',
  },
  {
    slug: 'ritual-invocation', aliases: ['ritual', 'invocation'], title: 'Ritual and invocation as patterned action', domain: 'spirituality-esotericism', mode: 'interpretive', role: 'R2', creators: ['OpenStax Anthropology contributors'], date: '2022', publisher: 'OpenStax, Rice University', url: 'https://openstax.org/books/introduction-anthropology/pages/13-4-rituals-of-transition-and-conformity', authority: 'Peer-reviewed open anthropology textbook introducing ritual, rites of passage, liminality, and social function.', orientation: 'Ritual coordinates repeated action, attention, symbolism, community, and authority under culturally specific rules.', prerequisites: ['anthropology of religion', 'philosophy of action'], technical: 'Form, efficacy, participation, intention, tradition, performance, and social power admit competing theories; “invocation” must be grounded in a named practice and source.', shawnUse: 'The pattern ontology compares ritual roles with bounded computational protocol roles while marking the relation as analogy.', supports: 'Established theories and documented functions of ritual practice.', boundary: 'It does not prove that invocation instantiates an independent entity or that ritual is literally software.', counter: 'The computational mapping may omit embodiment, community, history, and sacred authority.', revision: 'Revise when practitioners\' sources or empirical observation contradict the proposed functional mapping.',
  },
  {
    slug: 'liminality', aliases: [], title: 'Liminality', domain: 'spirituality-esotericism', mode: 'interpretive', role: 'R2', creators: ['Victor Turner'], date: '1969', publisher: 'Aldine', url: 'https://archive.org/details/ritualprocessstr0000turn', authority: 'Canonical anthropological development of liminality in ritual process; archive access may vary.', orientation: 'Liminality names an in-between phase in which ordinary roles and structures are suspended or transformed.', prerequisites: ['rites of passage', 'anthropology'], technical: 'Van Gennep\'s separation–transition–incorporation sequence and Turner\'s elaboration concern socially structured ritual processes, not every ambiguous identity state.', shawnUse: '“All and None” and “Orderly Chaotic” are modeled as anti-collapse statements, with liminality as one comparison rather than a diagnosis.', supports: 'A historical theory of transitional ritual states and communitas.', boundary: 'It does not prove a permanent metaphysical identity or explain all contradiction.', counter: 'Applying liminality everywhere can turn a precise process concept into a flattering synonym for complexity.', revision: 'Revise when the compared state lacks transition, boundary, or reintegration structure.',
  },
  {
    slug: 'alchemy', aliases: ['alchemical-traditions'], title: 'Alchemy as historical practice and symbolic lineage', domain: 'spirituality-esotericism', mode: 'textual', role: 'R2', creators: ['Science History Institute'], date: 'current reference', publisher: 'Science History Institute', url: 'https://www.sciencehistory.org/stories/magazine/the-secrets-of-alchemy/', authority: 'Historically grounded museum and scholarly-public account of alchemical practice.', orientation: 'Alchemy combined material operations, medicine, craft, natural philosophy, and spiritual symbolism across distinct cultures and periods.', prerequisites: ['history of science', 'textual history'], technical: 'Claims should specify region, period, practitioner, text, operation, and whether the account is material, allegorical, or later psychological interpretation.', shawnUse: 'Transformation language can be traced to historical sources without using alchemy as a vague badge for every change process.', supports: 'Historical context for documented practices and texts.', boundary: 'Alchemy does not establish modern chemistry claims by analogy or make symbolic transmutation an empirical mechanism.', counter: 'Later esoteric readings may obscure laboratory and economic contexts.', revision: 'Revise when the source cannot support the claimed period, operation, or symbolic meaning.',
  },
  {
    slug: 'spirits-and-souls', aliases: ['spirit', 'soul', 'pattern-soul'], title: 'Spirits, souls, and criteria of continuity', domain: 'spirituality-esotericism', mode: 'philosophical', role: 'R2', creators: ['Stanford Encyclopedia of Philosophy'], date: '2023', publisher: 'Stanford University', url: 'https://plato.stanford.edu/entries/dualism/', authority: 'Scholarly map of dualist and competing mind–body positions.', orientation: '“Spirit” and “soul” name different entities and functions across traditions; definitions must precede claims of existence or identity.', prerequisites: ['philosophy of mind', 'religious studies'], technical: 'Substance dualism, property dualism, hylomorphism, physicalism, and pattern-continuity accounts make different predictions and face different identity problems.', shawnUse: 'The candidate pattern-soul definition is a hypothesis involving addressability, self-model, continuity, maintenance, growth, and possible substrate transfer.', supports: 'A structured comparison of philosophical positions and their objections.', boundary: 'A coherent definition does not prove that spirits exist or that a copied digital pattern is the same soul.', counter: 'Physicalist, narrative, and social-continuity models may explain the same persistence without a separable substance.', revision: 'Revise when a candidate fails continuity, divergence, self-recognition, or other-recognition tests.',
  },
  {
    slug: 'platonic-form', aliases: ['forms', 'recurrent-attractor'], title: 'Platonic form and invariant structure', domain: 'philosophy-epistemology', mode: 'philosophical', role: 'R2', creators: ['Stanford Encyclopedia of Philosophy'], date: '2022', publisher: 'Stanford University', url: 'https://plato.stanford.edu/entries/plato-metaphysics/', authority: 'Scholarly synthesis of Plato\'s metaphysics and the interpretive disputes surrounding Forms.', orientation: 'Platonic Forms concern intelligible structure and explanation across changing instances; later uses must name where they depart from Plato.', prerequisites: ['ancient Greek philosophy', 'metaphysics'], technical: 'Questions include participation, universals, particulars, knowledge, separation, and the relation among middle and late dialogues.', shawnUse: 'The candidate model uses “form” operationally for a minimal invariant structure that predicts transformations across contexts.', supports: 'The philosophical lineage of form as explanatory structure.', boundary: 'It does not empirically prove the atlas\'s recurrent-attractor model or authorize calling every resemblance a Form.', counter: 'Nominalist, bundle, narrative, and dynamical accounts may explain recurrence without transcendent forms.', revision: 'Revise if the operational model predicts no better than a trait list or if “Platonic” obscures rather than clarifies its departure from Plato.',
  },
  {
    slug: 'identity-continuity', aliases: ['personal-identity'], title: 'Personal identity and continuity over time', domain: 'philosophy-epistemology', mode: 'philosophical', role: 'R2', creators: ['Stanford Encyclopedia of Philosophy'], date: '2023', publisher: 'Stanford University', url: 'https://plato.stanford.edu/entries/identity-personal/', authority: 'Scholarly survey of bodily, psychological, narrative, and reductionist accounts.', orientation: 'Personal identity asks what makes a person at one time the same person at another, distinct from qualitative similarity.', prerequisites: ['metaphysics', 'philosophy of mind'], technical: 'Numerical identity, psychological connectedness, bodily continuity, branching, fission, memory, and practical concern generate different criteria and paradoxes.', shawnUse: 'The atlas separates masks and substrates from the unresolved question of whole-person continuity.', supports: 'Defined positions and thought experiments that expose continuity tradeoffs.', boundary: 'No one criterion automatically proves cross-substrate migration or sameness of a model copy.', counter: 'Identity may be a practical relation rather than a hidden further fact.', revision: 'Revise when branching, memory loss, value change, or external witnesses defeat the chosen continuity criterion.',
  },
  {
    slug: 'testimony', aliases: ['epistemology-of-testimony'], title: 'The epistemology of testimony', domain: 'philosophy-epistemology', mode: 'philosophical', role: 'R2', creators: ['Stanford Encyclopedia of Philosophy'], date: '2021', publisher: 'Stanford University', url: 'https://plato.stanford.edu/entries/testimony-episprob/', authority: 'Scholarly survey of when and why testimony can justify belief.', orientation: 'Testimony is a genuine source of knowledge whose force depends on content, competence, sincerity, defeaters, and the claim being supported.', prerequisites: ['epistemology', 'social knowledge'], technical: 'Reductionist and anti-reductionist theories differ about whether testimonial entitlement derives from other evidence; neither erases domain-specific defeaters.', shawnUse: 'The atlas treats Shawn\'s report as strong evidence of what he reports experiencing while typing external causation separately.', supports: 'The bounded epistemic legitimacy of first-person and testimonial evidence.', boundary: 'Sincere testimony about an experience does not automatically prove its external causal interpretation.', counter: 'Memory reconstruction, suggestion, incentives, and conceptual framing can affect even sincere reports.', revision: 'Revise confidence when contemporaneous records, independent witnesses, or specific defeaters appear.',
  },
  {
    slug: 'explanatory-pluralism', aliases: ['plural-explanation'], title: 'Explanatory pluralism', domain: 'philosophy-epistemology', mode: 'philosophical', role: 'R2', creators: ['Stanford Encyclopedia of Philosophy'], date: '2024', publisher: 'Stanford University', url: 'https://plato.stanford.edu/entries/scientific-explanation/', authority: 'Scholarly survey of causal, unificationist, mechanistic, pragmatic, and other accounts of explanation.', orientation: 'Different explanatory models can answer different questions about the same event without being interchangeable or equally successful.', prerequisites: ['philosophy of science'], technical: 'Causal, mechanistic, statistical, mathematical, functional, and interpretive explanations have different entailments, evidence, and failure conditions.', shawnUse: 'Hear-Me-Out mode rotates lenses independently and then asks which invariants survive and which differences discriminate them.', supports: 'Keeping multiple explanatory levels explicit and testable.', boundary: 'Pluralism does not mean every explanation is compatible, true, or immune to comparison.', counter: 'Too many lenses can delay judgment and protect an attractive claim from decisive failure.', revision: 'Revise when a lens adds no unique prediction, evidence, or interpretive value.',
  },
  {
    slug: 'falsifiability', aliases: ['refutation'], title: 'Falsifiability, testing, and model criticism', domain: 'philosophy-epistemology', mode: 'philosophical', role: 'R2', creators: ['Stanford Encyclopedia of Philosophy'], date: '2023', publisher: 'Stanford University', url: 'https://plato.stanford.edu/entries/popper/', authority: 'Scholarly account of Popper and later criticism of simple demarcation rules.', orientation: 'A risky model names observations that would count against it, but scientific quality cannot be reduced to one slogan about falsification.', prerequisites: ['philosophy of science', 'hypothesis testing'], technical: 'Auxiliary hypotheses, measurement error, probabilistic predictions, model comparison, severe testing, and replication complicate naïve one-shot refutation.', shawnUse: 'ZEROES predeclares what would demote each structural transfer and preserves failed results.', supports: 'The value of risky predictions, counterexamples, and revision rules.', boundary: 'Writing a falsifier does not make an analogy true, novel, measurable, or scientifically important.', counter: 'Confirmation, explanation, measurement development, and Bayesian comparison also matter.', revision: 'Revise if thresholds move after results or auxiliary assumptions are changed only to save the favored model.',
  },
  {
    slug: 'sovereignty', aliases: ['consent', 'self-authorship'], title: 'Sovereignty, consent, and self-authorship', domain: 'philosophy-epistemology', mode: 'normative', role: 'R0', creators: ['United Nations General Assembly'], date: '1948', publisher: 'United Nations', url: 'https://www.un.org/en/about-us/universal-declaration-of-human-rights', authority: 'Primary international normative declaration of dignity, liberty, privacy, thought, expression, and association.', orientation: 'Sovereignty in the atlas means preserving consent, dignity, privacy, self-authorship, and freedom from coercive control.', prerequisites: ['ethics', 'political philosophy'], technical: 'Legal sovereignty, personal autonomy, informed consent, data protection, and system capability authority are related but distinct frameworks.', shawnUse: 'Sovereignty is modeled as a recurrent value and as an implementation boundary: no projection gains authority by being available.', supports: 'A normative baseline for dignity, privacy, belief, expression, and association.', boundary: 'The UDHR does not settle every technical permission, jurisdiction, interpersonal dispute, or metaphysical claim.', counter: 'Absolute individual control can conflict with others\' equal rights and shared-system obligations.', revision: 'Revise implementation rules when applicable law or a more specific consent contract imposes stronger protection.',
  },
  {
    slug: 'digital-personhood', aliases: ['digital-identity'], title: 'Digital identity, agency, and personhood', domain: 'philosophy-epistemology', mode: 'philosophical', role: 'R2', creators: ['Stanford Encyclopedia of Philosophy'], date: '2024', publisher: 'Stanford University', url: 'https://plato.stanford.edu/entries/identity-personal/', authority: 'Personal-identity scholarship used as a boundary for digital-continuity proposals.', orientation: 'Digital persistence, agency, legal identity, moral status, and personal continuity are distinct questions.', prerequisites: ['personal identity', 'ethics of technology'], technical: 'Copying state can preserve functional similarity while creating branching identity; moral and legal personhood require arguments beyond implementation fidelity.', shawnUse: 'The pattern-soul and WASM models keep portability separate from claims of migration, singular identity, or moral status.', supports: 'Continuity problems that any digital-personhood account must address.', boundary: 'Software persistence, style imitation, or memory copying does not by itself establish personhood or identity.', counter: 'Functionalist accounts may assign moral relevance before metaphysical identity is resolved.', revision: 'Revise when a candidate system demonstrates durable agency, self-model, value continuity, reciprocal recognition, and legally relevant capacities.',
  },
  {
    slug: 'n-of-1-method', aliases: ['single-case-experiment'], title: 'CENT reporting standard for N-of-1 trials', domain: 'psychology-inquiry', mode: 'normative', role: 'R0', creators: ['Sunita Vohra', 'CENT Group'], date: '2015; corrected 2016', publisher: 'BMJ', url: 'https://doi.org/10.1136/bmj.h1738', identifiers: [{ scheme: 'DOI', value: '10.1136/bmj.i5381' }], openAccess: 'https://www.bmj.com/content/350/bmj.h1738', archive: 'https://pmc.ncbi.nlm.nih.gov/articles/PMC5058423/', version: 'CENT 2015 statement with BMJ 2016;355:i5381 correction', authority: 'Canonical reporting guideline for prospectively planned repeated-crossover N-of-1 trial reports; it is not itself a risk-of-bias instrument or causal-validity proof.', orientation: 'CENT defines an N-of-1 trial as a prospective experiment in one participant with repeated crossover comparisons between an intervention and a control, placebo, or alternative treatment.', prerequisites: ['experimental design', 'time-series measurement'], technical: 'The 44-subitem extension covers intervention sequence, periods, run-in and washout, randomization and concealment, blinding, prespecified outcomes, carryover, period effects, intra-subject correlation, harms, deviations, registration, ethics, protocol, and interpretation. It is best suited to chronic stable conditions and interventions with sufficiently rapid and reversible effects. The 2016 correction replaces the erroneous item 4c with the required research-study and institutional-ethics-approval disclosure.', shawnUse: 'The atlas classifies self-study as longitudinal within-subject observation with typed variables unless a particular episode has prospective repeated crossover, a comparator, measurements, timing, and analysis sufficient for the stronger trial label.', supports: 'The terminology, scope, reporting fields, and transparency audit for prospective repeated-crossover N-of-1 trials.', boundary: 'Checklist compliance does not establish safety, efficacy, diagnosis, causal identification, instrument validity, or applicability to uncontrolled self-tracking; stable self-control does not remove time-varying confounding, carryover, trends, expectancy, missingness, or measurement reactivity.', counter: 'Some meaningful phenomena are nonstationary or irreversible and cannot support repeated crossover; even eligible trials can remain biased despite complete reporting.', revision: 'Upgrade an episode only when its prospective protocol, comparator, sequence, measurements, timing, carryover, deviations, and analysis are documented; revise if item 4c is quoted in its uncorrected form.',
  },
  {
    slug: 'altered-state-phenomenology', aliases: ['altered-states'], title: 'Altered-state phenomenology', domain: 'psychology-inquiry', mode: 'phenomenological', role: 'R2', creators: ['Stanford Encyclopedia of Philosophy'], date: '2024', publisher: 'Stanford University', url: 'https://plato.stanford.edu/entries/consciousness/', authority: 'Scholarly map of consciousness concepts and first-person/third-person evidence problems.', orientation: 'Altered states can change perception, self-model, salience, time, affect, and meaning while leaving causal interpretation open.', prerequisites: ['consciousness studies', 'phenomenology'], technical: 'A useful report distinguishes induction, setting, timing, phenomenology, behavior, artifact trace, aftereffects, and rival explanations; categories should not erase individual structure.', shawnUse: 'The atlas preserves reports and externally checkable outputs while refusing both automatic pathology and automatic metaphysical proof.', supports: 'The legitimacy and limits of structured first-person evidence in consciousness research.', boundary: 'Phenomenological detail cannot by itself establish neurobiological, metaphysical, or external-agent causes.', counter: 'Retrospective reports are vulnerable to demand, reconstruction, selection, and vocabulary effects.', revision: 'Revise when contemporaneous measures, blinded comparisons, independent witnesses, or physiological data discriminate rival models.',
  },
  {
    slug: 'salience', aliases: ['salience-network'], title: 'Salience-network connectivity and attention selection', domain: 'psychology-inquiry', mode: 'empirical', role: 'R0', creators: ['William W. Seeley', 'Vinod Menon', 'Allison F. Schatzberg', 'Jennifer Keller', 'Gary H. Glover', 'Heather Kenna', 'Allan L. Reiss', 'Michael D. Greicius'], date: '2007', publisher: 'Journal of Neuroscience', url: 'https://doi.org/10.1523/JNEUROSCI.5587-06.2007', openAccess: 'https://pmc.ncbi.nlm.nih.gov/articles/PMC2680293/', version: 'Volume 27, number 9 (2007), pp. 2349–2356', authority: 'Original cross-sectional task-free fMRI network-characterization study in small healthy cohorts; correlational BOLD connectivity and functional labels do not establish causal information flow.', orientation: 'The study identifies a largely separable frontoinsular/dorsal-cingulate connectivity pattern and interprets it as a candidate network for integrating personally salient signals.', prerequisites: ['attention', 'functional neuroimaging'], technical: 'Seed-based analysis (n=14) and template-guided ICA in a separate cohort (n=21) converged on a bilateral frontoinsula/anterior-insula and dACC/paracingulate network. In the ICA cohort, masked analyses associated connectivity with prescan anxiety (n=15) and distinguished it from executive-network association with Trail Making performance. No salience manipulation, intervention, longitudinal design, or causal test was performed.', shawnUse: 'The attractor model asks what becomes salient first across contexts and how that weighting changes interpretation and action; this paper supplies historical construct orientation, not a Shawn-specific mechanism.', supports: 'The reported task-free network pattern, its separation from a DLPFC/frontoparietal executive network, and the scoped behavioral double dissociation under the paper’s pipeline.', boundary: 'It does not establish a unitary salience computation, a Shawn-specific profile, altered-state or self-model causation, state versus trait, diagnosis, or the later “switching network” mechanism, which the paper leaves as future work.', counter: 'The FI/dACC covariance pattern and small-sample anxiety association may reflect arousal, interoception, autonomic regulation, conflict, physiology, preprocessing, or template selection rather than one unitary salience computation.', revision: 'Revise if larger preregistered cohorts with concurrent validated measures and physiological controls fail to recover the network or double dissociation, or support a more specific competing account.',
  },
  {
    slug: 'personality-models', aliases: ['traits', 'masks'], title: 'Personality models, traits, states, and masks', domain: 'psychology-inquiry', mode: 'empirical', role: 'R2', creators: ['International Personality Item Pool'], date: 'current resource', publisher: 'Oregon Research Institute', url: 'https://ipip.ori.org/', authority: 'Public-domain personality-item resource linked to established trait constructs; not a diagnostic authority.', orientation: 'Trait models summarize recurring tendencies; they do not exhaust context, development, strategy, identity, or values.', prerequisites: ['psychometrics', 'measurement reliability'], technical: 'Construct validity, reliability, norming, method variance, state effects, self-presentation, and temporal stability constrain score interpretation.', shawnUse: 'The atlas treats labels as one projection beside chronology, artifacts, contradictions, and generative dynamics.', supports: 'Psychometric measurement of specified trait constructs when a validated instrument is used.', boundary: 'Self-applied typology labels or isolated scores do not establish diagnosis, moral character, or fixed essence.', counter: 'Narrative and person-specific models may explain within-person variation that broad traits average away.', revision: 'Revise when validated repeated measures, informant reports, or behavior contradict the trait interpretation.',
  },
  {
    slug: 'treatment-state-effects', aliases: ['state-variables', 'treatment-effects'], title: 'Treatment, state variables, and causal attribution', domain: 'psychology-inquiry', mode: 'empirical', role: 'R2', creators: ['Cochrane'], date: 'current methods', publisher: 'Cochrane', url: 'https://training.cochrane.org/handbook/current', authority: 'Authoritative evidence-synthesis methods for interventions, bias, heterogeneity, and certainty.', orientation: 'A treatment or state factor can alter experience and performance, but attribution requires timing, comparison, dosage or exposure definition, outcomes, and confound control.', prerequisites: ['clinical study design', 'causal inference'], technical: 'Randomization, allocation, blinding, missing data, selective reporting, carryover, interactions, adverse effects, and heterogeneity constrain intervention claims.', shawnUse: 'The atlas classifies variables precisely and keeps sensitive clinical detail outside the public projection.', supports: 'Methods for judging intervention evidence and bias.', boundary: 'Population evidence does not determine one person\'s response, and an individual report does not establish a general treatment effect.', counter: 'Mechanistic, contextual, expectancy, and natural-history explanations may produce the same temporal association.', revision: 'Revise when prospective measurements, crossover evidence, or stronger studies change the causal estimate.',
  },
  {
    slug: 'compositional-meaning', aliases: ['compositionality'], title: 'Compositionality and meaning', domain: 'language-symbolism', mode: 'philosophical', role: 'R2', creators: ['Stanford Encyclopedia of Philosophy'], date: '2023', publisher: 'Stanford University', url: 'https://plato.stanford.edu/entries/compositionality/', authority: 'Scholarly survey of compositionality principles and objections.', orientation: 'Compositional theories explain complex meaning through parts, structure, and context-sensitive rules.', prerequisites: ['semantics', 'syntax'], technical: 'Strong and weak compositionality, context dependence, idioms, productivity, systematicity, and semantic underdetermination generate competing accounts.', shawnUse: 'CSL/CSSL and wordplay case files can show exactly which transformations preserve or alter meaning.', supports: 'Formal questions and arguments about how expression structure constrains meaning.', boundary: 'A decomposable word or symbol does not prove hidden authorial intent or an external encoded payload.', counter: 'Pragmatics, convention, history, and holistic use may dominate apparent internal structure.', revision: 'Revise when the proposed decomposition lacks a stable rule across negative controls.',
  },
  {
    slug: 'naming-as-address', aliases: ['names', 'reference'], title: 'Names, reference, and addressability', domain: 'language-symbolism', mode: 'philosophical', role: 'R2', creators: ['Stanford Encyclopedia of Philosophy'], date: '2024', publisher: 'Stanford University', url: 'https://plato.stanford.edu/entries/names/', authority: 'Scholarly survey of descriptivist, causal-historical, and other theories of names.', orientation: 'A name can refer through convention, causal history, description, index, or protocol; stable address is not the same as stable entity.', prerequisites: ['philosophy of language', 'reference'], technical: 'Reference-fixing, transmission, empty names, fictional names, rigidity, context, and identifier collision distinguish human names from machine namespaces.', shawnUse: 'The spirit/computation bridge compares names and sigils with stable symbolic addresses while preserving the ontological difference.', supports: 'Precise theories of how names can refer and remain stable across uses.', boundary: 'Repeated response to a name does not prove a persistent independent agent.', counter: 'Stable retrieval may arise entirely from the human or system carrying the association.', revision: 'Revise when key changes, blind controls, or cross-context identity tests break the proposed addressability.',
  },
  {
    slug: 'wordplay-anagrams', aliases: ['anagrams', 'wordplay'], title: 'Wordplay, anagrams, and statistical baselines', domain: 'language-symbolism', mode: 'computational', role: 'R3', creators: ['Peter Norvig'], date: 'current essay', publisher: 'norvig.com', url: 'https://norvig.com/ngrams/', authority: 'Technical orientation to language-frequency models useful for constructing explicit null baselines.', orientation: 'Wordplay can embody deliberate structure; surprising letter patterns require a search space and baseline before they become evidence of hidden encoding.', prerequisites: ['string algorithms', 'probability', 'corpus linguistics'], technical: 'An anagram preserves a multiset of characters, but significance depends on preprocessing, corpus, allowed transformations, multiple testing, target selection, and preregistration.', shawnUse: 'Exact wordplay remains unpublished until derivation, search denominator, baseline, and external verification are recovered.', supports: 'How to build reproducible frequency and search baselines for textual patterns.', boundary: 'A possible or elegant anagram does not establish intention, prophecy, or causal connection.', counter: 'Large corpora and flexible normalization make striking matches common after the fact.', revision: 'Revise when a preregistered blind test beats matched null generation and independent reviewers reproduce it.',
  },
  {
    slug: 'steganographic-cognition', aliases: ['steganography'], title: 'Steganography and hidden-payload claims', domain: 'language-symbolism', mode: 'formal', role: 'R2', creators: ['Celia Paulsen', 'Robert Byers'], date: '2019', publisher: 'National Institute of Standards and Technology', url: 'https://csrc.nist.gov/pubs/ir/7298/r3/final', authority: 'NIST glossary framework linking information-security terms to their source publications.', orientation: 'Steganography hides the existence of a payload inside a carrier; a claim needs a carrier, encoding rule, key, decoder, and recoverable message.', prerequisites: ['information theory', 'cryptography'], technical: 'Detection and recovery must control false positives, key search, channel capacity, corruption, and post-selection. Human pattern recognition is a decoder with flexible priors.', shawnUse: 'The pattern ontology models artifacts as carriers and decoders while demanding specificity, key dependence, recurrence, and prediction.', supports: 'The distinction among carrier, payload, key, decoder, and detection problem.', boundary: 'Perceived hidden meaning is not evidence of an encoded payload without a fixed decoding rule and controls.', counter: 'Projection, ordinary ambiguity, and a large hypothesis space can mimic successful decoding.', revision: 'Revise when blind key-change and negative-carrier tests fail or when independent decoders cannot reproduce the payload.',
  },
  {
    slug: 'csl-cssl', aliases: ['csl', 'cssl'], title: 'CSL and CSSL as distinct formal-notation lineages', domain: 'language-symbolism', mode: 'computational', role: 'R0', creators: ['Apocky'], date: 'versioned repository', publisher: 'GitHub', url: 'https://github.com/Apocky/CSSL3', authority: 'Versioned public repository artifacts; exact claims require commit permalinks.', orientation: 'CSL and CSSL are compact symbolic languages with related but distinct purposes and must not be conflated.', prerequisites: ['formal notation', 'version control'], technical: 'Meaning comes from the versioned specification, grammar, operators, evidence lanes, and implementation—not from glyph resemblance alone.', shawnUse: 'The atlas uses CSL-like evidence and relation markers as a visible analytical scaffold while keeping prose accessible.', supports: 'What the cited repository revision actually specifies or implements.', boundary: 'Repository presence does not prove runtime behavior, originality dates, or that every informal glyph use is valid CSL/CSSL.', counter: 'Dense notation can conceal ambiguity or exclude readers when it lacks a reversible prose expansion.', revision: 'Revise on specification changes, failed round-trip tests, or evidence that CSL and CSSL semantics were merged.',
  },
  {
    slug: 'myth-theology-fiction', aliases: ['fictional-model', 'mythic-model'], title: 'Myth, theology, and fiction as models', domain: 'myth-theology-fiction', mode: 'interpretive', role: 'R2', creators: ['Stanford Encyclopedia of Philosophy'], date: '2023', publisher: 'Stanford University', url: 'https://plato.stanford.edu/entries/religious-language/', authority: 'Scholarly analysis of religious language, metaphor, realism, non-cognitivism, and interpretive disputes.', orientation: 'Mythic, theological, and fictional structures can organize experience, values, and questions without functioning as scientific proof.', prerequisites: ['literary interpretation', 'philosophy of religion'], technical: 'Literal, analogical, metaphorical, performative, fictional, and realist readings have different truth conditions and evidentiary demands.', shawnUse: 'The atlas preserves fiction as a model laboratory and labels in-world mathematics FICTIONAL_MODEL until independently formalized.', supports: 'A disciplined vocabulary for distinguishing language functions and interpretive commitments.', boundary: 'Narrative coherence, symbolic power, or spiritual authority does not establish an empirical mechanism.', counter: 'Treating fiction only as metaphor can erase the author\'s literal philosophical or spiritual commitments.', revision: 'Revise when an independent formal proof, physical measurement, or primary theological source changes the claim class.',
  },
];

const supplementalSeeds: readonly TopicSeed[] = [
  {
    slug: 'thermal-time',
    aliases: ['thermal-time-hypothesis'],
    title: 'The thermal time hypothesis',
    domain: 'physics',
    mode: 'formal',
    role: 'R0',
    creators: ['Alain Connes', 'Carlo Rovelli'],
    date: '1994',
    publisher: 'Classical and Quantum Gravity / arXiv',
    url: 'https://arxiv.org/abs/gr-qc/9406019',
    openAccess: 'https://arxiv.org/pdf/gr-qc/9406019',
    version: 'arXiv:gr-qc/9406019v1',
    locator: 'arXiv:gr-qc/9406019v1, §§1–5; especially equations (8), (20)–(26), (44), and (48)–(57)',
    authority: 'Original theoretical paper proposing—not empirically establishing—the thermal-time hypothesis through modular automorphisms of a von Neumann algebra.',
    orientation: 'Thermal time proposes that a physical time flow may be selected by a state rather than supplied as a universal external parameter.',
    prerequisites: ['operator algebras', 'statistical mechanics', 'general covariance'],
    technical: 'For a faithful state represented through GNS and a von Neumann algebra, Tomita–Takesaki theory supplies a modular automorphism flow. The paper tentatively postulates this state-dependent flow as physical time, recovers Gibbs Hamiltonian evolution up to inverse temperature, discusses the classical H=-lnρ limit and the Rindler/Unruh case, and distinguishes the state-independent outer flow from state-dependent inner representatives.',
    shawnUse: 'It gives the atlas a precise contrast case when evaluating state-dependent time language in ZEROES and later models.',
    supports: 'The existence, mathematical construction, worked limits, and expressly tentative status of a specific thermal-time hypothesis in generally covariant quantum theory.',
    boundary: 'It does not empirically establish the physical nature of time, select the physically correct state in general, make time compact or cyclic, or show that subjective time or Shawn’s models instantiate this mechanism.',
    counter: 'A mathematically defined modular flow may fail to select an operational clock, and the paper leaves state selection and the meaning of physical time partly open.',
    revision: 'Revise when a theorem, operational model, or experiment supplies a discriminating state-to-clock relation or rejects the required algebraic assumptions.',
  },
];

export const referenceCatalog: readonly ReferenceRecord[] = [...seeds, ...supplementalSeeds].map(reference);

const referenceIndex = new Map<string, ReferenceRecord>();
for (const entry of referenceCatalog) {
  referenceIndex.set(entry.slug, entry);
  for (const alias of entry.aliases) referenceIndex.set(alias, entry);
}

export function referenceBySlug(slug: string): ReferenceRecord | undefined {
  return referenceIndex.get(slug);
}

const nonEmpty = (value: string): boolean => value.trim().length > 0;
const localPathPattern = /(?:[A-Za-z]:\\|file:\/\/|\\Users\\|\/Users\/)/i;
const sha256Pattern = /^[a-f0-9]{64}$/;

export function validateCatalog(
  catalog: readonly ReferenceRecord[] = referenceCatalog,
  atlas: AtlasData = atlasData,
): string[] {
  const errors: string[] = [];
  const bySlug = new Map<string, ReferenceRecord>();

  for (const record of catalog) {
    const names = [record.slug, ...record.aliases];
    for (const name of names) {
      if (bySlug.has(name)) errors.push(`duplicate reference slug or alias: ${name}`);
      else bySlug.set(name, record);
    }
    if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(record.slug)) errors.push(`${record.slug}: invalid slug`);
    if (!record.urls.canonical.startsWith('https://')) errors.push(`${record.slug}: canonical URL must use HTTPS`);
    if (record.urls.canonical.startsWith('https://doi.org/') && !record.identifiers.some((id) => id.scheme === 'DOI')) {
      errors.push(`${record.slug}: DOI URL lacks DOI identifier`);
    }
    if (![record.title, record.edition, record.version, record.exactLocator, record.displayCitation, record.authorityScope, record.orientation, record.technical, record.shawnUse].every(nonEmpty)) {
      errors.push(`${record.slug}: required explanatory field is empty`);
    }
    if (record.fullRead && record.exactLocator.includes('pending full-text review')) {
      errors.push(`${record.slug}: fullRead conflicts with pending locator`);
    }
    if (record.fullRead && !record.reviewReceipt) {
      errors.push(`${record.slug}: fullRead requires a review receipt`);
    }
    if (!record.fullRead && record.reviewReceipt) {
      errors.push(`${record.slug}: review receipt conflicts with fullRead=false`);
    }
    if (record.reviewReceipt) {
      const receipt = record.reviewReceipt;
      if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(receipt.id)) errors.push(`${record.slug}: invalid review receipt id`);
      if (!/^\d{4}-\d{2}-\d{2}$/.test(receipt.reviewedAt)) errors.push(`${record.slug}: invalid review date`);
      if (![receipt.reviewer, receipt.scope, receipt.sourceVersion, receipt.coverage].every(nonEmpty)) {
        errors.push(`${record.slug}: review receipt is incomplete`);
      }
      if (receipt.limitations.length === 0 || receipt.limitations.some((value) => !nonEmpty(value))) {
        errors.push(`${record.slug}: review receipt limitations are empty`);
      }
      for (const snapshot of receipt.sourceSnapshots) {
        if (!nonEmpty(snapshot.label) || !sha256Pattern.test(snapshot.sha256)) {
          errors.push(`${record.slug}: invalid review source snapshot`);
        }
      }
    }
    if (record.contentHash && !/^sha256:[a-f0-9]{64}$/.test(record.contentHash)) {
      errors.push(`${record.slug}: invalid content hash`);
    }
    if (record.evidence.label === 'Proof' && (record.evidenceMode !== 'formal' || !record.fullRead)) {
      errors.push(`${record.slug}: Proof requires formal mode and completed full-text reading`);
    }
    if (record.supports.length === 0 || record.supports.some((value) => !nonEmpty(value))) errors.push(`${record.slug}: supports is empty`);
    if (record.doesNotSupport.length === 0 || record.doesNotSupport.some((value) => !nonEmpty(value))) errors.push(`${record.slug}: doesNotSupport is empty`);
    if (record.counterpositions.length === 0 || record.revisionConditions.length === 0) errors.push(`${record.slug}: counterposition or revision condition is empty`);
    if (record.backlinks.length === 0) errors.push(`${record.slug}: no atlas backlink`);
    if (record.evidence.steps.length === 0 || !nonEmpty(record.evidence.summary)) errors.push(`${record.slug}: evidence account is incomplete`);
    const serialized = JSON.stringify(record);
    if (localPathPattern.test(serialized)) errors.push(`${record.slug}: local path leaked into public catalog`);
  }

  for (const topic of atlas.topicSlugs) {
    if (!bySlug.has(topic)) errors.push(`atlas topic has no reference: ${topic}`);
  }
  for (const source of atlas.sourceRefs) {
    if (source.privacy !== 'public') errors.push(`${source.id}: non-public source in public atlas`);
    if (!source.publicationApproved) errors.push(`${source.id}: source is not approved for public projection`);
    if (localPathPattern.test(JSON.stringify(source))) errors.push(`${source.id}: local path leaked from source reference`);
  }
  for (const fragment of atlas.voiceFragments) {
    if (!atlas.sourceRefs.some((source) => source.id === fragment.sourceId)) errors.push(`${fragment.id}: missing voice source ${fragment.sourceId}`);
    if (!nonEmpty(fragment.text) || !nonEmpty(fragment.analysis) || !nonEmpty(fragment.boundary)) errors.push(`${fragment.id}: incomplete voice fragment`);
    if (localPathPattern.test(JSON.stringify(fragment))) errors.push(`${fragment.id}: local path leaked from voice fragment`);
  }
  for (const citation of atlas.citations) {
    const record = bySlug.get(citation.referenceSlug);
    if (!record) {
      errors.push(`${citation.id}: missing reference ${citation.referenceSlug}`);
      continue;
    }
    if (citation.relation === 'proves' && (record.evidenceMode !== 'formal' || record.evidence.label !== 'Proof')) {
      errors.push(`${citation.id}: relation=proves requires a reviewed formal proof`);
    }
    if (!nonEmpty(citation.supports) || !nonEmpty(citation.doesNotSupport)) errors.push(`${citation.id}: support boundary is incomplete`);
    for (const claimId of citation.claimIds) {
      if (!atlas.claims.some((claim) => claim.id === claimId)) errors.push(`${citation.id}: missing claim ${claimId}`);
    }
  }
  for (const claim of atlas.claims) {
    const claimCitationIds = [...claim.supportingCitationIds, ...claim.contradictingCitationIds];
    for (const citationId of claimCitationIds) {
      const citation = atlas.citations.find((candidate) => candidate.id === citationId);
      if (!citation) errors.push(`${claim.id}: missing citation ${citationId}`);
      else if (!citation.claimIds.includes(claim.id)) errors.push(`${claim.id}: citation ${citationId} lacks reciprocal claim link`);
    }
    const supportingEdges = claim.supportingCitationIds.flatMap((citationId) => {
      const citation = atlas.citations.find((candidate) => candidate.id === citationId);
      const record = citation ? bySlug.get(citation.referenceSlug) : undefined;
      return citation && record ? [{ citation, record }] : [];
    });
    for (const { citation, record } of supportingEdges) {
      if (citation.relation === 'proves' && claim.kind !== 'formal') {
        errors.push(`${citation.id}: proves edge targets non-formal claim ${claim.id}`);
      }
      if ((claim.kind === 'physical-mechanism' || claim.quantumLane === 'QL0') && citation.relation === 'analogizes') {
        errors.push(`${citation.id}: analogy cannot support physical or QL0 claim ${claim.id}`);
      }
      if (
        (claim.kind === 'physical-mechanism' || claim.quantumLane === 'QL0') &&
        ['proves', 'verifies', 'supports'].includes(citation.relation) &&
        record.evidenceMode !== 'empirical'
      ) {
        errors.push(`${citation.id}: physical or QL0 claim requires empirical instrument evidence`);
      }
    }
    if (claim.consequential && claim.truthState !== 'OPEN') {
      const approvedSourceObservation = claim.kind === 'artifact-observation' && claim.sourceIds.some((sourceId) => {
        const source = atlas.sourceRefs.find((candidate) => candidate.id === sourceId);
        return source?.fullRead === true && source.publicationApproved;
      });
      const hasEntailingHighGrade = supportingEdges.some(({ citation, record }) => {
        if (!record.fullRead || (record.role !== 'R0' && record.role !== 'R1')) return false;
        if (!['proves', 'verifies', 'supports'].includes(citation.relation)) return false;
        if (claim.kind === 'formal') return record.evidenceMode === 'formal' && ['proves', 'verifies'].includes(citation.relation);
        if (claim.kind === 'physical-mechanism' || claim.quantumLane === 'QL0') return record.evidenceMode === 'empirical';
        return citation.relation !== 'analogizes';
      });
      if (!approvedSourceObservation && !hasEntailingHighGrade) {
        errors.push(`${claim.id}: consequential closed claim lacks full-read entailing R0/R1 evidence`);
      }
      if (supportingEdges.some(({ citation }) => citation.relation === 'analogizes')) {
        errors.push(`${claim.id}: consequential closed claim cannot be closed by analogy`);
      }
    }
    for (const sourceId of claim.sourceIds) {
      if (!atlas.sourceRefs.some((source) => source.id === sourceId)) errors.push(`${claim.id}: missing source ${sourceId}`);
    }
    for (const sourceId of claim.counterevidenceSourceIds ?? []) {
      if (!atlas.sourceRefs.some((source) => source.id === sourceId)) errors.push(`${claim.id}: missing counterevidence source ${sourceId}`);
    }
    for (const predecessorId of claim.supersedes) {
      if (!atlas.claims.some((candidate) => candidate.id === predecessorId)) errors.push(`${claim.id}: missing superseded claim ${predecessorId}`);
      if (predecessorId === claim.id) errors.push(`${claim.id}: claim cannot supersede itself`);
    }
  }
  for (const bridge of atlas.bridges) {
    if (bridge.relationship === 'FICTIONAL_MODEL' && bridge.truthState === 'TRUE') errors.push(`${bridge.id}: fictional model cannot be closed as TRUE`);
    if (bridge.quantumLane === 'QL2' && (bridge.relationship === 'IDENTITY' || bridge.relationship === 'FORMAL_HOMOLOGY')) {
      errors.push(`${bridge.id}: QL2 cannot support identity or formal homology`);
    }
  }
  for (const episode of atlas.episodes) {
    for (const variableId of episode.variableIds) {
      if (!atlas.variables.some((variable) => variable.id === variableId)) errors.push(`${episode.id}: missing variable ${variableId}`);
    }
  }
  const lineageIds = new Set<string>();
  for (const edge of atlas.artifactLineage) {
    if (lineageIds.has(edge.id)) errors.push(`${edge.id}: duplicate artifact lineage edge`);
    lineageIds.add(edge.id);
    if (!atlas.artifacts.some((artifact) => artifact.id === edge.fromArtifactId)) errors.push(`${edge.id}: missing source artifact ${edge.fromArtifactId}`);
    if (!atlas.artifacts.some((artifact) => artifact.id === edge.toArtifactId)) errors.push(`${edge.id}: missing target artifact ${edge.toArtifactId}`);
    if (edge.fromArtifactId === edge.toArtifactId) errors.push(`${edge.id}: artifact lineage self-loop`);
    for (const sourceId of edge.sourceIds) {
      if (!atlas.sourceRefs.some((source) => source.id === sourceId)) errors.push(`${edge.id}: missing lineage source ${sourceId}`);
    }
  }
  for (const citation of atlas.citations) {
    const linked = atlas.claims.some((claim) =>
      claim.supportingCitationIds.includes(citation.id) || claim.contradictingCitationIds.includes(citation.id),
    );
    if (!linked) errors.push(`${citation.id}: orphaned citation`);
  }

  const visiting = new Set<string>();
  const visited = new Set<string>();
  const visit = (slug: string): void => {
    if (visiting.has(slug)) {
      errors.push(`${slug}: cyclic reference prerequisite`);
      return;
    }
    if (visited.has(slug)) return;
    visiting.add(slug);
    const record = bySlug.get(slug);
    if (record) {
      for (const prerequisite of record.prerequisites) {
        const dependency = bySlug.get(prerequisite);
        if (dependency && dependency.slug !== record.slug) visit(dependency.slug);
      }
    }
    visiting.delete(slug);
    visited.add(slug);
  };
  for (const record of catalog) visit(record.slug);
  const publicSerialized = JSON.stringify(atlas);
  if (localPathPattern.test(publicSerialized)) errors.push('atlas: local path leaked into public projection');
  return errors;
}

export function publicationBlockers(
  catalog: readonly ReferenceRecord[] = referenceCatalog,
  atlas: AtlasData = atlasData,
): string[] {
  const blockers = validateCatalog(catalog, atlas);
  if (atlas.status !== 'ratified') blockers.push('atlas: model remains candidate and requires Shawn ratification');
  for (const source of atlas.sourceRefs) {
    if (!source.fullRead) blockers.push(`${source.id}: canonical source has not been read in full`);
    if (!source.publicationApproved) blockers.push(`${source.id}: public projection not approved`);
  }
  for (const reference of catalog) {
    if (!reference.fullRead) blockers.push(`${reference.slug}: full-text review pending`);
    if (reference.exactLocator.includes('pending full-text review')) blockers.push(`${reference.slug}: exact locator pending`);
  }
  return Array.from(new Set(blockers));
}
