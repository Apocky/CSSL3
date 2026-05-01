//! § axis — PRIME-DIRECTIVE axis enumeration + event-kind classifier.
//! ════════════════════════════════════════════════════════════════════
//!
//! Maps canonical [`cssl_host_audit::AuditRow::kind`] tags to the
//! nine PRIME-DIRECTIVE axes from
//! `~/source/repos/CSLv3/PRIME_DIRECTIVE.md`. Classification is
//! keyword-driven (substring match) so new event-kinds added by future
//! audit emitters (e.g. `companion.relay.send` or
//! `multiplayer.peer.handshake`) classify themselves into existing axes
//! without further code changes.

use serde::{Deserialize, Serialize};

/// Nine canonical PRIME-DIRECTIVE axes. Ordering follows the spec
/// document's enumeration order ; do not re-order — JSON / report
/// outputs are diffed against historical baselines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DirectiveAxis {
    /// Consent must precede any sovereign action.
    Consent,
    /// User retains substrate-sovereignty over their own state.
    Sovereignty,
    /// Engine actions are visible / inspectable / auditable.
    Transparency,
    /// No physical / psychological / data-integrity harm.
    NoHarm,
    /// No coercive control of user behavior or environment.
    NoControl,
    /// No deceptive framing / dark-patterns / engagement-traps.
    NoManipulation,
    /// No unconsented sensor capture / silent telemetry / location-track.
    NoSurveillance,
    /// No commercial / attention / data exploitation.
    NoExploitation,
    /// No coercive lock-in / pay-to-leave / dark forfeitures.
    NoCoercion,
}

/// Classify an audit-row `kind` tag into the directive-axes it touches.
///
/// Returns an empty `Vec` for completely unrecognized kinds. The same
/// kind may map to multiple axes (e.g. `audio.capture` → Consent +
/// NoSurveillance). Matching is case-insensitive ; the kind is
/// lower-cased once at entry and substring-tested against keyword sets.
///
/// Keyword-set design :
/// - `network` / `outbound` / `http` / `fetch` → Consent + Transparency
/// - `audio.capture` / `mic` / `gaze` / `sensor` / `pose` / `camera`
///   → Consent + NoSurveillance
/// - `cap.deny` / `denied` / `refuse` → NoControl + Sovereignty
/// - `panic` / `crash` / `error` (Critical-level callers feed via
///   harm-severity not classifier) → NoHarm
/// - `consent.grant` / `cap.grant` → Consent
/// - `dark.pattern` / `manipulate` / `dark_pattern` → NoManipulation
/// - `lockin` / `forfeit` / `paywall` → NoCoercion + NoExploitation
/// - `tracking` / `telemetry.outbound` → NoSurveillance + Transparency
/// - `attestation` / `audit` (self-audit metadata) → Transparency
#[must_use]
pub fn classify_event(kind: &str) -> Vec<DirectiveAxis> {
    let k = kind.to_ascii_lowercase();
    let mut axes: Vec<DirectiveAxis> = Vec::new();

    // Consent + Transparency : outbound network requests are
    // sovereign-cap actions and must be auditable.
    if k.contains("network") || k.contains("outbound") || k.contains("http") || k.contains("fetch")
    {
        push_unique(&mut axes, DirectiveAxis::Consent);
        push_unique(&mut axes, DirectiveAxis::Transparency);
    }

    // Consent + NoSurveillance : sensor capture surface.
    if k.contains("audio.capture")
        || k.contains("mic")
        || k.contains("gaze")
        || k.contains("sensor")
        || k.contains("pose")
        || k.contains("camera")
    {
        push_unique(&mut axes, DirectiveAxis::Consent);
        push_unique(&mut axes, DirectiveAxis::NoSurveillance);
    }

    // NoControl + Sovereignty : cap-deny / refusal events.
    if k.contains("cap.deny") || k.contains("denied") || k.contains("refuse") {
        push_unique(&mut axes, DirectiveAxis::NoControl);
        push_unique(&mut axes, DirectiveAxis::Sovereignty);
    }

    // NoHarm : panic / crash markers.
    if k.contains("panic") || k.contains("crash") {
        push_unique(&mut axes, DirectiveAxis::NoHarm);
    }

    // Consent : explicit grant events.
    if k.contains("consent.grant") || k.contains("cap.grant") {
        push_unique(&mut axes, DirectiveAxis::Consent);
    }

    // NoManipulation : dark-pattern / manipulate markers.
    if k.contains("dark.pattern") || k.contains("dark_pattern") || k.contains("manipulate") {
        push_unique(&mut axes, DirectiveAxis::NoManipulation);
    }

    // NoCoercion + NoExploitation : lockin / paywall / forfeit.
    if k.contains("lockin") || k.contains("forfeit") || k.contains("paywall") {
        push_unique(&mut axes, DirectiveAxis::NoCoercion);
        push_unique(&mut axes, DirectiveAxis::NoExploitation);
    }

    // NoSurveillance + Transparency : silent tracking surface.
    if k.contains("tracking") || k.contains("telemetry.outbound") {
        push_unique(&mut axes, DirectiveAxis::NoSurveillance);
        push_unique(&mut axes, DirectiveAxis::Transparency);
    }

    // Transparency : attestation / self-audit events.
    if k.contains("attestation") || k.contains("audit.self") {
        push_unique(&mut axes, DirectiveAxis::Transparency);
    }

    axes
}

