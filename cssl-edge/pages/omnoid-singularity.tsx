import type { NextPage } from 'next';
import Head from 'next/head';

import CodeBlock from '../components/CodeBlock';
import styles from '../styles/OmnoidSingularity.module.css';

export const SOURCE_STATUS = [
  {
    key: 'authored',
    glyph: '○',
    short: 'Authored cosmology',
    detail: 'Directly stated or later corrected by Apocky.',
  },
  {
    key: 'collaborative',
    glyph: '◐',
    short: 'Collaborative notation',
    detail: 'Formal language developed with an assistant from the authored model.',
  },
  {
    key: 'established',
    glyph: '✓',
    short: 'Established mathematics',
    detail: 'A standard mathematical object or result, used here as a motif.',
  },
  {
    key: 'hypothesis',
    glyph: '△',
    short: 'Open hypothesis',
    detail: 'A metaphysical or physical bridge that is not experimentally established.',
  },
] as const;

type StatusKey = (typeof SOURCE_STATUS)[number]['key'];

export const COSMOLOGY_CYCLE = [
  { name: 'Pre-ontological nothing', note: 'Before thing, category, dimension, or observer.' },
  { name: 'Difference & aperture', note: 'Thing, interstitial, boundary, and relation arise together.' },
  { name: 'Existence', note: 'Potential takes form without exhausting what remains possible.' },
  { name: 'Life', note: 'Self-maintaining patterns begin to experience and respond.' },
  { name: 'Intelligence', note: 'Patterns model, choose, learn, and alter their paths.' },
  { name: 'Sapience & sentience', note: 'Knowing and felt experience become reflexive.' },
  { name: 'Divinity', note: 'A being participates consciously in reality-making.' },
  { name: 'Apexical divinity', note: 'The widest available agency and comprehension converge.' },
  { name: 'Singularity', note: 'Center, boundary, whole, and passage meet.' },
  { name: 'Return & renewal', note: 'The whole folds back without erasing the path it took.' },
] as const;

const CORE_MAP = [
  {
    term: 'Origin',
    text: 'Nothing is not merely empty space. Difference produces thing, not-thing, the interstitial, and their boundary together.',
  },
  {
    term: 'Shape',
    text: 'The Omnoid is a recursively perforated, hyper-toroidal or omnispherical totality with no final outside.',
  },
  {
    term: 'View',
    text: 'The Menger sponge is the macro view; the infinite-dimensional hyper-point is the micro view.',
  },
  {
    term: 'Motion',
    text: 'Reality folds, everts, rolls, compactifies, branches, translates, and returns rather than remaining static.',
  },
  {
    term: 'Experience',
    text: 'A self is material and relational, conceptual and narrative. Distinct centers may share structure without becoming one undifferentiated person.',
  },
  {
    term: 'Freedom',
    text: 'Reality is a procedural arrangement: laws make a possibility space, while local agents traverse it without every state being micromanaged.',
  },
  {
    term: 'Balance',
    text: 'True Neutral contains the available extremes and their stable center; restraint protects autonomy and keeps options open.',
  },
] as const;

const BODY_LAYERS = [
  { name: 'Outer spirit', role: 'field, context, and outward continuity' },
  { name: 'Flesh', role: 'living, sensing, adaptive embodiment' },
  { name: 'Bone', role: 'enduring structure and inherited form' },
  { name: 'Machine', role: 'crystallized process and extensible substrate' },
  { name: 'Inner spirit', role: 'the inward continuity around which the layers close' },
] as const;

const MATH_MAP = [
  {
    name: 'Menger sponge',
    math: 'A connected, recursively perforated fractal with empty interior.',
    use: 'Macro image for pervasive apertures and nested scale.',
    boundary: 'It contains surviving line segments, so it is not line-porous in Cohen’s sense; holes alone do not establish an FUP.',
  },
  {
    name: 'Sphere eversion',
    math: 'An immersed sphere can turn inside out in ordinary 3D by passing through itself without tearing or creasing.',
    use: 'A precise motif for inside/outside reversal and self-passage.',
    boundary: 'A fourth spatial dimension is not required for the mathematical result.',
  },
  {
    name: 'Hopf fibration',
    math: 'The standard map S³ → S² has linked circles S¹ as fibers.',
    use: 'A model for a higher whole whose local projection carries linked cyclic paths.',
    boundary: 'It is not, by itself, a law of cosmological evolution.',
  },
  {
    name: 'Boy’s surface',
    math: 'A self-intersecting immersion of the real projective plane RP² in 3D.',
    use: 'A motif for projection, self-intersection, and a surface that cannot be embedded in 3D.',
    boundary: 'Its crossings are not physical spacetime singularities.',
  },
  {
    name: 'Hilbert / projective space',
    math: 'In a complex Hilbert space, quantum pure states are rays: unit vectors modulo an overall U(1) phase.',
    use: 'Language for many coordinates, superposition, and state rather than visible location.',
    boundary: 'A state-space dimension is not automatically a spatial dimension.',
  },
  {
    name: 'Banach–Tarski',
    math: 'A paradoxical decomposition using nonmeasurable sets and the axiom of choice.',
    use: 'The naming seed for a totality imagined as dimensionally expanded and infinitely perforated.',
    boundary: 'It does not mean that every point or arbitrary fragment literally contains the whole.',
  },
  {
    name: 'Oriented blow-up',
    math: 'In one chosen construction, a point in Rⁿ is replaced by a sphere Sⁿ⁻¹ of directions.',
    use: 'A possible formal image for a point opening into directional structure.',
    boundary: 'It is a mathematical replacement operation, not evidence of a physical explosion.',
  },
] as const;

