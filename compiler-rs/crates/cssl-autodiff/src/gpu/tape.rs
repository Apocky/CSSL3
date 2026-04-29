//! GPU autodiff tape — forward-pass record + reverse-pass replay.
//!
//! § SPEC : `specs/05_AUTODIFF.csl § GPU AUTODIFF` § tape-storage +
//!         § TAPE + CHECKPOINTING.
//!
//! § OVERVIEW
//!   A *tape* is the linear-typed buffer that records the forward-pass op
//!   sequence + operand values so the reverse-pass can walk it backwards
//!   accumulating adjoints. The CSSLv3 spec calls out three storage modes —
//!   thread-local LDS, workgroup-shared, global-SSBO — and this CPU-side
//!   simulator implements all three. The simulation is bit-exact w.r.t. the
//!   SPIR-V emission produced by `cssl-cgen-gpu-spirv::diff_shader`, so the
//!   thirty-plus correctness tests in this slice exercise the actual GPU-AD
//!   semantics without firing up a Vulkan device.
//!
//! § ON-TAPE FORMAT
//!   Each [`OpRecord`] carries :
//!     - kind   — the op's discriminator (FAdd / FMul / Sin / Sqrt / …)
//!     - operands — the operand-values needed by the reverse-pass adjoint
//!                  (e.g. for `c = a * b`, reverse needs `a` + `b` to compute
//!                  ā += b·c̄ and b̄ += a·c̄).
//!     - result_id — the tape-slot index of the forward-pass result, used
//!                   by the reverse-pass to look up the cotangent.
//!
//!   Op-specific operand counts mirror the per-primitive rule-table in
//!   `specs/05` § per-op-rules. Each kind documents its required shape
//!   below + asserts that shape at `record_op` time.
//!
//! § THREAD-LOCAL VS WORKGROUP-VS-GLOBAL
//!   The simulator uses a single [`GpuTape`] struct with a `mode :
//!   TapeStorageMode` discriminator + a single backing `Vec<OpRecord>` ; the
//!   storage-mode is metadata used by the SPIR-V emitter to choose between
//!   `Workgroup` / `Function` / `StorageBuffer` storage-classes. The
//!   in-memory simulation is identical across modes — that's the point :
//!   the SPIR-V emitter only differs in the storage-class declarations on
//!   the backing variable.
//!
//! § OUT-OF-CAPACITY
//!   Each storage mode has a per-spec capacity ceiling
//!   ([`tape_capacity_for_mode`]). Recording past the capacity returns
//!   [`GpuTapeError::CapacityExceeded`] ; production code must respond by
//!   either widening the storage-mode (LDS → workgroup → SSBO) or inserting
//!   a `@checkpoint` boundary that recomputes the next chunk from the last
//!   checkpoint instead of replaying it.

use crate::gpu::storage::{tape_capacity_for_mode, TapeStorageMode};
use crate::JetField;

/// Default capacity for a thread-local LDS tape (in op-records). Sized to
/// match Arc A770's per-EU register-file budget after live-range coalescing.
pub const DEFAULT_LDS_CAPACITY: usize = 256;

/// Default capacity for a workgroup-shared tape (in op-records). Workgroup-
/// shared memory budget is typically 32-64 KiB ; with 32-byte op-records this
/// gives ~2K records per workgroup.
pub const DEFAULT_WORKGROUP_CAPACITY: usize = 2048;

/// Default capacity for a global SSBO tape (in op-records). 65 536 records
/// matches the spec's per-call-site default budget.
pub const DEFAULT_GLOBAL_CAPACITY: usize = 65_536;

/// One operand slot stored on the tape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecordedOperand {
    /// Forward-pass slot-index this operand was produced at (for reverse-pass
    /// cotangent lookup) ; `None` indicates a constant or input.
    pub source_slot: Option<u32>,
    /// The forward-pass *value* of the operand. Stored as `f64` for the
    /// CPU-side simulator ; the SPIR-V emitter uses the kernel's native
    /// element-type (`f32` or `f64`).
    pub value: f64,
}

