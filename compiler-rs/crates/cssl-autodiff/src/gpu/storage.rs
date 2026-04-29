//! Tape-storage mode + the operation-density heuristic used to pick it.
//!
//! § SPEC : `specs/05_AUTODIFF.csl § GPU AUTODIFF` line :
//!   `tape-storage : thread-local | workgroup-shared | global-SSBO (scoped)`.
//!
//! § DESIGN
//!   The selector takes an [`OperationDensity`] descriptor — a per-fn count
//!   of (live-out tape-slots, recordable ops, expected workgroup size) —
//!   and produces the [`TapeStorageMode`] the SPIR-V emitter should declare
//!   the backing variable in. Three regimes :
//!
//!   - LDS / function-storage : `≤ LDS_OP_BUDGET` ops + per-thread access
//!   - workgroup-shared : `≤ WORKGROUP_OP_BUDGET` ops + cross-thread reads
//!   - global SSBO : everything else
//!
//!   The heuristic is intentionally conservative ; the spec gives the modes
//!   as alternatives but doesn't pin per-mode budgets, so we use values
//!   tuned to the Arc A770 / Quest-3 / RTX-50 base profiles called out in
//!   `Omniverse/07_AESTHETIC/07_KAN_RUNTIME_SHADING § II + III`.

use super::tape::{DEFAULT_GLOBAL_CAPACITY, DEFAULT_LDS_CAPACITY, DEFAULT_WORKGROUP_CAPACITY};

/// Per-thread LDS op-budget. Above this count we promote to workgroup-shared.
pub const LDS_OP_BUDGET: usize = 64;

/// Workgroup-shared op-budget per thread (assumes 64-thread workgroup, so the
/// total shared-budget is `WORKGROUP_OP_BUDGET * 64` ≈ 8 KiB at 16 bytes/op).
pub const WORKGROUP_OP_BUDGET: usize = 512;

/// Threshold over which cross-thread atomic-write density forces SSBO mode.
pub const SSBO_ATOMIC_WRITE_THRESHOLD: usize = 8;

/// Tape storage mode selector. Mirrors the SPIR-V `StorageClass` decoration
/// the backing variable carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TapeStorageMode {
    /// SPIR-V `Function` storage-class. Tape lives in per-thread registers /
    /// LDS spills. Smallest, fastest, lowest capacity.
    ThreadLocalLds,
    /// SPIR-V `Workgroup` storage-class. Tape lives in `groupshared` /
    /// `__local` memory. Cross-thread visible within a workgroup.
    WorkgroupShared,
    /// SPIR-V `StorageBuffer` storage-class with `BufferBlock` decoration.
    /// Tape lives in global VRAM ; cross-workgroup visible. Largest capacity,
    /// slowest access.
    GlobalSsbo,
}

impl TapeStorageMode {
    /// Canonical SPIR-V storage-class string emitted in the `OpVariable`
    /// type-pointer decoration.
    #[must_use]
    pub const fn spirv_storage_class(self) -> &'static str {
        match self {
            Self::ThreadLocalLds => "Function",
            Self::WorkgroupShared => "Workgroup",
            Self::GlobalSsbo => "StorageBuffer",
        }
    }

    /// HLSL/D3D12 storage-class name (for the DXIL backend mirror).
    #[must_use]
    pub const fn hlsl_storage_class(self) -> &'static str {
        match self {
            Self::ThreadLocalLds => "register",
            Self::WorkgroupShared => "groupshared",
            Self::GlobalSsbo => "RWStructuredBuffer",
        }
    }

    /// Metal storage-class name (for the MSL backend mirror).
    #[must_use]
    pub const fn metal_storage_class(self) -> &'static str {
        match self {
            Self::ThreadLocalLds => "thread",
            Self::WorkgroupShared => "threadgroup",
            Self::GlobalSsbo => "device",
        }
    }

    /// All three modes (test surface).
    pub const ALL: [Self; 3] = [
        Self::ThreadLocalLds,
        Self::WorkgroupShared,
        Self::GlobalSsbo,
    ];
}

/// Human-readable density classification used as input to the storage-mode
/// selector. The walker fills these in by counting MIR ops + estimating
/// workgroup-shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OperationDensity {
    /// Number of ops to record on the tape.
    pub op_count: usize,
    /// Number of operands per op (rough upper-bound).
    pub avg_operand_count: usize,
    /// Workgroup size (compute-fn `LocalSize.x * y * z`).
    pub workgroup_size: usize,
    /// True if the kernel has any `atomic` ops on the tape (forces SSBO).
    pub has_atomic_writes: bool,
    /// True if the tape data is read by other workgroups (forces SSBO).
    pub cross_workgroup_visible: bool,
    /// True if the user requested a `@checkpoint` on the parent fn — in that
    /// case smaller modes are preferred (recompute beats long tape).
    pub checkpoint_requested: bool,
}

impl OperationDensity {
    /// Total per-thread op-budget (op_count × avg_operand_count is the rough
    /// memory cost in u32 slots ; conservative upper-bound).
    #[must_use]
    pub fn per_thread_slot_estimate(self) -> usize {
        self.op_count.saturating_mul(self.avg_operand_count.max(1))
    }

    /// Estimated total VRAM cost in bytes (used for SSBO sizing).
    #[must_use]
    pub fn estimated_total_bytes(self) -> usize {
        // 16 bytes per operand-slot (kind + value + slot + pad). Plus 16 bytes
        // per record header.
        let per_record_bytes = 16 + (self.avg_operand_count.max(1) * 16);
        per_record_bytes
            .saturating_mul(self.op_count)
            .saturating_mul(self.workgroup_size.max(1))
    }
}

