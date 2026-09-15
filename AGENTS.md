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
