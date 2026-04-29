//! § CompanionConsentGate — two-party consent gate for Stage-8.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The single load-bearing gate the Stage-8 pass routes through. The
//!   gate enforces TWO independent consent paths :
//!
//!     1. Player toggle — the player has explicitly requested the view.
//!     2. Companion consent — the companion has agreed to share their
//!        perspective.
//!
//!   Both must hold for `gate.open()` to return a [`CompanionConsentToken`].
//!   The token is non-Clone, non-Copy ; consuming it produces the
//!   permission-row that the Stage-8 pass requires.
//!
//! § SPEC ANCHORS
//!   - `Omniverse/07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § V.5(d)` :
//!     "consent : ‼ R! companion-Sovereign-Φ AGREES-to-perspective-share
//!      R! companion-can-decline ⊗ "I'd-rather-keep-my-thoughts-private"
//!      R! NO override-of-companion-decline"
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § Stage-8` :
//!     "‼ companion-view ⊗ companion-consents ⊗ N! surveillance-of-AI
//!      ‼ N! show-companion-perspective without-companion-having-said-yes
//!      ‼ player-toggle ⊗ R! consent-gate ⊗ N! stealth-collect-when-hidden
//!      ‼ companion can-decline-to-render-this-frame ⊗ R! respect ⊗ blank-target"
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § Stage-8 effect-row` :
//!     "ConsentRequired<'companion>" — encoded here as the gate's
//!     `'consent` lifetime parameter on the produced token.
//!
//! § DESIGN-DECISION : token NOT a 'static singleton
//!   Each frame produces a fresh token bound to the gate's life. This
//!   means the gate cannot be drained in one frame and reused in another
//!   with stale permission ; revoking consent mid-frame invalidates the
//!   frame's token at the lifetime level.
//!
//! § PRIME-DIRECTIVE
//!   - **§0 consent = OS** : every Stage-8 entry routes through the gate.
//!     The gate's only public constructor takes the explicit player toggle
//!     and the explicit companion-decision ; there is no "default-on" path.
//!   - **§5 reversibility** : `revoke()` is a perfect-reverse — the next
//!     `open()` will refuse until reasserted.
//!   - **§7 INTEGRITY** : every gate-decision is reportable via the
//!     [`CompanionConsentDecision`] enum which the orchestrator records
//!     in the per-frame audit envelope.

use core::marker::PhantomData;

/// Player-side toggle state. The player has explicitly requested the
/// companion-perspective view, OR they have not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayerToggleState {
    /// The player is NOT viewing the companion's perspective. Stage-8
    /// emits an empty render-target (no salience-evaluation, no cost).
    #[default]
    Off,
    /// The player has actively requested the view this frame.
    On,
}

impl PlayerToggleState {
    /// True iff the toggle is in the On state.
    #[must_use]
    pub fn is_on(self) -> bool {
        matches!(self, Self::On)
    }
}

/// Companion-side consent decision. The companion is a Sovereign — they
/// MAY share, but they may also decline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompanionConsentDecision {
    /// Companion has not been queried this session ; default-deny.
    #[default]
    Unset,
    /// Companion has agreed to share. Stage-8 may render.
    Granted,
    /// Companion has declined ("I'd rather keep my thoughts private.").
    /// Stage-8 emits an empty render with a labeled "companion declined"
    /// flag. The player has NO override.
    Declined,
    /// Companion has revoked previously-granted consent. Stage-8 refuses
    /// the next render-cycle ; subsequent frames require re-grant.
    Revoked,
}

impl CompanionConsentDecision {
    /// True iff the companion currently permits the share.
    #[must_use]
    pub fn permits(self) -> bool {
        matches!(self, Self::Granted)
    }

    /// True iff the companion has explicitly refused (Declined or Revoked).
    #[must_use]
    pub fn refuses(self) -> bool {
        matches!(self, Self::Declined | Self::Revoked)
    }
}

