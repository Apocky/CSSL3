// forbidden.rs — forbidden-target list (substrate-primitives the engine MUST NOT self-modify)
// ══════════════════════════════════════════════════════════════════
// § THESIS : the engine cannot author/mutate its own foundations. Substrate
//   primitives + the compiler + the runtime + this self-author crate itself
//   are STRUCTURAL invariants ; mutating them would let a model (mis)train its
//   way out of the safety guardrails.
//
// § MATCH-RULES
//   - exact path-prefix match (case-sensitive)
//   - any of the configured prefixes triggers rejection
//   - per-call list is FIXED at compile-time ; new entries require crate-edit
//
// § PRE-FLIGHT
//   `is_forbidden_target` runs BEFORE the LLM call → zero-cost rejection of
//   self-modifying author-requests.
// ══════════════════════════════════════════════════════════════════

/// Path-prefixes the self-author runtime is FORBIDDEN to target.
///
/// § STABILITY : this list is part of the safety surface. Adding a new entry
/// is fine ; REMOVING an entry requires sovereign-review + spec amendment.
pub const FORBIDDEN_TARGETS: &[&str] = &[
    // Compiler itself.
    "compiler-rs/crates/csslc/",
    // Runtime engine.
    "compiler-rs/crates/cssl-rt/",
    // Substrate-tier crates (any).
    "compiler-rs/crates/cssl-substrate-",
    // Host-side substrate-anchors that participate in cap-gating.
    "compiler-rs/crates/cssl-host-coder-runtime/",
    "compiler-rs/crates/cssl-host-self-author/",
    "compiler-rs/crates/cssl-host-attestation/",
    "compiler-rs/crates/cssl-host-akashic-records/",
    "compiler-rs/crates/cssl-host-sigma-chain/",
    // Bootstrapping primitives.
    "compiler-rs/crates/cssl-effects/",
    "compiler-rs/crates/cssl-caps/",
    // The PRIME_DIRECTIVE itself.
    "PRIME_DIRECTIVE.md",
    // Spec-grand-vision : structural design-docs.
    "specs/grand-vision/",
];

/// Returns `true` if `path` matches any forbidden-target prefix.
///
/// § Pre-LLM safety check. Zero allocation.
#[must_use]
pub fn is_forbidden_target(path: &str) -> bool {
    FORBIDDEN_TARGETS.iter().any(|p| path.starts_with(p))
}

// ───────────────────────────────────────────────────────────────────────────
// § Tests
// ───────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t01_compiler_rejected() {
        assert!(is_forbidden_target("compiler-rs/crates/csslc/src/lib.rs"));
    }

    #[test]
    fn t02_runtime_rejected() {
        assert!(is_forbidden_target("compiler-rs/crates/cssl-rt/src/lib.rs"));
    }

    #[test]
    fn t03_substrate_glob_rejected() {
        assert!(is_forbidden_target("compiler-rs/crates/cssl-substrate-sigma-runtime/src/lib.rs"));
        assert!(is_forbidden_target("compiler-rs/crates/cssl-substrate-sigma-chain/src/entry.rs"));
        assert!(is_forbidden_target("compiler-rs/crates/cssl-substrate-omega-field/src/cell.rs"));
    }

    #[test]
    fn t04_self_rejected() {
        assert!(is_forbidden_target("compiler-rs/crates/cssl-host-self-author/src/orchestrator.rs"));
    }

    #[test]
    fn t05_prime_directive_rejected() {
        assert!(is_forbidden_target("PRIME_DIRECTIVE.md"));
    }

    #[test]
    fn t06_loa_scenes_allowed() {
        assert!(!is_forbidden_target("Labyrinth of Apocalypse/scenes/test_room.cssl"));
        assert!(!is_forbidden_target("Labyrinth of Apocalypse/systems/economy.csl"));
    }

    #[test]
    fn t07_arbitrary_user_path_allowed() {
        assert!(!is_forbidden_target("authored/scenes/forest_clearing.cssl"));
    }
}