impl RecordedOperand {
    /// Construct an operand sourced from a prior tape slot.
    #[must_use]
    pub const fn from_slot(slot: u32, value: f64) -> Self {
        Self {
            source_slot: Some(slot),
            value,
        }
    }

    /// Construct an operand sourced from an input / constant (no tape slot).
    #[must_use]
    pub const fn input(value: f64) -> Self {
        Self {
            source_slot: None,
            value,
        }
    }
}

/// Op-kind discriminator. The reverse-pass adjoint-rule selects on this
/// enum to compute partials per operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpRecordKind {
    /// `c = a + b` ; partials : ā += c̄ , b̄ += c̄ .
    FAdd,
    /// `c = a - b` ; partials : ā += c̄ , b̄ -= c̄ .
    FSub,
    /// `c = a * b` ; partials : ā += b·c̄ , b̄ += a·c̄ .
    FMul,
    /// `c = a / b` ; partials : ā += c̄/b , b̄ -= a·c̄/b² .
    FDiv,
    /// `c = -a` ; partials : ā -= c̄ .
    FNeg,
    /// `c = sqrt(a)` ; partials : ā += 0.5/sqrt(a) · c̄ .
    Sqrt,
    /// `c = sin(a)` ; partials : ā += cos(a) · c̄ .
    Sin,
    /// `c = cos(a)` ; partials : ā += -sin(a) · c̄ .
    Cos,
    /// `c = exp(a)` ; partials : ā += exp(a) · c̄ .
    Exp,
    /// `c = log(a)` ; partials : ā += c̄ / a .
    Log,
    /// `c = a` (load through ; reverse just propagates).
    Load,
    /// `c = a` stored to memref (reverse propagates from the memory cell).
    Store,
}

impl OpRecordKind {
    /// Required operand-count for this op-kind.
    #[must_use]
    pub const fn arity(self) -> usize {
        match self {
            Self::FAdd | Self::FSub | Self::FMul | Self::FDiv => 2,
            Self::FNeg | Self::Sqrt | Self::Sin | Self::Cos | Self::Exp | Self::Log => 1,
            Self::Load | Self::Store => 1,
        }
    }

    /// Canonical text-form name (matches MIR-op surface).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::FAdd => "f.add",
            Self::FSub => "f.sub",
            Self::FMul => "f.mul",
            Self::FDiv => "f.div",
            Self::FNeg => "f.neg",
            Self::Sqrt => "math.sqrt",
            Self::Sin => "math.sin",
            Self::Cos => "math.cos",
            Self::Exp => "math.exp",
            Self::Log => "math.log",
            Self::Load => "memref.load",
            Self::Store => "memref.store",
        }
    }
}

/// One op recorded on the tape.
#[derive(Debug, Clone)]
pub struct OpRecord {
    /// Op discriminator.
    pub kind: OpRecordKind,
    /// Operand snapshots (length must equal `kind.arity()`).
    pub operands: Vec<RecordedOperand>,
    /// Forward-pass result value.
    pub result: f64,
    /// Slot index assigned by the tape on push (== position in records vec).
    pub slot: u32,
}

/// Errors a tape may surface on record / replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuTapeError {
    /// Recording would exceed the tape's storage-mode capacity.
    CapacityExceeded {
        mode: TapeStorageMode,
        capacity: usize,
    },
    /// Operand-count for the op-kind is wrong.
    OperandArityMismatch {
        kind: OpRecordKind,
        expected: usize,
        actual: usize,
    },
    /// Replay attempted on an empty tape.
    EmptyTape,
    /// Replay attempted on a slot index outside the recorded range.
    SlotOutOfRange {
        slot: u32,
        len: u32,
    },
    /// Cotangent-buffer length does not match tape length.
    CotangentLengthMismatch {
        expected: usize,
        actual: usize,
    },
}