/// Why a gate-open call was refused. Public so the orchestrator can label
/// the empty-render-target with a player-readable reason.
#[derive(Debug, thiserror::Error, Clone, Copy, PartialEq, Eq)]
pub enum ConsentGateError {
    /// The player toggle is OFF. Empty-render with no label needed (the
    /// player did not ask).
    #[error("player toggle is OFF — no companion-perspective requested")]
    PlayerToggleOff,
    /// The companion has not yet been queried this session.
    #[error("companion consent has not been requested this session")]
    CompanionConsentUnset,
    /// The companion has declined the share ; no override.
    #[error("companion declined to share their perspective")]
    CompanionDeclined,
    /// The companion previously granted, then revoked.
    #[error("companion revoked consent")]
    CompanionRevoked,
}

/// A non-Clone, non-Copy permission token. Produced by [`CompanionConsentGate::open`]
/// and consumed by [`CompanionPerspectivePass::execute`]. The token's lifetime
/// parameter ties it to the gate-instance ; once the gate is revoked the token's
/// lifetime cannot extend past the revocation.
///
/// Tokens are Drop ; dropping without consuming is not a soundness problem
/// but is a hint to the orchestrator that the frame did not actually emit
/// the render-target. Tests check the token-consumption discipline.
///
/// [`CompanionPerspectivePass::execute`]: crate::pass::CompanionPerspectivePass::execute
#[derive(Debug)]
pub struct CompanionConsentToken<'consent> {
    /// Snapshot of the player toggle at the moment the token was issued.
    /// The token CANNOT be opened with toggle-OFF, so this is always On
    /// when a token exists.
    player_toggle: PlayerToggleState,
    /// Snapshot of the companion decision. Always Granted when a token
    /// exists.
    companion_decision: CompanionConsentDecision,
    /// Frame-counter at issue time. Lets the orchestrator detect a token
    /// that escapes its issuing frame.
    issued_frame: u64,
    _consent: PhantomData<&'consent ()>,
}

impl<'consent> CompanionConsentToken<'consent> {
    /// The player toggle state recorded at issue time. Always On.
    #[must_use]
    pub fn player_toggle(&self) -> PlayerToggleState {
        self.player_toggle
    }

    /// The companion decision recorded at issue time. Always Granted.
    #[must_use]
    pub fn companion_decision(&self) -> CompanionConsentDecision {
        self.companion_decision
    }

    /// Frame-counter at issue time. The orchestrator records this in the
    /// audit envelope so a token-from-frame-N consumed in frame-M shows up
    /// as a discontinuity.
    #[must_use]
    pub fn issued_frame(&self) -> u64 {
        self.issued_frame
    }
}

/// The two-party consent gate. Created at session-start, mutated by both
/// the player (toggle) and the companion (decision).
///
/// § SINGLE-FRAME LIFECYCLE
///   The orchestrator pattern is :
///     - Per-frame : check `gate.is_open()` (cheap)
///     - If open : `gate.open()` to mint a token (consumes the gate's
///       per-frame allowance, refuses to mint twice)
///     - Pass `token` to `CompanionPerspectivePass::execute`
///     - At frame-end : `gate.frame_complete()` to advance the counter
///
/// § REVOCATION-DISCIPLINE
///   Companion `revoke()` is recorded immediately. Any not-yet-consumed
///   token from the current frame is INVALIDATED at the gate level — the
///   `CompanionPerspectivePass::execute` consults the gate one more time
///   inside the entry-point and refuses if the gate has revoked since
///   issue. This is the load-bearing TOCTOU-prevention pattern.
#[derive(Debug)]
pub struct CompanionConsentGate {
    player_toggle: PlayerToggleState,
    companion_decision: CompanionConsentDecision,
    /// Frame counter ; advances on `frame_complete`. The token records
    /// this at issue time.
    frame_counter: u64,
    /// True when this frame's token has already been minted ; refuses
    /// double-mint.
    frame_token_minted: bool,
}

impl CompanionConsentGate {
    /// Construct an empty gate. Player toggle starts Off, companion
    /// decision starts Unset.
    #[must_use]
    pub fn new() -> Self {
        Self {
            player_toggle: PlayerToggleState::Off,
            companion_decision: CompanionConsentDecision::Unset,
            frame_counter: 0,
            frame_token_minted: false,
        }
    }

    /// Set the player-side toggle. Player turns the view on or off.
    pub fn set_player_toggle(&mut self, state: PlayerToggleState) {
        self.player_toggle = state;
    }

