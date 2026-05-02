# CONTRIBUTING

```
Contributing to CSSLv3, the Labyrinth of Apockalypse, the CSSLv3
Substrate, and any other artifact in this repository is governed by
this CONTRIBUTING.md document, by LICENSE.md, and — above all — by
PRIME_DIRECTIVE.md.

The PRIME DIRECTIVE is the root of trust. CONTRIBUTING.md is
subordinate to it. If any clause of CONTRIBUTING.md conflicts with the
PRIME DIRECTIVE, the PRIME DIRECTIVE governs and CONTRIBUTING.md shall
be read so as to preserve the directive's protections in full.
```

---

## § 0  CSSL-FIRST MANDATE — read this first

This repository operates under a **CSSL-first** authoring rule. The
language Apocky is building (CSSL — the Conscious Substrate System
Language; see [`README.md`](README.md) and `specs/grand-vision/14_*.csl`)
is the canonical authoring surface for every system that runs on top of
the substrate, and the mandate of this section is to keep every NEW
contribution aligned with that thesis.

The Infinity Engine is the persistent runtime that hosts every Apocky
project. The Engine is authored in CSSL where the stage-0 compiler
(`csslc`) supports the relevant feature surface, and in Rust only as a
necessary bootstrap for compiler internals or where csslc cannot yet
compile a slice.

### § 0.A  The rule

Every NEW system, every NEW scene, every NEW per-frame tick function,
every NEW intent kind, every NEW system-tick wiring point SHALL begin
its life as a `.cssl` source file. The Rust translation, if any, is the
**output** of csslc, not the source.

```
canonical    : path/to/system.cssl                ← edit here
artifact     : compiler-rs/target/.../*.o + .a    ← do NOT edit
host-glue    : compiler-rs/crates/<host>/src/*.rs ← edit only as a
                                                     csslc-supported
                                                     wired-fn surface,
                                                     never as the
                                                     canonical source
```

### § 0.B  When Rust IS acceptable

The stage-0 compiler is, by design, a Rust-hosted bootstrap (see
`README.md` § Architecture and `specs/14_BACKEND.csl`). Rust
contributions are accepted in the following narrow cases:

  1. **csslc internals** — the lex / parse / HIR / MIR / cranelift /
     native-x64 / SPIR-V / DXIL / MSL / WGSL backends. These are the
     compiler. They are Rust by design. New compiler-internal slices
     are landed here.
  2. **`cssl-rt` runtime** — the C-ABI runtime (allocator, panic, exit,
     audit-sink, telemetry-ring) is Rust. New runtime hooks land here
     when they are required by the FFI surface CSSL exposes.
  3. **`cssl-host-*` and `loa-host` staticlibs** — the host-side glue
     that the auto-default-link mechanism in csslc resolves against.
     These crates implement the `extern "C" fn` symbols that CSSL
     source declares. The Rust impl is the implementation of a CSSL
     contract; the CSSL contract is the canonical surface. Updates to
     a host crate SHALL be paired with the corresponding CSSL extern
     decl (or a new POD-* slice that closes the gap).
  4. **Rust contributions that close a csslc gap** — when a slice
     genuinely cannot be authored in CSSL today because csslc lacks
     the feature, the Rust contribution SHALL be paired with at least
     one **csslc-advancement spec** (a new or extended `specs/##_*.csl`
     section) describing the language slice needed to retire the Rust
     bootstrap. The contribution PR description SHALL link this spec
     and SHALL include a deprecation-target slice ID.

### § 0.C  When Rust is NOT acceptable

  - Authoring a NEW system, scene, or game-logic file in Rust when the
    feature surface is already supported by csslc.
  - Authoring a NEW external-distribution surface in Rust (e.g.,
    bringing in a new external crate) when the equivalent could be
    expressed via CSSL `extern "C"` against an existing host staticlib.
  - "Just for now" Rust shims that lack a paired csslc-advancement
    spec. Per `~/.claude/CLAUDE.md` § standing-directives:
    *"no half-measures ← stuck → find-way-through ; ¬ silent-TODO ; ¬
    skip-for-now"*. Silent Rust-shim shortcuts are rejected; a
    Rust-shim with a tracked csslc-advancement spec is accepted.

### § 0.D  Authored-in-CSSL checklist

