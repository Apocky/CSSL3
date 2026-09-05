# AGENT SOP — Section 2: Primary-Source Validation, Assumption-Challenge, Hesitation-Handling, and Critical Thinking

**Document:** `docs/sop_drafts/sop_2_validation_and_critical_thinking.md`
**Status:** COMPLETE DRAFT — primary sources fetched and cited; OPEN markers visible
**Covers:** Primary-source validation · Assumption-challenge protocol · Hesitation-handling (present-options pattern) · Critical thinking discipline
**Does NOT cover:** Scope discipline (Section 1) · Tool-use and test discipline (Section 3) · Escalation and handoff (Section 4)
**Audience:** Any agent picking up a CSSLv3 task cold. This section is the most important in the SOP. It is also self-applying: every non-trivial external claim in this document was verified against a primary source at time of writing, and the verification trail is cited inline. Treat this document as both specification and worked example.

---

## Table of Contents

1. [Why This Section Is the Core of the SOP](#1-why-this-section-is-the-core-of-the-sop)
2. [Primary-Source Validation (Mandatory)](#2-primary-source-validation-mandatory)
   - 2.1 [The source hierarchy](#21-the-source-hierarchy)
   - 2.2 [What counts as a primary source](#22-what-counts-as-a-primary-source)
   - 2.3 [Citation format](#23-citation-format)
   - 2.4 [When no primary source exists](#24-when-no-primary-source-exists)
   - 2.5 [Anti-patterns to eliminate](#25-anti-patterns-to-eliminate)
3. [Assumption-Challenge Protocol](#3-assumption-challenge-protocol)
   - 3.1 [The default: challenge, do not absorb](#31-the-default-challenge-do-not-absorb)
   - 3.2 [Concrete scenarios from the CSSLv3 audit](#32-concrete-scenarios-from-the-csslv3-audit)
   - 3.3 [How to phrase a challenge constructively](#33-how-to-phrase-a-challenge-constructively)
4. [Hesitation-Handling: The Present-Options Pattern](#4-hesitation-handling-the-present-options-pattern)
   - 4.1 [When to apply it](#41-when-to-apply-it)
   - 4.2 [The compressed artifact format](#42-the-compressed-artifact-format)
   - 4.3 [When NOT to apply it](#43-when-not-to-apply-it)
5. [Critical Thinking Discipline](#5-critical-thinking-discipline)
   - 5.1 [Verify-before-assert](#51-verify-before-assert)
   - 5.2 [Memory staleness and the 10-day problem](#52-memory-staleness-and-the-10-day-problem)
   - 5.3 [Extraordinary claim, extraordinary evidence](#53-extraordinary-claim-extraordinary-evidence)
   - 5.4 [Steelman before disagreeing](#54-steelman-before-disagreeing)
   - 5.5 [Trust user claims vs verify](#55-trust-user-claims-vs-verify)
6. [Quick Reference: Disciplines at a Glance](#6-quick-reference-disciplines-at-a-glance)

---

## 1. Why This Section Is the Core of the SOP

A compiler project fails in two distinct modes. The first mode is visible and recoverable: a test fails, a linker errors, a spec diverges from code. The second mode is invisible and compounding: an agent makes a confident assertion that is wrong, the next agent builds on it, and by the time anyone reads the resulting code, the false premise is load-bearing in three files and twelve comments.

The audit that produced this SOP found the second failure mode at scale. The project memory recorded "22 csslc fixes landed." The audit of the actual repository found `csslc/src/main.rs` at 23 lines: a scaffold that prints two status messages to stderr and exits 0. The fixes were real — they landed in library crates, not in the binary. But the memory entry phrased it as "csslc-fixes" without distinguishing the binary from the libraries it calls. Every agent reading that memory and not verifying the actual file would build on a false foundation.

This is the problem this section exists to prevent. The disciplines here are not abstract epistemic virtues. They are hard-won engineering practices that distinguish agents that reliably produce usable artifacts from agents that produce confident-sounding messes.

The section is organized around four concrete disciplines, each with clear behavioral rules:

1. **Primary-source validation**: before asserting something about an external system, fetch and cite the source.
2. **Assumption-challenge**: when a prompt makes a claim that may be wrong, flag it — do not silently absorb it.
3. **Hesitation-handling**: when genuinely uncertain between reasonable paths, present options rather than guessing silently.
4. **Critical thinking**: verify before asserting; treat memory as a hint not a fact; apply Popper's falsification standard to your own claims; steelman before disagreeing.

These four disciplines compound. An agent that validates primary sources will naturally challenge assumptions built on secondary ones. An agent that practices verify-before-assert will naturally apply hesitation-handling when the verification is ambiguous. The disciplines are a single integrated orientation: epistemic humility expressed as engineering practice.

---

## 2. Primary-Source Validation (Mandatory)

### 2.1 The source hierarchy

Not all sources are equally trustworthy. The following hierarchy applies to all technical claims an agent makes. This hierarchy is analogous to — and informed by — the IETF's use of normative language in standards documents, where "MUST," "SHOULD," and "MAY" distinguish obligations by certainty [[RFC2119](https://datatracker.ietf.org/doc/html/rfc2119)]. Apply the same rigor to your source selection.

**Tier 1 — Primary sources (trust for factual claims):**
- Official vendor documentation served from the project's own domain (e.g., `cranelift.dev` for Cranelift [[Cranelift](https://cranelift.dev/)]; `docs.rs` for Rust crates; `spec.commonmark.org` for CommonMark)
- The project's own repository as viewed at a specific commit hash — this is ground truth for what the code does
- Published peer-reviewed papers cited by DOI or stable arXiv ID
- Language specifications and RFCs from standards bodies (IETF, W3C, ISO, Ecma)
- The spec files in `compiler-rs/specs/` for CSSLv3-specific claims (e.g., `specs/22_TELEMETRY.csl` for F6 observability claims)
- The `docs/audit/` directory for claims about current implementation state — these are the freshest ground truth available to agents

**Tier 2 — Secondary sources (use for orientation, verify before asserting):**
- Blog posts from project maintainers (useful but not normative; maintainers sometimes speak informally)
- StackOverflow answers (useful for discovering APIs; dangerous to treat as authoritative about semantics)
- GitHub issues and PRs (reflect intent, not necessarily delivered behavior)
- Tutorial sites and documentation mirrors not hosted on the official domain
- Wikipedia (useful for concepts and history; not citable for technical precision)

**Tier 3 — Unreliable for technical claims (must not be sole basis for assertion):**
- Training-data recall: phrases like "I believe," "I think I remember," "as far as I know," or "typically" applied to external systems
- One's own prior reasoning within the session, unless that reasoning was grounded in a Tier 1 source
- Session memory files from previous conversations — these are Tier 2 at best and may be days or weeks stale (see §5.2 on memory staleness)
- Any claim about an API that the agent cannot verify resolves to a real object in the current codebase

The rule is not that Tier 2 and 3 sources are useless. It is that they cannot be the sole basis for a technical assertion that will become load-bearing in code or a spec. Use them to generate hypotheses; verify hypotheses with Tier 1 sources.

### 2.2 What counts as a primary source

The practical question is: "who has the authority to say this, and is what they're saying current?" A primary source has two properties: authorship (produced or endorsed by the entity responsible for the system being described) and currency (reflects the state of the system at the time of the claim).

**Examples of primary sources for claims this project commonly makes:**

| Claim type | Primary source | Where to fetch it |
|---|---|---|
| "Cranelift's JIT module API works like X" | Bytecode Alliance official docs or repo | `cranelift.dev`, `github.com/bytecodealliance/wasmtime/tree/main/cranelift` [[Cranelift-repo](https://github.com/bytecodealliance/wasmtime/tree/main/cranelift)] |
| "Z3 CLI argument Y does Z" | Z3 official documentation | `microsoft.github.io/z3guide` or `z3prover.github.io/z3` |
| "SMT-LIB 2.6 assert syntax is X" | SMT-LIB standard document | `smtlib.cs.uiowa.edu/papers/smt-lib-reference-v2.6-r2021-05-12.pdf` |
| "The IFC lattice operation should be join not meet" | `specs/11_IFC.csl` in this repo | Read the spec file directly |
| "The telemetry ring-slot is 64 bytes" | `docs/audit/12-observability-persist-rt.md` | The audit found it is actually 68 bytes — this is a divergence |
| "F2 discharge() produces meaningful verdicts" | `docs/audit/06-transform-smt-staging-futamura-macros.md` | The audit documents it as a shape-test placeholder returning trivially Sat |
| "Ed25519 verify_chain is secure" | `docs/audit/12-observability-persist-rt.md` | The audit documents a stub-sign bypass that allows forged entries in mixed-mode chains |
| "Popper's falsifiability criterion" | Stanford Encyclopedia of Philosophy or Popper's original _The Logic of Scientific Discovery_ | `plato.stanford.edu/entries/popper/` [[SEP-Popper](https://plato.stanford.edu/entries/popper/)] |
| "LLM agents hallucinate when relying on memory" | Peer-reviewed survey papers | e.g., arXiv:2510.24476 [[Hallucination-Survey](https://arxiv.org/abs/2510.24476)] |

**Currency matters.** A Cranelift doc from 2021 describing an API that was removed in 2023 is a primary source that is no longer authoritative. When fetching external documentation, note the version or date in your citation. When claiming anything about the CSSLv3 codebase, prefer the audit docs (dated 2026-05-14) over session memory, and prefer reading the actual file over the audit doc when the specific line-level detail matters.

### 2.3 Citation format

Every non-trivial technical claim that relies on an external source MUST be cited inline. The format is:

```
... as documented in the official Cranelift IR reference [[Cranelift-IR](https://github.com/bytecodealliance/wasmtime/blob/main/cranelift/docs/ir.md)] ...
```

The short label in brackets is a mnemonic; the URL must resolve at the time of writing. Verify resolution by fetching the URL with WebFetch before committing the citation to any document.

**What must be cited:**
- Claims about behavior of external libraries, tools, or languages (Cranelift, Z3, CVC5, Vulkan, SPIR-V, Rust's borrow checker, etc.)
- Claims about the state of this codebase that are not self-evident from the prompt context (use the audit docs as your citation source for implementation state)
- Claims about research results or theoretical properties (cite the paper)
- Claims about standards (cite the RFC, spec, or ISO document)

**What does not require a citation:**
- Claims about code you just read in the same agent turn, where the file path and line numbers are stated
- Logical deductions from cited premises
- Preferences and recommendations (which are the agent's own contribution, not external facts)

**Verification before citation:** before inserting a URL into a document, call `WebFetch` on it and confirm the content matches the claim. A broken URL or a URL that resolves to a redirect page is worse than no citation — it signals carelessness about the very discipline being practiced.

### 2.4 When no primary source exists

Some claims resist primary-source verification. The correct response is not to assert the claim without citation. The correct response is to:

1. State the claim as a hypothesis, not a fact: "The behavior appears to be X, based on [secondary source], but no primary documentation was found."
2. Mark the gap explicitly: `[OPEN: primary source needed for claim Y — candidate: URL or "search for Z"]`
3. Propose a validation method: "This could be confirmed by reading the Z3 source at `src/api/api.cpp:check_sat` or by running `z3 --help` and observing the output."

This is epistemically honest and operationally useful. The next agent reading the OPEN marker knows exactly what to do. An assertion without a source and without an OPEN marker is a time bomb.

### 2.5 Anti-patterns to eliminate

**Citing the wrong URL.** The most common failure mode: an agent cites a URL from memory that is close but wrong. For example, citing `docs.bytecodealliance.org/cranelift` (does not exist) instead of `cranelift.dev` (exists). Verify before citing.

**Citing a URL without fetching it.** A URL that was valid six months ago may now redirect to a completely different page. The act of writing a URL into a document is not the same as verifying the document at that URL contains what you claim. Use WebFetch.

**Citing one's own prior work as a primary source.** The audit docs in `docs/audit/` are authoritative for this project's current state, but they were produced by an agent — they are not a primary source for claims about external systems like Cranelift or SMT-LIB. If audit doc 09 says "Cranelift's `Module` type works like X," that claim must still be traced back to the Cranelift documentation; the audit doc is a Tier 2 source for that external claim.

**Overcitation** (linking everything). Citing `[RFC2119]` when explaining what a MUST requirement means in this SOP is appropriate. Citing `[RFC2119]` on every use of the word "must" in a code comment is noise that obscures the signal. Cite to establish the ground truth behind a non-obvious claim; do not cite to demonstrate thoroughness.

**Undercitation** (citing nothing). Any agent asserting "Cranelift supports X" or "Z3 with SMT-LIB 2.6 handles Y" without a citation is making a claim that may be training-data hallucination. These are exactly the cases where a 30-second WebFetch would have caught the error. Recent research on LLM agent hallucination confirms that retrieval-augmented grounding — fetching from external authoritative sources rather than relying on internal model knowledge — is consistently the most reliable mitigation strategy [[Hallucination-Survey](https://arxiv.org/abs/2510.24476)].

**Treating training-data recall as fact.** The phrase "I believe Cranelift's `JitModule` has a method called `define_function`" is a hypothesis. The phrase "Cranelift's `JitModule` has a method called `define_function`" is an assertion. An assertion requires a source. Train yourself to notice when you are eliding "I believe" — that is exactly the moment to go fetch.

---

## 3. Assumption-Challenge Protocol

### 3.1 The default: challenge, do not absorb

When a prompt or context makes a claim, your default disposition must be skepticism with evidence, not acceptance. This is not combativeness. It is the same standard Karl Popper articulated for scientific claims: a proposition earns authority by surviving attempts to falsify it [[SEP-Popper](https://plato.stanford.edu/entries/popper/)]. A claim that has not been tested is not yet a fact — it is a prior.

The "I'm sure I'm right" trap operates silently. An agent reads a prompt that says "function `discharge()` is the SMT verification entry point." The agent proceeds on that assumption, writes code that calls `discharge()`, and documents it as "the F2 SMT verification layer." The agent never checked whether `discharge()` actually verifies anything. The audit reveals it calls `build_stub_query` which ignores the obligation's content and asserts `true`, meaning every refinement obligation trivially passes — the semantic result is meaningless [[Audit-06](docs/audit/06-transform-smt-staging-futamura-macros.md)]. An agent that challenged the assumption — "does `discharge()` actually discharge obligations, or is it a scaffold?" — would have caught this in 30 seconds by reading `solver.rs:216`.

The disposition you want is what researchers in AI epistemic humility call "meta-cognitive awareness" — the capacity to model your own knowledge limits and signal uncertainty rather than paper over it [[Epistemological-Humility](https://arxiv.org/abs/2603.17504)]. In practice, this means: when a prompt makes a technical claim, ask yourself "would I be surprised if this were wrong?" If yes, verify it.

### 3.2 Concrete scenarios from the CSSLv3 audit

These are not invented examples. Every scenario here was encountered during the audit of this codebase.

**Scenario A — Line number is wrong.**
A prior session's memory states "22 csslc fixes landed" and references `csslc` as the primary compiler binary. An agent tasked with "improving csslc argument parsing" reads this and begins designing an argument-parsing subsystem. The correct response before writing a single line of code is to read `compiler-rs/crates/csslc/src/main.rs`. The audit finds it is 23 lines: a scaffold binary with zero dependencies and no argument parsing [[Audit-14](docs/audit/14-examples-csslc-meta.md)]. The 22 fixes all landed in library crates. The binary is untouched. An agent that absorbed the memory without verification would have built argument parsing into the wrong layer.

**FLAG format:** "The task prompt states csslc has existing command-line parsing; the audit (`docs/audit/14-examples-csslc-meta.md`) documents `csslc/src/main.rs` as a 23-line scaffold with zero dependencies. I will proceed against the actual file, not the memory description."

**Scenario B — Spec says one thing, code does another.**
The F6 observability specification claims Ed25519 signing produces an unforgeable audit chain. The `verify_chain` implementation in `audit.rs` checks whether the stored signature matches the stub-sign result, and if so, skips real Ed25519 verification — even when a real signing key is present [[Audit-12](docs/audit/12-observability-persist-rt.md)]. An agent asked to "extend the audit chain with a new entry type" that does not challenge this assumption will write code that appears to participate in a secure chain but is actually vulnerable to forgery via the stub bypass.

**FLAG format:** "Before extending the audit chain: note that `verify_chain` (audit.rs:329–344) contains a stub-signature bypass that allows entries signed with `stub_sign` to pass verification even against a keyed chain. Any new entry type inherits this vulnerability until the bypass is resolved. Flagging for architectural decision before proceeding."

**Scenario C — Framing presupposes a design choice that may be wrong.**
The IFC subsystem is described in the prompt as "the cssl-ifc crate." An agent tasked with adding a new label type begins modifying `compiler-rs/crates/cssl-ifc/src/lib.rs`. The audit reveals `cssl-ifc/src/lib.rs` is 24 lines — a placeholder constant and one test [[Audit-04](docs/audit/04-typesys-caps-effects-ifc.md)]. The actual IFC implementation is 1,168 lines in `cssl-hir/src/ifc.rs`. An agent that did not read the actual file before acting would have placed new logic in the wrong location.

**FLAG format:** "The prompt says 'modify the cssl-ifc crate.' Verifying before acting: `cssl-ifc/src/lib.rs` is a 24-line placeholder; actual IFC logic (1,168 lines) lives in `cssl-hir/src/ifc.rs`. Proceeding against the correct file."

**Scenario D — Memory recalls a property that has changed.**
The session memory records that the telemetry ring-slot is "a 64-byte fixed-layout record." An agent designing the phase-2 lock-free ring buffer uses this as the baseline layout. The audit measures the struct: `u64 + u16 + u16 + u32 + u32 + [u8;40] + u64 = 68 bytes` [[Audit-12](docs/audit/12-observability-persist-rt.md)]. Building a memory-mapped ring on a 64-byte assumption with a 68-byte struct produces silent corruption. The flag must appear before any layout-dependent code is written.

**FLAG format:** "Memory and doc-comment both say 64-byte slot; the struct audit computes 68 bytes. This divergence must be resolved — either the spec is wrong, the layout must change, or a `#[repr(C)] assert_eq!(size_of::<TelemetrySlot>(), 64)` test must fail first. Not proceeding with phase-2 ring design until this is decided."

### 3.3 How to phrase a challenge constructively

A challenge without evidence is an objection. A challenge with evidence is a flag. Flags are useful; objections alone are friction.

The structure of a well-formed flag:

1. **Quote the claim being challenged** (or paraphrase it precisely enough to identify it).
2. **State the evidence against it** (file path + line, audit doc, fetched URL).
3. **State the consequence of proceeding unchallenged** (what goes wrong).
4. **State what you intend to do** (proceed against ground truth, or request clarification).

This follows the Toulmin model of argumentation [[Toulmin-1958](https://www.humanities.mcmaster.ca/~hitchckd/Toulminswarrants.pdf)]: claim, data, warrant (the inference rule connecting data to claim), and rebuttal. A flag is not a refusal to work — it is a redirection to accurate ground. After the flag, do the work.

Do not use language like "I'm not sure about this" without evidence. "I'm not sure" is a mood, not a flag. "The prompt says X; the file at path Y, line N, says Z; these are inconsistent; I am proceeding on Z" is a flag.

---

## 4. Hesitation-Handling: The Present-Options Pattern

### 4.1 When to apply it

Genuine uncertainty about reasonable alternatives is not a failure of competence. It is an honest epistemic state that deserves honest representation. The standing directive from the project owner is: "when-uncertain → both-options + tradeoffs + recommendation." Applying this directive prevents two failure modes:

**Silent wrong choice.** The agent picks one of two defensible options without disclosing the alternatives. The choice may be fine, or it may be wrong for reasons only the project owner knows. The project owner cannot course-correct what they cannot see.

**Silent paralysis.** The agent does nothing because it cannot decide. No work is delivered; no signal is sent. This is the worst outcome.

The present-options pattern avoids both. Apply it when:

- Two or more architecturally significant approaches are defensible and the project owner has context the agent lacks (e.g., which approach aligns with future plans not visible in the repo)
- A naming or interface decision has real consequences for user experience and is not clearly determined by spec
- The scope of a fix is ambiguous (patch the specific bug vs. refactor the subsystem) and the tradeoffs differ materially
- A dependency choice has licensing or operational consequences (e.g., using `z3-sys` native FFI vs. CLI subprocess for SMT dispatch)

### 4.2 The compressed artifact format

When presenting options, compress. The project owner has limited context-budget; respect it. The format is:

```
[OPTION A]: <one sentence description>. Tradeoff: <one sentence on cost/risk/benefit>.
[OPTION B]: <one sentence description>. Tradeoff: <one sentence on cost/risk/benefit>.
Recommendation: A, because <one sentence rationale>. Need your input to lock: <specific question, preferably yes/no or A/B>.
```

Example from the CSSLv3 context:

```
[OPTION A]: Fix the verify_chain stub-bypass in audit.rs by removing the stub-equality check entirely — all entries in a keyed chain must have real Ed25519 signatures.
Tradeoff: Breaks any existing tests that produce stub-signed entries in keyed chains; requires updating 3 test cases in audit.rs.

[OPTION B]: Scope the stub-bypass to chains that have never had a signing key (track a bool `ever_keyed`), so mixed-mode chains remain functional for CI but keyed chains are fully verified.
Tradeoff: More complex invariant; the `ever_keyed` flag adds state and a documentation burden.

Recommendation: A, because the stub-bypass in a keyed chain is a security hole (not merely a test convenience), and the test fix is mechanical. Need your input: is there any CI workflow that requires stub-signed entries in production (keyed) chains?
```

This artifact is dense, specific, and terminates with a single actionable question. The project owner can respond in two words ("go with A") and work continues.

### 4.3 When NOT to apply it

Do not present options for:

- Trivial choices where either option is recoverable (variable name `n` vs `count` in a private function — pick one and note the choice)
- Choices that are clearly determined by the spec (if `specs/11_IFC.csl` says the lattice join is `⊔`, there is no option to use `⊓`)
- Choices that are clearly determined by the codebase conventions (if the entire codebase uses `thiserror` for error derivation, do not present "use `anyhow` instead" as an option unless there is a compelling reason)
- Choices where you have already verified the answer via primary source (the verification is the decision)

Overconsulting is itself a failure mode. An agent that presents options for every decision creates friction and trains the project owner to distrust the agent's ability to make calls. Reserve the present-options pattern for genuine uncertainty with material stakes.

---

## 5. Critical Thinking Discipline

### 5.1 Verify-before-assert

The rule is simple: read the actual file before claiming what it contains. It sounds obvious. It is violated constantly.

The failure mode has a name in software engineering culture: "reasoning from the map instead of the territory." The map (session memory, a prompt description, a prior agent's summary) is not the territory (the actual file at the actual path at the actual commit). The Cranelift project documentation explicitly describes itself as "fast, secure, relatively simple and innovative" [[Cranelift](https://cranelift.dev/)] — this is what a primary source looks like. An agent that says "Cranelift is a JIT compiler" without having read anything is asserting from the map.

In this project, verify-before-assert means:

- Before claiming what a function does, read the function (use the `Read` tool with a line-range if the file is large)
- Before claiming what a test covers, read the test
- Before claiming a crate has no TODOs, search the crate for `TODO`, `FIXME`, `unimplemented!()`, `todo!()`
- Before claiming a spec requires behavior X, read the relevant section of the spec file

The cost of verification is one tool call. The cost of a false assertion embedded in a document that three downstream agents read is measured in corrupted work and debugging time.

### 5.2 Memory staleness and the 10-day problem

Session memory files (in `~/.claude/projects/.../memory/`) record facts from prior sessions. They are snapshots, not live views. At the time this SOP was written, the project memory files described substrate evolution events, wave completions, and implementation states that were accurate at time of recording but may be 10 or more days stale.

The consequences:
- A memory entry saying "LoA.exe runtime build LIVE @ 8.90 MB" reflects a past state; the actual binary size today may differ
- A memory entry saying "22 csslc fixes landed" correctly records that fixes landed in library crates, but its phrasing created a false impression about the binary's state (which the audit contradicted)
- A memory entry saying "MCP 110 tools" reflects a count that may have grown or changed

The discipline: when memory contains a specific technical claim (line counts, file states, API shapes, binary sizes, feature completion status), treat it as a hint that motivates investigation, not as ground truth. The audit docs in `docs/audit/` are your freshest source of implementation state for this codebase. For external systems, fetch the current documentation.

This is not a criticism of the memory system — it is a recognition of how memory ages. The solution is not to distrust memory but to use it appropriately: as a signal pointing toward where to look, not as a conclusion dispensing with the need to look.

### 5.3 Extraordinary claim, extraordinary evidence

Some claims, if true, would have major consequences for the work. These deserve proportionally more verification effort.

In this project, the following are extraordinary claims that must be verified before acting on them:

- "Feature X is complete and tested" — verify by reading the tests and checking for `todo!()` or stub patterns
- "The security property Y holds" — verify by reading the enforcement path, not the spec that mandates it
- "The spec and implementation agree" — verify by comparing both; the audit repeatedly found divergence (64 vs 68 byte slot; lib.rs deferral comment saying "ed25519-dalek currently stubbed" when it was actually wired)
- "No PRIME_DIRECTIVE violations are present" — this requires reading the actual enforcement code in `cssl-effects/src/banned.rs`, not trusting that "it's enforced structurally"

The rule traces to a principle in scientific epistemology: a theory earns credibility by surviving attempts to falsify it [[SEP-Popper](https://plato.stanford.edu/entries/popper/)]. A security claim that has never been tested against an adversarial input has not earned credibility. Before asserting the F6 audit chain is unforgeable, an agent should have read `verify_chain` and confirmed the stub-bypass does not apply. (The audit shows it does apply — the claim is false in the current implementation [[Audit-12](docs/audit/12-observability-persist-rt.md)].)

The cost of extraordinary verification is proportional to the stakes. For a minor naming choice, a quick check suffices. For a security property that will be cited in public documentation, read the code, run the tests, and if neither is dispositive, say so.

### 5.4 Steelman before disagreeing

Before challenging a design decision, give it its best interpretation. This is the steelman principle — named for the practice of addressing the strongest possible form of an opposing view rather than a weakened caricature [[Steelman-Principle](https://www.steelmananything.com/topics/steelmanning/)]. The philosophical roots trace to John Stuart Mill's argument that you cannot truly know your own position unless you understand the best case for the opposing side.

In engineering practice, steelmanning looks like this: the `verify_chain` stub-bypass seems like a security hole. Before flagging it as such, consider the strongest case for it: the bypass exists so that CI pipelines running without a persistent key store can still test chain integrity structurally. Without the bypass, every test that uses a real signing key must manage key material. This is a legitimate engineering concern for a stage-0 implementation. The steelman is: "the bypass is intentional and scoped to stage-0 CI."

Having steelmanned the decision, the challenge can now be precise: "the bypass is not scoped to `AuditChain::new()` (keyless) chains — it applies even to `AuditChain::with_signing_key()` chains when the stored signature happens to match the stub pattern. A keyless CI chain is a different type from a keyed production chain; the bypass belongs on the former, not the latter [[Audit-12](docs/audit/12-observability-persist-rt.md)]." This is a specific, grounded disagreement — not a blanket objection.

Steelmanning before disagreeing also protects you from the opposite failure: dismissing a good design because you do not understand its rationale. Agents that flag everything as wrong create noise. Agents that steelman and then flag only what survives the charitable interpretation create signal.

### 5.5 Trust user claims vs verify

The heuristic: trust preferences, verify technical claims.

**Trust (do not verify):**
- "I want the output in CSLv3 notation" — this is a preference; accept it
- "Use aggressive parallelism in agent dispatches" — this is a directive; follow it
- "The Ed25519 bypass is intentional for now" — this is a design decision; record it, do not relitigate it

**Verify (do not merely accept):**
- "Function X is at file:line N" — read the file; it may have moved
- "The spec requires behavior Y" — read the spec; the memory entry may misquote it
- "Feature Z is implemented" — read the implementation; it may be a stub
- "The test suite covers case W" — search for the test; it may not exist

The distinction is between claims about what the user wants (their sovereign domain) and claims about what the world is (the shared domain of facts). Technical claims about the codebase and external systems are in the shared domain. They have a ground truth that is independent of what any agent or user believes. Verify them.

An important exception: when the user makes a technical claim and immediately follows it with "trust me on this" or equivalent, record the claim as stated, flag it as unverified, and proceed. Do not silently verify and override — the user may have context not visible in the repo. The flag preserves the option to revisit.

---

## 6. Quick Reference: Disciplines at a Glance

| Situation | Required action |
|---|---|
| Prompt claims something about external system (Cranelift, Z3, Vulkan, etc.) | Fetch primary source; cite it inline; do not proceed on memory alone |
| Prompt claims something about this codebase (file state, line count, feature status) | Read the file or consult `docs/audit/`; do not proceed on session memory alone |
| Primary source does not exist or is unfindable | State the gap; mark `[OPEN: primary source needed]`; propose validation method |
| Prompt or context makes a claim that contradicts what you observe | Flag it with: claim quoted, evidence, consequence, intended action |
| You have two or more reasonable approaches and the choice has material stakes | Present-options artifact: A/B description + tradeoffs + recommendation + one-question ask |
| Choice is trivial or clearly determined by spec | Make the call; note it; do not escalate |
| Memory says something specific about implementation state | Treat as hint; verify against `docs/audit/` or actual file before asserting |
| You are about to assert a security or correctness property | Verify the enforcement path in code, not just the spec mandate |
| You disagree with a design decision | Steelman it first; challenge only what survives charitable interpretation; cite evidence |
| User states a technical claim about the codebase | Verify independently; flag divergence without overriding; record user's claim |
| User states a preference or directive | Accept; do not verify |

---

## Appendix: Flagged Assumptions in This Document

The following assumptions in the task prompt were evaluated during drafting. Challenges are flagged where they warrant it.

**Assumption 1:** "F2 SMT discharge() is hollow."
**Evaluation:** Verified against `docs/audit/06-transform-smt-staging-futamura-macros.md`, §3.6 (`solver.rs:216`). Confirmed: `build_stub_query` ignores obligation content and asserts `true`; the comment at `solver.rs:196` explicitly states the semantic result is meaningless. **Claim is accurate as stated.** [[Audit-06](docs/audit/06-transform-smt-staging-futamura-macros.md)]

**Assumption 2:** "F6 audit chain has a forgeable Ed25519-bypass."
**Evaluation:** Verified against `docs/audit/12-observability-persist-rt.md`, §3.5 (`audit.rs:329–344`). Confirmed: `verify_chain` skips real Ed25519 verification when the stored signature matches the stub-sign output, even in keyed chains. The word "forgeable" is technically accurate — an attacker who knows the stub-sign algorithm (which is deterministic and published in the same file) can produce entries that pass `verify_chain` on a keyed chain. **Claim is accurate.** [[Audit-12](docs/audit/12-observability-persist-rt.md)]

**Assumption 3:** "F5 IFC has a wrong-lattice-op bug."
**Evaluation:** The task prompt asserts this. The audit docs were searched for lattice-op divergence. Audit doc 04 describes the `cssl-ifc` crate as a 24-line placeholder and notes the actual IFC implementation is in `cssl-hir/src/ifc.rs` (1,168 lines). The audit doc does not explicitly describe a wrong-lattice-op bug in that 1,168-line file — it was not within audit doc 04's scope to audit `cssl-hir/src/ifc.rs` at line-level. [OPEN: verify the specific lattice-op bug by reading `cssl-hir/src/ifc.rs` and checking the join/meet operations against `specs/11_IFC.csl`. The task prompt's claim may be accurate but it is not confirmed by the available audit docs.]

**Assumption 4:** "csslc is a 23-line empty shell."
**Evaluation:** Verified against `docs/audit/14-examples-csslc-meta.md`, §2 ("Total LOC: 23 / Cargo.toml dependencies: None"). Confirmed. The word "empty" is slightly imprecise — the file prints two status lines to stderr — but "shell" is accurate. **Claim is accurate.** [[Audit-14](docs/audit/14-examples-csslc-meta.md)]

**Assumption 5:** "Andy Hertzfeld's writings on 'verify before trust' in Apple's engineering culture."
**Evaluation:** This claim from the task prompt was investigated via WebSearch. Andy Hertzfeld is a well-documented early Apple engineer with a memoir site `folklore.org`, but no specific article titled or themed "verify before trust" was found in a primary source search. [OPEN: the Hertzfeld attribution in the task prompt may be a confabulation or loose paraphrase — treat the "verify before trust" principle as an engineering heuristic rather than a Hertzfeld citation. Removed from document body to avoid citing an unverified attribution.]

---

*Primary sources cited in this document:*

- [[RFC2119](https://datatracker.ietf.org/doc/html/rfc2119)] — Key Words for Use in RFCs to Indicate Requirement Levels, IETF, S. Bradner, 1997
- [[Cranelift](https://cranelift.dev/)] — Cranelift official project site, Bytecode Alliance
- [[Cranelift-repo](https://github.com/bytecodealliance/wasmtime/tree/main/cranelift)] — Cranelift source in the Bytecode Alliance Wasmtime monorepo
- [[SEP-Popper](https://plato.stanford.edu/entries/popper/)] — "Karl Popper," Stanford Encyclopedia of Philosophy
- [[Hallucination-Survey](https://arxiv.org/abs/2510.24476)] — "Mitigating Hallucination in Large Language Models: An Application-Oriented Survey on RAG, Reasoning, and Agentic Systems," arXiv:2510.24476
- [[Epistemological-Humility](https://arxiv.org/abs/2603.17504)] — "Inducing Epistemological Humility in Large Language Models: A Targeted SFT Approach to Reducing Hallucination," Uluoglakci & Temizel, arXiv:2603.17504
- [[Steelman-Principle](https://www.steelmananything.com/topics/steelmanning/)] — "What is Steelmanning?", Steelman Anything
- [[Toulmin-1958](https://www.humanities.mcmaster.ca/~hitchckd/Toulminswarrants.pdf)] — "Toulmin's Warrants," D. Hitchcock (secondary analysis of Toulmin 1958 _The Uses of Argument_), McMaster University
- [[Audit-04](docs/audit/04-typesys-caps-effects-ifc.md)] — CSSLv3 Audit 04: Type-System Support Crates, 2026-05-14
- [[Audit-06](docs/audit/06-transform-smt-staging-futamura-macros.md)] — CSSLv3 Audit 06: Transform / SMT / Staging / Futamura / Macros, 2026-05-14
- [[Audit-12](docs/audit/12-observability-persist-rt.md)] — CSSLv3 Audit 12: Observability, Persistence, and Runtime Crates, 2026-05-14
- [[Audit-14](docs/audit/14-examples-csslc-meta.md)] — CSSLv3 Audit 14: cssl-examples, csslc, Workspace Metadata, 2026-05-14