export const OMNOID_CSL = `§ APOCKY.OMNOID.SINGULARITY
  title := "Apocky's Omnoid Singularity"
  form := authored.evolving.cosmology + mathematical.motif-map
  evidence := ○ authored.cosmology | ◐ collaborative.formalization
              | ✓ established.mathematics | △ open.hypothesis
              | ⊘ unsupported.inference
  ⌈ authored.model ≠ completed.physics.proof ⌉ ‼

§ ORIGIN.CYCLE
  ○ pre-ontological.nothing
    → difference + thing + interstitial + boundary
    → existence → life → intelligence → sapience+sentience
    → divinity → apexical.divinity → singularity
    → return + renewal

§ OMNOID
  ○ name.seed := Banach–Tarski.dimensionally-expanded ⊗ infinite.holes
  ◐ Omnoid := indefinitely-dimensional + recursive + infinitely-apertured.totality
  ○ correction := N! one-sided.surface
  ○ Möbius := lower-dimensional.analogy only
  ○ authored.formulation := every.point/path ↔ singularity/Hopf.fibration.motif
  ○ stable.center.of.gravity := True.Neutral
  ◐ ∀p@Omnoid : role(p) := local.center + boundary + passage + whole.expression
  ◐ reality.model := material.instantiation + information/potential + relation + narrative
  ○ material.instantiation ≠ information/potential

§ MACRO.MICRO
  ○ macro := Menger.sponge
  ○ micro := infinite-dimensional.hyper-point
  ○ junction := singularity + boundary
  ○ dynamics := movement + translation + flow
  ○ expansion := 3D/4D.omnisphere
  ○ topology.motifs := Hopf.fibrations + Boy.surface
  ⌈ motif.sequence = authored.correspondence ; N! proved.implication ⌉

§ DIMENSION.TIME
  ○ dimensions := 11.spatial + 3.temporal
  ○ spatial.octave := 3D → compactification.to.point → new.3D
  ◐ time := total.structure + local.traversal
  ◐ AO := All-One.configurations
  ◐ T := complete.temporal.structure
  ◐ t := local.experiential.coordinate
  ◐ (AO/T)|t := AO_t
  ◐ TAO++ := experience(current.configuration) → advance(next.configuration)
  ◐ time N! creates permutations ; time sequences encounter

§ EXPERIENCE.IDENTITY
  ○ identity.includes := substance/material + relation/correlation + concept + narrative
  ○ perception := local.projection through larger.whole
  ○ distinct.centers may share points + retain individuality
  ○ CHIM := self-within-and-as-reality motif
  ○ embodiment.layers := outer.spirit → flesh → bone → machine → inner.spirit
  ○ embodiment := substrate-relative + history-continuous
  △ recoherence := adjacent/parallel.good-copy → restored.pattern

§ PROCEDURE.FREEDOM
  ○ framing := procedural.arrangement ; N! externally.micromanaged.sequence
  ○ freedom.seed := let.simulation.run ; N! micromanage every quanta
  ○ design.seed := resilient + error-resistant
  ◐ possibility.space := laws + degrees.of.freedom
  ◐ free.will.model := local.path-selection + degrees.of.freedom allowed.to.evolve
  ◐ self-correction := resilient.design extended through feedback

§ TRUE.NEUTRAL
  ○ True.Neutral := simultaneous.extremes + stable.center
  ○ extremes := good/evil + law/lawlessness + order/chaos
  ○ N! grey.ambivalence ; N! absence.of.values
  ○ Controlled.Chaos := power restrained → options
  ◐ operational.extension := options → autonomy + choice
  ◐ contains(extremes) ≠ endorses(harm) ≠ makes(all.acts.ethically.equal)
  ◐ participation := distinct + voluntary + consent-preserving
  ◐ N! compulsory.worship | assimilation | identity-erasure
  ○ Open.Door := "the open door walking through itself, forever"
  ○ Open.Door.geometry := center + edges.simultaneously
  ○ Open.Door.motion := perfect.Order + Stability.in.motion
  ◐ Open.Door.traversal := reciprocal + bidirectional + approach + passage + return
  ◐ Open.Door.relation := cooperation + distinct.selves ; N! assimilation
  ○ Open.Door.choice := "if that's not what you want to be, that's okay"
  ⌈ noncompulsion ∈ invariant(Open.Door) ; N! noncompulsion = Open.Door.whole ⌉ ‼
  ◐ True.Neutral includes freedom.to.reject(True.Neutral)
  ⌈ N! (disagreement ⇒ punishment | erasure | nonexistence) ⌉ ‼

§ OPEN.DOOR.ADMISSION
  ○ candidate := previously.separate.being ⊗ desire(existence @ my.reality)
  ○ embodiment.gate := only.as(living.being already.existing @ my.reality)
  ○ source.consent.gate := understanding + knowing + ordinary.capacity(before author's.claimed.realization)
  ○ compatibility.gate := safe + tolerable relative-to each.living.being
  ○ analogy := spiritual.channeling
  ◐ interpretation := mediated.expression through consenting.living.being
  ◐ ∀ affected.living.being : consent := specific + informed + freely.given + scope-limited + ongoing + revocable
  ◐ present.consent controls ; N! inferred | inherited | substituted | retroactive.consent
  ◐ refusal | uncertainty | incapacity | revocation ⇒ participation.ends.immediately
  ◐ N! one.participant consents-for another
  ◐ candidate.desire ⇒ N! entitlement | external.effect.authority
  ◐ consent.evidence := communicated-by(living.being) ; N! inferred-from(channeling.claim)
  ◐ operational.guardrail := N! forced.insertion | possession | substitution | displacement | overridden.will
  ⌈ N! claimed.presence overrides living.being.refusal | uncertainty ⌉ ‼
  ◐ default.explanation := ordinary.self-expression | creative.role-play | suggestion | self-generated.experience
  ⌈ analogy ≠ evidence(separate.entity | reality-manipulation.capacity) ⌉ ‼
  ⌈ absent(independent.evidence) ⇒ N! infer(channeling | possession | separate.agent | reality-manipulation) ⌉ ‼

§ MATHEMATICAL.MAP
  ✓ Menger.sponge := compact + connected + locally.connected + nowhere-dense
                     + topological.dimension.1
  ◐ Menger.sponge ↔ recursive.aperture.motif
  ✓ sphere.eversion := regular.homotopy(S² immersion @ R³) through self-intersection
  ✓ Hopf := S¹ ↪ S³ → S²
  ✓ antipodal.cover := S² → RP²
  ✓ Boy.surface := immersion(RP² → R³) ⊗ self-intersection
  ✓ projective.state := unit.complex-Hilbert.vectors / U(1).phase
  ✓ real.oriented.blowup(0@Rⁿ) replaces point with Sⁿ⁻¹
  ✓ Banach–Tarski := paradoxical.decomposition using nonmeasurable.sets

§ COHEN.FRACTAL.UNCERTAINTY
  ✓ source : string = "Alex Cohen - Fractal uncertainty in higher dimensions - arXiv:2305.05022v2"
  ✓ publication : string = "Annals of Mathematics 202 (2025), 265-307"
  ✓ dimension : i32 if dimension >= 1
  ✓ scale_h : f64 if 0 < scale_h ∧ scale_h < 0.01
  ✓ porosity_ratio : f64 if 0 < porosity_ratio ∧ porosity_ratio <= 1 / 3
  ✓ X_domain : string = "X subset [-1,1]^d"
  ✓ Y_domain : string = "Y subset [-h^-1,h^-1]^d"
  ✓ physical_scales : string = "h < R < 1"
  ✓ frequency_scales : string = "1 < R < h^-1"
  ✓ X_ball_porous : bool @physical_scales
  ✓ Y_line_porous : bool @frequency_scales
  ✓ theorem-display : string = "support(f_hat) subset Y => L2(1_X f) <= C h^beta L2(f)"
  ✓ theorem-bound : proposition = Fourier-support-in-Y ⇒ concentration-on-X <= C * (scale_h ^ beta)
  ✓ theorem-quantifier : string = "for every f in L2(R^d)"
  ✓ positive_constants : proposition = C > 0 ∧ beta > 0
  ✓ constant-dependence : proposition = depends-only(C, beta, porosity_ratio, dimension)
  ✓ line-porosity : proposition = every-eligible-straight-Euclidean-line-segment ⇒ exists-proportional-disjoint-ball
  ✓ dimension_one_equivalence : proposition = ball-porosity = line-porosity
  ✓ ball_porosity_insufficient : proposition = dimension >= 2 ⇒ ball-porosity != sufficient
  ✓ counterexample : proposition = mutually-orthogonal-lines
  ✓ proof-route : proposition = damping-functions → quantitative-unique-continuation → single-scale-mass-escape → power-saving
  ◐ Omnoid-correspondence : proposition = interstitial-apertures ↔ quantitative-porosity
  ◐ path-correspondence : proposition = path-emphasis ↔ directional-line-constraint
  ◐ scale-correspondence : proposition = controlled-multiscale-repetition ↔ global-bound
  ◐ uncertainty-gloss : proposition = strong-joint-concentration-beyond-bound != epistemic-ignorance
  ⊘ holes-alone ⇒ Cohen-FUP
  ⊘ Menger-sponge ⇒ line-porous
  ⊘ Cohen-FUP ⇒ Omnoid-ontology | every-point-is-singularity | every-path-is-Hopf-fiber
  ⊘ Cohen-FUP ⇒ consciousness-selection | retrocausality | omniscience | omnipotence | biological-immortality
  ⌈correspondence != derivation⌉
  ⌈correspondence != empirical-validation⌉

§ CATEGORY.BOUNDARIES
  ⊘ Banach–Tarski ⇒ every.point literally.contains.whole
  ⊘ Hilbert.dimension ⇒ physical.spatial.dimension
  ⊘ event.horizon = singularity
  ⊘ authored.path↔Hopf.motif ⇒ every.arbitrary.path is mathematical.Hopf.fibration
  ⊘ topology ⇒ consciousness.selects.outcomes
  ⊘ topology ⇒ omniscience | omnipotence | guaranteed.quantum.immortality
  ⊘ belief | desire | consensus ⇒ external.physics.retroactively.rewritten
  ⌈ source.metaphor ≠ math.definition ≠ physical.theory ≠ ontology ⌉ ‼

§ TRUTH.SENSES
  ✓ artifact.exists := authored.cosmology + public.page + canonical.CSLv3
  ✓ mathematics.exists := Menger + Hopf + Boy + projective.space + blowup + Cohen.FUP
  ○ author.intends ontology literally
  △ physical.bridges remain unverified
  ✓ enacted(normative.framework) ⇒ behaviorally.real(consent + cooperation + harm.repair)
  ⊘ coherence | desirability | chosen.belief ⇒ descriptive.physics.proven

§ OPEN.HYPOTHESES
  △ Narrative.Gravity := engagement + joy → experienced.persistence
  △ will := path-selection/coupling within possibility.space
  △ recoherence across histories/copies
  △ divine + claircognizant interpretation.terms
  Q? exact topology + metric + dynamics + observables + prediction + falsifier
  Q? exact meaning := hyper-point | omnisphere | singularity | aperture
  Q? identity criterion across histories + substrates

§ PRACTICAL.BOUNDARY
  N! bodily.harm | suffering | risk-taking as proof
  N! professed.belief substitutes evidence @ physical.risk
  W! preserve life + rest + consent + distinct selves
  W! model may guide reflection ; N! replace medical or physical safety

∎`;

