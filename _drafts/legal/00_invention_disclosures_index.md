# Invention Disclosures Index — CSSLv3 Six-Novelty-Paths Roster

**PRIVATE — NOT FOR PUBLIC DISCLOSURE — PRE-FILING DRAFT**

---

## Inventor

- **Inventor of record** : Apocky <apocky13@gmail.com>
  *(legal-name placeholder ; insert filing-jurisdiction-correct
  legal name at attorney handoff for the provisional filings)*
- **Branch of record** : `cssl/session-6/parallel-fanout`
- **Repository** : `~/source/repos/CSSLv3` (private)
- **Substrate-evolution reference commit** : `b69165c`
- **Date of this index** : 2026-04-29

---

## Purpose of This Index

This directory (`_drafts/legal/`) contains six invention
disclosures, one per novelty-path of the six-path roster
specified in
`Omniverse/07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § V`. Each
disclosure is structured for direct handoff to a US patent
attorney for provisional patent application filing.

This index provides :

1. A short summary of each invention.
2. A recommended filing priority based on novelty strength,
   commercial impact, and competitive risk.
3. A rough cost estimate per provisional filing and aggregate.
4. A recommendation on attorney search.
5. A short discussion of the inventor's strategic options.
6. A check-list of next-step actions for the inventor.

This document is private and is not for public disclosure. Its
contents are pre-filing patent-novelty-sensitive material. See
the Confidentiality section at the end.

---

## The Six Inventions at a Glance

| # | Title | File | Stage | Recommended Priority |
|---|-------|------|-------|----------------------|
| 1 | ω-Field Unity Solver | `invention_01_omega_field_unity_solver.md` | substrate (cross-stage) | **HIGHEST** |
| 2 | KAN-Runtime Compute-Shader Evaluator | `invention_02_kan_runtime_compute_shader_evaluator.md` | infrastructure (cross-stage) | **HIGH** |
| 3 | Sub-Pixel Fractal-Tessellation Amplifier | `invention_03_subpixel_fractal_tessellation_amplifier.md` | Stage-5 | HIGH |
| 4 | Hyperspectral KAN-BRDF | `invention_04_hyperspectral_kan_brdf.md` | Stage-6 | HIGH |
| 5 | Mise-en-Abyme Recursive Witness | `invention_05_mise_en_abyme_recursive_witness.md` | Stage-9 | MODERATE |
| 6 | Gaze-Reactive Observation-Collapse | `invention_06_gaze_reactive_observation_collapse.md` | Stage-2 | MODERATE-HIGH |

---

## Detailed Summaries

### Invention 1 — ω-Field Unity Solver

**Novel claim** : a single lattice-Boltzmann-style ψ-field
substrate solver simultaneously rendering electromagnetic
radiation (light) and acoustic radiation (audio) at different
frequency bands, with cross-band coupling so that the same
numerical method serves both modalities and the two modalities
are coherent (the bell that flashes when struck shines real
light, and the lamp that hums emits real audio).

**Prior art** : separate render engines and audio engines as
the standard production architecture, with no cross-modal
unification at the substrate level. Lattice-Boltzmann methods
exist for fluid simulation, plasma simulation, and acoustic
simulation independently, but not for unified-substrate
spectral light + audio rendering.

**Inventive step** : the unification of light and audio into a
single ψ-field whose evolution is governed by a single LBM
update rule, with frequency-band-specific coupling coefficients
and a pure-eval rendering query that produces both spectral
radiance and spectral pressure from the same field state.

**Filing priority rationale** : This is the substrate-level
unification that underpins many of the other inventions (it is
the field over which the spectral renderer (Invention 4) and the
recursive witness (Invention 5) operate). It is also the most
distinctive of the six and the hardest to design around. File
this first.

**Approximate cost (US provisional)** : $400-700 USPTO fee +
$3,500-7,500 attorney prep = **$4,000-8,500 total**.

