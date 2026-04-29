//! Dispatch arguments for compute / mesh nodes.
//!
//! § DESIGN
//!   `DispatchArgs` is the immutable shape `(x, y, z)` of a thread-group
//!   grid. Construction validates that all dimensions are ≥ 1 (zero-shape
//!   refused at the builder boundary).
//!
//! § GROUPING
//!   `DispatchGroup` aggregates several `DispatchArgs` for batched indirect
//!   dispatch — used by the DGC fallback to write a single sequence buffer.

/// Thread-group grid for a compute / mesh dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DispatchArgs {
    /// Thread-groups along X.
    pub x: u32,
    /// Thread-groups along Y.
    pub y: u32,
    /// Thread-groups along Z.
    pub z: u32,
}

impl DispatchArgs {
    /// Construct (any zero clamps to 1 on `validate` ; raw store keeps user value).
    #[must_use]
    pub const fn new(x: u32, y: u32, z: u32) -> Self {
        Self { x, y, z }
    }

    /// Single thread-group.
    #[must_use]
    pub const fn single() -> Self {
        Self::new(1, 1, 1)
    }

    /// Total thread-groups (x * y * z).
    #[must_use]
    pub const fn total_groups(self) -> u64 {
        (self.x as u64) * (self.y as u64) * (self.z as u64)
    }

    /// Validate : refuse zero in any dim.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.x > 0 && self.y > 0 && self.z > 0
    }

    /// Returns true iff the total group-count fits the D3D12_DISPATCH_THREAD_LIMIT
    /// (65535 in any dim ; total ≤ 2^32 - 1).
    #[must_use]
    pub const fn fits_d3d12_limit(self) -> bool {
        self.x <= 65_535
            && self.y <= 65_535
            && self.z <= 65_535
            && self.total_groups() <= u32::MAX as u64
    }

    /// Returns true iff the total group-count fits the typical Vulkan
    /// `maxComputeWorkGroupCount` of (65535, 65535, 65535).
    #[must_use]
    pub const fn fits_vulkan_limit(self) -> bool {
        self.x <= 65_535 && self.y <= 65_535 && self.z <= 65_535
    }
}

/// A batched group of [`DispatchArgs`] entries (for indirect-dispatch
/// streams).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DispatchGroup {
    entries: Vec<DispatchArgs>,
}

impl DispatchGroup {
    /// Empty.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Push a single entry.
    pub fn push(&mut self, args: DispatchArgs) {
        self.entries.push(args);
    }

    /// Slice.
    #[must_use]
    pub fn entries(&self) -> &[DispatchArgs] {
        &self.entries
    }

    /// Length.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Empty?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Sum of total-groups across entries (capacity check).
    #[must_use]
    pub fn aggregate_groups(&self) -> u64 {
        self.entries.iter().map(|d| d.total_groups()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::{DispatchArgs, DispatchGroup};

    #[test]
    fn single_is_one_group() {
        assert_eq!(DispatchArgs::single().total_groups(), 1);
    }

    #[test]
    fn zero_dim_invalid() {
        assert!(!DispatchArgs::new(0, 1, 1).is_valid());
        assert!(!DispatchArgs::new(1, 0, 1).is_valid());
        assert!(!DispatchArgs::new(1, 1, 0).is_valid());
    }

    #[test]
    fn nonzero_valid() {
        assert!(DispatchArgs::new(1, 1, 1).is_valid());
        assert!(DispatchArgs::new(8, 8, 1).is_valid());
    }

    #[test]
    fn fits_d3d12_limit_ok_for_typical() {
        assert!(DispatchArgs::new(64, 64, 1).fits_d3d12_limit());
        assert!(DispatchArgs::new(65_535, 1, 1).fits_d3d12_limit());
    }

    #[test]
    fn fits_d3d12_limit_overflow_dim() {
        assert!(!DispatchArgs::new(65_536, 1, 1).fits_d3d12_limit());
    }

    #[test]
    fn fits_vulkan_limit_matches_d3d12_per_dim() {
        assert!(DispatchArgs::new(65_535, 65_535, 1).fits_vulkan_limit());
        assert!(!DispatchArgs::new(70_000, 1, 1).fits_vulkan_limit());
    }

    #[test]
    fn group_push_grows() {
        let mut g = DispatchGroup::new();
        g.push(DispatchArgs::new(2, 2, 1));
        g.push(DispatchArgs::new(4, 4, 1));
        assert_eq!(g.len(), 2);
        assert!(!g.is_empty());
    }

    #[test]
    fn group_aggregate_sums_correctly() {
        let mut g = DispatchGroup::new();
        g.push(DispatchArgs::new(2, 2, 1));
        g.push(DispatchArgs::new(4, 4, 1));
        assert_eq!(g.aggregate_groups(), 4 + 16);
    }

    #[test]
    fn group_default_empty() {
        let g = DispatchGroup::default();
        assert!(g.is_empty());
    }

    #[test]
    fn group_entries_slice() {
        let mut g = DispatchGroup::new();
        g.push(DispatchArgs::new(1, 1, 1));
        assert_eq!(g.entries().len(), 1);
    }
}
