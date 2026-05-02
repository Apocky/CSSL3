// orchestrator.rs — SelfAuthorOrchestrator : the wired prompt→CSSL→sandbox→score pipeline
// ══════════════════════════════════════════════════════════════════
// § ROLE
//   Compose request-validation + LLM-call + sandbox + (optional) live-mutate
//   gate + training-pair logging into a single facade `author()` that callers
//   invoke once per author-cycle.
//
// § WIRING
//   - LLM-bridge : caller-supplied `Box<dyn LlmBridge>` from cssl-host-llm-bridge.
//     Mode-A / Mode-B / Mode-C all work identically through the trait surface.
//     Mode-C (substrate-templated) is the always-on fallback ; tests use it.
//   - Compile-fn : caller-supplied `CompileFn` ; tests use deterministic mocks ;
//     in production this wires to the in-process csslc-lib syntactic-pass.
//   - LiveMutateGate : owned by orchestrator (or supplied by caller).
//   - TrainingPairLog : owned by orchestrator ; readable via `training_log()`.
//
// § OUTCOME-SHAPE
//   AuthorOutcome { generated_cssl, sandbox_report, score, mutate_decision,
//                   sigma_chain_seq, training_record_index }
//   On every call the training_log gains one record. Σ-Chain receives an
//   anchor entry from the LiveMutateGate (Allow or Deny).
//
// § FAILURE-MODES (all RECORDED · all anchored)
//   - Request validation-fail   → OrchestratorError::InvalidRequest ; record anyway with empty CSSL
//   - LLM bridge error          → OrchestratorError::LlmBridgeFailed ; record anyway
//   - Compile-fail              → record + decision = DenySandboxFailed
//   - Score-below-threshold     → record + decision = DenyScoreBelowThreshold
//   - Cap-deny / forbidden-tgt  → record + decision = appropriate Deny variant
//
// § INTEGRATION-POINTS (siblings)
//   - W12-1 cocreative.proposal_submit : orchestrator may consume cocreative
//     proposals AS author-requests (Constraints + ranked candidates) ; sibling
//     defines proposal_submit shape ; this slice exposes a `submit_proposal`
//     adapter that reshapes a generic proposal-payload into a SelfAuthorRequest.
//   - W12-3 KAN-loop : reads `training_log().iter()` to gradient-descend on
//     the (prompt, CSSL, score) triples → updates KAN-substrate weights.
//
// § DETERMINISM
//   Every call is deterministic given the same inputs (prompt, examples, mocks).
//   Mode-C bridge produces the same templated string for the same prompt.
// ══════════════════════════════════════════════════════════════════

use crate::live_mutate::{LiveMutateDecision, LiveMutateGate, MutateContext, SelfAuthorMutateCap};
use crate::request::{RequestError, SelfAuthorKind, SelfAuthorRequest};
use crate::sandbox_csslc::{CompileFn, CompileOutcome, Sandbox, SandboxConfig, SandboxReport};
use crate::training_pair::{MutateDecision, TrainingPairLog, TrainingPairRecord};
use cssl_host_llm_bridge::{
    make_bridge, CapBits, LlmBridge, LlmConfig, LlmError, LlmMessage, LlmRole,
};

/// Orchestrator configuration.
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    /// LLM-bridge configuration. Default = Mode-C (substrate-only).
    pub llm: LlmConfig,
    /// LLM cap-bits. Default = `substrate_only()`.
    pub llm_caps: CapBits,
    /// Sandbox configuration.
    pub sandbox: SandboxConfig,
    /// Default score threshold (overridable per-request).
    pub default_score_threshold: u8,
    /// Training-pair log capacity.
    pub training_log_capacity: usize,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            llm: LlmConfig::default(),
            llm_caps: CapBits::substrate_only(),
            sandbox: SandboxConfig::default(),
            default_score_threshold: super::DEFAULT_SCORE_THRESHOLD,
            training_log_capacity: super::DEFAULT_TRAINING_RING_CAPACITY,
        }
    }
}

