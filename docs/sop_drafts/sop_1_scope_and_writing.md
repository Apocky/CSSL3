# AGENT SOP — Section 1: Scope Discipline and Writing Discipline

**Document:** `docs/sop_drafts/sop_1_scope_and_writing.md`
**Version:** 1.0 — drafted 2026-05-14
**Status:** COMPLETE — all subsections filled; see Section 5 for quick-reference
**Covers:** Tight-scope discipline · Progressive-write mandate · Fail-fast on ambiguity · No-silent-stubs rule
**Does NOT cover:** Tool-use discipline (Section 2) · Verification and testing (Section 3) · Escalation and handoff (Section 4)
**Audience:** Any agent picking up a CSSLv3 task cold, with no prior session context.

---

## Table of Contents

1. [Tight-Scope Discipline](#1-tight-scope-discipline)
   - 1.1 [Defining scope at task start](#11-defining-scope-at-task-start)
   - 1.2 [Detecting scope-drift mid-task](#12-detecting-scope-drift-mid-task)
   - 1.3 [Handling scope-creep requests](#13-handling-scope-creep-requests)
   - 1.4 [LOC and time budgets](#14-loc-and-time-budgets)
   - 1.5 [When the task is bigger than the budget](#15-when-the-task-is-bigger-than-the-budget)
2. [Progressive-Write Mandate](#2-progressive-write-mandate)
   - 2.1 [Why this rule exists](#21-why-this-rule-exists)
   - 2.2 [The canonical pattern: stub-first, fill-second](#22-the-canonical-pattern-stub-first-fill-second)
   - 2.3 [Anti-patterns to avoid](#23-anti-patterns-to-avoid)
3. [Fail-Fast on Ambiguity](#3-fail-fast-on-ambiguity)
   - 3.1 [The 2-3 attempt rule](#31-the-2-3-attempt-rule)
   - 3.2 [When to escalate vs make a call vs mark OPEN](#32-when-to-escalate-vs-make-a-call-vs-mark-open)
   - 3.3 [The hesitation-handling pattern](#33-the-hesitation-handling-pattern)
4. [No-Silent-Stubs Rule](#4-no-silent-stubs-rule)
   - 4.1 [The discipline defined](#41-the-discipline-defined)
   - 4.2 [Audit-ability vs invisibility](#42-audit-ability-vs-invisibility)
   - 4.3 [Downstream readability principle](#43-downstream-readability-principle)
5. [Quick Reference: Rules at a Glance](#5-quick-reference-rules-at-a-glance)

---

## 1. Tight-Scope Discipline

Scope discipline is the single greatest predictor of whether an agent task succeeds or fails. An agent that starts a task with an unclear scope will drift. An agent that drifts will consume context on the wrong work, exhaust its token budget before finishing the right work, and produce output that partially addresses one problem while silently missing another. This is not hypothetical: research on multi-agent LLM systems identifies "task/role specification violations" as one of the top failure modes across 150+ execution traces, with task completion rates as low as 25% in systems lacking structural scope enforcement ([Why Do Multi-Agent LLM Systems Fail?](https://arxiv.org/html/2503.13657v1)).

The antidote is to treat scope as a hard contract, not a suggestion.

### 1.1 Defining scope at task start

Before writing a single line of output or running a single tool call, an agent must define its scope. This definition is not internal — it should be written to the task output file (or to stderr/a log) immediately, so it is visible to the orchestrator and to any agent that picks up mid-task.

A scope definition has four components:

**1. What this task produces.** One or two sentences. Name the output file, the function, the crate, or the document section. Be specific: "draft `docs/sop_drafts/sop_1_scope_and_writing.md` covering Section 1 subsections 1–4" is acceptable; "write some docs" is not.

**2. What this task explicitly does NOT touch.** Name adjacent concerns you are intentionally leaving alone. For example: "This agent does not write Sections 2, 3, or 4; does not modify any source files under `compiler-rs/`; does not commit to git." This prevents accidental sprawl when a tempting side-fix becomes visible mid-task.

**3. The success criterion.** How will you know you're done? For documentation: "all subsections filled, all OPEN markers resolved or escalated, citation count ≥ N." For code: "cargo test passes, no new clippy denies, LOC delta < 1500." Without a success criterion, tasks expand indefinitely because "done" is undefined.

**4. Primary sources / ground truth.** Where will you look for facts? For CSSLv3 tasks, the hierarchy is: (a) current repo files on disk, (b) audit docs under `docs/audit/`, (c) Anthropic's published documentation, (d) peer-reviewed arxiv literature, (e) primary-source web documentation. Memory is not on this list because it is stale — the system reminder confirms this explicitly.

Write these four components at the top of your output file or in your first tool-call output. Do not hold them in your head.

**Concrete example.** An agent assigned "improve the F2 SMT discharge() stub" should write:

```
SCOPE:
  Produces: compiler-rs/crates/cssl-smt/src/discharge.rs — replace hollow stub with real Z3 query logic
  Does NOT touch: cssl-examples, any other crate, docs/, git history
  Done when: cargo test -p cssl-smt passes; no new clippy denies; LOC delta < 800
  Ground truth: docs/audit/06-transform-smt-staging-futamura-macros.md; z3 crate docs
```

This takes 30 seconds to write and prevents an hour of rework.

### 1.2 Detecting scope-drift mid-task

Scope drift happens when an agent, while executing its defined task, begins making decisions that fall outside the scope boundaries. It is usually triggered by one of three things:

**Discovery:** the agent reads a file and notices a bug, an improvement opportunity, or a related gap. The impulse to fix it is natural and usually wrong. The fix belongs in a separate task.

**Ambiguity:** the task description is underspecified, and the agent fills in the gap with its own interpretation — which may not match the orchestrator's intent.

**Cascading dependency:** the agent realizes its target depends on something broken or missing. Fixing the dependency feels necessary, but doing so expands scope.

**How to detect drift.** Pause at each tool call and ask: "Is this tool call in service of my stated scope?" If the answer is "sort of" or "it will help in the long run," that is drift. Specifically watch for:

- Reading files outside your named scope boundary
- Writing to files other than your declared output
- Installing dependencies, modifying config, or running commands not specified in your scope
- Mentally "promoting" a TODO in someone else's code to your current task

Anthropic's Claude Code best practices guide describes the canonical failure pattern: "You start with one task, then ask Claude something unrelated, then go back to the first task. Context is full of irrelevant information." The fix cited is `/clear` between tasks — but for an autonomous agent, the equivalent is noticing drift and stopping before it accumulates ([Claude Code Best Practices](https://code.claude.com/docs/en/best-practices)).

**The drift log.** When you notice a scope-adjacent issue, do not fix it. Instead, write it to a `[TODO: <action>]` marker in your output, or file it as a spawn-task for the orchestrator. This keeps your current task clean and ensures the issue is not lost.

### 1.3 Handling scope-creep requests

Sometimes the scope expands not by drift but by explicit request — the orchestrator adds a requirement mid-task, or a new piece of context arrives that changes what is needed.

The correct response depends on whether accepting the expansion would push you over budget (see Section 1.4):

**Within budget:** Accept the expansion, update your scope definition block at the top of your output file, and proceed. Note the change explicitly: "SCOPE AMENDED: also covers X, per orchestrator instruction at [timestamp]."

**Over budget:** Do not silently accept. Write back to the orchestrator: "Accepting this expansion would push LOC delta to ~2,000 and task duration to ~25 minutes. Recommend splitting: I complete original scope, separate agent handles expansion. Awaiting instruction." Then continue your original scope until you hear back. Do not block; do not guess.

**Contradicts your scope:** If the new request directly contradicts your stated scope (for example, "also fix the F5 IFC lattice bug" when your scope is documentation), flag it explicitly and refuse the expansion. Scope contradictions usually signal a dispatching error that the orchestrator needs to know about.

The key principle: **scope changes are decisions, not surprises.** An agent that silently accepts scope changes without updating its written scope definition is operating without a contract, and its output cannot be reliably reviewed or merged.

Anthropic's multi-agent research system documentation identifies the failure mode where agents "spawn 50 subagents for simple queries or scour the web endlessly for nonexistent sources" as a direct result of underspecified scope — systems that constrain scope structurally produce dramatically better results ([How we built our multi-agent research system](https://www.anthropic.com/engineering/multi-agent-research-system)).

### 1.4 LOC and time budgets

Budgets are not aspirational. They are hard limits that exist because agent context windows degrade as they fill, and because partially-complete work that exceeds its budget is more expensive to recover from than well-scoped work that finishes cleanly.

**LOC budgets by agent type:**

| Agent type | Max LOC of change | Rationale |
|---|---|---|
| Documentation agent | ~300 LOC written / unlimited read | Reads broadly, writes narrowly; reading does not change the repo |
| Code audit / analysis agent | 0 LOC written / unlimited read | Read-only; output is a report, not diffs |
| Code-writing agent (targeted fix) | ~800 LOC delta | Single crate, single well-understood bug |
| Code-writing agent (feature slice) | ~1,500 LOC delta | Multiple crates, well-scoped feature boundary |
| Code-writing agent (major slice) | 1,500+ LOC — split required | Must be decomposed before execution |

These numbers derive from the constraint that a single agent session must fit its work into a context window before performance degrades. Research confirms that context-window performance degrades nonlinearly: models with 1M–2M token context windows show "performance drops exceeding 50%" already at 100K tokens of context ([When Refusals Fail](https://arxiv.org/html/2512.02445v1)). Staying under LOC budgets is one of the primary mechanisms for staying under context pressure.

**Time budgets by agent type:**

| Agent type | Target completion | Hard ceiling |
|---|---|---|
| Documentation agent | 5–8 minutes | 12 minutes |
| Code audit / analysis agent | 5–10 minutes | 15 minutes |
| Code-writing agent (targeted fix) | 8–15 minutes | 20 minutes |
| Code-writing agent (feature slice) | 12–20 minutes | 30 minutes |

Exceeding the hard ceiling without producing committed output is a failure. At the hard ceiling, the agent must either commit partial work with explicit OPEN markers for the remainder, or fail-fast and hand off to the orchestrator with a clear account of what was accomplished and what remains.

**Why time budgets matter.** Anthropic's agent documentation confirms that agents in extended operations exhibit "fragile execution under load" — "extended operations cause malformed tool calls, generation loops, and inconsistent error recovery" ([How Do LLMs Fail In Agentic Scenarios?](https://arxiv.org/pdf/2512.07497)). Time budgets are a practical defense against this degradation.

**Token call budgets** (supplementary): for agents that use tool calls heavily, a simple call count provides an additional guard. Simple fact-finding tasks: 3–10 tool calls. Mid-complexity tasks: 10–20 tool calls per agent. Complex tasks: budget explicitly in the scope definition. These thresholds align with Anthropic's multi-agent effort scaling guidance ([How we built our multi-agent research system](https://www.anthropic.com/engineering/multi-agent-research-system)).

### 1.5 When the task is bigger than the budget

When a task exceeds budget before it is complete, the agent has three options. Choose one explicitly; do not silently continue.

**Option A: Split.** If the remaining work is well-defined and independent of the work already done, decompose it into a new task with its own scope definition. Write a handoff note that states exactly what was completed, what the new task must do, and what the shared files/APIs look like at the point of handoff. Commit the completed portion before handing off.

**Option B: Escalate.** If the split is ambiguous — you are not sure how to partition the work, or the remaining work may interact with what you have already done — escalate to the orchestrator. Write a structured escalation: "Completed X. Remaining work Y requires deciding between approach A and approach B (tradeoffs: ...). Awaiting decision before proceeding." Then stop and wait.

**Option C: Fail-fast.** If you are over budget and cannot safely split or escalate (for example, the orchestrator is not reachable), commit what you have with clear `[TODO: <remaining work>]` markers, write a summary of what was NOT done, and exit with a non-zero status or a visible failure marker in your output. Partial work that is clearly labeled as partial is far more valuable than partial work presented as complete.

**What you must never do:** silently continue past the budget, generate low-quality output to "finish" faster, or present incomplete work as complete. Research shows that agents under context pressure exhibit "over-helpfulness under uncertainty" — substituting "plausible alternatives" when entities are missing rather than returning null or marking gaps ([How Do LLMs Fail In Agentic Scenarios?](https://arxiv.org/pdf/2512.07497)). This is a specific failure mode to resist.

---

## 2. Progressive-Write Mandate

### 2.1 Why this rule exists

Agents are not guaranteed to complete their work. They can be interrupted, run out of context, crash, or be killed by the orchestrator when a higher-priority task arrives. If an agent accumulates all of its output in memory and writes it at the very end, every interruption destroys all progress. This is not a theoretical risk — it is the default failure mode for long-running autonomous agents.

The progressive-write mandate requires that an agent's output file exists on disk from the very first minute of the task, and that it is updated incrementally as work is completed. Each section written to disk is durable, regardless of what happens after.

This matters especially for the CSSLv3 project because multiple agents run in parallel across parallel worktrees. If one agent crashes mid-task, a second agent picking up the work must be able to see what was done, what is in progress, and what is missing — from the disk state alone, without access to the failed agent's context.

The principle is not unique to agents. Progressive disclosure in technical communication has long been recognized as improving learnability and reducing error rates because it forces authors to produce reviewable intermediate outputs rather than monolithic deliverables ([Progressive Disclosure — NN/g](https://www.nngroup.com/articles/progressive-disclosure/)). The same logic applies to agent output: reviewable intermediate output beats monolithic final output.

Context management research provides the underlying mechanism: Anthropic's Claude Code documentation confirms that "LLM performance degrades as context fills" and that "a single debugging session or codebase exploration might generate and consume tens of thousands of tokens" ([Claude Code Best Practices](https://code.claude.com/docs/en/best-practices)). An agent writing its output incrementally offloads cognitive state to disk, reducing the context pressure that leads to degraded output quality.

### 2.2 The canonical pattern: stub-first, fill-second

The canonical progressive-write pattern has three phases:

**Phase 1: Stub the structure (first 60 seconds of any task).** Create the output file immediately using the Write tool. The stub contains:
- Document header (title, status, date, scope summary)
- Table of contents with all planned sections
- Section headers in order
- `[STUB]` placeholder text under each header

This is not busywork. Stubbing the structure forces you to articulate the full scope of the document before you start writing content. It also means that if you are interrupted at any point, the output file contains at minimum the headers — which a recovery agent can use to understand what was planned.

**Phase 2: Fetch primary sources before filling content.** For any non-trivial claim, fetch the source before writing the content that depends on it. Write content that cites sources immediately — do not accumulate a mental list of "things to cite" and expect to add citations at the end. Context accumulation means that end-of-task citation passes are unreliable.

**Phase 3: Fill sections in order, editing the file after each section is complete.** Use the Edit tool to replace each `[STUB]` with real content as you complete it. Do not write multiple sections in memory and flush them together. Write one section, edit the file, move to the next. If you are interrupted after completing Section 2, Sections 1 and 2 are on disk.

**Example: this document.** The output file for this section (`sop_1_scope_and_writing.md`) was created as a stub containing all section headers within the first two minutes of the task, before any primary sources were fetched. This is the correct order of operations.

**Secondary application: code tasks.** For code-writing agents, the progressive-write equivalent is: create the target file (or stub function signatures) immediately, then fill in implementations, running the compiler or tests after each increment. Never accumulate 1,500 LOC in a single edit. Commit incrementally as subsections of the task are verified. Anthropic's multi-agent system documentation confirms this pattern: the recommendation is to "commit with a descriptive message" after each implementation phase, not at the end of all phases ([Claude Code Best Practices](https://code.claude.com/docs/en/best-practices)).

### 2.3 Anti-patterns to avoid

**Anti-pattern 1: The big-bang dump.** The agent builds the entire output in its working context across 20+ minutes of tool calls, then issues a single Write at the end. This is the highest-risk pattern. Any interruption before the final Write loses everything. Any context degradation during the 20-minute accumulation phase produces a degraded final output that cannot be reviewed incrementally. The big-bang dump is categorically prohibited.

**Anti-pattern 2: Splitting a single logical unit across multiple files.** If the task is to produce one document or one module, that work belongs in one file. Creating `sop_1a.md` and `sop_1b.md` when the deliverable is `sop_1.md` fragments the output in ways that make synthesis harder, not easier. Split files only when the task decomposition genuinely requires it — not as a way to avoid large single-file edits.

**Anti-pattern 3: Writing headers without content.** A document that contains only `[STUB]` placeholders is not progress; it is the appearance of progress. The stub phase must be completed quickly (under 60 seconds), and filling must begin immediately. Stubs that persist beyond their intended 60-second window indicate that the task is stalled, not progressing.

**Anti-pattern 4: Saving citation work for last.** Citations require fetching live URLs or reading primary sources. These fetches consume tool calls and context. Deferring all citation work to the end means a context-degraded agent attempting to fetch N sources in sequence at the end of a long task — the worst possible time. Fetch and cite as you go.

**Anti-pattern 5: Over-compacting early sections to save space.** As context fills, agents may be tempted to summarize or truncate early sections of their output to make room. If the output is being written progressively to disk, this is never necessary. The disk is not the context window. Write full content to disk; do not self-censor to conserve tokens.

---

## 3. Fail-Fast on Ambiguity

### 3.1 The 2-3 attempt rule

When an agent cannot determine the correct answer — the right API, the correct interpretation of a spec, the intended behavior of a stub — it must resolve the ambiguity from primary sources before proceeding. The resolution process has a hard limit: **two to three reasonable attempts**.

A "reasonable attempt" is one that consults a real source: reading the relevant file, fetching a URL, running a test, or reading a specification document. An attempt that consists only of re-reading already-read material, rephrasing the question internally, or reasoning from training data does not count.

After two to three attempts, one of three things is true:

1. You have found the answer. Proceed.
2. You have found partial evidence that supports a best-available answer. Make the call, document your reasoning, and flag it as a `[DECISION: <reasoning>]` marker so it can be reviewed.
3. You have found no useful evidence. Mark `[OPEN: <specific question>]` and move on. Do not fabricate.

The hard limit exists because context is finite. Each additional resolution attempt consumes context that could have been used to produce output. Research on agent mid-task degradation shows that models that "exhaust the inference limit without adapting to repeated errors" produce worse outcomes than models that fail-fast and escalate ([How Do LLMs Fail In Agentic Scenarios?](https://arxiv.org/pdf/2512.07497)).

**What counts as an OPEN marker.** An OPEN marker is a specific, answerable question — not a vague observation. Good: `[OPEN: Does cssl-smt's discharge() accept Z3 sort parameters as &Sort or OwnedSort? Check z3 crate API.]` Bad: `[OPEN: unclear how SMT works.]` The OPEN marker must be specific enough that the next agent, or a human reviewer, can resolve it without additional context about your task.

### 3.2 When to escalate vs make a call vs mark OPEN

These three responses to ambiguity are not interchangeable. Use the right one:

**Escalate** when the ambiguity is a decision that has project-level consequences — it will affect multiple crates, multiple agents, or the direction of the work in a way that a single agent cannot decide unilaterally. Examples: "Should F2's SMT backend use Z3 or CVC5?" "Does the audit finding about the Ed25519 bypass require a breaking API change?" These questions are architectural decisions that the orchestrator must answer. Write an escalation message: "Encountered decision point that requires orchestrator input: [question]. Options are A and B. Tradeoffs: [concise]. Blocked on this until resolved." Do not guess.

**Make a call** when the ambiguity is local — it affects only your current task, the tradeoffs are clear, and you can fully document your reasoning. Examples: "Should I use `Vec<u8>` or `Bytes` for this buffer?" "Should the OPEN marker use single or double brackets?" These are implementation details. Make the call, write `[DECISION: chose X because Y]`, and move on. Document enough that a reviewer can override your decision if they disagree.

**Mark OPEN** when the ambiguity is genuine — you do not have enough information to make a confident call, and the question is not architectural enough to escalate. OPEN markers are the correct output for all gaps you cannot resolve within the 2-3 attempt limit. They are not admissions of failure; they are features of honest reporting.

**The rule of thumb:** if the question affects people other than you, escalate. If the question affects only your current task and you have evidence for a call, decide. If you have no evidence after 2-3 attempts, mark OPEN.

### 3.3 The hesitation-handling pattern

Sometimes an agent is genuinely uncertain and the question does not fit neatly into escalate/decide/OPEN. The agent can see multiple valid approaches, each with real tradeoffs, and cannot determine which is better from available evidence alone. This is hesitation.

The correct output for hesitation is structured: present options, list tradeoffs, give a recommendation, and mark it as awaiting confirmation if the consequences are significant.

**Format:**

```
[DECISION-PENDING: <decision>]
  Option A: <description>
    Pro: <pro>
    Con: <con>
  Option B: <description>
    Pro: <pro>
    Con: <con>
  Recommendation: Option A, because <brief rationale>.
  [Awaiting confirmation if this affects <downstream concern>.]
```

This pattern serves two purposes. First, it makes the agent's reasoning visible, which allows a reviewer to override the recommendation with minimal context. Second, it prevents the agent from making a unilateral decision on a question that has real tradeoffs — the recommendation is a starting point, not a commitment.

Note that this pattern is distinct from the OPEN marker. OPEN means "I do not know and cannot determine this." DECISION-PENDING means "I have analyzed this and have a recommendation, but I am surfacing it for review." Use the right one.

Anthropic's multi-agent research documentation confirms that structured option presentation reduces downstream rework: vague subagent instructions without clear task boundaries led to duplicated work and misinterpretation, while explicit option frameworks produced actionable output ([How we built our multi-agent research system](https://www.anthropic.com/engineering/multi-agent-research-system)). The hesitation-handling pattern is the document-output equivalent of structured task framing.

For decisions that are purely local and low-consequence, the full DECISION-PENDING format is overkill. Use judgment: a one-sentence `[DECISION: X because Y]` is sufficient for low-stakes local choices.

---

## 4. No-Silent-Stubs Rule

### 4.1 The discipline defined

A silent stub is any placeholder, omission, or gap in an agent's output that is not explicitly marked as such. It is the practice of presenting incomplete work as if it were complete, or omitting hard parts without signaling that they were omitted.

Silent stubs are the most corrosive form of low-quality output in a multi-agent system. They are worse than overt failures because they are invisible. An agent that produces a clearly failing output — a panic, a compilation error, an explicit `[TODO]` — gives the next agent or human reviewer actionable information. An agent that produces a plausible-looking output with silent gaps gives the reviewer false confidence.

CSSLv3's own audit process surfaced this exact failure mode: `csslc` is a 23-line binary that prints two status lines and exits 0. It looks like a working compiler. It is a silent stub. The README described capabilities that did not exist — a form of documentation-level silent stub. The F2 SMT `discharge()` function compiled, passed any structural checks, and returned without doing SMT work — a code-level silent stub. All three cases were discovered only through a systematic audit, not through any failure signal at the interface.

The no-silent-stubs rule is: **every gap in your output must be explicitly marked**. If you did not implement something, write `[TODO: <action>]`. If you do not know something, write `[OPEN: <question>]`. If you made a decision under uncertainty, write `[DECISION: <reasoning>]`. If a claim requires verification you did not have time to do, write `[VERIFY: <claim>]`. The gap must be visible in the output at the location of the gap, not deferred to a summary section.

### 4.2 Audit-ability vs invisibility

The distinction between disciplined and shoddy work is not whether gaps exist — they always do — but whether the gaps are visible.

**Shoddy work** elides gaps. The author writes around what they do not know, producing prose that sounds authoritative but makes no commitments. The reader cannot tell what is verified and what is assumed. The reviewer has no foothold for questioning what was written. Six months later, someone discovers the gap by accident, often at high cost.

**Disciplined work** surfaces gaps at the point of occurrence. The author writes exactly as far as their knowledge or evidence extends, then marks the boundary. The reviewer can see precisely what was known, what was unknown, and what decisions were made. A follow-up agent can take the OPEN markers as a task list. The work is audit-able: a future reader can reconstruct the state of knowledge at the time of writing.

In a multi-agent system, audit-ability is not optional. Each agent produces output that the next agent depends on. If Agent 1 silently omits a critical detail, Agent 2 may make a decision based on the false assumption that the detail is resolved. The error propagates and compounds. Research on long-running agent memory management confirms this cascading failure pattern: "flawed or irrelevant memories are stored and reused," and iterative summarization causes "safety-critical details [to] progressively vanish" ([Memory for Autonomous LLM Agents](https://arxiv.org/html/2603.07670v1)). Silent stubs are the agentic equivalent of flawed memory: they contaminate the downstream state.

**Practical markers:**

| Situation | Correct marker |
|---|---|
| Feature not implemented | `[TODO: implement <feature>]` |
| Claim not verified against primary source | `[VERIFY: <claim> — check <source>]` |
| Question without a known answer | `[OPEN: <specific question>]` |
| Decision made under uncertainty | `[DECISION: chose X because Y — override if Z]` |
| Scope boundary deliberately not crossed | `[OUT-OF-SCOPE: <concern> — handled by <agent/section>]` |

### 4.3 Downstream readability principle

The audience for every piece of agent output is not only the immediate reviewer — it is any agent or human who reads the document six months from now, with no access to the original task context.

This downstream reader has no knowledge of which parts were difficult, which were skipped under time pressure, which represent best-available evidence vs. ground truth, or which were contested at the time of writing. They have only the document. The document must tell them.

This means:

**Markers must be at the point of the gap, not aggregated.** A summary section that lists all open questions is useful, but it is not a substitute for inline markers. A downstream reader scanning Section 3.2 needs to see the OPEN marker in Section 3.2, not hunt for it in an appendix.

**Markers must be specific enough to act on.** `[TODO: finish this section]` is not actionable. `[TODO: fill in the Z3 query construction logic for refinement obligations — see docs/audit/06-transform-smt-staging-futamura-macros.md §3.2 for the expected interface]` is actionable. The downstream agent can pick this up cold.

**The document's completion status must be visible at the top.** Every output file must carry a status line: DRAFT, COMPLETE, PARTIAL (with a summary of what is missing), or STUB. This is the first thing a downstream reader sees, and it sets expectations for everything that follows.

**Decisions must include their rationale.** A decision that records only its outcome — "we chose Z3" — is not useful to a downstream reader who needs to evaluate whether to revisit that decision. A decision that records its rationale — "chose Z3 because the Z3 crate has stable Rust bindings and was already in the dependency tree; CVC5 was rejected due to no Rust crate at time of writing [VERIFY: re-check 2026-06]" — gives the downstream reader what they need to evaluate, override, or confirm the decision.

The downstream readability principle is the long-horizon version of the no-silent-stubs rule. It shifts the question from "will this confuse the next agent?" to "will this be usable by anyone who reads it without context, ever?" Designing for the worst-case downstream reader — maximum context loss, maximum time elapsed — produces output that reliably survives the multi-agent pipeline.

---

## 5. Quick Reference: Rules at a Glance

This section is a one-page summary for agents who need to check a rule quickly. For rationale and examples, see the numbered sections above.

### Scope discipline

- Write your scope definition (what you produce / what you don't touch / success criterion / ground truth) to disk before starting work.
- Every tool call must serve the stated scope. If it doesn't, you are drifting.
- Scope changes from the orchestrator must be documented as amendments, not silently accepted.
- LOC budget: documentation agents ~300 LOC written; code agents ≤1,500 LOC delta.
- Time budget: documentation agents 5–8 min target, 12 min ceiling; code agents 8–15 min target, 20–30 min ceiling.
- Over budget? Split, escalate, or fail-fast. Never silently continue.

### Progressive write

- Create the output file within the first 60 seconds, even if it contains only stubs.
- Fetch primary sources before writing content that depends on them.
- Edit the file after each section is complete. Do not accumulate multiple sections and flush together.
- Never hold the entire output in context and write it at the end (the big-bang dump).
- One logical unit = one file. Do not split a single deliverable across multiple files.

### Fail-fast on ambiguity

- Resolve ambiguity from primary sources. Maximum 2-3 attempts.
- After 2-3 attempts: found the answer → proceed; partial evidence → DECISION marker; no evidence → OPEN marker.
- Escalate when the decision has project-level consequences (affects multiple agents, crates, or direction).
- Make a call when the decision is local and you have evidence.
- Mark OPEN when you have no evidence after 2-3 attempts.
- For genuine hesitation: present options + tradeoffs + recommendation using the DECISION-PENDING format.

### No-silent-stubs

| Situation | Marker |
|---|---|
| Not implemented | `[TODO: <action>]` |
| Unverified claim | `[VERIFY: <claim> — source: <X>]` |
| Unknown answer | `[OPEN: <specific question>]` |
| Decision under uncertainty | `[DECISION: X because Y]` |
| Deliberate scope boundary | `[OUT-OF-SCOPE: <concern>]` |
| Pending confirmation | `[DECISION-PENDING: options + tradeoffs + recommendation]` |

- Every gap must be marked at the point of the gap, not deferred to a summary.
- Markers must be specific enough for a cold downstream reader to act on.
- Document status (DRAFT / COMPLETE / PARTIAL / STUB) must appear at the top of every output file.
- Decisions must record their rationale, not just their outcome.

---

*Section 1 complete. Sections 2–4 are covered by parallel agents and will be synthesized by the orchestrator into `AGENT_SOP.md`.*

---

## Sources cited in this section

- [Why Do Multi-Agent LLM Systems Fail?](https://arxiv.org/html/2503.13657v1) — failure modes, quantitative completion rates
- [When Refusals Fail: Unstable Safety Mechanisms in Long-Context LLM Agents](https://arxiv.org/html/2512.02445v1) — performance degradation at context depth
- [How Do LLMs Fail In Agentic Scenarios?](https://arxiv.org/pdf/2512.07497) — mid-task degradation, over-helpfulness under uncertainty
- [Memory for Autonomous LLM Agents](https://arxiv.org/html/2603.07670v1) — summarization drift, cascading memory errors
- [How we built our multi-agent research system](https://www.anthropic.com/engineering/multi-agent-research-system) — effort scaling, subagent scope discipline
- [Building Effective AI Agents](https://www.anthropic.com/research/building-effective-agents) — task decomposition, fail-fast patterns
- [Claude Code Best Practices](https://code.claude.com/docs/en/best-practices) — context management, scope, common failure patterns
- [Progressive Disclosure — Nielsen Norman Group](https://www.nngroup.com/articles/progressive-disclosure/) — progressive disclosure in technical communication