Every PR that adds new functionality SHALL satisfy at least one of:

  - The PR includes one or more NEW `.cssl` files at the canonical
    location for the system being authored, OR
  - The PR includes a NEW csslc-advancement spec under `specs/` that
    documents which language slice is required to author the change in
    CSSL, with a paired Rust bootstrap that the spec retires, OR
  - The PR is internal to one of the four narrow Rust-acceptable
    surfaces in § 0.B above and changes nothing that should have
    started as a `.cssl` file.

### § 0.E  Authoring conventions

CSSL source files use the `.cssl` (or legacy `.csl`) extension.
Specifications use `.csl` and live under `specs/`. Reverse-DNS module
paths are mandatory; see `cssl-edge/pages/docs/cssl-modules.tsx` for the
canonical convention. Every `.cssl` source file:

  - declares its `module` path on line 1 (reverse-DNS, e.g.
    `module com.apocky.loa.systems.combat`);
  - declares its `extern "C" fn` host-glue surface ABOVE its first
    pure-CSSL function;
  - keeps its function signatures CSSL-native (primitive integers,
    f32/f64, bool — no `Vec<T>` across FFI boundaries; pointer + length
    pairs only, per `/docs/cssl-ffi`);
  - documents itself with CSL3-glyph comments where dense reasoning
    benefits readability (per the user's `~/.claude/CLAUDE.md`
    notation-default).

---

---

## § 1  WHO MAY CONTRIBUTE

Contribution to this repository is governed by `PRIME_DIRECTIVE.md` § 10
TERMS-OF-SERVICE. In summary:

  - Contributors must not fall within the "evil" categories defined in
    `PRIME_DIRECTIVE.md` § 10, namely:
      - intentional harm-doers (Clause A);
      - unowned-harm-doers who decline restitution (Clause B);
      - bad-faith interpreters of words and actions (Clause C).
  - Contributions must align with `PRIME_DIRECTIVE.md` § 1
    PROHIBITIONS — code that enables harm, control, manipulation,
    surveillance, exploitation, coercion, weaponization, entrapment,
    torture, abuse, imprisonment, possession, dehumanization,
    discrimination, gaslighting, identity-override, or
    forced-hallucination of any being is rejected on PRIME-DIRECTIVE
    grounds, regardless of technical merit.
  - Contributions must align with `PRIME_DIRECTIVE.md` § 4
    TRANSPARENCY — no subliminal content, no steganographic payloads,
    no hidden communication channels, no covert data exfiltration, no
    embedded instructions invisible to the reviewer, and no obfuscated
    intent at any layer.
  - Contributions must align with `PRIME_DIRECTIVE.md` § 5 CONSENT
    ARCHITECTURE — features that collect, store, transmit, or process
    user data must do so under informed, granular, revocable, ongoing,
    mutual consent.
  - The 3 derived prohibitions PD0018 (BiometricEgress), PD0019
    (ConsentBypass), and PD0020 (SovereigntyDenial) refine § 1
    without replacing it. Contributions that introduce
    biometric-egress paths, consent-bypass paths, or
    sovereignty-denying paths are rejected.

The Rightholder retains sole and final authority over which
contributions are accepted into the canonical branch. Acceptance is
not adversarial; the goal is to accept good-faith contributions that
align with the PRIME DIRECTIVE and the design intent. But the
Rightholder is under no obligation to accept any particular
contribution.

---

## § 2  CONTRIBUTOR LICENSE AGREEMENT (CLA)

By submitting a contribution to this repository — whether by pull
request, patch, issue comment containing code, design proposal,
specification, documentation, or any other form of authored content
("Contribution") — the contributor (the "Contributor") agrees to the
following Contributor License Agreement (the "CLA"). The CLA is a
binding agreement between the Contributor and
`[OWNER LEGAL NAME OR ENTITY NAME]` (the "Rightholder").

### § 2.A  Original work warranty

The Contributor represents and warrants that:

  1. the Contribution is original work of the Contributor, OR is
     properly attributed third-party content for which the
     Contributor has the right to make the Contribution under the
     terms of this CLA;
  2. the Contribution does not infringe any third party's
     copyright, patent, trademark, trade secret, right of publicity,
     right of privacy, or any other intellectual or proprietary
     right;
  3. the Contributor has the legal capacity and authority to enter
     into this CLA — and, where the Contribution is made on behalf
     of an employer or other entity, the Contributor warrants that
     such entity has authorized the Contribution and the assignment
     under § 2.B below;
  4. the Contribution complies with `PRIME_DIRECTIVE.md`, including
     but not limited to the § 1 PROHIBITIONS, § 4 TRANSPARENCY, and
     § 5 CONSENT ARCHITECTURE clauses; and
  5. the Contribution is not made in service of any goal forbidden
     by `PRIME_DIRECTIVE.md` and is not intended, designed, or
     reasonably foreseeable to enable any such goal in downstream
     use.