/// Pick the storage mode for a given operation-density profile.
///
/// § DECISION-RULE (per spec) :
///   1. Cross-workgroup visibility OR atomic-writes OR `op_count > workgroup-budget` ⇒ Global-SSBO.
///   2. Per-thread slot-count ≤ `LDS_OP_BUDGET` AND no cross-thread reads ⇒ Thread-Local-LDS.
///   3. Else ⇒ Workgroup-Shared.
///
///   Checkpoint requested narrows the bands : prefer LDS even at slightly
///   higher op-counts since a checkpoint boundary will recompute the next
///   chunk anyway.
#[must_use]
pub fn select_storage_mode(d: OperationDensity) -> TapeStorageMode {
    // Hard-forces to SSBO :
    if d.cross_workgroup_visible || d.has_atomic_writes || d.op_count > WORKGROUP_OP_BUDGET {
        return TapeStorageMode::GlobalSsbo;
    }

    let lds_threshold = if d.checkpoint_requested {
        // Checkpoints recompute on demand ; a small tape is fine.
        LDS_OP_BUDGET * 2
    } else {
        LDS_OP_BUDGET
    };

    if d.op_count <= lds_threshold {
        TapeStorageMode::ThreadLocalLds
    } else {
        TapeStorageMode::WorkgroupShared
    }
}

/// Tape capacity (in op-records) for a given storage-mode. Used by
/// [`crate::gpu::tape::GpuTape::new`] when the caller doesn't override.
#[must_use]
pub const fn tape_capacity_for_mode(mode: TapeStorageMode) -> usize {
    match mode {
        TapeStorageMode::ThreadLocalLds => DEFAULT_LDS_CAPACITY,
        TapeStorageMode::WorkgroupShared => DEFAULT_WORKGROUP_CAPACITY,
        TapeStorageMode::GlobalSsbo => DEFAULT_GLOBAL_CAPACITY,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lds_picked_for_small_op_count_no_atomic() {
        let d = OperationDensity {
            op_count: 12,
            avg_operand_count: 2,
            workgroup_size: 64,
            has_atomic_writes: false,
            cross_workgroup_visible: false,
            checkpoint_requested: false,
        };
        assert_eq!(select_storage_mode(d), TapeStorageMode::ThreadLocalLds);
    }

    #[test]
    fn workgroup_picked_for_medium_op_count() {
        let d = OperationDensity {
            op_count: 256,
            avg_operand_count: 2,
            workgroup_size: 64,
            ..Default::default()
        };
        assert_eq!(select_storage_mode(d), TapeStorageMode::WorkgroupShared);
    }

    #[test]
    fn ssbo_picked_for_large_op_count() {
        let d = OperationDensity {
            op_count: 5_000,
            avg_operand_count: 2,
            workgroup_size: 64,
            ..Default::default()
        };
        assert_eq!(select_storage_mode(d), TapeStorageMode::GlobalSsbo);
    }

    #[test]
    fn ssbo_picked_when_atomic_writes_present() {
        let d = OperationDensity {
            op_count: 4,
            has_atomic_writes: true,
            ..Default::default()
        };
        assert_eq!(select_storage_mode(d), TapeStorageMode::GlobalSsbo);
    }

    #[test]
    fn ssbo_picked_when_cross_workgroup_visible() {
        let d = OperationDensity {
            op_count: 4,
            cross_workgroup_visible: true,
            ..Default::default()
        };
        assert_eq!(select_storage_mode(d), TapeStorageMode::GlobalSsbo);
    }

    #[test]
    fn checkpoint_extends_lds_band() {
        let d_no_cp = OperationDensity {
            op_count: 100,
            avg_operand_count: 2,
            workgroup_size: 64,
            checkpoint_requested: false,
            ..Default::default()
        };
        let d_with_cp = OperationDensity {
            checkpoint_requested: true,
            ..d_no_cp
        };
        assert_eq!(
            select_storage_mode(d_no_cp),
            TapeStorageMode::WorkgroupShared
        );
        assert_eq!(
            select_storage_mode(d_with_cp),
            TapeStorageMode::ThreadLocalLds
        );
    }

    #[test]
    fn storage_class_strings_unique_per_backend() {
        use std::collections::HashSet;
        for backend_set in [
            TapeStorageMode::ALL.map(|m| m.spirv_storage_class()),
            TapeStorageMode::ALL.map(|m| m.hlsl_storage_class()),
            TapeStorageMode::ALL.map(|m| m.metal_storage_class()),
        ] {
            let set: HashSet<_> = backend_set.iter().collect();
            assert_eq!(set.len(), 3);
        }
    }

    #[test]
    fn capacity_for_mode_is_monotone() {
        let lds = tape_capacity_for_mode(TapeStorageMode::ThreadLocalLds);
        let ws = tape_capacity_for_mode(TapeStorageMode::WorkgroupShared);
        let ssbo = tape_capacity_for_mode(TapeStorageMode::GlobalSsbo);
        assert!(lds < ws);
        assert!(ws < ssbo);
    }

    #[test]
    fn estimated_bytes_grows_with_workgroup_size() {
        let small = OperationDensity {
            op_count: 16,
            avg_operand_count: 2,
            workgroup_size: 1,
            ..Default::default()
        };
        let large = OperationDensity {
            workgroup_size: 256,
            ..small
        };
        assert!(large.estimated_total_bytes() > small.estimated_total_bytes());
    }
}