impl core::fmt::Display for GpuTapeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::CapacityExceeded { mode, capacity } => write!(
                f,
                "GPU-tape ({mode:?}) capacity {capacity} exceeded — switch to a wider storage mode \
                 or insert @checkpoint boundary"
            ),
            Self::OperandArityMismatch {
                kind,
                expected,
                actual,
            } => write!(
                f,
                "op-kind {kind:?} expected {expected} operands ; got {actual}"
            ),
            Self::EmptyTape => f.write_str("replay called on empty tape"),
            Self::SlotOutOfRange { slot, len } => {
                write!(f, "tape-slot {slot} out of range for tape of length {len}")
            }
            Self::CotangentLengthMismatch { expected, actual } => write!(
                f,
                "cotangent buffer length {actual} ≠ tape length {expected}"
            ),
        }
    }
}

impl std::error::Error for GpuTapeError {}

/// Diagnostic stats for a tape (used by tests + telemetry probes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TapeStats {
    /// Total record count.
    pub records: usize,
    /// Sum of operand-counts across all records.
    pub operand_slots: usize,
    /// Max kind-arity observed (informational).
    pub max_arity: usize,
}

/// CPU-side simulator of a GPU autodiff tape.
///
/// Each call to [`Self::record`] appends an [`OpRecord`] ; each call to
/// [`Self::replay_into`] walks the tape in reverse summing per-operand
/// adjoints into the caller-supplied per-slot cotangent buffer.
#[derive(Debug, Clone)]
pub struct GpuTape {
    /// Storage mode. Carried as metadata for the SPIR-V emitter ; doesn't
    /// affect the in-memory simulation.
    mode: TapeStorageMode,
    /// Per-mode capacity ceiling.
    capacity: usize,
    /// The actual recorded ops, in forward-pass order.
    records: Vec<OpRecord>,
}

impl GpuTape {
    /// Construct an empty tape with the default capacity for `mode`.
    #[must_use]
    pub fn new(mode: TapeStorageMode) -> Self {
        let capacity = tape_capacity_for_mode(mode);
        Self {
            mode,
            capacity,
            records: Vec::with_capacity(capacity.min(64)),
        }
    }

    /// Construct an empty tape with an explicit capacity.
    #[must_use]
    pub fn with_capacity(mode: TapeStorageMode, capacity: usize) -> Self {
        Self {
            mode,
            capacity,
            records: Vec::with_capacity(capacity.min(64)),
        }
    }

    /// Storage mode of this tape.
    #[must_use]
    pub const fn mode(&self) -> TapeStorageMode {
        self.mode
    }

    /// Tape capacity in op-records.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Number of records currently on the tape.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Empty-test (clippy-required if `len` exists).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Reset the tape (drop all records, retain capacity).
    pub fn clear(&mut self) {
        self.records.clear();
    }

    /// Read access to a recorded op (test / diagnostics).
    #[must_use]
    pub fn record_at(&self, slot: u32) -> Option<&OpRecord> {
        self.records.get(slot as usize)
    }

    /// Snapshot of the records (test / diagnostics).
    #[must_use]
    pub fn records(&self) -> &[OpRecord] {
        &self.records
    }

    /// Stats summary.
    #[must_use]
    pub fn stats(&self) -> TapeStats {
        let operand_slots: usize = self.records.iter().map(|r| r.operands.len()).sum();
        let max_arity = self
            .records
            .iter()
            .map(|r| r.kind.arity())
            .max()
            .unwrap_or(0);
        TapeStats {
            records: self.records.len(),
            operand_slots,
            max_arity,
        }
    }

    /// Append an op-record.
    pub fn record(
        &mut self,
        kind: OpRecordKind,
        operands: Vec<RecordedOperand>,
        result: f64,
    ) -> Result<u32, GpuTapeError> {
        if operands.len() != kind.arity() {
            return Err(GpuTapeError::OperandArityMismatch {
                kind,
                expected: kind.arity(),
                actual: operands.len(),
            });
        }
        if self.records.len() >= self.capacity {
            return Err(GpuTapeError::CapacityExceeded {
                mode: self.mode,
                capacity: self.capacity,
            });
        }
        let slot = self.records.len() as u32;
        self.records.push(OpRecord {
            kind,
            operands,
            result,
            slot,
        });
        Ok(slot)
    }