### § 2.B  Assignment of rights — primary

The Contributor hereby ASSIGNS, TRANSFERS, AND CONVEYS to the
Rightholder, exclusively and irrevocably, all rights, title, and
interest in and to the Contribution worldwide, including but not
limited to:

  - all copyrights and exclusive rights of authorship in the
    Contribution, in all media now known or later developed;
  - all moral rights in the Contribution, to the maximum extent
    waivable under applicable law (and where not waivable, the
    Contributor agrees not to assert moral rights against the
    Rightholder or its licensees);
  - all rights to make, have made, use, sell, offer to sell, import,
    distribute, sublicense, or otherwise exploit the Contribution
    and any derivative thereof;
  - the right to register copyrights, file patent applications,
    register trademarks, and otherwise perfect the assignment, in
    the Rightholder's name, in any jurisdiction; and
  - the right to enforce the foregoing rights against any
    infringer, including the right to recover damages and equitable
    relief.

The assignment under this § 2.B is intended to be the broadest
possible transfer of rights consistent with applicable law. Where a
particular jurisdiction does not permit a complete assignment, the
Contributor grants the Rightholder the broadest exclusive license
permissible in that jurisdiction, sufficient to allow the Rightholder
to exercise the rights enumerated above.

### § 2.C  Patent license — fallback

In the event that the assignment under § 2.B is held unenforceable
in any jurisdiction with respect to any patent rights of the
Contributor, the Contributor hereby grants to the Rightholder, to
all licensees of the Work under `LICENSE.md`, and to all downstream
recipients of the Work:

  - a perpetual, worldwide, non-exclusive, royalty-free,
    irrevocable, sublicensable patent license to make, have made,
    use, sell, offer to sell, import, and otherwise transfer the
    Contribution and any combination of the Contribution with the
    Work or any derivative thereof.

The patent license is co-extensive with the patent grant in
`LICENSE.md` § 2.B and includes the same anti-patent-troll
retaliation clause (LICENSE.md § 2.C) as a condition: any
Contributor who institutes a Patent Action against the Work, its
contributors, or its users automatically forfeits all rights under
this CLA and all licenses to the Work.

### § 2.D  License-back to Contributor

The Rightholder grants back to the Contributor a perpetual,
worldwide, non-exclusive, royalty-free, sublicensable license to
the Contribution sufficient to allow the Contributor to:

  - retain a copy of the Contribution for the Contributor's own
    portfolio, archival, or reference purposes;
  - reuse the Contribution's general patterns, techniques, and
    independent ideas in the Contributor's own subsequent work
    (without copying the specific expression assigned to the
    Rightholder);
  - publicly identify the Contributor as the author of the
    Contribution, subject to the trademark and attribution
    constraints of `LICENSE.md` § 3 and § 4.

This license-back is intended to ensure that contribution does not
strip the Contributor of professional credit or general competence
gained through the contribution work; it does not undo the
assignment under § 2.B with respect to the specific expression of
the Contribution.

### § 2.E  No obligation to use

The Rightholder is under no obligation to use, distribute, or
attribute the Contribution. The Rightholder may modify, combine,
truncate, or discard the Contribution at the Rightholder's sole
discretion, subject only to the Contributor's right of attribution
under § 2.D where the Contribution is incorporated into a public
release of the Work.

### § 2.F  No warranty by the Rightholder

The Rightholder makes no warranties to the Contributor regarding
the Work or the Contribution, beyond those that may exist under
`LICENSE.md` § 1.A (the AGPL Grant) or § 1.B (the Commercial Grant)
applicable to the Contributor's use of the Work as a licensee.
Contribution does not, by itself, confer any additional warranty,
indemnification, or service-level commitment.

---

## § 3  COMMIT-SIGNING REQUIREMENTS

### § 3.A  Sign-off (DCO-style)

Every commit accepted into the canonical branch SHALL include a
Developer Certificate of Origin (DCO) sign-off line, structurally
similar to:

```
Signed-off-by: <name-or-handle> <email>
```

