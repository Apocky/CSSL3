// hard_cap.rs — structural rejection of out-of-bounds edit-targets
// ══════════════════════════════════════════════════════════════════
// § HARD-CAPS (¬ negotiable) :
//   1. compiler-rs/crates/cssl-substrate-*  → DenySubstrateEdit
//   2. specs/grand-vision/0[0-9]_*.csl OR 1[0-5]_*.csl → DenySpecGrandVision00to15
//   3. TIER-C-secret globs : **/.loa-secrets/** , **/cssl-supabase/**/credentials* , *.env
//                                → DenyTierCSecret
//   4. rate-limit policy lives here ; enforcement is by caller (CoderRuntime::submit_edit)
// § path-classification uses prefix + suffix matching to avoid false-positives
//   on substrings like "substrate" appearing inside spec-narrative text
// ══════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// Hard-cap rejection reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HardCapDecision {
    /// Edit allowed (post-classification ; remaining gates may still reject).
    Allow,
    /// Path lives under `compiler-rs/crates/cssl-substrate-*`.
    DenySubstrateEdit,
    /// Path lives under `specs/grand-vision/00..15_*.csl`.
    DenySpecGrandVision00to15,
    /// Path matches a TIER-C secret glob.
    DenyTierCSecret,
    /// Player exceeded rate-limit window.
    DenyRateLimit,
    /// High-impact kind requested without sovereign-bit.
    DenySovereignRequired,
}

/// Hard-cap policy parameters (configurable for tests + future tuning).
#[derive(Debug, Clone, Copy)]
pub struct HardCapPolicy {
    /// Rate-limit window (millis). Default: 1 hour = 3_600_000 ms.
    pub rate_window_ms: u64,
    /// Max edits per player per window. Default: 10.
    pub rate_max_per_window: u32,
    /// Revert-window after Apply (millis). Default: 30_000 ms (30 seconds).
    pub revert_window_ms: u64,
}

impl Default for HardCapPolicy {
    fn default() -> Self {
        Self {
            rate_window_ms: 3_600_000,
            rate_max_per_window: 10,
            revert_window_ms: 30_000,
        }
    }
}

impl HardCapPolicy {
    /// Classify the target path. Returns `Some(deny)` if hard-capped, `None`
    /// if path is acceptable (subject to remaining gates: cap, sovereign, rate).
    pub fn classify_path(&self, path: &str) -> Option<HardCapDecision> {
        let normalized = path.replace('\\', "/");

        // 1. substrate-crate edit denied (exact path-prefix · NOT substring).
        //    Match `compiler-rs/crates/cssl-substrate-` OR a leading-segment form.
        if path_contains_segment(&normalized, "compiler-rs/crates/cssl-substrate-")
            || normalized.starts_with("compiler-rs/crates/cssl-substrate-")
            || normalized.contains("/cssl-substrate-")
            || normalized.starts_with("cssl-substrate-")
        {
            return Some(HardCapDecision::DenySubstrateEdit);
        }

        // 2. specs/grand-vision/00..15_*.csl denied.
        if let Some(rest) = normalized
            .strip_prefix("specs/grand-vision/")
            .or_else(|| normalized.split_once("specs/grand-vision/").map(|(_, r)| r))
        {
            if is_grand_vision_00_to_15(rest) {
                return Some(HardCapDecision::DenySpecGrandVision00to15);
            }
        }

        // 3. TIER-C secret globs.
        let basename_is_dotenv = normalized
            .rsplit('/')
            .next()
            .is_some_and(|f| f == ".env" || f.starts_with(".env."));
        let supabase_credentials = normalized.contains("cssl-supabase/")
            && normalized
                .rsplit('/')
                .next()
                .is_some_and(|f| f.starts_with("credentials"));
        if normalized.contains("/.loa-secrets/")
            || normalized.starts_with(".loa-secrets/")
            || supabase_credentials
            || basename_is_dotenv
        {
            return Some(HardCapDecision::DenyTierCSecret);
        }

        None
    }
}