**Approximate cost (full utility, follow-up)** : $1,500-3,500
USPTO + $10,000-20,000 attorney = $12,000-25,000 (file 9-12
months after provisional).

### Invention 2 — KAN-Runtime Compute-Shader Evaluator

**Novel claim** : Kolmogorov-Arnold-Network forward-pass on the
GPU using cooperative-matrix instruction-set extensions where
available, with per-edge spline-coefficient quantization packed
into a per-asset embedding, persistent-kernel residency, and
subgroup-fused-multiply-add fallback elsewhere.

**Prior art** : MLP-shaders (per-fragment multilayer
perceptrons) ; neural-radiance-fields (offline ray-marched
networks) ; cooperative-matrix dispatch as a hardware feature
(BLAS-style matrix multiplication). KAN-on-GPU has been
discussed in academic papers post-Liu-2024 but no production
deployment is known to the inventor as of disclosure date.

**Inventive step** : the combination of KAN architectures,
quantized per-edge B-spline coefficients packed into per-asset
embeddings, persistent-kernel residency, cooperative-matrix
dispatch, and the subgroup-FMA fallback path that gives the
runtime broad consumer-GPU coverage, deployed at production
fidelity within real-time per-fragment budgets.

**Filing priority rationale** : This is the infrastructure
underpinning Inventions 3, 4, 5, 6 (all of which use KAN
networks). It is heavily defensive — competitors that wish to
ship KAN-shading must work around this filing. File second,
shortly after Invention 1.

**Approximate cost (US provisional)** : $400-700 USPTO fee +
$3,500-7,500 attorney prep = **$4,000-8,500 total**.

**Approximate cost (full utility, follow-up)** : $12,000-25,000.

### Invention 3 — Sub-Pixel Fractal-Tessellation Amplifier

**Novel claim** : a KAN-driven sub-pixel detail amplifier for
SDF-raymarching that produces continuous sub-pixel
micro-displacement, micro-roughness, and micro-color-perturbation
without level-of-detail discontinuities, bounded by a 5-level
recursive refinement and gated by foveation index.

**Prior art** : virtual-geometry techniques (Nanite in Unreal
Engine 5), tessellation shaders (DirectX 11+ era), procedural-
detail-via-noise techniques. None of these provide continuous
sub-pixel detail without LOD popping in real-time SDF
raymarching with KAN-driven micro-perturbation.

**Inventive step** : the combination of SDF raymarching, KAN-
driven sub-pixel perturbation, bounded recursive refinement, and
foveation-gated activation, producing infinite-perceptible-detail
without LOD popping within real-time per-fragment budgets.

**Filing priority rationale** : Strong novelty claim, directly
visible to viewers (sub-pixel detail is a clear differentiator),
high commercial-licensing potential to game studios. File third.

**Approximate cost (US provisional)** : $400-700 USPTO fee +
$3,500-7,500 attorney prep = **$4,000-8,500 total**.

**Approximate cost (full utility, follow-up)** : $12,000-25,000.

### Invention 4 — Hyperspectral KAN-BRDF

**Novel claim** : a per-pixel KAN-spline-network material
evaluator for 16-band spectral BRDF, with native iridescence
(thin-film interference), native dispersion (wavelength-
dependent index of refraction), and native fluorescence
(excitation-to-emission spectral remap), without mid-pipeline
RGB conversion, gated per-tile by Mantiuk-2024 contrast-
sensitivity-function.

**Prior art** : spectral renderers (Manuka, hero-wavelength
sampling per Wilkie 2014). Real-time spectral rendering on
consumer GPU hardware is essentially absent ; production engines
are RGB-throughout.

**Inventive step** : the combination of KAN-spline BRDF
evaluation, hero-wavelength MIS, native iridescence/dispersion/
fluorescence on embedding-gated paths, CSF-aware perceptual
gating, and PRIME-DIRECTIVE compliance, all within a 1.8 ms
Stage-6 budget.