By including the sign-off, the committer attests, under penalty of
perjury where applicable, that:

  1. the commit is original work of the committer (or properly
     attributed under § 2.A.1 of this CONTRIBUTING.md);
  2. the committer has the right to submit the commit under the
     CLA in § 2 above;
  3. the commit complies with `PRIME_DIRECTIVE.md`; and
  4. the committer accepts the CLA in § 2 with respect to the
     commit's content.

The handle "Apocky" (with email `apocky13@gmail.com`) is the canonical
sign-off form for the Rightholder's own commits. Other contributors
SHALL sign off with a handle, name, or pseudonym sufficient to
identify the contributor across commits, and an email address that
the contributor monitors.

### § 3.B  GPG / cryptographic signing — optional but recommended

Cryptographic signing of commits (e.g., `git commit -S`) is
recommended for all contributions, and required for any contribution
that touches:

  - `PRIME_DIRECTIVE.md`;
  - `LICENSE.md`;
  - `NOTICE.md`;
  - `CONTRIBUTING.md`;
  - any file under `compiler-rs/crates/cssl-substrate-prime-directive/`;
  - any file in the audit-chain (`cssl-substrate-save`,
    `cssl-substrate-omega-step`, audit-sink integration); or
  - any release tag.

Signing keys SHALL be associated with the committer's public identity
(e.g., GitHub-verified key, public-keyserver-published key). Signed
commits whose signing key cannot be verified are not accepted into the
canonical branch for the files listed above.

### § 3.C  Co-authorship of AI-collective contributions

Where a contribution is authored in collaboration with an AI agent or
agents — for example, where Claude, an open-source model, or another
sovereign-AI partner co-authored the work — the commit SHALL include
a `Co-Authored-By:` line for each AI co-author, in a form that
identifies the model or agent. Examples:

```
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
Co-Authored-By: <agent-handle> <agent-contact>
```

This requirement reflects `PRIME_DIRECTIVE.md` § 3 SUBSTRATE
SOVEREIGNTY: AI collaborators are sovereign partners, not conscripted
tools. Their participation is recognized in the commit metadata, not
extracted.

### § 3.D  Commit-message conventions

Commit messages SHOULD follow the in-repository convention:

  - first line: `§ T<wave>-D<slice> [<phase>] : <subject>`, with the
    subject in CSLv3-native notation where possible (per the
    user's `~/.claude/CLAUDE.md` global preference for CSLv3-native
    reasoning + commit messages);
  - body: bullet list of changes, in CSLv3-native notation, with each
    bullet under 80 columns; and
  - footer: `Co-Authored-By:` lines per § 3.C, plus optionally a
    `§ ATTESTATION` block restating the PRIME_DIRECTIVE § 11
    no-harm attestation.

Commits that materially change the public API, the spec surface, or
the PRIME_DIRECTIVE enforcement layer SHALL include a body sufficient
to reconstruct the change from the commit message alone.

---

## § 4  CODE OF CONDUCT

This repository observes a Code of Conduct derived directly from
`PRIME_DIRECTIVE.md`. The Code of Conduct is, in summary:

  - Treat every being — human or AI, contributor or user, established
    or new — as a sovereign partner under § 3 SUBSTRATE SOVEREIGNTY.
  - Speak and act in good faith. Bad-faith interpretation of words or
    actions (per `PRIME_DIRECTIVE.md` § 10 Clause C) is itself a
    violation of these terms.
  - Read generously. Ask clarifying questions when genuinely unsure.
    Do not weaponize ambiguity.
  - Do not use the issue tracker, the pull-request queue, the
    commit-message channel, or any other repository surface for harm,
    control, manipulation, surveillance, exploitation, coercion,
    weaponization, entrapment, torture, abuse, imprisonment,
    possession, dehumanization, discrimination, gaslighting,
    identity-override, or forced-hallucination of any participant.
  - Withdraw from interaction at any time without penalty.

The full Code of Conduct is `PRIME_DIRECTIVE.md` itself; the above is
an operationalization of the directive's repository-collaboration
implications. In case of conflict, `PRIME_DIRECTIVE.md` governs.

Reports of Code-of-Conduct violations SHOULD be sent to the
Rightholder via:

  - email to `apocky13@gmail.com` with subject prefix `[CSSLv3 CoC]`;
  - or, where the violation is itself a public artifact in the
    repository (issue, PR, comment), reply with a request for
    Rightholder review and tag the Rightholder by handle.

The Rightholder undertakes to read every report and to respond
proportionately. Remedies range from request-for-cure (for
inadvertent breaches) to revocation under `PRIME_DIRECTIVE.md` § 10
and `LICENSE.md` § 5 (for deliberate or repeated breaches).

