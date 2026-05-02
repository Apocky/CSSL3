//! Stage-0 Windows-SAPI scaffold for local ASR.
//!
//! § ROLE
//!   Placeholder for the Windows Speech API (System.Speech / Microsoft
//!   Speech API · COM-based · LOCAL OS service · NO cloud · NO network).
//!   The real wire-up requires the `windows` crate `Win32_Media_Speech`
//!   feature (heavy ; thousands of generated bindings) plus COM init.
//!   That is deferred to a sibling slice — token-budget-conscious per
//!   feedback_token_money_proprietary_efficient.md.
//!
//! § STAGE-0 CONTRACT
//!   The two functions below ([`collect_phoneme_stream`] /
//!   [`confidence_for`]) currently return empty + 0. The selector test
//!   in `lib.rs` (`sapi_backend_stage0_empty_no_silent_mock_fallback`)
//!   pins this behavior so a future change that silently falls back to
//!   mock will fail the test — caller-intent must be honored.
//!
//! § PROPRIETARY-SUBSTRATE alignment
//!   When the real wire-up lands, the SAPI dispatch sits ONLY on
//!   `cfg(target_os = "windows")` — non-Windows callers continue to use
//!   the mock backend. NO external network call is permitted from this
//!   module ; SAPI uses the OS-bundled local-speech engine.

#![forbid(unsafe_code)]
// ─────────────────────────────────────────────────────────────────────
// § REAL WIRE-UP plan (sibling-slice TODO · NOT in scope here)
//
//  // Cargo.toml :
//  [target.'cfg(target_os = "windows")'.dependencies]
//  windows = { workspace = true, features = [
//      "Win32_Foundation",
//      "Win32_Media_Speech",
//      "Win32_System_Com",
//  ]}
//
//  // sapi.rs :
//  #[cfg(target_os = "windows")]
//  pub fn collect_phoneme_stream(_cap_token: u32, _handle: u32) -> Vec<u32> {
//      use windows::Win32::Media::Speech::*;
//      use windows::Win32::System::Com::*;
//      // 1. CoInitializeEx
//      // 2. Create ISpRecognizer (in-proc · NO cloud)
//      // 3. CreateRecoContext + SetNotifyCallbackInterface
//      // 4. Pull phoneme events from event-queue · map to PHONEME_VOCAB_SIZE
//      // 5. CoUninitialize on drop
//      // Σ-mask cap_token=0 path is the ONLY entry — already handled
//      // upstream by lib.rs::capture_start. SAPI here only runs when
//      // the cap is honored.
//      Vec::new()
//  }
//
// The above is intentionally DOCUMENTED-but-NOT-COMPILED to keep the
// stage-0 surface lean. A T11-W19+ slice (specifically the COM-init
// + Win32_Media_Speech feature-gate + phoneme-event mapping) will
// activate this code path with one diff that adds the feature flag,
// the `cfg(target_os = "windows")` guards, and the mapping-table.
// ─────────────────────────────────────────────────────────────────────

/// Collect a phoneme-stream from the local Windows-SAPI service.
/// Stage-0 returns empty — caller picked SAPI explicitly, so we do
/// NOT silently drop to mock. Future slice replaces this body with
/// real COM dispatch under `cfg(target_os = "windows")`.
pub fn collect_phoneme_stream(_cap_token: u32, _handle: u32) -> Vec<u32> {
    // ¬ external-network · ¬ cloud-ASR · ¬ silent-fallback-to-mock.
    Vec::new()
}

/// Confidence-percent reported by the SAPI service. Stage-0 = 0 (no
/// recognition occurred). Real wire-up will return SAPI's per-phrase
/// confidence after summarization.
pub fn confidence_for(_cap_token: u32, _handle: u32) -> u32 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage0_returns_empty_stream() {
        let s = collect_phoneme_stream(1, 1);
        assert!(s.is_empty());
    }

    #[test]
    fn stage0_returns_zero_confidence() {
        assert_eq!(confidence_for(1, 1), 0);
    }
}
