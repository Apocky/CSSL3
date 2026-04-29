//! Event types for the hot-reload pump.
//!
//! § DESIGN
//!
//! `SwapKind` is the closed enum of supported hot-swap targets. The four
//! variants mirror § 3.2 of the L4 hot-reload spec :
//!
//! - `Asset { kind, path_hash, handle }`
//! - `Shader { kind, path_hash, pipeline }`
//! - `Config { kind, path_hash, subsystem }`
//! - `KanWeight { network_handle, fingerprint_pre, fingerprint_post }`
//!
//! `SwapEvent` is the queued envelope : a `SwapKind` plus the logical frame
//! number the event was pushed on, plus a monotone `sequence` index. Logical
//! frames are the ordinal authority — the replay log keys on them and never
//! on wall-clock. Calling `std::time::Instant::now()` anywhere in this crate
//! is forbidden (see crate-level docs).

#![allow(clippy::module_name_repetitions)]

/// Kind of asset reloaded — selects which decoder + GPU upload the engine
/// will route through. Stage-0 is opaque (no parsers live here).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssetKind {
    /// PNG image → `cssl-asset::ImageBuffer<Rgba<u8>>` → GPU texture upload.
    Png,
    /// glTF mesh + skin → vertex / index buffer upload.
    Gltf,
    /// WAV sample → audio engine sample registry update.
    Wav,
    /// TTF font → glyph atlas re-rasterization (lazy).
    Ttf,
}

impl AssetKind {
    /// Logical extension typically observed for this kind (lower-case, no dot).
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Gltf => "gltf",
            Self::Wav => "wav",
            Self::Ttf => "ttf",
        }
    }

    /// Is this kind a binary blob the engine GPU-uploads ?
    #[must_use]
    pub const fn is_gpu_resource(self) -> bool {
        matches!(self, Self::Png | Self::Gltf)
    }
}

/// Kind of shader reloaded. Determines validator + pipeline-build path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShaderKind {
    /// Vulkan SPIR-V module — validated via `spirv-val` + entry-point check.
    SpirV,
    /// Direct3D 12 DXIL bytecode — validated via dxc + signature match.
    Dxil,
    /// Apple Metal Shading Language source — Metal compiler validate + AIR.
    Msl,
    /// WebGPU Shading Language source — naga validate + reflection.
    Wgsl,
}

impl ShaderKind {
    /// Diagnostic name (lower-case).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::SpirV => "spirv",
            Self::Dxil => "dxil",
            Self::Msl => "msl",
            Self::Wgsl => "wgsl",
        }
    }
}

/// Kind of config reloaded. Each kind is bound to a typed-struct schema +
/// per-subsystem `re_init` callback (§ 3.7 spec).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConfigKind {
    /// `engine.toml` → top-level engine config.
    Engine,
    /// `render.toml` → renderer tunables.
    RenderTunables,
    /// `ai.toml` → AI subsystem tunables.
    AiTunables,
    /// `physics.toml` → physics subsystem tunables.
    PhysicsTunables,
    /// `audio.toml` → audio subsystem tunables.
    AudioTunables,
    /// `cap_budget.toml` → Cap-budget table.
    CapBudget,
    /// `replay.toml` → replay-policy.
    ReplayPolicy,
}

impl ConfigKind {
    /// Diagnostic name (lower-case).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Engine => "engine",
            Self::RenderTunables => "render-tunables",
            Self::AiTunables => "ai-tunables",
            Self::PhysicsTunables => "physics-tunables",
            Self::AudioTunables => "audio-tunables",
            Self::CapBudget => "cap-budget",
            Self::ReplayPolicy => "replay-policy",
        }
    }
}

