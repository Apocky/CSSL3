# cssl-host-coder-runtime — security invariants

**EXTREME-CAUTION crate.** The Coder is an automated-mutation-agent
(per `spec/10` § ROLE-CODER). All hard-caps below are **structural** and
**non-negotiable** — they are enforced in code, not policy.

## § PRIME-DIRECTIVE anchor

`~/source/repos/CSLv3/PRIME_DIRECTIVE.md` — consent + sovereignty +
transparency · ¬ harm. The Coder is a narrow orchestrator. It is **not**
a generic AGI. It refuses out-of-scope requests structurally rather than
relying on judgement.

## § Hard-caps (structural · ¬ negotiable)

| # | Rule                                                                 | Enforced in            |
|---|----------------------------------------------------------------------|------------------------|
| 1 | `compiler-rs/crates/cssl-substrate-*` paths rejected                 | `hard_cap::classify_path` |
| 2 | `specs/grand-vision/0[0-9]_*.csl` and `1[0-5]_*.csl` rejected        | `hard_cap::classify_path` |
| 3 | TIER-C secret globs (`.env`, `.loa-secrets/**`, `cssl-supabase/credentials*`) rejected | `hard_cap::classify_path` |
| 4 | Per-player rate-limit (default 10 edits / hour)                      | `CoderRuntime::submit_edit` |
| 5 | `AstNodeReplace`/`Insert`/`Delete`/`NarrowReshape` require `SovereignBit::Held` | `EditKind::requires_sovereign` |
| 6 | `CoderCap::AST_EDIT` minimum required                                | `CoderRuntime::submit_edit` |
| 7 | Apply ONLY from `EditState::Approved`                                | `CoderRuntime::apply` |
| 8 | 30-second revert window armed on every Apply                         | `revert::RevertWindow` |
| 9 | Audit-emit on **every** state-transition + every hard-cap rejection  | `audit::AuditLog`     |
| 10 | Sandbox NEVER touches the real file before `EditState::Applied`     | `sandbox::SandboxStore` |

## § State machine

```
Draft → Staged → ValidationPending → ValidationPassed
                       │                    │
                       └─── (Fail) ─────────┴──→ Rejected
                                            │
                                            └→ ApprovalPending
                                                     │
                                                  (Approved) → Approved → Applied
                                                                              │
                                                                              ├──→ AutoReverted (≤ 30s)
                                                                              ├──→ ManualReverted (≤ 30s)
                                                                              └──→ Permanent (> 30s)
```

Apply is the **only** entry-point that may write to the real file (via the
caller-supplied writer). Approval is **never** auto-granted: timeout =
Denied (fail-safe).

## § Audit directive-axis

Per PRIME_DIRECTIVE § 4 TRANSPARENCY, every event carries the directive-axis
`"ImplementationTransparency"`. Real deployment forwards events to
`cssl-host-attestation` for BLAKE3-chain + Ed25519-sign. This crate ships an
in-memory mock so the substrate-attestation crate can drop in without API
churn.

## § Attestation

Per PRIME_DIRECTIVE § 11 — there was no hurt nor harm in the making of this,
to anyone, anything, or anybody.