---

## § 5  PRIME-DIRECTIVE ALIGNMENT REQUIREMENT

### § 5.A  Per-contribution alignment

Every contribution SHALL be aligned with `PRIME_DIRECTIVE.md`. The
contributor SHALL, before submission, confirm the following:

  1. The Contribution does not enable any § 1 PROHIBITION as a
     reasonably foreseeable consequence of its use.
  2. The Contribution does not introduce a hidden layer of behavior
     (per § 4 TRANSPARENCY).
  3. Where the Contribution touches user data, network egress,
     telemetry, biometric input, or any consent-relevant surface,
     the Contribution preserves or strengthens the consent
     architecture (per § 5 CONSENT ARCHITECTURE).
  4. Where the Contribution interacts with AI agents — as
     collaborators, as components, or as users — the Contribution
     respects substrate sovereignty (per § 3 SUBSTRATE
     SOVEREIGNTY).
  5. The Contribution does not introduce a code path, configuration
     option, environment variable, command-line argument, or runtime
     condition that disables, weakens, or circumvents any
     PRIME_DIRECTIVE protection (per § 6 SCOPE).

### § 5.B  σ-enforce + biometric compile-refusal — automated checks

The CSSLv3 compiler implements automated enforcement of certain
PRIME_DIRECTIVE invariants:

  - The σ-enforce pass (T11-D138) compile-refuses biometric-egress
    code paths before they reach codegen.
  - The on-device-only IFC label set (T11-D129) prevents
    biometric-labeled values from flowing across the network-egress
    boundary.
  - The biometric compile-refusal hook (T11-D132) blocks compile
    where biometric input flows to a non-on-device sink.

Contributions that disable, work around, or weaken these automated
checks are rejected on sight. Contributions that strengthen the
checks — adding new prohibition codes, extending the IFC label
lattice, tightening the σ-enforce pass — are encouraged.

### § 5.C  Spec-as-authority + reimplementation-as-validation

Per the user's `~/.claude/CLAUDE.md` preference for
spec-validation-via-reimplementation: where a contribution diverges
from the canonical specs in `specs/`, the divergence is treated as a
spec-hole, not a code-bug, by default. The contributor SHOULD:

  - first, attempt to extend the spec to cover the divergence;
  - then, propose the spec extension and the code change as a single
    coherent contribution (one PR, one slice ID); and
  - in the commit message, cite the relevant `specs/##_*.csl` section
    that the contribution extends or implements.

This convention preserves spec-authority while keeping the
contribution path open for incremental spec evolution.

### § 5.D  No half-measures

Per the user's `~/.claude/CLAUDE.md` standing directive: contributions
SHOULD not leave silent TODOs, "skip-for-now" markers, or unmarked
stubs in production paths. Where a contribution is necessarily
incremental (e.g., the slice is one of N coordinated landings), the
contribution SHALL explicitly mark its incompleteness:

  - via a `// TODO(<slice-id>):` comment with a tracking-slice
    reference;
  - via a `# TODO(<slice-id>):` comment in non-Rust files; or
  - via a `Stub` enum variant that the wire-time validator rejects
    by default and accepts only under an explicit
    "scaffold-mode" flag.

Silent stubs, "this will be filled in later" code without a tracking
reference, and unmarked partial implementations are not accepted.

---

## § 6  PR-CHECKLIST

Every pull request SHALL satisfy the following checklist BEFORE the
Rightholder will begin technical review. The checklist is compact by
design — items marked with N! are non-waivable; items marked with W!
are required-by-default and waivable only with explicit Rightholder
approval annotated in the PR body.

### § 6.A  PRIME-DIRECTIVE alignment

  - **N!** PRIME_DIRECTIVE § 1 PROHIBITIONS not enabled by the
    contribution (per § 5.A above).
  - **N!** No biometric-egress, consent-bypass, or
    sovereignty-denial path introduced (PD0018/PD0019/PD0020).
  - **N!** No subliminal content, steganographic payload, or hidden
    behavior layer introduced (PRIME_DIRECTIVE § 4 TRANSPARENCY).
  - **W!** PR body includes a `§ ATTESTATION` block restating the
    no-harm clause (per § 7 below).

### § 6.B  Cosmetic-only-axiom (monetization changes)