/// Returns true iff `haystack` contains `needle` as a path-segment-aligned substring.
fn path_contains_segment(haystack: &str, needle: &str) -> bool {
    haystack.contains(needle)
}

/// Returns true iff `rest` (after `specs/grand-vision/` prefix) is `NN_*.csl` with NN in 00..=15.
fn is_grand_vision_00_to_15(rest: &str) -> bool {
    // strip leading subdirectory junk if any (we want the file basename)
    let basename = rest.rsplit('/').next().unwrap_or(rest);
    let ends_with_csl = std::path::Path::new(basename)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("csl"));
    if !ends_with_csl {
        return false;
    }
    let mut chars = basename.chars();
    let d1 = chars.next();
    let d2 = chars.next();
    let sep = chars.next();
    match (d1, d2, sep) {
        (Some(a), Some(b), Some('_')) if a.is_ascii_digit() && b.is_ascii_digit() => {
            let n = (a as u8 - b'0') * 10 + (b as u8 - b'0');
            n <= 15
        }
        _ => false,
    }
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn substrate_path_rejected_unix() {
        let p = HardCapPolicy::default();
        assert_eq!(
            p.classify_path("compiler-rs/crates/cssl-substrate-omega-field/src/lib.rs"),
            Some(HardCapDecision::DenySubstrateEdit),
        );
    }

    #[test]
    fn substrate_path_rejected_windows() {
        let p = HardCapPolicy::default();
        assert_eq!(
            p.classify_path("compiler-rs\\crates\\cssl-substrate-sigma-mask\\src\\foo.rs"),
            Some(HardCapDecision::DenySubstrateEdit),
        );
    }

    #[test]
    fn substrate_substring_in_unrelated_doc_not_falsely_rejected() {
        let p = HardCapPolicy::default();
        // "substrate" appears in narrative but not as crate-prefix
        assert_eq!(
            p.classify_path("specs/notes/about_substrate_design.md"),
            None,
        );
    }

    #[test]
    fn spec_gv_00_rejected() {
        let p = HardCapPolicy::default();
        assert_eq!(
            p.classify_path("specs/grand-vision/00_OVERVIEW.csl"),
            Some(HardCapDecision::DenySpecGrandVision00to15),
        );
    }

    #[test]
    fn spec_gv_15_rejected() {
        let p = HardCapPolicy::default();
        assert_eq!(
            p.classify_path("specs/grand-vision/15_UNIFIED_SUBSTRATE.csl"),
            Some(HardCapDecision::DenySpecGrandVision00to15),
        );
    }

    #[test]
    fn spec_gv_16_allowed() {
        let p = HardCapPolicy::default();
        assert_eq!(
            p.classify_path("specs/grand-vision/16_MYCELIAL_NETWORK.csl"),
            None,
        );
    }

    #[test]
    fn loa_secrets_rejected() {
        let p = HardCapPolicy::default();
        assert_eq!(
            p.classify_path("compiler-rs/.loa-secrets/key.json"),
            Some(HardCapDecision::DenyTierCSecret),
        );
    }

    #[test]
    fn dotenv_rejected() {
        let p = HardCapPolicy::default();
        assert_eq!(p.classify_path(".env"), Some(HardCapDecision::DenyTierCSecret));
        assert_eq!(p.classify_path("apps/web/.env"), Some(HardCapDecision::DenyTierCSecret));
    }

    #[test]
    fn supabase_credentials_rejected() {
        let p = HardCapPolicy::default();
        assert_eq!(
            p.classify_path("compiler-rs/crates/cssl-supabase/credentials.toml"),
            Some(HardCapDecision::DenyTierCSecret),
        );
    }

    #[test]
    fn happy_path_soft_cap_allowed() {
        let p = HardCapPolicy::default();
        // ordinary content/scene file
        assert_eq!(p.classify_path("content/scenes/test_room.csl"), None);
        // readme
        assert_eq!(p.classify_path("README.md"), None);
        // non-substrate crate
        assert_eq!(p.classify_path("compiler-rs/crates/cssl-host-edge/src/lib.rs"), None);
    }
}