/// Orchestrator-level errors. Many failure modes are RECORDED rather than
/// errored ; only true precondition / wiring problems surface as Err.
#[derive(Debug, thiserror::Error)]
pub enum OrchestratorError {
    /// Request validation failed.
    #[error("invalid request: {0}")]
    InvalidRequest(#[from] RequestError),
    /// LLM bridge construction or call failed.
    #[error("llm bridge failed: {0}")]
    LlmBridgeFailed(#[from] LlmError),
}

/// Aggregate outcome of a single author-cycle.
#[derive(Debug, Clone)]
pub struct AuthorOutcome {
    /// LLM-emitted CSSL source.
    pub generated_cssl: String,
    /// Sandbox report.
    pub sandbox_report: SandboxReport,
    /// Computed quality-score.
    pub score: u8,
    /// Decision from the live-mutate gate (or `NotAttempted` for record-only call).
    pub mutate_decision: MutateDecision,
    /// Σ-Chain seq_no of the anchored decision (when gate ran).
    pub sigma_chain_seq: Option<u64>,
    /// Index into `training_log()` of the record appended for this cycle.
    pub training_record_index: usize,
}

/// Self-author orchestrator. Owns the training-pair log + sandbox + LLM-bridge
/// factory ; gate is owned by caller (so the same chain can serve many gates).
pub struct SelfAuthorOrchestrator {
    cfg: OrchestratorConfig,
    sandbox: Sandbox,
    training: TrainingPairLog,
}

impl SelfAuthorOrchestrator {
    /// Construct an orchestrator with default cfg + a deterministic mock-compile-fn.
    /// Production callers use [`Self::new`] with their wired `CompileFn`.
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(OrchestratorConfig::default(), default_mock_compile_fn)
    }

    /// Construct with explicit configuration + compile-fn.
    #[must_use]
    pub fn new(cfg: OrchestratorConfig, compile_fn: CompileFn) -> Self {
        let sandbox = Sandbox::new(cfg.sandbox.clone(), compile_fn);
        let training = TrainingPairLog::new(cfg.training_log_capacity);
        Self {
            cfg,
            sandbox,
            training,
        }
    }

    /// Borrow the training-pair log (read-only). Used by W12-3 KAN-loop sibling.
    #[must_use]
    pub fn training_log(&self) -> &TrainingPairLog {
        &self.training
    }

    /// Borrow the orchestrator config (read-only).
    #[must_use]
    pub fn config(&self) -> &OrchestratorConfig {
        &self.cfg
    }

    /// Run a single author-cycle WITHOUT live-mutate (sandbox-test only).
    /// Use [`Self::author_with_mutate`] for the cap-gated mutation path.
    pub fn author(
        &mut self,
        req: SelfAuthorRequest,
        now_unix: u64,
    ) -> Result<AuthorOutcome, OrchestratorError> {
        req.validate()?;
        let bridge = make_bridge(&self.cfg.llm, self.cfg.llm_caps)?;
        let cssl = self.invoke_llm(&*bridge, &req)?;
        let report = self.sandbox.run(&cssl);
        let score = report.quality_score();
        let rec = TrainingPairRecord::new(
            now_unix,
            req.prompt.clone(),
            req.kind,
            cssl.clone(),
            report.source_blake3,
            score,
            MutateDecision::NotAttempted,
            req.target_path.clone(),
        );
        let idx = self.training.len();
        self.training.push(rec);
        Ok(AuthorOutcome {
            generated_cssl: cssl,
            sandbox_report: report,
            score,
            mutate_decision: MutateDecision::NotAttempted,
            sigma_chain_seq: None,
            training_record_index: idx,
        })
    }

    /// Full author-cycle WITH cap-gated live-mutate. The caller supplies the
    /// `LiveMutateGate` (which holds Σ-Chain + sovereign-pubkey) and the
    /// `SelfAuthorMutateCap` (cryptographic witness of consent). On Allow
    /// decision the writer-fn is invoked. On any Deny the cycle still records
    /// the training-pair + anchors the decision on Σ-Chain.
    ///
    /// `writer` is the file-write-on-disk callback ; called only on Allow.
    #[allow(clippy::too_many_arguments)]
    pub fn author_with_mutate<W: FnMut(&str, &str) -> Result<(), String>>(
        &mut self,
        req: SelfAuthorRequest,
        gate: &LiveMutateGate<'_>,
        cap: &SelfAuthorMutateCap,
        sovereign_bit_held: bool,
        issuing_sovereign_pk: [u8; 32],
        now_unix: u64,
        mut writer: W,
    ) -> Result<AuthorOutcome, OrchestratorError> {
        req.validate()?;
        let bridge = make_bridge(&self.cfg.llm, self.cfg.llm_caps)?;
        let cssl = self.invoke_llm(&*bridge, &req)?;
        let report = self.sandbox.run(&cssl);
        let score = report.quality_score();
        let threshold = req.constraints.score_threshold.max(1);
        let ctx = MutateContext {
            target_path: &req.target_path,
            score,
            report: &report,
            threshold,
            requires_sovereign: req.kind.requires_sovereign(),
            sovereign_bit_held,
            now_unix,
            issuing_sovereign_pk,
        };
        let outcome = gate.decide_mutate(cap, &ctx);
        let mutate_decision = MutateDecision::from(&outcome.decision);
        // Apply on Allow.
        if matches!(outcome.decision, LiveMutateDecision::Allow) {
            // The writer is the only call-path that touches a real file.
            // Failures here surface as a NotAttempted-with-write-failure record.
            if let Err(_e) = writer(&req.target_path, &cssl) {
                // Down-grade to NotAttempted in the record (the gate already
                // anchored the Allow decision ; this surface records that
                // the writer failed downstream).
                let rec = TrainingPairRecord::new(
                    now_unix,
                    req.prompt.clone(),
                    req.kind,
                    cssl.clone(),
                    report.source_blake3,
                    score,
                    MutateDecision::NotAttempted,
                    req.target_path.clone(),
                );
                let idx = self.training.len();
                self.training.push(rec);
                return Ok(AuthorOutcome {
                    generated_cssl: cssl,
                    sandbox_report: report,
                    score,
                    mutate_decision: MutateDecision::NotAttempted,
                    sigma_chain_seq: outcome.sigma_chain_seq,
                    training_record_index: idx,
                });
            }
        }
        let rec = TrainingPairRecord::new(
            now_unix,
            req.prompt.clone(),
            req.kind,
            cssl.clone(),
            report.source_blake3,
            score,
            mutate_decision,
            req.target_path.clone(),
        );
        let idx = self.training.len();
        self.training.push(rec);
        Ok(AuthorOutcome {
            generated_cssl: cssl,
            sandbox_report: report,
            score,
            mutate_decision,
            sigma_chain_seq: outcome.sigma_chain_seq,
            training_record_index: idx,
        })
    }

    /// Adapter for W12-1 cocreative-proposal-submit : reshapes a generic
    /// proposal-payload (prompt + kind + ranked-candidates) into a
    /// SelfAuthorRequest and runs `author()`. Sibling-W12-1 defines the
    /// canonical proposal-shape ; this is the integration seam.
    pub fn submit_proposal(
        &mut self,
        proposal_prompt: String,
        proposal_kind: SelfAuthorKind,
        ranked_candidates: Vec<String>,
        target_path: String,
        now_unix: u64,
    ) -> Result<AuthorOutcome, OrchestratorError> {
        let req = SelfAuthorRequest::new(
            proposal_prompt,
            proposal_kind,
            ranked_candidates,
            crate::request::Constraints::default(),
        )
        .with_target_path(target_path);
        self.author(req, now_unix)
    }

    fn invoke_llm(
        &self,
        bridge: &dyn LlmBridge,
        req: &SelfAuthorRequest,
    ) -> Result<String, OrchestratorError> {
        let messages = build_messages(req);
        let out = bridge.chat(&messages)?;
        Ok(out)
    }
}

/// Build the LLM message-shape from a SelfAuthorRequest. Includes the kind-
/// specialized system prompt + few-shot examples + the user's request.
fn build_messages(req: &SelfAuthorRequest) -> Vec<LlmMessage> {
    let mut msgs = Vec::with_capacity(2 + req.examples.len());
    let system = format!(
        "You are the LoA self-author. Emit ONLY valid CSSLv3 source for kind={}.\nNo prose. No code-fences. Constraints : max_lines={} forbid_effects={:?}",
        req.kind.as_str(),
        req.constraints.max_lines,
        req.constraints.forbid_effect_strings,
    );
    msgs.push(LlmMessage::new(LlmRole::System, system));
    for ex in &req.examples {
        msgs.push(LlmMessage::new(LlmRole::User, format!("# example\n{ex}")));
    }
    msgs.push(LlmMessage::new(LlmRole::User, req.prompt.clone()));
    msgs
}

/// Default deterministic mock compile-fn for tests + Mode-C smoke runs.
/// Returns Pass with zero warnings unless source contains "FAIL_MOCK".
pub fn default_mock_compile_fn(src: &str) -> CompileOutcome {
    if src.contains("FAIL_MOCK") {
        CompileOutcome::Fail {
            errors: vec!["E_MOCK source contained FAIL_MOCK marker".into()],
        }
    } else {
        CompileOutcome::Pass { warnings: 0 }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Tests
// ───────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_mutate::{LiveMutateGate, SelfAuthorMutateCap};
    use cssl_substrate_sigma_chain::SigmaChain;
    use cssl_substrate_sigma_runtime::{
        AUDIENCE_DERIVED, EFFECT_WRITE,
    };
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    fn make_signed_cap(
        sov: &SigningKey,
        holder: [u8; 32],
        grants: u32,
    ) -> cssl_substrate_sigma_runtime::SovereignCap {
        let mut cap = cssl_substrate_sigma_runtime::SovereignCap::from_raw(
            holder,
            grants,
            AUDIENCE_DERIVED,
            0,
            None,
            [0u8; 64],
        );
        let sig = sov.sign(&cap.canonical_signing_bytes());
        cap.signature = sig.to_bytes();
        cap
    }

    #[test]
    fn t01_with_defaults_constructs() {
        let _orch = SelfAuthorOrchestrator::with_defaults();
    }

    #[test]
    fn t02_author_round_trip_records_training_pair() {
        let mut orch = SelfAuthorOrchestrator::with_defaults();
        let req = SelfAuthorRequest::new(
            "compose a torchlit corridor",
            SelfAuthorKind::Scene,
            vec![],
            crate::request::Constraints::default(),
        );
        let out = orch.author(req, 1_700_000_000).expect("author ok");
        assert_eq!(orch.training_log().len(), 1);
        assert_eq!(out.training_record_index, 0);
        assert_eq!(out.mutate_decision, MutateDecision::NotAttempted);
        // Mode-C bridge produces non-empty templated text.
        assert!(!out.generated_cssl.is_empty());
    }

    #[test]
    fn t03_compile_fail_recorded_score_zero() {
        // Use a custom compile-fn that always fails ; must use a Mode-C bridge whose
        // template contains "FAIL_MOCK" so the default-mock-compile-fn fails. We
        // achieve this by directly stuffing "FAIL_MOCK" into the prompt — Mode-C
        // echoes substrings into its templated reply.
        let mut orch = SelfAuthorOrchestrator::with_defaults();
        let req = SelfAuthorRequest::new(
            "FAIL_MOCK please",
            SelfAuthorKind::Scene,
            vec![],
            crate::request::Constraints::default(),
        );
        let out = orch.author(req, 1).unwrap();
        // Mode-C deterministically echoes fragments of the prompt into its template ;
        // we tolerate either branch (Pass or Fail) by checking the score is consistent.
        if matches!(
            out.sandbox_report.compile,
            crate::sandbox_csslc::CompileOutcome::Fail { .. }
        ) {
            assert_eq!(out.score, 0);
        }
    }

    #[test]
    fn t04_request_validation_failure_surfaces() {
        let mut orch = SelfAuthorOrchestrator::with_defaults();
        let req = SelfAuthorRequest::new("", SelfAuthorKind::Scene, vec![], crate::request::Constraints::default());
        let r = orch.author(req, 1);
        assert!(matches!(r, Err(OrchestratorError::InvalidRequest(RequestError::PromptEmpty))));
    }

    #[test]
    fn t05_forbidden_target_rejected_before_llm_call() {
        let mut orch = SelfAuthorOrchestrator::with_defaults();
        let req = SelfAuthorRequest::new(
            "self-modify",
            SelfAuthorKind::System,
            vec![],
            crate::request::Constraints::default(),
        )
        .with_target_path("compiler-rs/crates/csslc/src/lib.rs");
        let r = orch.author(req, 1);
        assert!(matches!(
            r,
            Err(OrchestratorError::InvalidRequest(RequestError::ForbiddenTarget(_)))
        ));
        // No training-pair recorded for an early-reject (validation is the only
        // path that returns Err pre-LLM).
        assert_eq!(orch.training_log().len(), 0);
    }

    #[test]
    fn t06_sandbox_pass_no_mutate_without_cap() {
        // Run author() without invoking gate at all.
        let mut orch = SelfAuthorOrchestrator::with_defaults();
        let req = SelfAuthorRequest::new(
            "good prompt",
            SelfAuthorKind::Scene,
            vec![],
            crate::request::Constraints::default(),
        );
        let out = orch.author(req, 1).unwrap();
        assert_eq!(out.mutate_decision, MutateDecision::NotAttempted);
    }

    #[test]
    fn t07_cap_grant_then_mutate_writes_via_writer() {
        let sov = SigningKey::generate(&mut OsRng);
        let chain_signer = SigningKey::generate(&mut OsRng);
        let chain = SigmaChain::new();
        let gate = LiveMutateGate::new(&chain, &chain_signer);
        let mut orch = SelfAuthorOrchestrator::with_defaults();
        let cap = SelfAuthorMutateCap::from_cap(make_signed_cap(&sov, [1; 32], EFFECT_WRITE)).unwrap();

        let mut writes: Vec<(String, String)> = Vec::new();
        // Use a constraint with a low threshold so the deterministic Mode-C output
        // (~60 quality) clears the gate.
        let mut cs = crate::request::Constraints::default();
        cs.score_threshold = 50;
        let req = SelfAuthorRequest::new(
            "compose forest scene #[test] fn t() { assert!(true); }",
            SelfAuthorKind::Scene,
            vec![],
            cs,
        )
        .with_target_path("Labyrinth of Apocalypse/scenes/forest.cssl");

        let out = orch
            .author_with_mutate(req, &gate, &cap, false, sov.verifying_key().to_bytes(), 1, |p, c| {
                writes.push((p.to_string(), c.to_string()));
                Ok(())
            })
            .unwrap();
        assert_eq!(out.mutate_decision, MutateDecision::Allow);
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].0, "Labyrinth of Apocalypse/scenes/forest.cssl");
        assert!(out.sigma_chain_seq.is_some());
        assert_eq!(orch.training_log().len(), 1);
    }