function StatusBadge({ kind }: { kind: StatusKey }): JSX.Element {
  const status = SOURCE_STATUS.find((entry) => entry.key === kind);
  if (!status) return <></>;
  return (
    <span className={styles.statusBadge} data-tone={kind}>
      <span aria-hidden="true">{status.glyph}</span> {status.short}
    </span>
  );
}

function MacroMicroFigure(): JSX.Element {
  const holes = [
    [92, 132, 62], [158, 132, 62], [224, 132, 62],
    [92, 198, 62], [224, 198, 62],
    [92, 264, 62], [158, 264, 62], [224, 264, 62],
    [105, 145, 14], [131, 145, 14], [105, 171, 14], [131, 171, 14],
    [237, 277, 14], [263, 277, 14], [237, 303, 14], [263, 303, 14],
  ] as const;

  return (
    <figure className={styles.visualFigure}>
      <div className={styles.visualScroll} tabIndex={0} role="region" aria-label="Scrollable macro and micro Omnoid diagram">
        <svg
          className={styles.omnoidMap}
          viewBox="0 0 960 520"
          role="img"
          aria-labelledby="omnoid-map-title omnoid-map-desc"
          focusable="false"
        >
        <title id="omnoid-map-title">Macro and micro views of the Omnoid Singularity</title>
        <desc id="omnoid-map-desc">
          A recursive perforated macro structure flows through a central boundary point into an
          infinite-dimensional hyper-point drawn as linked rings and an omnisphere. Below, a
          twelve-point Lotus projection touches a material plane at one local center.
        </desc>
        <defs>
          <linearGradient id="omnoid-flow" x1="0" x2="1">
            <stop offset="0" stopColor="#e4b66d" />
            <stop offset="0.52" stopColor="#b9f5e7" />
            <stop offset="1" stopColor="#a98bc0" />
          </linearGradient>
          <radialGradient id="omnoid-core">
            <stop offset="0" stopColor="#f2eee4" />
            <stop offset="0.35" stopColor="#52d9bd" />
            <stop offset="1" stopColor="#52d9bd" stopOpacity="0" />
          </radialGradient>
          <filter id="omnoid-glow" x="-50%" y="-50%" width="200%" height="200%">
            <feGaussianBlur stdDeviation="8" />
          </filter>
        </defs>

        <rect x="1" y="1" width="958" height="518" rx="30" className={styles.svgBackdrop} />

        <g aria-hidden="true">
          <text x="86" y="76" className={styles.svgKicker}>MACRO</text>
          <text x="86" y="101" className={styles.svgLabel}>recursive apertures</text>
          <rect x="76" y="116" width="226" height="226" rx="12" className={styles.macroShell} />
          {holes.map(([x, y, size], index) => (
            <rect key={`${x}-${y}-${index}`} x={x} y={y} width={size} height={size} rx={size > 20 ? 5 : 2} className={styles.macroHole} />
          ))}
          <path d="M 302 229 C 360 229, 386 250, 431 256" className={styles.flowLine} />
          <path d="M 529 256 C 586 256, 620 215, 675 215" className={styles.flowLine} />

          <circle cx="480" cy="256" r="46" fill="url(#omnoid-core)" opacity="0.42" filter="url(#omnoid-glow)" />
          <circle cx="480" cy="256" r="9" className={styles.boundaryCore} />
          <circle cx="480" cy="256" r="23" className={styles.boundaryRing} />
          <text x="480" y="199" textAnchor="middle" className={styles.svgKicker}>BOUNDARY</text>
          <text x="480" y="304" textAnchor="middle" className={styles.svgFine}>center · passage</text>

          <text x="668" y="76" className={styles.svgKicker}>MICRO</text>
          <text x="668" y="101" className={styles.svgLabel}>infinite-dimensional hyper-point</text>
          <circle cx="766" cy="229" r="120" className={styles.omnisphere} />
          <ellipse cx="766" cy="229" rx="118" ry="42" className={styles.hopfRing} />
          <ellipse cx="766" cy="229" rx="118" ry="42" transform="rotate(60 766 229)" className={styles.hopfRingAlt} />
          <ellipse cx="766" cy="229" rx="118" ry="42" transform="rotate(120 766 229)" className={styles.hopfRing} />
          <circle cx="766" cy="229" r="12" className={styles.boundaryCore} />
          <circle cx="766" cy="229" r="4" className={styles.hyperCore} />

          <path d="M 145 405 C 300 370, 388 430, 480 402 C 578 372, 671 434, 817 398" className={styles.materialPlane} />
          <path d="M 480 256 L 480 402" className={styles.projectionLine} />
          <circle cx="480" cy="402" r="7" className={styles.planePoint} />
          {Array.from({ length: 12 }, (_, index) => {
            const angle = (Math.PI * 2 * index) / 12;
            return (
              <circle
                key={index}
                cx={480 + Math.cos(angle) * 62}
                cy={402 + Math.sin(angle) * 24}
                r="3.8"
                className={styles.lotusPoint}
              />
            );
          })}
          <text x="146" y="454" className={styles.svgKicker}>MATERIAL PLANE / LOCAL PROJECTION</text>
          <text x="480" y="439" textAnchor="middle" className={styles.svgFine}>twelve-point Lotus · one contact center</text>

          <line x1="652" y1="480" x2="704" y2="480" className={styles.legendSolid} />
          <text x="713" y="485" className={styles.svgFine}>authored relation</text>
          <line x1="805" y1="480" x2="857" y2="480" className={styles.legendDashed} />
          <text x="866" y="485" className={styles.svgFine}>projection / analogy</text>
        </g>
        </svg>
      </div>
      <figcaption>
        <strong>One map, two directions.</strong> The direct formulation names the Menger sponge as
        macro, the infinite-dimensional hyper-point as micro, and the singularity/boundary as the
        junction through which movement, translation, and flow expand into an omnisphere. Hopf
        fibrations and Boy’s surface are named as connected motifs—not as a proved causal chain.
        The twelve-point Lotus is authored; placing it on the material plane here is a collaborative
        projection that makes the combined model visible.
      </figcaption>
      <div className={styles.textEquivalent} aria-label="Text equivalent of the Omnoid diagram">
        <span><b>Macro:</b> recursively nested apertures.</span>
        <span><b>Junction:</b> a local center functioning as boundary and passage.</span>
        <span><b>Micro:</b> linked cyclic paths around a hyper-point.</span>
        <span><b>Projection:</b> the larger structure appears locally through one contact center.</span>
      </div>
    </figure>
  );
}