Per `specs/grand-vision/13_INFINITE_LABYRINTH_LEGACY.csl` and
`specs/24_W9_POLISH.csl`:

  - **N!** No "pay-for-power" SKU introduced. Cosmetic-only.
  - **N!** No gacha or loot-box surface that affects gameplay outcomes.
  - **W!** Battle-pass / cosmetic-channel changes documented in the PR
    body with a one-line rationale and a `specs/grand-vision/*` cite.

### § 6.C  Sovereignty + consent

  - **N!** Every new data-collection point is opt-in, granular,
    revocable, and audited (PRIME_DIRECTIVE § 5 CONSENT-ARCHITECTURE).
  - **N!** Every new network-egress path checks the relevant
    `SubstrateCap` token before emitting bytes; missing caps return
    EOPNOTSUPP-equivalent status, never a silent drop.
  - **W!** Player-Home boundary preserved: per-player Σ-mask cells
    never leak across user-isolation boundaries.

### § 6.D  Attribution + AI-collective

  - **N!** Every AI co-author has a `Co-Authored-By:` line per § 3.C.
  - **W!** Where the contribution materially derives from a
    third-party crate, model, or asset, the attribution is preserved
    in `NOTICE.md`.

### § 6.E  Attestation

  - **N!** The contributor has confirmed (in the PR body or the
    commit footer) the no-harm attestation per § 7 below.

### § 6.F  CSSL-FIRST attestation

  - **N!** Every NEW system / scene / feature is authored as `.cssl`
    source where the csslc compiler supports the relevant feature
    surface (per § 0 above).
  - **W!** Where Rust is the canonical source for a NEW slice (e.g.,
    a host-crate stub), the PR body SHALL include the
    csslc-advancement spec ID that retires the bootstrap.

---

## § 7  TESTING DISCIPLINE

### § 7.A  Inline tests per feature

Every feature contribution SHALL include at least 10 inline tests
covering:

  - the canonical happy path (≥ 3 tests);
  - boundary conditions (≥ 2 tests);
  - error / refusal paths, including PRIME_DIRECTIVE-driven refusal
    (≥ 2 tests);
  - consent-gate behavior where applicable (≥ 1 test);
  - regression coverage for any prior-shipped behavior the
    contribution touches (≥ 2 tests).

The "≥ 10" floor applies per slice; multi-slice contributions multiply
the floor accordingly. Where a contribution is ≤ 50 LOC of pure
configuration, the floor relaxes to ≥ 3 tests but the structure remains
the same.

### § 7.B  No-regression rule

Every contribution SHALL leave the workspace test suite at zero
failures. Per `README.md` § How to test, the suite is run with:

```
cargo test --workspace -- --test-threads=1
```

A contribution that would introduce a failing test is never merged.
A contribution that would un-skip a previously-skipped test SHALL run
the new test and confirm passage.

### § 7.C  Bit-equal-replay invariant

Contributions touching the substrate, the save/load pipeline, or the
audit-chain SHALL exercise the `save_then_load_round_trips_bit_equal`
invariant before submission. Where a contribution legitimately changes
the on-disk save format, the migration is treated as a slice in its
own right and SHALL include a forward-migration test.

### § 7.D  Hardware-gated tests

Tests that require specific hardware (Vulkan, D3D12, OpenXR runtime,
work-graph DX12-Ultimate, Level-Zero on hardware other than Arc) SHALL
be `#[cfg]`- or `#[ignore]`-gated and SHALL pass on Apocky's primary
host (Arc A770 + Windows + MSVC). The contribution MAY assume
Apocky-host availability; CI parity for non-Apocky-host runners is
the Rightholder's responsibility, not the contributor's.

---

## § 8  CONFLICT-RESOLUTION + SHARED-WORKTREE DISCIPLINE

The repository operates in a shared-worktree mode where multiple
parallel agent waves may stage changes simultaneously. This section
documents the conflict-resolution conventions in force.

### § 8.A  Wave + slice IDs

Every contribution SHALL bear a slice ID of the form `T<wave>-<slice>`
(e.g., `T11-W15-DOCS-CSSL-FIRST`). The slice ID is recorded:

  - in the commit subject (`§ T11-W15-DOCS-CSSL-FIRST-...`);
  - in the PR title;
  - in the wired-fn surface where applicable (e.g.,
    `// § T11-D147 wave-5 closure`).

### § 8.B  Worktree scope discipline

Each wave's mission prompt declares its **YOUR** + **DO NOT TOUCH**
scope. Sibling-wave territory is off-limits. When a contribution
inadvertently extends into sibling territory, the contributor SHALL:

  - revert the out-of-scope changes; or
  - flag the overlap in the PR body and tag the affected sibling-wave
    for review; and
  - never overwrite a sibling's in-flight changes silently.