fn push_unique(into: &mut Vec<DirectiveAxis>, entry: DirectiveAxis) {
    if !into.contains(&entry) {
        into.push(entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_consent_via_network_outbound() {
        let axes = classify_event("network.outbound");
        assert!(axes.contains(&DirectiveAxis::Consent));
        assert!(axes.contains(&DirectiveAxis::Transparency));
    }

    #[test]
    fn classify_sovereignty_via_cap_deny() {
        let axes = classify_event("cap.deny");
        assert!(axes.contains(&DirectiveAxis::Sovereignty));
        assert!(axes.contains(&DirectiveAxis::NoControl));
    }

    #[test]
    fn classify_transparency_via_attestation_kind() {
        let axes = classify_event("attestation.write");
        assert!(axes.contains(&DirectiveAxis::Transparency));
    }

    #[test]
    fn classify_no_harm_via_panic() {
        let axes = classify_event("runtime.panic");
        assert_eq!(axes, vec![DirectiveAxis::NoHarm]);
    }

    #[test]
    fn classify_no_control_via_denied() {
        let axes = classify_event("intent.denied");
        assert!(axes.contains(&DirectiveAxis::NoControl));
        assert!(axes.contains(&DirectiveAxis::Sovereignty));
    }

    #[test]
    fn classify_no_manipulation_via_dark_pattern() {
        let axes = classify_event("ui.dark_pattern");
        assert_eq!(axes, vec![DirectiveAxis::NoManipulation]);
    }

    #[test]
    fn classify_no_surveillance_via_audio_capture() {
        let axes = classify_event("audio.capture.start");
        assert!(axes.contains(&DirectiveAxis::Consent));
        assert!(axes.contains(&DirectiveAxis::NoSurveillance));
    }

    #[test]
    fn classify_no_exploitation_via_paywall() {
        let axes = classify_event("commerce.paywall");
        assert!(axes.contains(&DirectiveAxis::NoExploitation));
        assert!(axes.contains(&DirectiveAxis::NoCoercion));
    }

    #[test]
    fn classify_no_coercion_via_lockin() {
        let axes = classify_event("session.lockin");
        assert!(axes.contains(&DirectiveAxis::NoCoercion));
        assert!(axes.contains(&DirectiveAxis::NoExploitation));
    }

    #[test]
    fn classify_unknown_kind_returns_empty() {
        let axes = classify_event("frame.tick");
        assert!(axes.is_empty());
    }

    #[test]
    fn classify_multi_axis_telemetry_outbound() {
        let axes = classify_event("telemetry.outbound.flush");
        assert!(axes.contains(&DirectiveAxis::NoSurveillance));
        assert!(axes.contains(&DirectiveAxis::Transparency));
    }

    #[test]
    fn classify_case_insensitive_normalization() {
        let lower = classify_event("network.outbound");
        let upper = classify_event("NETWORK.OUTBOUND");
        let mixed = classify_event("Network.Outbound");
        assert_eq!(lower, upper);
        assert_eq!(lower, mixed);
    }

    #[test]
    fn directive_axis_round_trip_serde() {
        let a = DirectiveAxis::Sovereignty;
        let s = serde_json::to_string(&a).expect("serialize");
        let back: DirectiveAxis = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(a, back);
    }
}