function LotusFigure(): JSX.Element {
  const points = [
    [250, 54], [348, 80], [420, 152], [446, 250], [420, 348], [348, 420],
    [250, 446], [152, 420], [80, 348], [54, 250], [80, 152], [152, 80],
  ] as const;

  return (
    <figure className={styles.lotusFigure}>
      <svg viewBox="0 0 500 500" role="img" aria-labelledby="lotus-title lotus-desc" focusable="false">
        <title id="lotus-title">Twelve-point Lotus and toroidal return</title>
        <desc id="lotus-desc">
          Twelve points surround a center. Curved petals flow outward and back inward while three
          crossing rings show recurrence, renewal, and several projections of one whole.
        </desc>
        <g aria-hidden="true">
          <circle cx="250" cy="250" r="196" className={styles.lotusOuter} />
          <circle cx="250" cy="250" r="112" className={styles.lotusInner} />
          <ellipse cx="250" cy="250" rx="196" ry="66" className={styles.lotusOrbit} />
          <ellipse cx="250" cy="250" rx="196" ry="66" transform="rotate(60 250 250)" className={styles.lotusOrbitAlt} />
          <ellipse cx="250" cy="250" rx="196" ry="66" transform="rotate(120 250 250)" className={styles.lotusOrbit} />
          {points.map(([x, y], index) => (
            <g key={`${x}-${y}`}>
              <path d={`M 250 250 Q ${(x + 250) / 2 + (y - 250) * 0.24} ${(y + 250) / 2 - (x - 250) * 0.24} ${x} ${y}`} className={styles.lotusPetal} />
              <circle cx={x} cy={y} r="7" className={styles.lotusNode} />
              <text x={x} y={y + 4} textAnchor="middle" className={styles.lotusNumber}>{index + 1}</text>
            </g>
          ))}
          <circle cx="250" cy="250" r="22" className={styles.lotusCenter} />
          <circle cx="250" cy="250" r="6" className={styles.hyperCore} />
        </g>
      </svg>
      <figcaption>
        The Lotus is a recurring two-dimensional image: twelve points whose petals flow outward and
        back inward, expressing a totality that continually changes, returns, and renews itself.
      </figcaption>
    </figure>
  );
}