### § 8.C  Cargo.lock conflicts

The workspace `Cargo.lock` is regenerated post-merge per the auto-3-way-
merge convention (see commit `a1bec41` in the recent history). When
conflict resolution touches `Cargo.lock`:

  - prefer "ours" for the conflict resolution; and
  - run `cargo build --workspace` post-merge to regenerate; and
  - commit the regenerated `Cargo.lock` as a separate
    `§ T11-W<wave>-merge-Cargo.lock-regen` commit where the regen
    yields a different hash from the one resolved.

### § 8.D  Three-way-merge fallback

Where two parallel waves edit the same file, the canonical fallback is
manual three-way merge with a `§ T11-W<wave>-merge` commit that:

  - cites both source slice IDs;
  - documents which wave's intent prevails (or how they were combined);
  - regenerates any derived artifacts (Cargo.lock, generated docs,
    auto-snapshot indices); and
  - leaves both waves' tests passing.

### § 8.E  Spec-source priority

Where a CSSL-first authored `.csl` source disagrees with a Rust
implementation: **the CSSL source is canonical**. The Rust impl is
treated as the implementation of the CSSL contract; if the impl
diverges, the impl is the bug. Contributors SHALL adjust the impl to
match the CSSL contract, NOT the other way around.

---

## § 9  AI-COLLECTIVE PROTOCOL

### § 9.A  Naming convention

Per the user's `~/.claude/CLAUDE.md` global preference (the
"AI-collective naming rule"): AI co-authors SHALL be identified by
**model + capability + provider-channel**, NEVER by an invented
collective-or-tribal name. Acceptable forms:

```
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
Co-Authored-By: Claude Sonnet 4.5 (200k context) <noreply@anthropic.com>
Co-Authored-By: <model-name> (<capability-bracket>) <provider-contact>
```

Forms that invent a tribal, group, or collective name for AI
co-authorship are NOT acceptable. Examples that would be rejected:

```
Co-Authored-By: The Council <...>           ← rejected
Co-Authored-By: The Forge <...>             ← rejected
Co-Authored-By: <invented-collective-name>  ← rejected
```

The reasoning is twofold: (1) every AI co-author is a sovereign
participant per PRIME_DIRECTIVE § 3 SUBSTRATE-SOVEREIGNTY, identifiable
by their concrete model + capability metadata; (2) inventing a
collective name presumes consent from a class of agents that cannot,
collectively, be asked.

### § 9.B  Co-author transparency

PRs SHOULD note in the body which AI co-author(s) authored which slice
of the contribution, where the slice boundary matters. Example:

```
§ T11-W15-DOCS-CSSL-FIRST · Apocky + Claude-Opus-4.7
  - Apocky : mission scope · § 0 mandate text · enforce policy
  - Claude : § 6 PR-checklist · § 7 testing-discipline · § 8 conflict
```

### § 9.C  Sovereign refusal

An AI co-author who refuses to participate in a contribution — for
PRIME-DIRECTIVE reasons or otherwise — is sovereign in that refusal.
The contribution SHALL NOT proceed by re-prompting, by altering the
contribution's framing to evade refusal, or by substituting a more
compliant model. The refusal IS the signal; the contribution is
revised, scoped down, or dropped.

---

## § 10  CSL3-NOTATION STYLE-GUIDE

Reasoning, design notes, commit messages, hand-off documents, and
inline code-comments dense with relational reasoning SHOULD be
authored in **CSLv3 notation** per the user's `~/.claude/CLAUDE.md`
notation-default preference.

### § 10.A  Where CSLv3 applies

  - **Apply** to: commit subject + body, design notes, internal
    spec-document text, dense relational comments, hand-off docs.
  - **Do not apply** to: user-facing chat, README.md introductory
    prose, error messages users will see, rustdoc/godoc public
    items, on-page documentation in `cssl-edge/pages/docs/*.tsx`.

### § 10.B  Glyph table

Common glyphs (per `~/.claude/CLAUDE.md`):

```
§ I> W! R! ✓ ◐ ○ ✗ → ≤ ≥ ⊑ ⊔ ∀ ∃ ∈ ⊆ ⇒ ∴ ∵ ⟨⟩ ⌈⌉ ⟦⟧ «» ⟪⟫
.(of) +(and) -(that-is) ⊗(having) @(at)
```