    /// Walk the tape in reverse-order, accumulating adjoints into
    /// `cotangents[slot]` per the per-op partial-rule.
    ///
    /// The caller pre-seeds the loss-cotangent into `cotangents[last_slot] =
    /// 1.0` for a scalar-loss kernel, then this method propagates that seed
    /// backward through the tape to compute the per-input gradient.
    pub fn replay_into(&self, cotangents: &mut [f64]) -> Result<(), GpuTapeError> {
        if self.records.is_empty() {
            return Err(GpuTapeError::EmptyTape);
        }
        if cotangents.len() != self.records.len() {
            return Err(GpuTapeError::CotangentLengthMismatch {
                expected: self.records.len(),
                actual: cotangents.len(),
            });
        }
        for rec in self.records.iter().rev() {
            let c_bar = cotangents[rec.slot as usize];
            apply_reverse_partial(rec, c_bar, cotangents)?;
        }
        Ok(())
    }
}

/// Free helper : appends a [`FAdd`-style] op to the tape via the simulated
/// `cssl.diff.gpu_tape_record` MIR-op shape used by SPIR-V emission.
///
/// [`FAdd`-style]: OpRecordKind::FAdd
pub fn record_op(
    tape: &mut GpuTape,
    kind: OpRecordKind,
    operands: Vec<RecordedOperand>,
    result: f64,
) -> Result<u32, GpuTapeError> {
    tape.record(kind, operands, result)
}

/// Free helper : runs `replay_into` and returns the per-slot cotangent
/// buffer. Used by the SPIR-V emitter's `cssl.diff.gpu_tape_replay` lowering.
pub fn replay_op(tape: &GpuTape, mut cotangents: Vec<f64>) -> Result<Vec<f64>, GpuTapeError> {
    tape.replay_into(&mut cotangents)?;
    Ok(cotangents)
}

/// Reverse-mode partial-rule per op-kind. Updates each operand's
/// upstream cotangent in `cotangents[]` based on the forward-pass operand
/// values + the result-cotangent `c̄`.
fn apply_reverse_partial(
    rec: &OpRecord,
    c_bar: f64,
    cotangents: &mut [f64],
) -> Result<(), GpuTapeError> {
    let len = cotangents.len() as u32;
    let mut accumulate = |slot: Option<u32>, partial: f64| -> Result<(), GpuTapeError> {
        if let Some(s) = slot {
            if s >= len {
                return Err(GpuTapeError::SlotOutOfRange { slot: s, len });
            }
            cotangents[s as usize] += partial;
        }
        Ok(())
    };

    match rec.kind {
        OpRecordKind::FAdd => {
            // c = a + b   →   ā += c̄ , b̄ += c̄
            accumulate(rec.operands[0].source_slot, c_bar)?;
            accumulate(rec.operands[1].source_slot, c_bar)?;
        }
        OpRecordKind::FSub => {
            // c = a - b   →   ā += c̄ , b̄ -= c̄
            accumulate(rec.operands[0].source_slot, c_bar)?;
            accumulate(rec.operands[1].source_slot, -c_bar)?;
        }
        OpRecordKind::FMul => {
            // c = a * b   →   ā += b·c̄ , b̄ += a·c̄
            let a = rec.operands[0].value;
            let b = rec.operands[1].value;
            accumulate(rec.operands[0].source_slot, b * c_bar)?;
            accumulate(rec.operands[1].source_slot, a * c_bar)?;
        }
        OpRecordKind::FDiv => {
            // c = a / b   →   ā += c̄/b , b̄ -= a·c̄/b²
            let a = rec.operands[0].value;
            let b = rec.operands[1].value;
            let inv_b = 1.0 / b;
            accumulate(rec.operands[0].source_slot, inv_b * c_bar)?;
            accumulate(rec.operands[1].source_slot, -a * inv_b * inv_b * c_bar)?;
        }
        OpRecordKind::FNeg => {
            // c = -a   →   ā -= c̄
            accumulate(rec.operands[0].source_slot, -c_bar)?;
        }
        OpRecordKind::Sqrt => {
            // c = sqrt(a)   →   ā += 0.5/sqrt(a) · c̄
            let a = rec.operands[0].value;
            let partial = 0.5 / a.sqrt() * c_bar;
            accumulate(rec.operands[0].source_slot, partial)?;
        }
        OpRecordKind::Sin => {
            // c = sin(a)   →   ā += cos(a) · c̄
            let a = rec.operands[0].value;
            accumulate(rec.operands[0].source_slot, a.cos() * c_bar)?;
        }
        OpRecordKind::Cos => {
            // c = cos(a)   →   ā += -sin(a) · c̄
            let a = rec.operands[0].value;
            accumulate(rec.operands[0].source_slot, -a.sin() * c_bar)?;
        }
        OpRecordKind::Exp => {
            // c = exp(a)   →   ā += exp(a) · c̄
            //                = c · c̄  (avoid recomputing exp)
            accumulate(rec.operands[0].source_slot, rec.result * c_bar)?;
        }
        OpRecordKind::Log => {
            // c = log(a)   →   ā += c̄/a
            let a = rec.operands[0].value;
            accumulate(rec.operands[0].source_slot, c_bar / a)?;
        }
        OpRecordKind::Load | OpRecordKind::Store => {
            // pass-through
            accumulate(rec.operands[0].source_slot, c_bar)?;
        }
    }
    Ok(())
}