/// Closed enum of hot-swap targets. Kept fully concrete : every variant is a
/// stage-0 mock (no real-world handles, just opaque IDs). Real types
/// (`AssetHandle`, `PipelineHandle`, `KanNetworkHandle`) wire in at Wave-Jη.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SwapKind {
    /// Asset hot-swap (PNG / GLTF / WAV / TTF).
    Asset {
        /// Decoder selection.
        kind: AssetKind,
        /// BLAKE3 hash of the canonical path the watcher fired on. Stage-0
        /// uses any 32-byte slice — the audit chain hasn't wired to BLAKE3
        /// in this crate yet ; the field is preserved verbatim into the
        /// replay log so a later re-hash matches.
        path_hash: [u8; 32],
        /// Asset-handle ID assigned by the engine. Stage-0 = opaque u64.
        handle: u64,
    },
    /// Shader pipeline rebuild.
    Shader {
        /// Validator selection.
        kind: ShaderKind,
        /// BLAKE3 hash of the canonical shader source path.
        path_hash: [u8; 32],
        /// Pipeline-handle ID.
        pipeline: u64,
    },
    /// Config subsystem live re-init.
    Config {
        /// Subsystem schema selection.
        kind: ConfigKind,
        /// BLAKE3 hash of the canonical config path.
        path_hash: [u8; 32],
        /// Opaque subsystem-ID (e.g., renderer, AI, physics).
        subsystem: u64,
    },
    /// KAN-network weight hot-swap (preserves persistent-kernel residency).
    KanWeight {
        /// Opaque KAN-network handle.
        network_handle: u64,
        /// BLAKE3 fingerprint of the OLD weight bundle.
        fingerprint_pre: [u8; 32],
        /// BLAKE3 fingerprint of the NEW weight bundle.
        fingerprint_post: [u8; 32],
    },
}

impl SwapKind {
    /// Tag-name (one of `asset` / `shader` / `config` / `kan-weight`).
    #[must_use]
    pub const fn tag(&self) -> &'static str {
        match self {
            Self::Asset { .. } => "asset",
            Self::Shader { .. } => "shader",
            Self::Config { .. } => "config",
            Self::KanWeight { .. } => "kan-weight",
        }
    }

    /// Returns the path-hash for the three filesystem-bound variants ;
    /// `KanWeight` returns `None` (weights flow through an in-memory bundle).
    #[must_use]
    pub fn path_hash(&self) -> Option<&[u8; 32]> {
        match self {
            Self::Asset { path_hash, .. }
            | Self::Shader { path_hash, .. }
            | Self::Config { path_hash, .. } => Some(path_hash),
            Self::KanWeight { .. } => None,
        }
    }

    /// Is this swap a no-op (KAN weights with identical pre/post fingerprint) ?
    #[must_use]
    pub fn is_noop(&self) -> bool {
        match self {
            Self::KanWeight {
                fingerprint_pre,
                fingerprint_post,
                ..
            } => fingerprint_pre == fingerprint_post,
            _ => false,
        }
    }
}

/// Logical frame index (monotone u64 ; NEVER wall-clock).
pub type FrameId = u64;

/// Queue envelope wrapping a `SwapKind`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SwapEvent {
    /// Logical frame the event was pushed on.
    pub frame_id: FrameId,
    /// Per-frame monotone index (0, 1, 2, … resets per frame).
    pub sequence: u32,
    /// Wrapped swap kind.
    pub kind: SwapKind,
}

impl SwapEvent {
    /// Construct a new event.
    #[must_use]
    pub const fn new(frame_id: FrameId, sequence: u32, kind: SwapKind) -> Self {
        Self {
            frame_id,
            sequence,
            kind,
        }
    }