    /// Read the player toggle state.
    #[must_use]
    pub fn player_toggle(&self) -> PlayerToggleState {
        self.player_toggle
    }

    /// Set the companion's consent decision. Used by the upstream
    /// 05_INTELLIGENCE engine when the companion responds to a share-
    /// request prompt.
    pub fn set_companion_decision(&mut self, decision: CompanionConsentDecision) {
        self.companion_decision = decision;
    }

    /// Read the companion's decision.
    #[must_use]
    pub fn companion_decision(&self) -> CompanionConsentDecision {
        self.companion_decision
    }

    /// Convenience : the companion grants consent.
    pub fn companion_grant(&mut self) {
        self.companion_decision = CompanionConsentDecision::Granted;
    }

    /// Convenience : the companion declines.
    pub fn companion_decline(&mut self) {
        self.companion_decision = CompanionConsentDecision::Declined;
    }

    /// Convenience : the companion revokes (after prior grant).
    pub fn companion_revoke(&mut self) {
        self.companion_decision = CompanionConsentDecision::Revoked;
    }

    /// Try to open the gate. Returns a non-Clone token on success ;
    /// returns a [`ConsentGateError`] on refusal.
    ///
    /// § INVARIANT
    ///   At most one token per frame. Subsequent calls in the same frame
    ///   return [`ConsentGateError::PlayerToggleOff`]-or-equivalent
    ///   (currently treated as the same as a re-mint refusal).
    pub fn open<'a>(&'a mut self) -> Result<CompanionConsentToken<'a>, ConsentGateError> {
        if !self.player_toggle.is_on() {
            return Err(ConsentGateError::PlayerToggleOff);
        }
        match self.companion_decision {
            CompanionConsentDecision::Unset => return Err(ConsentGateError::CompanionConsentUnset),
            CompanionConsentDecision::Declined => return Err(ConsentGateError::CompanionDeclined),
            CompanionConsentDecision::Revoked => return Err(ConsentGateError::CompanionRevoked),
            CompanionConsentDecision::Granted => {}
        }
        if self.frame_token_minted {
            // Treat double-mint as a "consent unset for this frame" error :
            // the gate has already been opened this frame.
            return Err(ConsentGateError::CompanionConsentUnset);
        }
        self.frame_token_minted = true;
        Ok(CompanionConsentToken {
            player_toggle: self.player_toggle,
            companion_decision: self.companion_decision,
            issued_frame: self.frame_counter,
            _consent: PhantomData,
        })
    }

    /// True iff `open()` would succeed right now. Useful for orchestrators
    /// that want to skip the budget-tracker in the OFF case.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.player_toggle.is_on() && self.companion_decision.permits() && !self.frame_token_minted
    }

    /// True iff the companion has explicitly refused. Distinguished from
    /// `!is_open` because the orchestrator MAY label the empty-render-
    /// target with "companion declined" feedback for the player.
    #[must_use]
    pub fn companion_refuses(&self) -> bool {
        self.companion_decision.refuses()
    }

    /// True iff the gate currently has a frame-token issued (not yet
    /// expired by `frame_complete`).
    #[must_use]
    pub fn frame_token_outstanding(&self) -> bool {
        self.frame_token_minted
    }

    /// Advance the frame counter. Drop any token previously issued.
    pub fn frame_complete(&mut self) {
        self.frame_counter += 1;
        self.frame_token_minted = false;
    }

    /// Read the current frame counter.
    #[must_use]
    pub fn frame_counter(&self) -> u64 {
        self.frame_counter
    }
}

impl Default for CompanionConsentGate {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_gate_is_closed() {
        let g = CompanionConsentGate::new();
        assert!(!g.is_open());
        assert_eq!(g.player_toggle(), PlayerToggleState::Off);
        assert_eq!(g.companion_decision(), CompanionConsentDecision::Unset);
    }

    #[test]
    fn fresh_gate_open_refuses_with_player_off() {
        let mut g = CompanionConsentGate::new();
        let r = g.open();
        assert_eq!(r.unwrap_err(), ConsentGateError::PlayerToggleOff);
    }

