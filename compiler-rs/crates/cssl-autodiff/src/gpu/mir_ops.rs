//! Symbolic MIR-op vocabulary for GPU autodiff tape ops.
//!
//! § SPEC : `specs/05_AUTODIFF.csl § GPU AUTODIFF` + `specs/02_IR.csl § DIFF
//!         OPS` (referenced from §§05).
//!
//! § DESIGN
//!   At MIR level, GPU-AD introduces three new dialect ops :
//!
//!   - `cssl.diff.gpu_tape_alloc`  — allocates a tape at fn-entry
//!   - `cssl.diff.gpu_tape_record` — appends a forward-pass op-record
//!   - `cssl.diff.gpu_tape_replay` — walks the tape backward at the
//!     reverse-pass exit
//!
//!   Per the `gpu/mod.rs` rejected-designs note, these names land as
//!   `CsslOp::Std` markers ; the canonical names are recognized by the
//!   SPIR-V `diff_shader` emitter via [`GpuAdOpName`]. This avoids cascading
//!   `ALL_CSSL.len()` invariant changes.

/// Canonical symbolic vocabulary for GPU-AD tape ops.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GpuAdOp {
    /// Allocate a tape buffer at the entry-block of a `@differentiable {GPU}`
    /// fn. Op-attributes (per the SPIR-V emitter) :
    ///
    /// - `(storage_mode, "<TapeStorageMode name>")` — required.
    /// - `(capacity, "<usize>")` — required.
    /// - `(element_type, "f32" | "f64")` — required.
    Alloc,
    /// Append a forward-pass op-record. Op-attributes :
    ///
    /// - `(kind, "<OpRecordKind name>")` — required.
    /// - `(arity, "<usize>")` — required, must match kind.
    ///
    /// Operands : tape handle (slot 0) + per-op operand values (slots 1..).
    Record,
    /// Walk the tape in reverse, accumulating per-slot adjoints. Operands :
    ///
    /// - tape handle (slot 0)
    /// - cotangent buffer base-pointer (slot 1)
    /// - seed-index (slot 2) — typically the loss-output's tape-slot.
    ///
    /// Result : the per-input gradient (read out from `cotangents[input_slot]`).
    Replay,
}

/// Free-form name interface for the SPIR-V emitter — keyed off the
/// `CsslOp::Std` op-name string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GpuAdOpName(pub &'static str);

impl GpuAdOp {
    /// Canonical text-form name (matches the spec text in `specs/05`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Alloc => "cssl.diff.gpu_tape_alloc",
            Self::Record => "cssl.diff.gpu_tape_record",
            Self::Replay => "cssl.diff.gpu_tape_replay",
        }
    }

    /// All three ops (test surface + dispatch table).
    pub const ALL: [Self; 3] = [Self::Alloc, Self::Record, Self::Replay];

    /// Attempt to recognize a `CsslOp::Std` op-name as a GPU-AD op.
    #[must_use]
    pub fn from_std_name(name: &str) -> Option<Self> {
        match name {
            "cssl.diff.gpu_tape_alloc" => Some(Self::Alloc),
            "cssl.diff.gpu_tape_record" => Some(Self::Record),
            "cssl.diff.gpu_tape_replay" => Some(Self::Replay),
            _ => None,
        }
    }

    /// Standard-op name as a [`GpuAdOpName`] (sentinel newtype for the
    /// recognizer in `cssl-cgen-gpu-spirv::diff_shader`).
    #[must_use]
    pub const fn as_op_name(self) -> GpuAdOpName {
        GpuAdOpName(self.name())
    }

    /// True iff this op should be emitted in the entry-block (allocation).
    #[must_use]
    pub const fn is_entry_only(self) -> bool {
        matches!(self, Self::Alloc)
    }

    /// True iff this op should be emitted in the exit-block (replay).
    #[must_use]
    pub const fn is_exit_only(self) -> bool {
        matches!(self, Self::Replay)
    }
}

/// Mapping of attribute keys recognized by the SPIR-V emitter for a given
/// op. Returned as a slice of `(key, required)` pairs ; the emitter walks
/// this when it lowers a recognized `CsslOp::Std` to its real SPIR-V form.
#[must_use]
pub const fn required_attributes(op: GpuAdOp) -> &'static [(&'static str, bool)] {
    match op {
        GpuAdOp::Alloc => &[
            ("storage_mode", true),
            ("capacity", true),
            ("element_type", true),
        ],
        GpuAdOp::Record => &[("kind", true), ("arity", true)],
        GpuAdOp::Replay => &[("seed_slot", false)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_spec_verbatim() {
        assert_eq!(GpuAdOp::Alloc.name(), "cssl.diff.gpu_tape_alloc");
        assert_eq!(GpuAdOp::Record.name(), "cssl.diff.gpu_tape_record");
        assert_eq!(GpuAdOp::Replay.name(), "cssl.diff.gpu_tape_replay");
    }

    #[test]
    fn round_trip_from_std_name() {
        for op in GpuAdOp::ALL {
            assert_eq!(GpuAdOp::from_std_name(op.name()), Some(op));
        }
    }

    #[test]
    fn unknown_name_returns_none() {
        assert_eq!(GpuAdOp::from_std_name("cssl.unrelated"), None);
    }

    #[test]
    fn entry_only_alloc() {
        assert!(GpuAdOp::Alloc.is_entry_only());
        assert!(!GpuAdOp::Record.is_entry_only());
        assert!(!GpuAdOp::Replay.is_entry_only());
    }

    #[test]
    fn exit_only_replay() {
        assert!(GpuAdOp::Replay.is_exit_only());
        assert!(!GpuAdOp::Alloc.is_exit_only());
        assert!(!GpuAdOp::Record.is_exit_only());
    }

    #[test]
    fn all_op_names_unique() {
        use std::collections::HashSet;
        let set: HashSet<_> = GpuAdOp::ALL.iter().map(|op| op.name()).collect();
        assert_eq!(set.len(), GpuAdOp::ALL.len());
    }

    #[test]
    fn required_attributes_for_alloc_includes_storage_mode() {
        let attrs = required_attributes(GpuAdOp::Alloc);
        assert!(attrs.iter().any(|(k, req)| *k == "storage_mode" && *req));
    }

    #[test]
    fn required_attributes_for_record_includes_kind_and_arity() {
        let attrs = required_attributes(GpuAdOp::Record);
        assert!(attrs.iter().any(|(k, _)| *k == "kind"));
        assert!(attrs.iter().any(|(k, _)| *k == "arity"));
    }
}