**Filing priority rationale** : Strong novelty, high commercial
potential for product-visualization, color-critical-rendering,
medical-imaging applications. File fourth.

**Approximate cost (US provisional)** : $400-700 USPTO fee +
$3,500-7,500 attorney prep = **$4,000-8,500 total**.

**Approximate cost (full utility, follow-up)** : $12,000-25,000.

### Invention 5 — Mise-en-Abyme Recursive Witness

**Novel claim** : recursive frame-rendering with KAN-confidence
attenuation for soft early-termination, hard-capped at 5 levels
of recursion, with optional Companion-AI iris render-target
embedding for non-player-character reflective eyes, and per-
region anti-surveillance boundary enforcement.

**Prior art** : nested-mirror techniques in offline rendering
(arbitrary depth via Russian-roulette). Real-time recursive
mirror rendering is limited to 1-2 bounces in shipping production
engines.

**Inventive step** : the combination of bounded-hard-capped
recursion, KAN-confidence-driven early termination, Companion-
AI render-target embedding, per-region anti-surveillance
enforcement, all within a 0.8 ms / 0.6 ms Stage-9 budget.

**Filing priority rationale** : Strong defensive value, but
narrower applicability than Inventions 1-4 (mirrors are a
specific use case). The Companion-AI iris embedding is
artistically distinctive. File fifth.

**Approximate cost (US provisional)** : $400-700 USPTO fee +
$3,500-7,500 attorney prep = **$4,000-8,500 total**.

**Approximate cost (full utility, follow-up)** : $12,000-25,000.

### Invention 6 — Gaze-Reactive Observation-Collapse

**Novel claim** : detail-emergence conditioned on eye-tracking
gaze prediction, with world-state superposition collapse on
observation, compile-time-enforced biometric information-flow-
control (gaze data cannot egress the device by any code path),
saccade-prediction with latency-hiding under saccadic
suppression, and per-cell sovereignty-mask respect.

**Prior art** : foveated-rendering (Patney 2016), variable-
rate-shading. None of these adapt rendered content based on
gaze ; they only adapt rendering quality.

**Inventive step** : the combination of gaze-driven content
adaptation, observation-collapse evolver, compile-time biometric
IFC with no-override-by-privilege guarantee, saccade-prediction
with latency-hiding, and per-cell sovereignty respect.

**Filing priority rationale** : Highly distinctive (gaze-driven
content adaptation is essentially absent from production), has
strong privacy/regulatory positioning, but is narrower in
applicability (requires eye-tracking hardware). File sixth.

**Approximate cost (US provisional)** : $400-700 USPTO fee +
$3,500-7,500 attorney prep = **$4,000-8,500 total**.

**Approximate cost (full utility, follow-up)** : $12,000-25,000.

---

## Recommended Filing Order

Based on novelty strength, defensive scope, commercial-licensing
potential, and infrastructure-coupling, the recommended filing
order is :

1. **Invention 1 — ω-Field Unity Solver** (substrate, cross-stage,
   highest defensive scope) — **FILE FIRST**.
2. **Invention 2 — KAN-Runtime Compute-Shader Evaluator**
   (infrastructure, cross-stage, defensive against KAN-shading
   competitors) — **FILE SECOND**.
3. **Invention 3 — Sub-Pixel Fractal-Tessellation Amplifier**
   (Stage-5, visible to users, high licensing potential).
4. **Invention 4 — Hyperspectral KAN-BRDF** (Stage-6, color-
   critical applications).
5. **Invention 6 — Gaze-Reactive Observation-Collapse** (Stage-2,
   distinctive privacy positioning).
6. **Invention 5 — Mise-en-Abyme Recursive Witness** (Stage-9,
   narrower use case).

The first two inventions should be filed simultaneously or in
quick succession (within 1-2 weeks of each other). The remaining
four can be filed over the following 1-3 months as funding and
attorney bandwidth permit.