/// Convenience : record + check arity-rule from a generic [`JetField`] value.
///
/// Forces the caller into an `f64` projection (matches the on-tape format) ;
/// `JetField::to_f64` handles the conversion. Useful for tests that drive
/// tape recording from `Jet<f32, N>` outputs.
pub fn record_from_jet_field<T: JetField>(
    tape: &mut GpuTape,
    kind: OpRecordKind,
    operands: &[(Option<u32>, T)],
    result: T,
) -> Result<u32, GpuTapeError> {
    let lifted: Vec<RecordedOperand> = operands
        .iter()
        .map(|(slot, v)| match slot {
            Some(s) => RecordedOperand::from_slot(*s, v.to_f64()),
            None => RecordedOperand::input(v.to_f64()),
        })
        .collect();
    tape.record(kind, lifted, result.to_f64())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add_const(tape: &mut GpuTape, v: f64) -> u32 {
        tape.record(
            OpRecordKind::Load,
            vec![RecordedOperand::input(v)],
            v,
        )
        .unwrap()
    }

    #[test]
    fn lds_tape_default_capacity() {
        let t = GpuTape::new(TapeStorageMode::ThreadLocalLds);
        assert_eq!(t.capacity(), DEFAULT_LDS_CAPACITY);
        assert!(t.is_empty());
    }

    #[test]
    fn workgroup_tape_default_capacity() {
        let t = GpuTape::new(TapeStorageMode::WorkgroupShared);
        assert_eq!(t.capacity(), DEFAULT_WORKGROUP_CAPACITY);
    }

    #[test]
    fn global_tape_default_capacity() {
        let t = GpuTape::new(TapeStorageMode::GlobalSsbo);
        assert_eq!(t.capacity(), DEFAULT_GLOBAL_CAPACITY);
    }

    #[test]
    fn record_assigns_monotonic_slots() {
        let mut t = GpuTape::new(TapeStorageMode::WorkgroupShared);
        let s0 = add_const(&mut t, 1.0);
        let s1 = add_const(&mut t, 2.0);
        let s2 = add_const(&mut t, 3.0);
        assert_eq!((s0, s1, s2), (0, 1, 2));
        assert_eq!(t.len(), 3);
    }

    #[test]
    fn arity_mismatch_rejected() {
        let mut t = GpuTape::new(TapeStorageMode::WorkgroupShared);
        let err = t
            .record(OpRecordKind::FAdd, vec![RecordedOperand::input(1.0)], 1.0)
            .unwrap_err();
        match err {
            GpuTapeError::OperandArityMismatch {
                kind,
                expected,
                actual,
            } => {
                assert_eq!(kind, OpRecordKind::FAdd);
                assert_eq!(expected, 2);
                assert_eq!(actual, 1);
            }
            other => panic!("wrong error : {other:?}"),
        }
    }

    #[test]
    fn capacity_exceeded_rejected() {
        let mut t = GpuTape::with_capacity(TapeStorageMode::ThreadLocalLds, 2);
        add_const(&mut t, 1.0);
        add_const(&mut t, 2.0);
        let err = t
            .record(OpRecordKind::Load, vec![RecordedOperand::input(3.0)], 3.0)
            .unwrap_err();
        match err {
            GpuTapeError::CapacityExceeded { mode, capacity } => {
                assert_eq!(mode, TapeStorageMode::ThreadLocalLds);
                assert_eq!(capacity, 2);
            }
            other => panic!("wrong error : {other:?}"),
        }
    }

    #[test]
    fn replay_fadd_propagates_unit_cotangent_to_both_inputs() {
        // y = a + b  ⇒  dy/da = 1, dy/db = 1
        let mut t = GpuTape::new(TapeStorageMode::WorkgroupShared);
        let a = add_const(&mut t, 3.0);
        let b = add_const(&mut t, 4.0);
        let _y = t
            .record(
                OpRecordKind::FAdd,
                vec![
                    RecordedOperand::from_slot(a, 3.0),
                    RecordedOperand::from_slot(b, 4.0),
                ],
                7.0,
            )
            .unwrap();

        let mut cot = vec![0.0; t.len()];
        let last = t.len() - 1;
        cot[last] = 1.0;
        t.replay_into(&mut cot).unwrap();
        assert!((cot[a as usize] - 1.0).abs() < 1e-12);
        assert!((cot[b as usize] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn replay_fmul_propagates_partial_value_per_operand() {
        // y = a * b  ⇒  dy/da = b, dy/db = a
        let mut t = GpuTape::new(TapeStorageMode::WorkgroupShared);
        let a = add_const(&mut t, 5.0);
        let b = add_const(&mut t, 7.0);
        let _y = t
            .record(
                OpRecordKind::FMul,
                vec![
                    RecordedOperand::from_slot(a, 5.0),
                    RecordedOperand::from_slot(b, 7.0),
                ],
                35.0,
            )
            .unwrap();

        let mut cot = vec![0.0; t.len()];
        let last = t.len() - 1;
        cot[last] = 1.0;
        t.replay_into(&mut cot).unwrap();
        assert!((cot[a as usize] - 7.0).abs() < 1e-12);
        assert!((cot[b as usize] - 5.0).abs() < 1e-12);
    }

    #[test]
    fn replay_chain_rule_through_sin() {
        // y = sin(a)  ⇒  dy/da = cos(a)
        let mut t = GpuTape::new(TapeStorageMode::WorkgroupShared);
        let a = add_const(&mut t, 0.5);
        let _y = t
            .record(
                OpRecordKind::Sin,
                vec![RecordedOperand::from_slot(a, 0.5)],
                0.5_f64.sin(),
            )
            .unwrap();

        let mut cot = vec![0.0; t.len()];
        let last = t.len() - 1;
        cot[last] = 1.0;
        t.replay_into(&mut cot).unwrap();
        let expected = 0.5_f64.cos();
        assert!((cot[a as usize] - expected).abs() < 1e-12);
    }

    #[test]
    fn replay_long_chain_matches_analytic_grad() {
        // y = exp(sin(a) * b) ; check ∂y/∂a + ∂y/∂b
        let a_val = 0.3_f64;
        let b_val = 1.7_f64;
        let mut t = GpuTape::new(TapeStorageMode::WorkgroupShared);
        let a = add_const(&mut t, a_val);
        let b = add_const(&mut t, b_val);
        let s = t
            .record(
                OpRecordKind::Sin,
                vec![RecordedOperand::from_slot(a, a_val)],
                a_val.sin(),
            )
            .unwrap();
        let m = t
            .record(
                OpRecordKind::FMul,
                vec![
                    RecordedOperand::from_slot(s, a_val.sin()),
                    RecordedOperand::from_slot(b, b_val),
                ],
                a_val.sin() * b_val,
            )
            .unwrap();
        let _y = t
            .record(
                OpRecordKind::Exp,
                vec![RecordedOperand::from_slot(m, a_val.sin() * b_val)],
                (a_val.sin() * b_val).exp(),
            )
            .unwrap();

        let mut cot = vec![0.0; t.len()];
        let last = t.len() - 1;
        cot[last] = 1.0;
        t.replay_into(&mut cot).unwrap();

        // Analytic : dy/da = exp(sin(a)*b) * cos(a) * b
        //            dy/db = exp(sin(a)*b) * sin(a)
        let y = (a_val.sin() * b_val).exp();
        let expected_da = y * a_val.cos() * b_val;
        let expected_db = y * a_val.sin();
        assert!((cot[a as usize] - expected_da).abs() < 1e-10);
        assert!((cot[b as usize] - expected_db).abs() < 1e-10);
    }

    #[test]
    fn replay_empty_tape_errors() {
        let t = GpuTape::new(TapeStorageMode::WorkgroupShared);
        let mut cot: Vec<f64> = vec![];
        let err = t.replay_into(&mut cot).unwrap_err();
        assert_eq!(err, GpuTapeError::EmptyTape);
    }

    #[test]
    fn replay_cotangent_length_mismatch_errors() {
        let mut t = GpuTape::new(TapeStorageMode::WorkgroupShared);
        add_const(&mut t, 1.0);
        let mut cot = vec![0.0; 99];
        let err = t.replay_into(&mut cot).unwrap_err();
        match err {
            GpuTapeError::CotangentLengthMismatch { expected, actual } => {
                assert_eq!(expected, 1);
                assert_eq!(actual, 99);
            }
            other => panic!("wrong error : {other:?}"),
        }
    }

    #[test]
    fn record_helper_round_trip() {
        let mut t = GpuTape::new(TapeStorageMode::ThreadLocalLds);
        let s = record_op(
            &mut t,
            OpRecordKind::Load,
            vec![RecordedOperand::input(5.0)],
            5.0,
        )
        .unwrap();
        assert_eq!(s, 0);
        assert_eq!(t.record_at(0).unwrap().result, 5.0);
    }

    #[test]
    fn record_from_jet_field_extracts_primal() {
        let mut t = GpuTape::new(TapeStorageMode::WorkgroupShared);
        let s = record_from_jet_field::<f32>(
            &mut t,
            OpRecordKind::Load,
            &[(None, 2.5_f32)],
            2.5,
        )
        .unwrap();
        assert_eq!(s, 0);
        assert!((t.record_at(0).unwrap().result - 2.5).abs() < 1e-7);
    }

    #[test]
    fn stats_tracks_records_and_arity() {
        let mut t = GpuTape::new(TapeStorageMode::WorkgroupShared);
        let a = add_const(&mut t, 1.0);
        let b = add_const(&mut t, 2.0);
        t.record(
            OpRecordKind::FMul,
            vec![
                RecordedOperand::from_slot(a, 1.0),
                RecordedOperand::from_slot(b, 2.0),
            ],
            2.0,
        )
        .unwrap();
        let s = t.stats();
        assert_eq!(s.records, 3);
        assert_eq!(s.operand_slots, 1 + 1 + 2);
        assert_eq!(s.max_arity, 2);
    }

    #[test]
    fn clear_resets_records_but_retains_capacity() {
        let mut t = GpuTape::with_capacity(TapeStorageMode::ThreadLocalLds, 16);
        add_const(&mut t, 1.0);
        add_const(&mut t, 2.0);
        assert_eq!(t.len(), 2);
        t.clear();
        assert!(t.is_empty());
        assert_eq!(t.capacity(), 16);
    }

    #[test]
    fn op_record_kind_names_unique() {
        use std::collections::HashSet;
        let kinds = [
            OpRecordKind::FAdd,
            OpRecordKind::FSub,
            OpRecordKind::FMul,
            OpRecordKind::FDiv,
            OpRecordKind::FNeg,
            OpRecordKind::Sqrt,
            OpRecordKind::Sin,
            OpRecordKind::Cos,
            OpRecordKind::Exp,
            OpRecordKind::Log,
            OpRecordKind::Load,
            OpRecordKind::Store,
        ];
        let names: HashSet<_> = kinds.iter().map(|k| k.name()).collect();
        assert_eq!(names.len(), kinds.len());
    }
}