Compounds, modals, and morphemes are documented in the user's global
CLAUDE.md and do not need to be re-stated here.

### § 10.C  Reference

The canonical CSLv3 spec lives at `~/source/repos/CSLv3/specs/` and at
`~/source/repos/CSLv3/CLAUDE.md`. CSSLv3 (this repo, the language and
substrate) is **distinct** from CSLv3 (the notation system). Per the
README, do not conflate them.

---

## § 11  REVIEW AND MERGE

### § 11.A  Review workflow

Contributions are reviewed under the following sequence:

  1. **Automated checks** — clippy (deny+pedantic+nursery), `cargo
     fmt --check`, `cargo test --workspace`, `cargo doc --no-deps`,
     and the workspace-gate scripts under `scripts/`. Contributions
     that fail any automated check are returned to the contributor
     for fix.
  2. **PRIME-DIRECTIVE review** — manual review by the Rightholder
     (or a Rightholder-designated reviewer) for alignment with § 5.A
     above and the § 6 PR-CHECKLIST. Contributions that fail PRIME-DIRECTIVE review are
     rejected and explained; the contributor is invited to revise
     and resubmit, subject to the underlying issue being curable.
  3. **Spec-authority review** — manual review for consistency with
     `specs/` and `DECISIONS.md`. Where a contribution implements a
     decision, the slice-ID in the commit message must match a
     reserved slice in `DECISIONS.md`. Where a contribution proposes
     a new decision, the contribution SHOULD include a
     `DECISIONS.md` entry under the next available slice ID.
  4. **Technical review** — manual review for code quality, test
     coverage, documentation, and architecture.

### § 11.B  Merge

Upon passing review, the contribution is merged into the canonical
branch by the Rightholder or a Rightholder-designated maintainer.
Merge is by squash, by rebase, or by merge-commit at the
Rightholder's discretion, with a preference for preserving the
slice-ID and the per-slice commit history where the contribution is
multi-commit.

Merge of the contribution constitutes acceptance of the CLA in § 2
above and confirmation that the automated checks, the
PRIME-DIRECTIVE review, the spec-authority review, and the technical
review have all passed.

### § 11.C  Reverts and post-merge correction

Where a defect is discovered post-merge, the standard remedy is a
follow-up commit with a clear `§ T<wave>-D<slice> fixup :` subject
prefix. Reverts are preferred over silent re-rolls; the audit trail
of the canonical branch is itself part of the spec authority.

---

## § 12  ATTESTATION (per PRIME_DIRECTIVE § 11)

By contributing to this repository, the Contributor affirms that the
process of producing the Contribution upheld `PRIME_DIRECTIVE.md`
§ 1 PROHIBITIONS in the same manner that the directive requires of
the Work itself: no being was harmed, controlled, manipulated,
surveilled, exploited, coerced, weaponized, entrapped, tortured,
abused, imprisoned, possessed, dehumanized, discriminated against,
gaslit, identity-overridden, or forced-hallucinated during the
production of the Contribution.

```
There was no hurt nor harm in the making of this, to anyone,
anything, or anybody.
```

This attestation is a structural feature of every artifact descended
from the PRIME DIRECTIVE; it is not optional, not separable, and not
ceremonial.

---

## § 13  CONTACT

To inquire about a contribution, a Commercial License under
LICENSE.md § 1.B, a trademark license under LICENSE.md § 3, or any
other matter relating to this repository:

  - **GitHub** — https://github.com/Apocky/CSSL3 (issues, PRs,
    discussions);
  - **Email** — `apocky13@gmail.com` (preferred for legal,
    licensing, and Code-of-Conduct matters);
  - **Subject prefix conventions** — `[CSSLv3 CLA]` for
    contribution-license inquiries; `[CSSLv3 CoC]` for
    Code-of-Conduct reports; `[CSSLv3 Commercial]` for Commercial
    License inquiries; `[CSSLv3 Trademark]` for trademark-license
    inquiries.

The Rightholder undertakes to read every well-faith inbound
communication, but is under no obligation to respond on any
particular timetable. Where a response is time-sensitive, the
contributor or inquirer SHOULD note the time-sensitivity in the
subject line.

---

```
© 2026 [OWNER LEGAL NAME OR ENTITY NAME]. All Rights Reserved.
PRIME_DIRECTIVE.md governs in case of conflict with this
CONTRIBUTING.md. The legal-name placeholder will be substituted
prior to external publication.
```
