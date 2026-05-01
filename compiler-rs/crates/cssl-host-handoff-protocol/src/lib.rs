//! cssl-host-handoff-protocol
//! ════════════════════════════════════════════════════════════════
//! § T11-W6-HANDOFF
//! Inter-role handoff matrix for LoA narrow-orchestrators :
//!   DM ↔ GM ↔ Collaborator ↔ Coder
//!
//! Enforces :
//!   • valid transition matrix (DM-hub topology)
//!   • cap-bit-bleed prohibition (a DM cannot directly use
//!     GM_CAP_VOICE_EMIT — must hand off properly)
//!   • bounded history with FIFO eviction
//!   • policy layer (loopback / max-chain / sovereign-required for Coder)
//!   • JSON audit-event emission for replay
//!
//! Reference: specs/grand-vision/10_INTELLIGENCE.csl
//!
//! § PRIME-DIRECTIVE alignment
//!   • consent = OS — sovereign flag tracked + required for Coder by default
//!   • no surveillance — handoff records intent + reason, not content semantics
//!   • narrow-roles ¬ general-AI — each role's caps explicitly scoped
//!
//! § Scope boundary
//!   This crate is the STATE-MACHINE. It does NOT call AI models. The
//!   orchestrator-host layer (intent_router, companion-relay, etc.) wires
//!   actual role-instances onto these handoffs. Wave-7 work.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod handoff;
pub mod policy;
pub mod role;
pub mod state_machine;

pub use handoff::{Handoff, HandoffErr};
pub use policy::{HandoffPolicy, PolicyDecision, default_policy};
pub use role::{Role, RoleCaps, cap_name_for};
pub use state_machine::HandoffStateMachine;