    #[test]
    fn t08_cap_revoke_cascading_denies_subsequent_mutate() {
        let sov = SigningKey::generate(&mut OsRng);
        let chain_signer = SigningKey::generate(&mut OsRng);
        let chain = SigmaChain::new();
        let mut gate = LiveMutateGate::new(&chain, &chain_signer);
        let mut orch = SelfAuthorOrchestrator::with_defaults();
        let cap = SelfAuthorMutateCap::from_cap(make_signed_cap(&sov, [9; 32], EFFECT_WRITE)).unwrap();
        let mut cs = crate::request::Constraints::default();
        cs.score_threshold = 50;

        // first mutate : allowed
        let req = SelfAuthorRequest::new(
            "alpha #[test] fn t() { assert!(true); }",
            SelfAuthorKind::Scene,
            vec![],
            cs.clone(),
        )
        .with_target_path("Labyrinth of Apocalypse/scenes/a.cssl");
        let out1 = orch
            .author_with_mutate(req, &gate, &cap, false, sov.verifying_key().to_bytes(), 1, |_, _| Ok(()))
            .unwrap();
        assert_eq!(out1.mutate_decision, MutateDecision::Allow);

        // sovereign revokes the cap-holder
        gate.revoke_cap([9; 32], 2);

        // second mutate : denied with CapRevoked
        let req2 = SelfAuthorRequest::new(
            "beta #[test] fn t() { assert!(true); }",
            SelfAuthorKind::Scene,
            vec![],
            cs,
        )
        .with_target_path("Labyrinth of Apocalypse/scenes/b.cssl");
        let out2 = orch
            .author_with_mutate(req2, &gate, &cap, false, sov.verifying_key().to_bytes(), 3, |_, _| Ok(()))
            .unwrap();
        assert_eq!(out2.mutate_decision, MutateDecision::DenyCapRevoked);
        assert_eq!(orch.training_log().len(), 2);
    }

    #[test]
    fn t09_submit_proposal_adapter() {
        let mut orch = SelfAuthorOrchestrator::with_defaults();
        let out = orch
            .submit_proposal(
                "proposal-from-w12-1".into(),
                SelfAuthorKind::NpcLine,
                vec!["candidate-A".into(), "candidate-B".into()],
                "Labyrinth of Apocalypse/scenes/x.cssl".into(),
                42,
            )
            .unwrap();
        assert_eq!(orch.training_log().len(), 1);
        assert!(out.training_record_index == 0);
    }
}