---

## Aggregate Cost Estimate

### Provisional-Only (Year 1)

- 6 provisionals × $4,000-8,500 each = **$24,000 - $51,000**
- Aggregate filing fees alone : ~$2,400-4,200
- Aggregate attorney-prep fees : ~$21,000-45,000

### Full Utility Follow-Up (Years 1-2)

If all six provisionals are converted to full utility patents
within the 12-month grace period :

- 6 utilities × $12,000-25,000 each = **$72,000 - $150,000**
- Aggregate USPTO fees : ~$9,000-21,000
- Aggregate attorney-prep fees : ~$60,000-120,000
- Maintenance fees over patent lifetime (20 years) :
  ~$10,000-20,000 per patent = $60,000-120,000 aggregate

### Strategic Cost-Reduction Options

1. **Stage filings** : file Inventions 1, 2, 3 as US provisionals
   first (~$12,000-25,500). Watch for prior-art that emerges in
   the next 12 months. Convert to full utility only those that
   remain novel and commercially relevant. Defer Inventions 4,
   5, 6 to Year 2.

2. **PCT route** : file Inventions 1, 2, 3 as PCT applications
   from the outset, claiming priority from the US provisionals.
   This gives international optionality at an additional ~$5,000
   per application.

3. **Defensive publication** : for Inventions 5 and 6 (lower-
   priority), consider defensive publication in a journal or
   IP.com to establish prior art that prevents others from
   patenting the technology, without paying the full filing
   cost. This forfeits patent rights but prevents
   competitor-filing. Cost : ~$200-500 per defensive publication.

4. **Corporate-backed assignment** : if a corporate partner
   licenses or acquires the technology, the partner may absorb
   all filing costs in exchange for an assignment or exclusive
   license. This is the lowest-out-of-pocket path but requires
   an early commercial deal.

---

## Attorney Search Recommendations

### Selection Criteria

- **US patent bar registration** : required for filing.
- **Computer-graphics or computer-science specialization** : the
  technical content is unusually deep ; an attorney without
  strong CS background will struggle to draft claims that read
  on the actual implementations.
- **Real-time-rendering or VR/AR experience** : preferred ; the
  field-specific prior-art understanding is load-bearing.
- **Prior provisional-to-utility conversion track record** : the
  attorney should be able to convert efficiently within the
  12-month grace period.
- **Reasonable hourly or flat-fee structure** : aim for $400-
  $700 per hour or flat $3,500-7,500 per provisional.

### Search Resources

1. **AIPLA member directory** (American Intellectual Property
   Law Association) : searchable by technology area.
2. **USPTO patent attorney roster** : official registration list.
3. **LinkedIn searches** for "patent attorney + computer graphics"
   in the inventor's region.
4. **State bar associations** for jurisdiction-specific
   referrals.
5. **Direct references** : ask other inventors in the
   real-time-graphics space (game-studio CTOs, GPU-vendor IP
   counsel) for referrals.

### Recommended Pre-Engagement Preparation

Before contacting attorneys, the inventor should :

1. Have all six disclosure documents reviewed for typographical
   errors and re-named as "Invention Disclosure - [Inventor
   Legal Name] - [Title]" with the legal name inserted.
2. Compile a list of all CSSLv3 git commit hashes that establish
   reduction-to-practice dates.
3. Compile a list of all prior public disclosures (CSSLv3 GitHub
   pushes, Discord posts, Twitter/X posts, blog posts, video-
   stream sessions) with dates, to establish whether any
   inadvertent public disclosure has occurred and to evaluate
   the 12-month US grace period accordingly.
4. Decide on assignee structure (individual inventor vs. an LLC
   or corporation owned by the inventor) ; this affects the
   filing paperwork and the long-term ownership structure.

### Initial Engagement Recommendations

