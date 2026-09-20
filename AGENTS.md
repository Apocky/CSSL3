<!-- METHOD-KERNEL-LOADLINE -->
## I> METHOD  !!  <- read this FIRST, before any substantive work

    canonical : C:\Users\Apocky\source\repos\METHOD.md
    shape     : ten primitives + five loops. COMPOSE them. Your situation is not in a list,
                and it is not supposed to be -- that is where the coverage comes from.
    field     : name the discipline the task belongs to and work to THAT field's bar,
                switching as the task moves. Meta-human means holding more of the field at
                once, not inventing a private standard.
    FIRST MOVE, every new prompt, before you plan or answer:
                PARK the work already in flight. Append to UNFINISHED_WORK.md at the root
                of what you are working in -- WHAT is in flight, HOW to resume it (a handle:
                run id, branch, file:line, command -- not a memory), and the NEXT concrete
                step. "Nothing in flight" is a valid entry and you write it explicitly.
                Append, never overwrite; close items in place with the date and evidence.
                A new instruction arriving does not mean the old work finished.
    notation  : think and reason in on-disk CSLv3 (ASCII profile). Explain and ask in
                simple, plain, concise English. The split is by AUDIENCE, never by topic.
    if nothing else : LOOK at the thing. Say the denominator. Break the check before you
                trust it. Record what you did NOT measure.

    PRIME_DIRECTIVE.md outranks it. VERIFICATION_KERNEL.md supplies the gates it composes with.
<!-- /METHOD-KERNEL-LOADLINE -->

## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.

Rules:
- For codebase questions, first run `graphify query "<question>"` when graphify-out/graph.json exists. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost).


§ APOCKY-COMMIT-PUSH-2026-09-05
  scope := goals/tasks with authorized owned file changes; read-only/no-change completion keeps outcome evidence; no empty commits
  canonical.contract := ~/source/repos/CSLv3/specs/governance/2026-09-05-commit-push/COMPLETION_CONTRACT.csl
  W! integration.goal.done := outcome.verified ∧ (authorized.owned.file.delta.exists ⇒ committed ∧ intended.remote.pushed ∧ remote.commit+tree.readback.verified)
  W! root.integrates.verified.agent.slices ; preserve.foreign.work + secrets + prior.protections
  W! existing.local-only.surfaces → scoped.authorized.delta in established.backing.repo
  W! absent.remote/auth | rejected.push → explicit.remaining.delivery.blocker ; N! complete.claim
  W! goal.objectives+statuses+budgets unchanged ; inactive.goals remain.inactive
  W! record repo+branch+commit+remote.ref+remote.commit/tree+verification.evidence
∎