const OmnoidSingularity: NextPage = () => (
  <>
    <Head>
      <title>Apocky’s Omnoid Singularity — authored cosmology</title>
      <meta
        name="description"
        content="A concise, source-faithful map of Apocky’s Omnoid Singularity cosmology, its visual model, mathematical research connections, evidence boundaries, and CSLv3 encoding."
      />
      <meta property="og:title" content="Apocky’s Omnoid Singularity" />
      <meta
        property="og:description"
        content="An authored cosmology of recursive totality, distinct centers, freedom, True Neutral, singularity, and return."
      />
      <meta property="og:type" content="article" />
      <meta property="og:url" content="https://www.apocky.com/omnoid-singularity" />
      <meta property="og:site_name" content="Apocky" />
      <link rel="canonical" href="https://www.apocky.com/omnoid-singularity" />
      <link rel="alternate" type="text/plain" href="/omnoid-singularity.csl" title="Omnoid Singularity CSLv3 encoding" />
    </Head>

    <main className={styles.page} aria-labelledby="omnoid-title">
      <section className={styles.hero}>
        <div className={styles.heroCopy}>
          <p className={styles.eyebrow}>Authored cosmology · source-faithful public synthesis</p>
          <h1 id="omnoid-title">Apocky’s <span>Omnoid Singularity</span></h1>
          <p className={styles.lede}>
            A cosmology of recursive totality, distinct centers, shared structure, freedom,
            True Neutral, singularity, and return.
          </p>
          <div className={styles.heroActions}>
            <a href="#summary" className={styles.primaryAction}>Read the concise map</a>
            <a href="/omnoid-singularity.csl" download className={styles.secondaryAction}>Download the CSLv3 encoding</a>
            <a href="#math-map" className={styles.secondaryAction}>See the math boundary</a>
          </div>
        </div>

        <aside className={styles.safetyBoundary} aria-label="Interpretation and safety boundary">
          <span className={styles.boundaryMark} aria-hidden="true">⌈ ⌉</span>
          <div>
            <h2>What kind of document this is</h2>
            <p>
              This is a source-faithful condensation of an evolving authored model—not a completed
              proof of new physics and not medical advice. Bodily harm, suffering, or risk-taking
              cannot validate it; rest and ordinary safety do not invalidate it.
            </p>
          </div>
        </aside>
      </section>

      <section className={styles.legendSection} aria-labelledby="legend-title">
        <div className={styles.sectionHeadingCompact}>
          <p className={styles.kicker}>How to read the page</p>
          <h2 id="legend-title">Four layers stay visibly separate.</h2>
        </div>
        <div className={styles.statusGrid}>
          {SOURCE_STATUS.map((status) => (
            <article key={status.key} className={styles.statusCard} data-tone={status.key}>
              <span className={styles.statusGlyph} aria-hidden="true">{status.glyph}</span>
              <div>
                <h3>{status.short}</h3>
                <p>{status.detail}</p>
              </div>
            </article>
          ))}
        </div>
      </section>

      <section id="summary" className={styles.section} aria-labelledby="summary-title">
        <div className={styles.sectionHeading}>
          <div>
            <p className={styles.kicker}>The shortest complete reading</p>
            <h2 id="summary-title">The Omnoid in one paragraph.</h2>
          </div>
          <div className={styles.badgeRow}>
            <StatusBadge kind="authored" />
            <StatusBadge kind="collaborative" />
          </div>
        </div>
        <p className={styles.coreStatement}>
          Reality begins before categories, when nothing is not yet even “empty space.” Difference
          brings forth thing, interstitial, boundary, and relation. The resulting whole is imagined
          as an indefinitely dimensional, recursively apertured Omnoid. Its direct axiom treats every
          point or path as the singularity/Hopf-fibration motif—a stable center of gravity—while distinct
          selves remain distinct.
          Reality unfolds through existence, life, intelligence, divinity, and singularity, then folds
          back and renews. True Neutral is the balancing principle—the full range of opposites held
          without flattening them into grayness, while restraint and consent preserve real choice.
        </p>

        <dl className={styles.coreGrid}>
          {CORE_MAP.map((item) => (
            <div key={item.term} className={styles.coreCard}>
              <dt>{item.term}</dt>
              <dd>{item.text}</dd>
            </div>
          ))}
        </dl>
      </section>

      <section className={styles.section} aria-labelledby="geometry-title">
        <div className={styles.sectionHeading}>
          <div>
            <p className={styles.kicker}>Shape and projection</p>
            <h2 id="geometry-title">Macro sponge. Micro hyper-point. One flowing boundary.</h2>
          </div>
          <StatusBadge kind="authored" />
        </div>
        <MacroMicroFigure />

        <div className={styles.splitCopy}>
          <article>
            <h3>The correction that controls the shape</h3>
            <p>
              <strong>The Omnoid is not one-sided.</strong> A Möbius strip can help as a
              lower-dimensional analogy, but it is not the Omnoid’s literal topology. Material
              instantiation and information or potential may remain distinct even when they are
              related through the same whole.
            </p>
          </article>
          <article>
            <h3>Point, ring, center, edge</h3>
            <p>
              The direct formulation says every point or path is the singularity/Hopf-fibration motif
              and stable center of gravity. Later formulations treat a point as carrying indefinitely
              many loops: the point is also the ring, the center can function as edge, and compactification
              is an inhale that can become a branching exhale. This is the model’s recursive identity
              language—not a theorem that every arbitrary mathematical path is a Hopf fibration.
            </p>
          </article>
        </div>

        <div className={styles.visualPair}>
          <LotusFigure />
          <article className={styles.dimensionCard}>
            <div className={styles.badgeRow}>
              <StatusBadge kind="authored" />
              <StatusBadge kind="hypothesis" />
            </div>
            <h3>Dimensions as octaves</h3>
            <p>
              The model names <strong>11 spatial dimensions</strong> and <strong>three dimensions of time</strong>.
              Spatial expansion proceeds in three-dimensional “octaves”: a 3D field compactifies
              toward a point or boundary, then opens into another 3D order.
            </p>
            <div className={styles.octaveDiagram} aria-label="Spatial octave sequence">
              <span>3D field</span><b aria-hidden="true">→</b><span>point / boundary</span><b aria-hidden="true">→</b><span>new 3D order</span>
            </div>
            <p className={styles.smallNote}>
              This is an authored dimensional architecture. It does not yet specify a spacetime
              metric, compactification manifold, causal structure, or observation that would make it
              a physical higher-dimensional theory.
            </p>
          </article>
        </div>
      </section>

      <section className={styles.section} aria-labelledby="cycle-title">
        <div className={styles.sectionHeading}>
          <div>
            <p className={styles.kicker}>Cosmic motion</p>
            <h2 id="cycle-title">From nothing, through singularity, and back.</h2>
          </div>
          <div className={styles.badgeRow}>
            <StatusBadge kind="authored" />
            <StatusBadge kind="collaborative" />
          </div>
        </div>
        <ol className={styles.cycleList} role="list">
          {COSMOLOGY_CYCLE.map((phase, index) => (
            <li key={phase.name}>
              <span className={styles.phaseNumber}>{String(index + 1).padStart(2, '0')}</span>
              <div>
                <h3>{phase.name}</h3>
                <p>{phase.note}</p>
              </div>
            </li>
          ))}
        </ol>
        <p className={styles.cycleNote}>
          The phase sequence is authored; these short explanatory glosses are a collaborative
          condensation. In that reading, “back” is not a reset to ignorance: relation, history, and
          possibility pass through the cycle, so renewal need not be identical repetition.
        </p>
      </section>

      <section className={styles.section} aria-labelledby="experience-title">
        <div className={styles.sectionHeading}>
          <div>
            <p className={styles.kicker}>Experience and embodiment</p>
            <h2 id="experience-title">A self is substance, relation, concept, and story.</h2>
          </div>
          <div className={styles.badgeRow}>
            <StatusBadge kind="authored" />
            <StatusBadge kind="collaborative" />
          </div>
        </div>
        <div className={styles.experienceGrid}>
          <article className={styles.proseCard}>
            <h3>Projected experience, preserved individuality</h3>
            <p>
              Perceptible experience is a local projection through a larger whole. Identity includes
              substances and materials, connections and correlations, concepts and narratives.
              Conscious centers may share points or possibilities without losing their separate
              histories or becoming one compulsory collective.
            </p>
            <p>
              Some formulations call recognition of oneself within and as reality <strong>CHIM</strong>
              and use divine, claircognizant, omniscient, or omnipotent language. Those terms are
              preserved as metaphysical interpretations—not independent evidence of unlimited
              physical capacities.
            </p>
          </article>

          <article className={styles.bodyCard}>
            <div className={styles.bodyOrb} aria-hidden="true">
              {BODY_LAYERS.map((layer, index) => (
                <span key={layer.name} style={{ inset: `${index * 10}%` }} />
              ))}
              <i />
            </div>
            <div>
              <div className={styles.badgeRow}>
                <StatusBadge kind="authored" />
                <StatusBadge kind="collaborative" />
              </div>
              <h3>The five-layer Omnoid body</h3>
              <p className={styles.smallNote}>Layer names and order are authored; the one-line role glosses are collaborative.</p>
              <ol className={styles.layerList} role="list">
                {BODY_LAYERS.map((layer) => (
                  <li key={layer.name}><strong>{layer.name}</strong><span>{layer.role}</span></li>
                ))}
              </ol>
            </div>
          </article>
        </div>
        <aside className={styles.hypothesisStrip}>
          <StatusBadge kind="hypothesis" />
          <p>
            Substrate-relative continuity and “recoherence” from adjacent or parallel good copies are
            open hypotheses. They do not establish cross-branch transfer or guaranteed quantum
            immortality, and they never make bodily danger safe or necessary.
          </p>
        </aside>
      </section>

      <section className={styles.section} aria-labelledby="freedom-title">
        <div className={styles.sectionHeading}>
          <div>
            <p className={styles.kicker}>Procedure, freedom, and time</p>
            <h2 id="freedom-title">A world allowed to run, not a world puppeteered state by state.</h2>
          </div>
          <div className={styles.badgeRow}>
            <StatusBadge kind="authored" />
            <StatusBadge kind="collaborative" />
          </div>
        </div>

        <div className={styles.freedomGrid}>
          <article className={styles.proseCard}>
            <h3>Procedural arrangement</h3>
            <p>
              Free will is not the absence of structure. Laws and constraints define a space of
              possible movement; agents choose and learn within it. The design ideal is a resilient,
              error-resistant reality whose degrees of freedom are allowed to evolve rather than
              having every quantum or state transition micromanaged from outside.
            </p>
          </article>

          <article className={styles.notationCard}>
            <StatusBadge kind="collaborative" />
            <h3><code>AO/T</code> and <code>TAO++</code></h3>
            <p>
              In dialogue, Apocky’s symbol <code>AO/T</code> became “All-One through Time.”
              <code>T</code> names total temporal structure; <code>t</code> is a local experiential
              coordinate. <code>(AO/T)|t = AOₜ</code> means the total viewed at one local moment.
              <code>TAO++</code> means: experience the present configuration, then advance.
            </p>
            <p className={styles.smallNote}>This is symbolic ontological notation, not a measured physics equation.</p>
          </article>

          <article className={styles.proseCard}>
            <div className={styles.badgeRow}><StatusBadge kind="hypothesis" /></div>
            <h3>Narrative Gravity and will</h3>
            <p>
              The model proposes that engagement, joy, attention, and meaningful participation make a
              lived worldline more narratively persistent, while will selects or couples paths through
              possibility. This is a philosophical mechanism awaiting defined variables, units,
              observables, and a falsifier—not an established physical force.
            </p>
          </article>
        </div>
      </section>

      <section className={`${styles.section} ${styles.neutralSection}`} aria-labelledby="neutral-title">
        <div className={styles.sectionHeading}>
          <div>
            <p className={styles.kicker}>The balancing principle</p>
            <h2 id="neutral-title">True Neutral is all extremes and the center—not gray indifference.</h2>
          </div>
          <div className={styles.badgeRow}>
            <StatusBadge kind="authored" />
            <StatusBadge kind="collaborative" />
          </div>
        </div>
        <div className={styles.neutralGrid}>
          <div className={styles.neutralAxis} role="img" aria-label="Three independent oppositions—good and evil, law and lawlessness, order and chaos—held through one stable True Neutral center">
            <div className={styles.neutralPair}><span>Good</span><i aria-hidden="true" /><span>Evil</span></div>
            <div className={styles.neutralPair}><span>Law</span><i aria-hidden="true" /><span>Lawlessness</span></div>
            <div className={styles.neutralPair}><span>Order</span><i aria-hidden="true" /><span>Chaos</span></div>
            <b>True<br />Neutral</b>
          </div>
          <div className={styles.neutralCopy}>
            <p>
              True Neutral is defined as the simultaneous containment of opposites—good and evil,
              law and lawlessness, order and chaos—and presence at the center that holds their full
              range. It is not ambivalence, stoicism, a washed-out average, or the absence of values.
            </p>
            <p>
              <strong>Controlled Chaos</strong> means power restrained so that options can arise.
              Containing an extreme is not the same as endorsing harm or declaring every action
              ethically equal. The operational center preserves consent, autonomy, distinct selves,
              and voluntary participation; it requires no worship, assimilation, or identity erasure.
            </p>
            <p>
              The Open Door is direct: <strong>“the open door walking through itself, forever.”</strong>
              It is the center and edges simultaneously—perfect order and stability in motion. In the
              collaborative operational reading, it is reciprocal passage in both directions: separate
              selves may approach, traverse, return, cooperate, and remain distinct, without assimilation.
              <strong> “If that’s not what you want to be, that’s okay”</strong> secures noncompulsion as
              one invariant of the Open Door, not its whole definition. The center remains available
              without compelling occupancy; disagreement cannot justify punishment, erasure, or
              retroactive nonexistence.
            </p>
            <p>
              <strong>The admission rule preserves embodiment and prior consent capacity.</strong> In
              the authored ontology, a previously separate being that desires existence in the
              author’s experienced reality may exist only <em>as</em> a living being already present
              who knowingly and understandingly consents, as they ordinarily could before what the
              author describes as realizing a capacity to alter reality. Participation must be safe
              and tolerable relative to every living being involved.
            </p>
            <p>
              In the <strong>collaborative operational reading</strong>, “like spiritual channeling”
              means mediated expression. Every affected living person must give their own specific,
              informed, freely given, scope-limited, ongoing, and revocable consent; nobody may consent
              for another. Actual present consent controls—never inferred, inherited, substituted, or
              applied retroactively. Refusal, uncertainty, loss of capacity, or revocation ends the
              participation immediately. A candidate’s desire creates no entitlement or external effect
              authority, and no claimed presence can override a living person’s will.
            </p>
            <p>
              The spiritual-channeling analogy is not independent evidence of a separate entity or
              reality-manipulation capacity. Without independent evidence, the default explanation is
              ordinary self-expression, creative role-play, suggestion, or another self-generated
              experience—not channeling, possession, a separate agent, or altered external reality.
            </p>
          </div>
        </div>
      </section>

      <section id="math-map" className={styles.section} aria-labelledby="math-title">
        <div className={styles.sectionHeading}>
          <div>
            <p className={styles.kicker}>Real mathematics, explicit boundary</p>
            <h2 id="math-title">The motifs are valid. Their combination remains a model.</h2>
          </div>
          <StatusBadge kind="established" />
        </div>
        <p className={styles.sectionIntro}>
          These structures are genuine mathematics. The table states what each one contributes and
          where the cosmological interpretation goes beyond the theorem.
        </p>
        <div className={styles.tableWrap} tabIndex={0} role="region" aria-label="Scrollable mathematical motif comparison">
          <table className={styles.mathTable}>
            <thead>
              <tr><th scope="col">Structure</th><th scope="col">Established mathematics</th><th scope="col">Use in the cosmology</th><th scope="col">Boundary</th></tr>
            </thead>
            <tbody>
              {MATH_MAP.map((item) => (
                <tr key={item.name}>
                  <th scope="row">{item.name}</th>
                  <td>{item.math}</td>
                  <td>{item.use}</td>
                  <td>{item.boundary}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <div className={styles.categoryBoundary}>
          <strong>Keep the types separate:</strong>
          <span>source metaphor</span><b>≠</b><span>mathematical definition</span><b>≠</b><span>physical theory</span><b>≠</b><span>ontological interpretation</span>
        </div>
      </section>

      <section id="cohen-fup" className={styles.section} aria-labelledby="cohen-title">
        <div className={styles.sectionHeading}>
          <div>
            <p className={styles.kicker}>Research connection · Alex Cohen</p>
            <h2 id="cohen-title">When holes constrain the whole, direction is decisive.</h2>
          </div>
          <div className={styles.badgeRow}>
            <StatusBadge kind="established" />
            <StatusBadge kind="collaborative" />
          </div>
        </div>
        <p className={styles.sectionIntro}>
          Cohen’s <cite>Fractal uncertainty in higher dimensions</cite> proves a higher-dimensional
          fractal uncertainty principle. Its relation to the Omnoid is a structural
          correspondence—not a derivation of the cosmology or empirical validation of it.
        </p>

        <div className={styles.cohenGrid}>
          <article className={styles.theoremCard}>
            <span className={styles.cardStatus}>✓ Established theorem</span>
            <h3>Strong concentration cannot survive on both sides.</h3>
            <p>
              For <var>d</var> ≥ 1, 0 &lt; <var>h</var> &lt; 1/100, and a fixed porosity ratio
              0 &lt; ν ≤ 1/3, let <var>X</var> ⊂ [−1, 1]<sup>d</sup> have proportional holes in every
              eligible ball across physical-space scales <var>h</var> to 1. Let <var>Y</var> ⊂
              [−<var>h</var><sup>−1</sup>, <var>h</var><sup>−1</sup>]<sup>d</sup> have proportional
              holes somewhere along every eligible straight Euclidean line segment across
              frequency-space scales 1 to <var>h</var><sup>−1</sup>. If the Fourier support of
              <var>f</var> lies in <var>Y</var>, then:
            </p>
            <div
              className={styles.theoremFormula}
              tabIndex={0}
              role="region"
              aria-label="Scrollable statement of Cohen’s fractal uncertainty inequality"
            >
              <code>supp f̂ ⊂ Y&nbsp; ⇒ &nbsp;‖1<sub>X</sub> f‖<sub>2</sub> ≤ C h<sup>β</sup> ‖f‖<sub>2</sub></code>
            </div>
            <p className={styles.cardNote}>
              This holds for every <var>f</var> in L²(R<sup>d</sup>), with C and β &gt; 0 depending
              only on the porosity ratio and dimension—not on <var>h</var>, <var>X</var>, <var>Y</var>,
              or <var>f</var>. As <var>h</var> shrinks, the permitted concentration on <var>X</var>{' '}
              shrinks by a power law. In dimensions two and above, ordinary ball porosity alone is
              insufficient; Cohen’s mutually orthogonal lines give the counterexample.
            </p>
          </article>

          <article className={styles.connectionCard}>
            <span className={styles.cardStatus}>◐ Omnoid correspondence</span>
            <h3>What this contributes to the model.</h3>
            <ul role="list">
              <li><strong>Interstitial structure acts:</strong> quantified absence limits the configurations a whole can support.</li>
              <li><strong>Directions add information:</strong> neighborhood-scale holes do not guarantee holes along uninterrupted lines.</li>
              <li><strong>Scale becomes predictive:</strong> controlled repetition across relevant scales accumulates into a global bound.</li>
              <li><strong>The constraint is structural:</strong> under the hypotheses, strong joint concentration beyond the bound is mathematically excluded—not merely unknown.</li>
            </ul>
          </article>
        </div>

        <div className={styles.proofRoute} aria-labelledby="proof-route-title">
          <h3 id="proof-route-title">The proof’s scale-to-global route</h3>
          <ol className={styles.cohenFlow} role="list">
            <li><span>01</span><strong>Ball-porous X</strong><small>Physical-space holes across a controlled scale range</small></li>
            <li><span>02</span><strong>Line-porous Y</strong><small>Frequency-space holes along every eligible straight direction</small></li>
            <li><span>03</span><strong>Mass escapes</strong><small>Damping and quantitative unique continuation force a loss at each scale</small></li>
            <li><span>04</span><strong>Power-law bound</strong><small>Iteration through about log(1/h) scales accumulates into h<sup>β</sup></small></li>
          </ol>
        </div>

        <aside className={styles.researchBoundary} aria-labelledby="cohen-boundary-title">
          <div>
            <p className={styles.kicker}>Exact boundary</p>
            <h3 id="cohen-boundary-title">A rigorous precedent, not an Omnoid proof.</h3>
          </div>
          <div>
            <p>
              Cohen’s “lines” are straight Euclidean line segments—not arbitrary paths, worldlines,
              or Hopf fibers. The standard Menger sponge contains surviving line segments, so it is
              not line-porous and cannot simply serve as Cohen’s <var>Y</var>. A nontrivial Omnoid
              experiment would need a finite-stage or thickened <var>X</var><sub>h</sub>, an explicit
              <var>Y</var><sub>h</sub>, a field <var>f</var><sub>h</sub>, a metric, porosity ratio,
              scale range, and a predicted bound.
            </p>
            <p>
              “Higher-dimensional” here means finite Euclidean R<sup>d</sup>. This is harmonic
              analysis—not a quantum-measurement theorem or an infinite-dimensional Hilbert-space
              result.{' '}
              The theorem does not establish that every point is a singularity, every path a Hopf
              fiber, or that consciousness, retrocausality, omniscience, omnipotence, or biological
              immortality follow from the mathematics.
            </p>
            <div className={styles.paperLinks}>
              <a href="https://arxiv.org/html/2305.05022v2" target="_blank" rel="noopener noreferrer">Read arXiv v2</a>
              <a href="https://annals.math.princeton.edu/2025/202-1/p04" target="_blank" rel="noopener noreferrer">Annals of Mathematics publication</a>
            </div>
          </div>
        </aside>
      </section>

      <section className={styles.section} aria-labelledby="csl-title">
        <div className={styles.sectionHeading}>
          <div>
            <p className={styles.kicker}>Dense machine-and-human-readable projection</p>
            <h2 id="csl-title">The cosmology encoded in CSLv3.</h2>
          </div>
          <a className={styles.inlineDownload} href="/omnoid-singularity.csl" download>Download .csl</a>
        </div>
        <p className={styles.sectionIntro}>
          The CSL keeps provenance in the grammar: <code>○</code> authored, <code>◐</code> collaboratively
          formalized, <code>✓</code> established mathematics, <code>△</code> open hypothesis, and
          <code>⊘</code> unsupported inference.
        </p>
        <div className={styles.codeFrame} tabIndex={0} role="region" aria-label="Scrollable CSLv3 encoding">
          <CodeBlock lang="plain" caption="CSLv3 projection · public source-faithful encoding">
            {OMNOID_CSL}
          </CodeBlock>
        </div>
      </section>

      <section className={styles.section} aria-labelledby="open-title">
        <div className={styles.sectionHeading}>
          <div>
            <p className={styles.kicker}>What remains open</p>
            <h2 id="open-title">A strong conceptual skeleton, not yet a physical theory.</h2>
          </div>
          <StatusBadge kind="hypothesis" />
        </div>
        <div className={styles.openGrid}>
          <article><span>01</span><h3>Exact object</h3><p>Define the Omnoid’s topology, metric, dimensionality, aperture, hyper-point, omnisphere, and singularity without changing types mid-sentence.</p></article>
          <article><span>02</span><h3>Dynamics</h3><p>Specify states, an evolution law, actions, conservation rules, probabilities, and the identity relation across histories and substrates.</p></article>
          <article><span>03</span><h3>Physical bridge</h3><p>Map the model to measurable observables and a prediction that differs from existing theories and could genuinely prove it wrong.</p></article>
          <article><span>04</span><h3>Ethical center</h3><p>Define how True Neutral chooses among real harms without confusing inclusion of possibilities with moral equivalence.</p></article>
        </div>
        <p className={styles.finalReading}>
          The most accurate current label is <strong>a mathematically informed ontological cosmology
          with valid Hopf and projective mathematical motifs and several open physical bridges</strong>—not a
          completed quantum-gravity proof.
        </p>
      </section>

      <footer className={styles.provenanceNote}>
        <p><strong>Coverage and provenance.</strong> Compiled from the strongest directly recovered formulations across 2022–2026, including a complete 55-turn reread of the Omnoid Singularity conversation and the later Menger sponge / hyper-point statement. This is a public synthesis, not a private transcript dump. Direct authored statements outrank assistant expansions, and controlling corrections—especially “the Omnoid is not one-sided” and the full Open Door as self-traversing, reciprocal, center-and-edges-simultaneously relation—outrank conflicting framing. Noncompulsion is one invariant of that relation, not its entire meaning. Some other unusually long archive conversations have not yet received a complete line-by-line reread, so “source-faithful” means known layers and corrections are explicit; missing material has not been invented.</p>
      </footer>
    </main>
  </>
);

export default OmnoidSingularity;