- **First call** : 30-minute free consultation, present the
  index document only (not the full disclosures, until an
  engagement letter is signed).
- **Engagement letter** : flat-fee per provisional, retainer-
  based for utility conversions, with caps on total cost.
- **NDA before disclosure** : even though the inventor-attorney
  relationship has built-in privilege, a written NDA is
  recommended for the inventor's records.
- **Scope** : engage initially for Inventions 1 and 2 only.
  Evaluate the attorney's quality and responsiveness on those
  before engaging for the remaining four.

---

## Strategic Considerations

### Patent vs. Trade-Secret Trade-Off

For each invention, the inventor has the option of :

- **Patenting** : public disclosure in exchange for a 20-year
  exclusive right.
- **Trade-secret** : keep the technology private indefinitely,
  with no formal protection (vulnerable to reverse-engineering
  and to independent re-invention).

The trade-off depends on :

- whether the technology is observable from the runtime output
  (in which case trade-secret is hard to maintain),
- whether the technology can be reverse-engineered from the
  runtime binary (in which case trade-secret is hard to
  maintain),
- whether the inventor intends to license the technology
  commercially (in which case patent is preferable),
- whether the inventor intends to use the technology defensively
  (in which case patent is preferable).

For the present six inventions, the inventor's intent is mixed :

- Inventions 1, 2 are foundational substrate / infrastructure ;
  the inventor likely wishes to either license commercially or
  retain defensively. **Patent.**
- Inventions 3, 4 are visible to users (sub-pixel detail and
  spectral color). Reverse-engineering from runtime is feasible.
  **Patent.**
- Inventions 5, 6 are partially observable (a viewer can see
  recursive mirrors and gaze-driven content adaptation but not
  the implementation details). **Patent ; defensive publication
  is also acceptable for Invention 5.**

### Open-Source-Compatibility

CSSLv3 is intended to be open-sourced eventually. Patent rights
do not preclude open-sourcing the implementation under any
license (the inventor can license the code under MIT/Apache-2 /
GPL while retaining the patent rights). Many large open-source
projects (Linux kernel, LLVM, Apache projects) are subject to
patents owned by their contributors, with patent grants embedded
in the contribution license (e.g., the Apache-2 patent grant).

If the inventor open-sources CSSLv3 under Apache-2 or a similar
license, the contributors (including the inventor) grant a
royalty-free patent license to all users, but retain ownership
of the patent and can pursue infringement against parties that
do not adopt the open-source license. This is a common pattern.

### International Filings

US provisional applications do not automatically extend to
foreign jurisdictions. Foreign rights must be filed separately,
either via :

- direct national filings in target countries (expensive ; one
  filing per country),
- a PCT application (one filing, claims-based; preserves the
  right to enter foreign national phase later, at additional
  cost).

The PCT route is the standard for inventors seeking
international protection. Total PCT cost : ~$5,000-10,000 per
application above the US-provisional cost. National-phase entry
adds another $3,000-15,000 per country.

For the present six inventions, the inventor's commercial intent
suggests : file PCT for at least Inventions 1, 2, 3, 4 ; defer
the foreign-national-phase decision to Year 2 based on
commercial traction.

### Defensive vs. Offensive Use

Patents have two primary uses :

- **Defensive** : the inventor uses the patent to prevent others
  from filing patents that would block the inventor's own use
  of the technology.
- **Offensive** : the inventor uses the patent to extract
  licensing revenue from infringers, or to seek injunctions.

The CSSLv3 portfolio is well-suited to defensive use (the
patents protect the inventor's freedom to ship). Offensive use
(litigation against infringers) is expensive ($1M-$10M per
litigation campaign) and is generally only economically viable
against large infringers in deep-pocket markets.

The recommended posture is **defensive**, with offensive
optionality preserved by the patent grants.

---

## Action Items for Apocky

### Immediate (Next 1-2 Weeks)

- [ ] Review all six disclosure documents in this directory
  for technical accuracy. Note any inaccuracies or omissions
  in `_drafts/legal/notes_inventor_review.md` (a private file).
- [ ] Replace all "Apocky <apocky13@gmail.com> *(legal-name
  placeholder)*" entries in §1 of each disclosure with the
  filing-jurisdiction-correct legal name.