    /// Order key (frame, sequence) for stable sort.
    #[must_use]
    pub const fn order_key(&self) -> (FrameId, u32) {
        (self.frame_id, self.sequence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    #[test]
    fn asset_kind_extensions() {
        assert_eq!(AssetKind::Png.extension(), "png");
        assert_eq!(AssetKind::Gltf.extension(), "gltf");
        assert_eq!(AssetKind::Wav.extension(), "wav");
        assert_eq!(AssetKind::Ttf.extension(), "ttf");
    }

    #[test]
    fn asset_kind_gpu_classification() {
        assert!(AssetKind::Png.is_gpu_resource());
        assert!(AssetKind::Gltf.is_gpu_resource());
        assert!(!AssetKind::Wav.is_gpu_resource());
        assert!(!AssetKind::Ttf.is_gpu_resource());
    }

    #[test]
    fn shader_kind_names() {
        assert_eq!(ShaderKind::SpirV.name(), "spirv");
        assert_eq!(ShaderKind::Dxil.name(), "dxil");
        assert_eq!(ShaderKind::Msl.name(), "msl");
        assert_eq!(ShaderKind::Wgsl.name(), "wgsl");
    }

    #[test]
    fn config_kind_names() {
        assert_eq!(ConfigKind::Engine.name(), "engine");
        assert_eq!(ConfigKind::RenderTunables.name(), "render-tunables");
        assert_eq!(ConfigKind::AiTunables.name(), "ai-tunables");
        assert_eq!(ConfigKind::PhysicsTunables.name(), "physics-tunables");
        assert_eq!(ConfigKind::AudioTunables.name(), "audio-tunables");
        assert_eq!(ConfigKind::CapBudget.name(), "cap-budget");
        assert_eq!(ConfigKind::ReplayPolicy.name(), "replay-policy");
    }

    #[test]
    fn swap_kind_tags() {
        let asset = SwapKind::Asset {
            kind: AssetKind::Png,
            path_hash: h(1),
            handle: 7,
        };
        let shader = SwapKind::Shader {
            kind: ShaderKind::SpirV,
            path_hash: h(2),
            pipeline: 8,
        };
        let config = SwapKind::Config {
            kind: ConfigKind::Engine,
            path_hash: h(3),
            subsystem: 9,
        };
        let kan = SwapKind::KanWeight {
            network_handle: 10,
            fingerprint_pre: h(4),
            fingerprint_post: h(5),
        };
        assert_eq!(asset.tag(), "asset");
        assert_eq!(shader.tag(), "shader");
        assert_eq!(config.tag(), "config");
        assert_eq!(kan.tag(), "kan-weight");
    }

    #[test]
    fn swap_kind_path_hash_present_for_filesystem_variants() {
        let asset = SwapKind::Asset {
            kind: AssetKind::Png,
            path_hash: h(11),
            handle: 1,
        };
        let shader = SwapKind::Shader {
            kind: ShaderKind::Wgsl,
            path_hash: h(12),
            pipeline: 1,
        };
        let config = SwapKind::Config {
            kind: ConfigKind::AiTunables,
            path_hash: h(13),
            subsystem: 1,
        };
        assert_eq!(asset.path_hash(), Some(&h(11)));
        assert_eq!(shader.path_hash(), Some(&h(12)));
        assert_eq!(config.path_hash(), Some(&h(13)));
    }

    #[test]
    fn swap_kind_path_hash_absent_for_kan() {
        let kan = SwapKind::KanWeight {
            network_handle: 1,
            fingerprint_pre: h(0),
            fingerprint_post: h(1),
        };
        assert_eq!(kan.path_hash(), None);
    }

    #[test]
    fn kan_swap_noop_detection() {
        let same = SwapKind::KanWeight {
            network_handle: 1,
            fingerprint_pre: h(7),
            fingerprint_post: h(7),
        };
        let diff = SwapKind::KanWeight {
            network_handle: 1,
            fingerprint_pre: h(7),
            fingerprint_post: h(8),
        };
        assert!(same.is_noop());
        assert!(!diff.is_noop());
    }

    #[test]
    fn non_kan_swaps_are_never_noop() {
        let asset = SwapKind::Asset {
            kind: AssetKind::Png,
            path_hash: h(1),
            handle: 1,
        };
        assert!(!asset.is_noop());
    }

    #[test]
    fn swap_event_order_key() {
        let kind = SwapKind::Asset {
            kind: AssetKind::Png,
            path_hash: h(0),
            handle: 0,
        };
        let e = SwapEvent::new(42, 3, kind);
        assert_eq!(e.order_key(), (42, 3));
        assert_eq!(e.frame_id, 42);
        assert_eq!(e.sequence, 3);
    }

    #[test]
    fn swap_event_clone_eq() {
        let kind = SwapKind::Config {
            kind: ConfigKind::ReplayPolicy,
            path_hash: h(99),
            subsystem: 7,
        };
        let a = SwapEvent::new(1, 0, kind);
        let mut v = vec![a.clone()];
        v.push(a);
        assert_eq!(v[0], v[1]);
        assert_eq!(v[0].frame_id, 1);
    }
}