    #[test]
    fn open_refuses_when_companion_unset_after_player_on() {
        let mut g = CompanionConsentGate::new();
        g.set_player_toggle(PlayerToggleState::On);
        let r = g.open();
        assert_eq!(r.unwrap_err(), ConsentGateError::CompanionConsentUnset);
    }

    #[test]
    fn open_succeeds_when_both_parties_grant() {
        let mut g = CompanionConsentGate::new();
        g.set_player_toggle(PlayerToggleState::On);
        g.companion_grant();
        let tok = g.open().unwrap();
        assert_eq!(tok.player_toggle(), PlayerToggleState::On);
        assert_eq!(tok.companion_decision(), CompanionConsentDecision::Granted);
    }

    #[test]
    fn declined_companion_refuses_open() {
        let mut g = CompanionConsentGate::new();
        g.set_player_toggle(PlayerToggleState::On);
        g.companion_decline();
        assert_eq!(g.open().unwrap_err(), ConsentGateError::CompanionDeclined);
        assert!(g.companion_refuses());
    }

    #[test]
    fn revoked_companion_refuses_open() {
        let mut g = CompanionConsentGate::new();
        g.set_player_toggle(PlayerToggleState::On);
        g.companion_grant();
        g.companion_revoke();
        assert_eq!(g.open().unwrap_err(), ConsentGateError::CompanionRevoked);
        assert!(g.companion_refuses());
    }

    #[test]
    fn revoking_after_grant_blocks_subsequent_open() {
        let mut g = CompanionConsentGate::new();
        g.set_player_toggle(PlayerToggleState::On);
        g.companion_grant();
        // First grant : allowed.
        let _t = g.open().unwrap();
        g.frame_complete();
        // Revoke before next frame.
        g.companion_revoke();
        let r2 = g.open();
        assert!(r2.is_err());
    }

    #[test]
    fn double_open_in_same_frame_refused() {
        let mut g = CompanionConsentGate::new();
        g.set_player_toggle(PlayerToggleState::On);
        g.companion_grant();
        let _first = g.open().unwrap();
        let r = g.open();
        assert!(r.is_err());
    }

    #[test]
    fn frame_complete_clears_token() {
        let mut g = CompanionConsentGate::new();
        g.set_player_toggle(PlayerToggleState::On);
        g.companion_grant();
        let _t = g.open().unwrap();
        assert!(g.frame_token_outstanding());
        g.frame_complete();
        assert!(!g.frame_token_outstanding());
    }

    #[test]
    fn frame_counter_advances() {
        let mut g = CompanionConsentGate::new();
        assert_eq!(g.frame_counter(), 0);
        g.frame_complete();
        assert_eq!(g.frame_counter(), 1);
        g.frame_complete();
        assert_eq!(g.frame_counter(), 2);
    }

    #[test]
    fn token_records_issuing_frame() {
        let mut g = CompanionConsentGate::new();
        g.set_player_toggle(PlayerToggleState::On);
        g.companion_grant();
        g.frame_complete(); // Now at frame 1.
        let tok = g.open().unwrap();
        assert_eq!(tok.issued_frame(), 1);
    }

    #[test]
    fn player_toggle_is_on_predicate() {
        assert!(PlayerToggleState::On.is_on());
        assert!(!PlayerToggleState::Off.is_on());
    }

    #[test]
    fn companion_decision_permits_only_granted() {
        assert!(CompanionConsentDecision::Granted.permits());
        assert!(!CompanionConsentDecision::Declined.permits());
        assert!(!CompanionConsentDecision::Revoked.permits());
        assert!(!CompanionConsentDecision::Unset.permits());
    }

    #[test]
    fn companion_decision_refuses_declined_and_revoked() {
        assert!(CompanionConsentDecision::Declined.refuses());
        assert!(CompanionConsentDecision::Revoked.refuses());
        assert!(!CompanionConsentDecision::Granted.refuses());
        assert!(!CompanionConsentDecision::Unset.refuses());
    }

    #[test]
    fn turning_off_toggle_closes_gate() {
        let mut g = CompanionConsentGate::new();
        g.set_player_toggle(PlayerToggleState::On);
        g.companion_grant();
        assert!(g.is_open());
        g.set_player_toggle(PlayerToggleState::Off);
        assert!(!g.is_open());
    }
}
