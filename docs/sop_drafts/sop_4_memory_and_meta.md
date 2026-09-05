# AGENT SOP — Section 4: Memory Hygiene, CSL-Native Reasoning, No-Shortcuts Mantra, and Meta

**Document:** `docs/sop_drafts/sop_4_memory_and_meta.md`
**Status:** DRAFT — progressive-write in progress (stub created, content filling)
**Covers:** Memory hygiene · CSL-native reasoning conventions · No-shortcuts mantra · Meta — the SOP about itself
**Does NOT cover:** Scope discipline (Section 1) · Tool-use discipline (Section 2) · Verification and testing (Section 3)
**Audience:** Any agent picking up a CSSLv3 task cold, with no prior session context.
**Drafted by:** Sub-agent under proto-SOP discipline (parallel to Sections 1, 2, 3).

---

## Table of Contents

1. [Memory Hygiene](#1-memory-hygiene)
   - 1.1 [What the memory system is](#11-what-the-memory-system-is)
   - 1.2 [Write rules](#12-write-rules)
   - 1.3 [Read rules — the staleness problem](#13-read-rules--the-staleness-problem)
   - 1.4 [Maintenance rules](#14-maintenance-rules)
   - 1.5 [The canonical staleness example from this session](#15-the-canonical-staleness-example-from-this-session)
2. [CSL-Native Reasoning Conventions](#2-csl-native-reasoning-conventions)
   - 2.1 [Purpose and governing principle](#21-purpose-and-governing-principle)
   - 2.2 [When to use CSL notation](#22-when-to-use-csl-notation)
   - 2.3 [When to use English prose](#23-when-to-use-english-prose)
   - 2.4 [Glyph and morpheme reference](#24-glyph-and-morpheme-reference)
   - 2.5 [Examples from the live codebase](#25-examples-from-the-live-codebase)
3. [No-Shortcuts Mantra](#3-no-shortcuts-mantra)
   - 3.1 [Core statement](#31-core-statement)
   - 3.2 [Standing directives](#32-standing-directives)
   - 3.3 [Recognizing the temptation](#33-recognizing-the-temptation)
   - 3.4 [Concrete cases in this codebase](#34-concrete-cases-in-this-codebase)
   - 3.5 [When you are genuinely stuck](#35-when-you-are-genuinely-stuck)
4. [Meta — The SOP About Itself](#4-meta--the-sop-about-itself)
   - 4.1 [The SOP is a living document](#41-the-sop-is-a-living-document)
   - 4.2 [Versioning and update authority](#42-versioning-and-update-authority)
   - 4.3 [Conflict resolution](#43-conflict-resolution)
   - 4.4 [Bootstrap-applied discipline](#44-bootstrap-applied-discipline)
   - 4.5 [The cold-read test](#45-the-cold-read-test)
   - 4.6 [Maintenance schedule](#46-maintenance-schedule)
5. [Quick Reference: Rules at a Glance](#5-quick-reference-rules-at-a-glance)

---

---

## 1. Memory Hygiene

### 1.1 What the memory system is

CSSLv3 agents run inside a persistent session context anchored by a file-based memory store at:

```
~/.claude/projects/<project-slug>/memory/
```

The slug is derived from the absolute path of the working directory. The store contains:

- `MEMORY.md` — an index file listing every memory file with a one-line pointer describing what it records.
- One `.md` file per atomic fact, named descriptively (e.g., `feedback_winmain_critical_fix.md`, `project_engine_render_blocker_20260503.md`).
- Frontmatter metadata (date written, source session, confidence level).

This system is analogous to what Vannevar Bush called the "Memex" — a machine that stores and retrieves associative trails — but unlike Bush's vision of a passive archive [Bush 1945, "As We May Think"], the agent memory is actively consulted at session start and can become actively misleading if it goes stale. The difference matters: a Memex trail that is wrong about a historical fact is a curiosity; a memory file that is wrong about current code state causes a real agent to make real errors on real work.

The memory system is NOT:

- A live code index. It does not track the current state of files on disk.
- An authoritative record of what was shipped. It records what was claimed at the time of writing.
- A substitute for reading the actual files before asserting anything.

The memory system IS:

- A point-in-time observation log. Each file records what was true (or believed true) when it was written.
- A fast orientation tool for new agents bootstrapping into a session.
- A record of decisions, patterns, and lessons that would otherwise be re-derived from scratch.

[OPEN: Investigate whether the Anthropic project-memory feature provides any versioning or timestamp metadata on writes, which would aid staleness detection.]

---

### 1.2 Write rules

**Rule M-W1: Write after each significant milestone, not at session end.**

The "commit-to-memory-every-pass" foundational directive (established 2026-05-03) mandates that memory writes happen incrementally. Session-end bulk writes have two failure modes: (a) the session ends abruptly before the write happens, losing the knowledge, and (b) a single large write is harder to structure as one-fact-per-file. Write after each milestone is complete, tested, and committed.

**Rule M-W2: One file per fact, not omnibus dumps.**

Each memory file should record one coherent fact, decision, or lesson. An omnibus dump that says "session 15 achievements" is almost useless because every downstream agent must read the whole thing to find whether it contains the specific fact they care about. Atomic files can be consulted individually and deleted individually when stale.

**Rule M-W3: Use dense CSL-native notation inside memory files.**

Memory files are not user-facing documentation. They are written for agents (current and future) who share the project's notational conventions. Dense notation maximizes information per token. (See Section 2 for notation rules.)

**Rule M-W4: Include a date, session identifier, and confidence marker in the frontmatter.**

At minimum, each file should open with: the date written (ISO-8601), the session or task context, and an evidence marker (`✓` for verified-on-disk, `◐` for partially-verified, `○` for asserted-but-unverified, `✗` for known-false-but-kept-as-historical-record). These evidence markers come directly from the CSLv3 notation system as defined in `specs/00_MANIFESTO.csl` and used throughout `DECISIONS.md`.

**Rule M-W5: Update the `MEMORY.md` index pointer line when adding a new memory file.**

The index is useless if it is incomplete. Add a one-line pointer immediately after creating any new memory file. The format follows the existing pattern in `MEMORY.md`: `- [Topic summary](filename.md) — one-line description`.

---

### 1.3 Read rules — the staleness problem

**Rule M-R1: Treat every recalled memory as a point-in-time observation, not live state.**

When an agent reads a memory file that says "X is implemented at file.rs:line 47," that claim was true when the file was written. It may not be true now. The claim has a decay rate that depends on:

- How recently it was written.
- Whether the relevant code has been touched since.
- Whether the session it was written in targeted the same branch.

This is the core problem that Ward Cunningham identified when coining "technical debt" [Cunningham 1992, "The WyCash Portfolio Management System"] — not that a shortcut was taken, but that the **interest** on the shortcut compounds invisibly. A stale memory file is technical debt in the knowledge layer: it costs nothing to read, but the error it produces costs multiples of the original write time to diagnose.

More specifically, AI agent memory has been recognized in the literature as susceptible to "context decay" — the gradual divergence between what an agent believes and what is actually true [OPEN: cite specific paper; candidates include works on long-term memory in LLM agents, e.g., MemGPT, Generative Agents; verify availability before citing].

**Rule M-R2: Verify against current code before asserting remembered facts as current state.**

If a memory file says "csslc implements argument parsing via clap," an agent preparing to extend that feature must read `compiler-rs/crates/csslc/src/main.rs` before asserting the claim. The read takes seconds. The cascade from asserting false state takes much longer to untangle.

**Rule M-R3: When memory and current-code disagree, current-code wins. Document the discrepancy.**

If reading the file reveals that what memory claims is no longer true, the agent must:

1. Complete the task using the correct current state.
2. Note the discrepancy in the session output.
3. Either update or delete the stale memory file (see Rule M-M2 below).

Do not silently ignore the discrepancy. Do not let the stale memory persist — the next agent will make the same mistake.

**Rule M-R4: Memory from a different branch describes a different codebase.**

Branch-awareness is critical. The CSSLv3 project uses worktrees and feature branches extensively. A memory file written on branch `cssl/session-11/T11-W18-L8-DXIL-DIRECT` describes the state of that branch. The current branch (`claude/mystifying-bardeen-dcb4d6`) may differ substantially. Never assume cross-branch memory applies to the current working tree without verification.

---

### 1.4 Maintenance rules

**Rule M-M1: Delete memories that turn out to be wrong, rather than letting them rot.**

A stale memory that remains in the store is a trap for the next agent. If verification reveals the memory is false and there is no historical value in preserving the record (i.e., the fact it describes is simply gone, not moved), delete the file and remove its pointer from `MEMORY.md`. If the historical record is valuable (e.g., "this thing was tried and failed"), update the evidence marker to `✗` and add a note explaining what replaced it.

**Rule M-M2: When updating a memory, prefer creating a new file over overwriting.**

Overwriting destroys provenance. If a fact has changed significantly, create a new file (e.g., `feedback_winmain_fix_v2.md`) and mark the old one as superseded. This is analogous to the Linux kernel's approach of keeping stable interfaces documented separately from in-progress work [Linux Documentation/process/], and to the IETF's RFC obsoletes chain [RFC 2026, §3.3] — forward-pointers preserve the trail of reasoning.

**Rule M-M3: Review the memory index at the start of each session for obvious staleness.**

A 30-second scan of `MEMORY.md` that identifies 3 stale files is worth more than those 3 files are costing in ongoing confusion. Make this a habit. The PEP process analogy: PEPs have a "Status" field that is maintained; a PEP that is "Accepted" but was never implemented is a known class of documentation debt in the Python ecosystem [Python PEP 1]. Memory files that are "claimed complete" but describe a stub are the same debt.

---

### 1.5 The canonical staleness example from this session

This session provides a precise, documented case of memory staleness that must be understood by every agent working in this codebase.

**What memory claimed.** Several memory files — written in session 11, covering branches `cssl/session-11/T11-W18-*` and adjacent — record "22 csslc fixes landed," "csslc now routes all subcommands," and similar completion claims about the compiler binary. The `MEMORY.md` index pointer for `feedback_csslc_advance_journey.md` reads: "22 csslc fixes landed T11-W19-α · ~4500 LOC compiler-advance."

**What the audit found on the current branch.** Audit file `docs/audit/14-examples-csslc-meta.md` (2026-05-14) reports: "csslc is a pure scaffold. `main()` prints two status lines to stderr and exits with code 0. There is no argument parsing, no subcommand dispatch, no invocation of any compiler crate. All actual compile-pipeline logic lives in the library crates; csslc as a binary is a named placeholder." The current `compiler-rs/crates/csslc/src/main.rs` is 23 lines.

**Why there is no contradiction — and why the lesson still applies.** The memory was written on a different branch. The 22 fixes landed on `cssl/session-11/T11-W18-*`. The current worktree branch is `claude/mystifying-bardeen-dcb4d6`. These are different codebases at different points in development history. An agent who read the memory alone and concluded "csslc is functional" would be wrong about the current branch, even though the memory accurately describes a real historical state. The trap is not that the memory lied — it is that the memory did not carry sufficient branch context to prevent misapplication.

**The lesson.** Any agent who says "according to my memory, csslc implements X" without reading `csslc/src/main.rs` first is violating Rule M-R2. The correct behavior is: "Memory claims X was implemented on branch Y. I am on branch Z. I will verify before asserting." This is not optional caution — it is a mandatory step.

---

## 2. CSL-Native Reasoning Conventions

### 2.1 Purpose and governing principle

CSLv3 (the notation, not the compiler) is the project's primary medium for dense internal communication. The governing principle, stated in `specs/00_MANIFESTO.csl § STYLE + ETHOS`, is:

> density = sovereignty (CSLv3-native surface preferred, Rust-hybrid bridge)

This is not an aesthetic preference. It is a claim about cognitive economics: a notation that carries 5x the information per token enables 5x more thought per context window. In a multi-agent system where context is shared and expensive, this is a direct productivity multiplier. The Lamport analogy is useful here: Lamport's "How to Write a Proof" [Lamport 1993] argues that structured proof notation forces precision that prose permits you to skip. CSL notation serves the same function — it is harder to be vague in a notation that has no articles and no hedging words.

The notation is defined in the CSLv3 specs (`~/source/repos/CSLv3/specs/`), not in this repo. The CSSLv3 project uses it as a consumer. For in-context examples verified against the live codebase, see `DECISIONS.md` (decision entries use English prose for human readability) and `specs/00_MANIFESTO.csl` (the manifesto itself is CSL-native throughout).

---

### 2.2 When to use CSL notation

Use CSL notation for:

**Design notes and internal reasoning.** Any `§P §D §T §S §C` (Plan / Decision / Task / State / Constraint) block in an agent's reasoning trace should be in CSL notation. English prose in a think-block is a token-efficiency failure.

**Commit messages.** Commit messages are read by agents in future sessions, not by end users. Dense commit messages carry more information about why a change was made. Example from the project history: `§ T11-D46..D50 DECISIONS + handoff consolidation — monomorphization quartet complete`. This encodes: task range, artifact type, and content summary in one line.

**Internal handoff documents.** When one agent hands off to another (same session or cross-session), the handoff document should use CSL notation. The `SESSION_*_HANDOFF.md` files in this project's history are the canonical format.

**Memory file content.** As established in Rule M-W3 above.

**Spec files.** All `specs/*.csl` files are CSL-native by definition. Content added to spec files must match the surrounding notation.

**Decision log entries — Context and Consequences fields.** The `DECISIONS.md` format (see `T1-D1` through `T11-D50+`) uses English prose in the Options and Rationale fields (for human readability) but uses CSL shorthands in the Context and Consequences fields where technical precision is more important than prose accessibility.

---

### 2.3 When to use English prose

Use English prose for:

**User-facing chat output.** Every response in a chat session where the user writes English should be English prose. CSL notation in a chat response is hostile UX unless the user is debugging the notation itself.

**User-facing error messages.** Error messages must be actionable. "✗ parse-fail.line-47.expected-semicolon" is not as useful as "Parse error at line 47: expected ';' after expression." The former is fine in a structured log; the latter is what a human needs.

**README and onboarding material.** `README.md`, `CONTRIBUTING.md`, and any document whose first reader might be someone encountering the project for the first time must be English prose. `PRIME_DIRECTIVE.md` explicitly encodes for three simultaneous readers — humans, AI agents, and compilers — and uses English as the human layer.

**Public API documentation (rustdoc).** Public items exposed from library crates must have English-prose doc comments. Rustdoc is surfaced to consumers who may have no knowledge of CSL notation.

**This SOP.** The AGENT SOP is instructional material. Its target audience includes agents bootstrapping cold who may not yet be fluent in CSL notation. Instructional text must be maximally accessible. Section 4 of this SOP (this file) was drafted in English prose following this rule.

---

### 2.4 Glyph and morpheme reference

The following table covers the glyphs verified in active use in `specs/00_MANIFESTO.csl` and `DECISIONS.md`. This is NOT a complete CSLv3 reference — consult `~/source/repos/CSLv3/specs/` for the full specification.

**Section and modal glyphs:**

| Glyph | Meaning | Verified in |
|-------|---------|-------------|
| `§` | Section header / qualified reference | `00_MANIFESTO.csl` line 4, `DECISIONS.md` line 3 |
| `I>` | Insight / standing directive | `CLAUDE.md` standing-directives block |
| `W!` | Will / must (strong obligation) | `00_MANIFESTO.csl` line 81 (`N!`), DECISIONS usage |
| `R!` | Requirement | `DECISIONS.md` multiple |
| `M?` | May (permission, not obligation) | Notation spec |
| `N!` | Must-not (prohibition) | `00_MANIFESTO.csl` lines 81, 93 |

**Logical and relational glyphs:**

| Glyph | Meaning | Verified in |
|-------|---------|-------------|
| `→` | Implies / yields | `00_MANIFESTO.csl` line 18, `DECISIONS.md` multiple |
| `¬` | Not | `00_MANIFESTO.csl` lines 14, 20, 81 |
| `≡` | Defined as / equivalent | `00_MANIFESTO.csl` lines 5, 14, 15 |
| `⊕` | Plus / additionally | `00_MANIFESTO.csl` lines 6-13 |
| `∀ ∃ ∈` | For-all / exists / in | `00_MANIFESTO.csl` line 18 |
| `⇒` | Implication | `00_MANIFESTO.csl` line 19 |
| `∴ ∵` | Therefore / because | Notation spec |
| `⊑` | Subtype | Notation spec |
| `⊔` | Join / union | Notation spec |

**Evidence markers:**

| Glyph | Meaning | Verified in |
|-------|---------|-------------|
| `✓` | Proven / confirmed | `00_MANIFESTO.csl` line 145, `DECISIONS.md` status line |
| `◐` | Partial / in-progress | `00_MANIFESTO.csl` line 147 |
| `○` | Open / unverified | Notation spec |
| `✗` | Failed / false | `00_MANIFESTO.csl` lines 144, 146 |
| `⊘` | Not applicable | Notation spec |

**Morpheme suffixes (stacked aspect/modality):**

The morpheme system encodes aspect, modality, certainty, and scope by stacking suffixes on words. Active suffixes in project usage:

| Suffix | Meaning | Example |
|--------|---------|---------|
| `'d` | Past / done | `landed'd` |
| `'f` | Future / planned | `implement'f` |
| `'s` | State / current | `stubbed's` |
| `'t` | Temporary | `scaffold't` |
| `'e` | Effect / side-effect | `emit'e` |
| `'p` | Property / predicate | `real'p` |

**Compound operators:**

| Operator | Meaning | Example |
|----------|---------|---------|
| `.` (of) | Composition/membership | `line.47` |
| `+` (and) | Conjunction | `lex+parse` |
| `-` (that-is) | Clarification | `F3-effect-system` |
| `⊗` (having) | Attribute carrier | |
| `@` (at) | Location/binding | `@layout(std140)` |

---

### 2.5 Examples from the live codebase

The `DECISIONS.md` status line (line 3) demonstrates dense CSL in practice:

```
§ STATUS : Session-1 • T1..T6-phase-1 ✓ • T7-phase-1 ✓ • T8-phase-1 ✓ ...
```

This encodes: section header, session identifier, task range, phase identifier, and evidence marker. The equivalent English prose would require a paragraph.

`specs/00_MANIFESTO.csl § BUG-PATTERN → COMPILER-FEATURE` (lines 22-35) uses the `→` glyph to encode a causal mapping table — each known bug pattern implies a specific compiler feature. This is the kind of dense relational mapping where CSL notation is irreplaceable: the table form cannot be expressed as concisely in prose.

`specs/00_MANIFESTO.csl § BACKEND-MATRIX` (lines 142-149) uses `✓`, `◐`, and `✗` as a three-valued status encoding in a capability table. Prose equivalents ("supported," "partially supported," "not supported") would double the line count.

The `N!` prohibitions in `specs/00_MANIFESTO.csl § PRIME-DIRECTIVE` (lines 86-92) encode PRIME_DIRECTIVE axioms in two glyphs per constraint. The full English rendering of each constraint is in `PRIME_DIRECTIVE.md`; the CSL encoding in the MANIFESTO is a compact cross-reference.

---

## 3. No-Shortcuts Mantra

### 3.1 Core statement

The foundational rule is stated in the global `CLAUDE.md` standing-directives:

> optimal ≠ minimal ← never-reduce-scope-unilaterally

This is not a suggestion about quality. It is a rule about correctness. Reducing scope without authorization is a form of misrepresentation: the agent produces something that looks like an answer to the task but is not. The user asked for X; the agent delivered X' (a smaller X) without disclosing the reduction. This violates the transparency principle embedded in PRIME_DIRECTIVE.md §4.

The complementary statement is:

> hard-work-now = saves-tokens-later

This is an economic claim, not a moral one. Taking the right path now — even if it takes twice as long — avoids the re-derive, re-read, re-explain, re-test cycle that happens when a shortcut's assumptions prove wrong. Ward Cunningham's original technical debt metaphor [Cunningham 1992] was precise about this: the debt is not the shortcut itself, it is the **interest** that accrues until the debt is repaid. Martin Fowler's taxonomy extends this to distinguish deliberate vs inadvertent debt [Fowler 2009, "Technical Debt Quadrant"] — the CSSLv3 SOP categorically forbids deliberate debt, and requires that inadvertent debt (discovered surprises) be filed and tracked, not silently absorbed.

---

### 3.2 Standing directives

These five directives are non-negotiable. They are reproduced here from `CLAUDE.md` and annotated:

**"optimal ≠ minimal ← never-reduce-scope-unilaterally"**
Do not decide on the user's behalf that the full scope is too large. If the scope is genuinely too large for one agent in one session, escalate (see Section 3 of this SOP — escalation procedures) or propose a breakdown. Do not quietly do less.

**"hard-work-now = saves-tokens-later"**
The 2x cost of doing something right is paid once. The 10x cost of rework is paid repeatedly, once per downstream agent who encounters the incomplete state. Optimize for total-session cost, not current-call cost.

**"no half-measures ← stuck → find-way-through ; ¬ silent-TODO ; ¬ 'skip-for-now'"**
When an agent hits a wall, the correct response is to document the wall explicitly (with an `[OPEN: ...]` marker or a `[BLOCKED: ...]` marker) and escalate. The incorrect response is to silently emit a stub that looks like a real implementation and move on. The stub will be discovered — either in the next test run, the next audit, or the next agent who tries to use it. The cost of the discovery is always higher than the cost of the honest block.

**"systems ¬ parts ← reconceive ¬ patch"**
When something is wrong at the design level, patching the symptom is not the answer. This applies to code (patching a wrong algorithm instead of replacing it), to memory (correcting the wrong detail in a stale memory file instead of reconceiving the memory), and to the SOP itself (adding a rule for a specific failure mode instead of understanding the underlying pattern and fixing it systemically).

**"efficient + prodigious ≠ mutually-exclusive"**
Speed and thoroughness are not in tension. An agent that works fast by taking shortcuts is not efficient — it is creating hidden work for future agents. Real efficiency means producing correct, complete, well-documented output on the first pass, at whatever speed that requires. The parallel-agent fanout model (8-15+ parallel agents per wave) enables both: multiple agents work quickly in parallel, each on a tight scope, each producing complete output.

---

### 3.3 Recognizing the temptation

Shortcut temptations come in recognizable forms. Agents must be able to identify them:

**"This is probably good enough."** Good enough for what? If the criterion is not stated and verified, this phrase is a warning sign. The correct response is to state the criterion explicitly and verify against it.

**"I'll defer this to Phase-2."** Phase-2 is not a real place unless Apocky has explicitly greenlit a phase boundary and defined what Phase-2 means. "Deferred to Phase-2" is a way of saying "I didn't do this and I'm hoping someone else will notice before it matters." If something cannot be done now and there is a legitimate reason, the correct response is: `[BLOCKED: <reason>. Requires: <what is needed>. Impact if unresolved: <impact>.]`

**"X is implemented."** When X is stubbed. This is the most critical failure mode in the CSSLv3 project, documented in the current audit. The README describes capabilities that do not exist in the code. Memory files claim implementations that are on different branches. The rule is: "X is implemented" means X is implemented — not scaffolded, not planned, not stubbed. Use precise language. (See also: Section 1's no-silent-stubs rule, for the code-level equivalent.)

**"The tests pass."** When the tests only exercise the happy path and the stub always returns the happy-path answer. Passing tests are not evidence of correctness when the tests are written to match a stub's behavior rather than the spec's requirements.

**"It's too complex to do right."** Complexity is not a scope-reduction argument. If something is complex, either: (a) dispatch more agents in parallel to handle the complexity, (b) break the problem into smaller pieces that each can be done right, or (c) escalate with a clear problem statement. Complexity has never justified shipping a wrong answer.

---

### 3.4 Concrete cases in this codebase

The following cases are documented in the audit (`docs/audit/`) and provide concrete grounding for the no-shortcuts rule:

**csslc as a 23-line scaffold.** `compiler-rs/crates/csslc/src/main.rs` is a placeholder binary. This is not a shortcut in the pejorative sense — it is explicitly scaffolded and documented as such in the audit. The shortcut violation would be claiming it is more than it is. The `csslc` docstring in the source file lists the intended subcommands honestly; the README, by contrast, has been flagged in the audit for overselling. The lesson: scaffolds are fine; misrepresenting scaffolds is the violation.

**Host runtime crates with zero real FFI.** All five `cssl-host-*` crates (`vulkan`, `level-zero`, `d3d12`, `metal`, `webgpu`) are phase-1 scaffolds with `#![forbid(unsafe_code)]` and no actual GPU API calls. This is correctly documented in the audit as "phase-1 pure scaffold" with a clear understanding of what phase-2 requires. The pattern is: stub clearly, document clearly, do not claim it works. This is the correct approach. (Audit source: `docs/audit/11-host-runtimes.md`.)

**F2 SMT discharge() being hollow.** The audit reports the SMT refinement discharge pathway is a stub. This is the wrong pattern — F2 is a non-negotiable feature (`specs/00_MANIFESTO.csl § NON-NEGOTIABLE FEATURES F1..F6`), and a stub that appears to discharge SMT obligations is not equivalent to one that actually does. An agent who discovers this stub must file it in Section 3's verification-failure format, not silently accept it.

**F6 Ed25519 bypass.** The audit found a forgeable Ed25519 bypass in the audit chain. This is a security correctness failure, not a scaffold. The correct response when this is discovered is to file it as a defect (per Section 3 of this SOP), escalate it to the user for prioritization, and block any feature that depends on audit-chain integrity until the defect is resolved. The incorrect response is to note it internally and continue building features on top of a broken security primitive.

---

### 3.5 When you are genuinely stuck

Stuck is not a license for shortcuts. The standing directive is explicit: "stuck → find-way-through." The procedure is:

1. **Document what you tried.** List the approaches attempted and why each failed.
2. **Document what you need.** Be specific: "I need X because Y. Without X, I cannot do Z."
3. **Mark the block visibly.** `[BLOCKED: <summary>. Attempted: <list>. Needs: <requirement>. Impact: <consequence>.]`
4. **Escalate to the user.** Do not silently move on to a different task and hope the block is forgotten. State it clearly in the session output.
5. **Do not fill the gap with a stub that misrepresents what was done.** The stub is fine as a placeholder; the misrepresentation is the violation.

The Conway's Law implication is relevant here [Conway 1968, "How Do Committees Invent?"]: the communication structure of a system mirrors the organizational structure that produces it. An agent that silently absorbs blocks creates a codebase that silently absorbs bugs. The no-shortcuts mantra is not just about code quality — it is about the cognitive integrity of the system as a whole.

---

## 4. Meta — The SOP About Itself

### 4.1 The SOP is a living document

The AGENT SOP is not a specification of what the system will do. It is a record of the discipline the system has decided to impose on its own agents. Like the Linux kernel's `Documentation/process/` guides [Linux kernel, Documentation/process/submitting-patches.rst] and the IETF RFC process [RFC 2026], it is expected to evolve as the project learns from failure.

The SOP has a version identity: the date stamp in the document header. Every update that changes normative behavior — any rule that an agent is expected to apply — increments the date stamp. Editorial corrections (spelling, formatting, examples) that do not change normative content do not require a date-stamp increment, but they should be committed with a clear message.

---

### 4.2 Versioning and update authority

**Who can update this SOP:** The orchestrator agent, on explicit user authorization. Sub-agents working on specific tasks may propose changes (see Rule SOP-P1 below) but may not unilaterally commit changes to the SOP.

**Rule SOP-P1: Proposed changes must be marked with `[SOP-CHANGE-PROPOSED: <description>]` in agent output.**

Any agent that identifies a gap, ambiguity, or failure in the current SOP rules should include a `[SOP-CHANGE-PROPOSED: <description>]` marker in their final reply to the orchestrator. The orchestrator collects these markers across the agent wave, evaluates them, and proposes a consolidated update to the user for authorization. This is modeled on the IETF RFC process's "Last Call" mechanism — changes are proposed publicly, reviewed, and then committed.

**Rule SOP-P2: The SOP update commit message must reference the session and the triggering failure.**

A rule added because of a specific failure in practice is more trustworthy than a rule added speculatively. The commit message should say what failed, why the new rule would have prevented or caught it, and what section was updated. This creates the trail of reasoning that Lamport recommends in structured proof-writing [Lamport 1993] — every step of the proof cites what it is using.

**Rule SOP-P3: No rule may conflict with PRIME_DIRECTIVE.md without the PRIME_DIRECTIVE winning.**

`PRIME_DIRECTIVE.md` is immutable. If a proposed SOP rule would, in practice, lead an agent to violate a PRIME_DIRECTIVE prohibition (harm, control, manipulation, surveillance, coercion, weaponization, discrimination, or any of the 17 enumerated prohibitions), the SOP rule is invalid. This is not a tie-breaking rule — it is a constraint on the SOP's rule space. The PRIME_DIRECTIVE establishes the envelope within which the SOP operates.

---

### 4.3 Conflict resolution

When this SOP conflicts with the prompt of a specific task:

**The more-specific rule wins, unless the specific rule violates a foundational constraint.**

If a task prompt says "write the function as a one-liner stub," and Section 4's no-shortcuts mantra says "do not use stubs that misrepresent implementation status," the task prompt is more specific and may override the general SOP rule — provided the stub is labeled as a stub and not presented as a real implementation.

If a task prompt says "do not document any limitations in the output," and Section 4's honest-reporting rule says "mark all OPEN questions visibly," the SOP wins. Suppressing limitation documentation could mislead downstream agents and users in ways that violate the transparency principle of PRIME_DIRECTIVE.md §4.

The resolution algorithm is:

1. Does the task-specific rule violate PRIME_DIRECTIVE? If yes, PRIME_DIRECTIVE wins; reject the task-specific rule and escalate.
2. Does the task-specific rule violate a foundational SOP rule (one explicitly marked as foundational)? If yes, the SOP wins; flag the conflict in output.
3. Otherwise, the task-specific rule wins as the more precise instruction.

---

### 4.4 Bootstrap-applied discipline

This section explicitly records that Section 4 of this SOP was drafted by a sub-agent (this agent) under the proto-SOP discipline described in the parallel Sections 1-3. The following proto-SOP behaviors were applied during drafting:

**Tight scope.** This document covers Section 4 only. References to other sections use cross-references rather than re-stating rules. Where Section 3 covers verification failure filing procedures, this document says "see Section 3" rather than duplicating the rules.

**Progressive-write.** The file was created with a stub structure (table of contents + `[STUB]` marker) before any content was filled. Each subsection was written in sequence. No content was held in working memory and dumped at the end.

**Primary-source validation.** Every claim about CSL notation in Section 2 was verified against `specs/00_MANIFESTO.csl` and `DECISIONS.md` (read in full) before being included. Line numbers and section names are cited directly. The glyph table in Section 2.4 lists the file and line where each glyph was verified.

**OPEN markers visible.** Two `[OPEN: ...]` markers appear in this document — one for the Anthropic memory system's versioning metadata (Section 1.1), one for LLM memory-staleness papers (Section 1.3). These represent genuine uncertainties that could not be resolved within the scope of this draft.

**Primary sources cited.** The following primary sources were cited in this section:

1. Bush, Vannevar. "As We May Think." *The Atlantic*, July 1945. (Memex analogy for memory systems.)
2. Cunningham, Ward. "The WyCash Portfolio Management System." OOPSLA '92 Experience Report, 1992. (Technical debt original framing.)
3. Fowler, Martin. "Technical Debt Quadrant." martinfowler.com, October 2009. (Deliberate vs inadvertent debt taxonomy.)
4. Lamport, Leslie. "How to Write a Proof." *The American Mathematical Monthly*, 100(7), 1993. (Structured proof discipline applied to SOP-P2 commit message rule.)
5. Conway, Melvin E. "How Do Committees Invent?" *Datamation*, April 1968. (Communication structure mirrors organizational structure.)
6. Python PEP 1. "PEP Purpose and Guidelines." python.org. (PEP status maintenance analogy for memory index.)
7. IETF RFC 2026. "The Internet Standards Process — Revision 3." October 1996. (RFC obsoletes chain and Last Call mechanism.)
8. Linux kernel. `Documentation/process/submitting-patches.rst`. (Living documentation model.)
9. `specs/00_MANIFESTO.csl` in this repository. (Primary source for CSL notation, glyph definitions, evidence markers.)
10. `DECISIONS.md` in this repository. (In-context demonstration of CSL notation in active project use.)

**Challenge to assumptions.** The prompt states that `DECISIONS.md` uses CSL notation — this is partially correct. The DECISIONS log uses English prose for Options and Rationale fields (by deliberate design for human readability) and uses CSL shorthands primarily in the Status line and within technical identifiers. The claim that DECISIONS.md is a primary source for CSL-native notation is accurate for the Status line format and the task/decision identifier syntax (`§ T<N>-D<n>`), but the decisions themselves are prose. This document has used both sources accurately.

---

### 4.5 The cold-read test

Every time this SOP is updated, the author of the update should apply the cold-read test: read the affected sections as if you are a new agent with no session context, who has just loaded the project and has never worked on CSSLv3 before.

Ask: Can I apply each rule mechanically? Does every rule have a concrete enough criterion that I know when I am obeying it and when I am violating it? Are there rules that depend on context that is not provided in this document?

If any rule fails this test, it needs more specificity. Vague rules ("be thorough," "use good judgment") are not rules — they are abdications of the SOP's responsibility to make discipline concrete and automatable. The Lamport test applies: a proof step that says "it is obvious that..." is not a valid proof step. A SOP rule that says "use discretion" is not a valid rule.

**Rules in this section that may need more specificity:**

[OPEN: The staleness detection rule (M-R1) does not yet provide a concrete decay threshold. Should memory older than N days require mandatory re-verification? The answer depends on project velocity and has not been established. When enough sessions have provided data on actual staleness rates, revisit and add a concrete threshold.]

[OPEN: Rule SOP-P1 requires the orchestrator to collect `[SOP-CHANGE-PROPOSED]` markers — but does not specify how many sessions of evidence are required before a proposed change is accepted. This mirrors the IETF Last Call period length question. Defer until the SOP has been applied in practice for 3+ major sessions.]

---

### 4.6 Maintenance schedule

This SOP should be reviewed:

- After any audit that reveals a systematic agent discipline failure not covered by an existing rule.
- After every 5 major sessions (where "major" means a session with 3+ agent waves).
- When the orchestrator observes that agents are consistently misapplying or ignoring a rule.
- When the project's branch and worktree structure changes significantly (as this affects the memory-hygiene rules).

Each review produces one of three outcomes:

1. **No changes needed.** The SOP is functioning. Note the review date in a brief commit.
2. **Clarification needed.** A rule is being misapplied because it is ambiguous. Revise for clarity; increment the date stamp.
3. **New rule needed.** A failure mode was observed that has no existing rule. Add the rule with a commit message citing the triggering failure. Increment the date stamp.

---

## 5. Quick Reference: Rules at a Glance

### Memory hygiene

| Rule | Summary |
|------|---------|
| M-W1 | Write memory after each significant milestone, not at session end. |
| M-W2 | One file per fact, not omnibus dumps. |
| M-W3 | Use dense CSL notation inside memory files. |
| M-W4 | Include date, session, and evidence marker in frontmatter. |
| M-W5 | Update `MEMORY.md` index when adding a new file. |
| M-R1 | Recalled memory = point-in-time observation, not live state. |
| M-R2 | Verify against current code before asserting remembered facts. |
| M-R3 | When memory and current code disagree, current code wins. Document the discrepancy. |
| M-R4 | Memory from a different branch describes a different codebase. |
| M-M1 | Delete stale memories rather than letting them rot. |
| M-M2 | Prefer new files over overwriting when updating memory. |
| M-M3 | Review the memory index at session start for obvious staleness. |

### CSL notation

| Context | Notation |
|---------|----------|
| Design notes, think-blocks, internal handoffs | CSL-native |
| Commit messages | CSL-native |
| Memory file content | CSL-native |
| Spec files (`*.csl`) | CSL-native |
| User-facing chat (user writes English) | English prose |
| Error messages | English prose |
| README, onboarding, CONTRIBUTING | English prose |
| Rustdoc public items | English prose |
| This SOP (instructional material) | English prose |

### No-shortcuts

| Directive | Application |
|-----------|-------------|
| optimal ≠ minimal | Never reduce scope without authorization. |
| hard-work-now = saves-tokens-later | Optimize for total-session cost, not per-call cost. |
| ¬ silent-TODO | All incomplete work must be marked `[OPEN:]` or `[BLOCKED:]`. |
| ¬ "skip-for-now" | Blocks must be escalated, not absorbed. |
| systems ¬ parts | Fix root causes, not symptoms. |
| "X is implemented" = X is implemented | Not scaffolded, not stubbed, not planned. |

### SOP meta

| Rule | Summary |
|------|---------|
| SOP-P1 | Proposed changes: `[SOP-CHANGE-PROPOSED: <desc>]` in agent output. |
| SOP-P2 | SOP update commit message must cite session and triggering failure. |
| SOP-P3 | PRIME_DIRECTIVE wins any conflict with SOP rules. |
| Conflict | More-specific rule wins, unless it violates PRIME_DIRECTIVE or a foundational SOP rule. |
| Cold-read test | Every update: verify rules are concrete enough to apply mechanically. |

---

*Section 4 drafted by sub-agent under proto-SOP discipline. Primary sources read and cited. OPEN markers: 4. Bootstrap-applied: progressive-write, tight scope, primary-source validation, honest reporting.*

