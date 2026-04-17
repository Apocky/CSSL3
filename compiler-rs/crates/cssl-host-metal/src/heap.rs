//! Metal heap + resource-option enumeration.

/// `MTLStorageMode` (effective heap kind).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetalHeapType {
    /// `MTLStorageModeShared` — CPU + GPU shared.
    Shared,
    /// `MTLStorageModePrivate` — GPU-only.
    Private,
    /// `MTLStorageModeManaged` — CPU-synchronized.
    Managed,
    /// `MTLStorageModeMemoryless` — tile-memory only.
    Memoryless,
}

impl MetalHeapType {
    /// Short name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::Private => "private",
            Self::Managed => "managed",
            Self::Memoryless => "memoryless",
        }
    }
}

/// `MTLResourceOptions` subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MetalResourceOptions {
    /// Hazard-tracked by the runtime.
    pub hazard_tracked: bool,
    /// CPU cache mode : default vs write-combined.
    pub cpu_cache_mode_default: bool,
    /// Storage mode.
    pub storage_mode: MetalHeapType,
}

impl MetalResourceOptions {
    /// Default (shared + hazard-tracked + cpu-cache-default).
    #[must_use]
    pub const fn default_shared() -> Self {
        Self {
            hazard_tracked: true,
            cpu_cache_mode_default: true,
            storage_mode: MetalHeapType::Shared,
        }
    }

    /// Private + hazard-tracked (GPU-only).
    #[must_use]
    pub const fn gpu_private() -> Self {
        Self {
            hazard_tracked: true,
            cpu_cache_mode_default: true,
            storage_mode: MetalHeapType::Private,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MetalHeapType, MetalResourceOptions};

    #[test]
    fn heap_type_names() {
        assert_eq!(MetalHeapType::Shared.as_str(), "shared");
        assert_eq!(MetalHeapType::Private.as_str(), "private");
        assert_eq!(MetalHeapType::Managed.as_str(), "managed");
        assert_eq!(MetalHeapType::Memoryless.as_str(), "memoryless");
    }

    #[test]
    fn default_shared_options() {
        let o = MetalResourceOptions::default_shared();
        assert_eq!(o.storage_mode, MetalHeapType::Shared);
        assert!(o.hazard_tracked);
    }

    #[test]
    fn gpu_private_options() {
        let o = MetalResourceOptions::gpu_private();
        assert_eq!(o.storage_mode, MetalHeapType::Private);
    }
}