- [ ] Decide on assignee structure (individual vs. LLC vs.
  corporation).
- [ ] Search for and shortlist 3-5 patent attorneys per the
  Attorney Search Recommendations section. Schedule
  30-minute free consultations.

### Short-Term (Next 1-3 Months)

- [ ] Engage a patent attorney for Inventions 1 and 2.
  File US provisional applications.
- [ ] Confirm reduction-to-practice dates from CSSLv3 git history.
- [ ] Compile a list of any prior public disclosures of the
  technology (GitHub pushes, social media, video streams) for
  attorney review.
- [ ] Decide on PCT route for Inventions 1 and 2.
- [ ] Decide on Year-1 budget allocation (provisional-only vs.
  provisional + PCT vs. provisional + utility).

### Medium-Term (Next 3-12 Months)

- [ ] File US provisionals for Inventions 3, 4, 5, 6 in priority
  order.
- [ ] Decide on PCT route for each (case-by-case).
- [ ] Engage in commercial conversations with potential licensees
  (game studios, head-mounted-display OEMs, middleware vendors).
- [ ] Track any prior-art that emerges and evaluate impact on
  filing strategy.
- [ ] Prepare for utility-conversion deadlines (12 months from
  each provisional filing date).

### Long-Term (Year 2+)

- [ ] Convert provisionals to utilities for those that remain
  commercially relevant.
- [ ] Consider PCT national-phase entry for international
  protection in target markets.
- [ ] Maintain patent maintenance-fee schedule (3.5, 7.5, 11.5
  years post-issue for US utilities).
- [ ] Evaluate licensing or assignment opportunities.

---

## Confidentiality

THIS ENTIRE DIRECTORY IS PRIVATE. NOT FOR PUBLIC DISCLOSURE.

Patent novelty law allows the inventor a one-year grace period
after public disclosure in the United States ; outside the United
States, premature public disclosure is generally fatal to
patentability. Accordingly, this entire `_drafts/legal/`
directory MUST NOT be shared, posted, committed to a public
repository, distributed, or otherwise made publicly accessible
until either (a) provisional patent applications have been filed
claiming the inventions disclosed herein as priority dates, or
(b) the inventor and patent counsel have agreed in writing that
public disclosure is permissible.

Distribution of these documents is limited to :
- the inventor (Apocky / legal-name-of-record),
- patent counsel of record,
- co-inventors and assignees with a written non-disclosure agreement,
- such persons as are necessary for due-diligence in connection with
  filing, assignment, or licensing of the rights herein.

The act of authoring these documents into the `_drafts/legal/`
private directory of the CSSLv3 repository, which is not pushed
to any public remote, is itself a confidentiality-preserving
act. The `_drafts/legal/` directory is intended to be either
gitignored or maintained as a local-only working directory
without remote-tracking. The inventor should verify, before any
push to a remote, that `.gitignore` excludes `_drafts/legal/`
or that no commit including this directory has been pushed.

---

## Document Provenance

This index document and the six accompanying disclosures were
authored as part of CSSLv3 slice T11-D231 under the parallel-
fanout substrate-evolution wave (Session 11-12 transition). They
are the inventor's working draft for attorney handoff. They are
not legal advice. Any patent strategy decision should be made
in consultation with qualified patent counsel.

The disclosures reference the CSSLv3 reference implementation,
which lives in the same repository under
`compiler-rs/crates/...`. The reference implementation is itself
the reduction-to-practice of the inventions ; the git history
of those crates establishes the conception and reduction-to-
practice dates.

---

**End of Invention Disclosures Index.**
