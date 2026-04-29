//! Vulkan device-generated-commands hooks.
//!
//! § SLICE T11-D123 (W4-09)
//!
//! Provides extension probe + indirect-commands-layout descriptor for the
//! Vulkan fallback path. Two extensions are catalogued :
//!
//!   - `VK_NV_device_generated_commands` (initial NV-only ; broad device
//!     coverage today). Token-stream layout matches what
//!     `cssl-work-graph::DgcSequence` produces.
//!   - `VK_EXT_device_generated_commands` (newer Khronos ratification ;
//!     same shape with a different VK_DGC_TOKEN_TYPE_EXT enum). The
//!     compiler emits identical token streams for both ; the runtime
//!     binds whichever extension is available.
//!
//! § STAGE-0 SCOPE
//!   This slice :
//!     - declares the extension constants
//!     - exposes a probe trait `DgcProbe`
//!     - declares the indirect-commands-layout descriptor
//!
//!   Real `ash`-backed binding lives in `ffi/dgc.rs` in a follow-up
//!   dispatch ; here we focus on the API + lint-clean cross-platform
//!   surface.

/// Which Vulkan DGC extension is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DgcExtension {
    /// `VK_NV_device_generated_commands` (NV-extension, broad device base).
    Nv,
    /// `VK_EXT_device_generated_commands` (Khronos-promoted ; future-default).
    Ext,
    /// Neither is supported.
    None,
}

impl DgcExtension {
    /// Stable string tag.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Nv => "VK_NV_device_generated_commands",
            Self::Ext => "VK_EXT_device_generated_commands",
            Self::None => "none",
        }
    }

    /// True iff some DGC variant is available.
    #[must_use]
    pub const fn is_supported(self) -> bool {
        matches!(self, Self::Nv | Self::Ext)
    }
}

/// Per-token kind in an indirect-commands-layout (mirrors
/// `VkIndirectCommandsTokenTypeNV` / `VkIndirectCommandsTokenTypeEXT`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DgcTokenKind {
    /// `VK_INDIRECT_COMMANDS_TOKEN_TYPE_PIPELINE_NV`.
    Pipeline,
    /// `VK_INDIRECT_COMMANDS_TOKEN_TYPE_PUSH_CONSTANT_NV`.
    PushConstant,
    /// `VK_INDIRECT_COMMANDS_TOKEN_TYPE_DISPATCH_NV`.
    Dispatch,
    /// `VK_INDIRECT_COMMANDS_TOKEN_TYPE_DRAW_TASKS_NV` (mesh).
    DispatchMesh,
    /// `VK_INDIRECT_COMMANDS_TOKEN_TYPE_DRAW_NV`.
    Draw,
    /// `VK_INDIRECT_COMMANDS_TOKEN_TYPE_INDEX_BUFFER_NV`.
    IndexBuffer,
    /// `VK_INDIRECT_COMMANDS_TOKEN_TYPE_VERTEX_BUFFER_NV`.
    VertexBuffer,
}

impl DgcTokenKind {
    /// Stable string tag.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pipeline => "pipeline",
            Self::PushConstant => "push-constant",
            Self::Dispatch => "dispatch",
            Self::DispatchMesh => "dispatch-mesh",
            Self::Draw => "draw",
            Self::IndexBuffer => "index-buffer",
            Self::VertexBuffer => "vertex-buffer",
        }
    }

    /// On-wire byte-size estimate for this token-kind.
    #[must_use]
    pub const fn wire_size(self) -> u32 {
        match self {
            Self::Pipeline => 4,
            Self::PushConstant => 8, // 4 byte offset + 4 byte size
            Self::Dispatch | Self::DispatchMesh => 12, // x,y,z u32
            Self::Draw => 16,        // vertex_count, instance_count, first_vertex, first_instance
            Self::IndexBuffer => 8,
            Self::VertexBuffer => 8,
        }
    }
}

/// Per-token descriptor for `VkIndirectCommandsLayoutTokenNV`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DgcLayoutToken {
    /// Kind.
    pub kind: DgcTokenKind,
    /// Stream offset in bytes (0 if single-stream).
    pub stream_offset: u32,
    /// Stride between consecutive tokens of this kind.
    pub stride: u32,
}

impl DgcLayoutToken {
    /// Construct.
    #[must_use]
    pub const fn new(kind: DgcTokenKind, stream_offset: u32) -> Self {
        Self {
            kind,
            stream_offset,
            stride: kind.wire_size(),
        }
    }
}

/// `VkIndirectCommandsLayoutNV` descriptor (compiled from a [`DgcSequence`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DgcLayoutDesc {
    /// Tokens in stream order.
    pub tokens: Vec<DgcLayoutToken>,
    /// Maximum sequence count this layout will dispatch.
    pub max_sequence_count: u32,
    /// Optional debug label.
    pub label: Option<String>,
}

impl DgcLayoutDesc {
    /// Empty.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            tokens: Vec::new(),
            max_sequence_count: 0,
            label: None,
        }
    }

    /// Builder : label.
    #[must_use]
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = Some(l.into());
        self
    }

    /// Builder : max-sequence-count.
    #[must_use]
    pub const fn with_max_sequence_count(mut self, n: u32) -> Self {
        self.max_sequence_count = n;
        self
    }

    /// Append a token.
    pub fn push(&mut self, t: DgcLayoutToken) {
        self.tokens.push(t);
    }

    /// Total stream size in bytes.
    #[must_use]
    pub fn stream_size_bytes(&self) -> u32 {
        self.tokens.iter().map(|t| t.kind.wire_size()).sum()
    }

    /// Number of tokens.
    #[must_use]
    pub fn token_count(&self) -> usize {
        self.tokens.len()
    }

    /// Empty?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }
}

/// Probe trait — concrete impl in a follow-up FFI dispatch.
pub trait DgcProbe {
    /// Detect which DGC extension (if any) is supported.
    fn probe_dgc(&self) -> DgcExtension;
}

/// Stub probe used for unit-testing — always returns `None`.
#[derive(Debug, Default, Clone, Copy)]
pub struct StubDgcProbe;

impl DgcProbe for StubDgcProbe {
    fn probe_dgc(&self) -> DgcExtension {
        DgcExtension::None
    }
}

/// Stub probe variant that reports NV-extension support (for tests).
#[derive(Debug, Default, Clone, Copy)]
pub struct NvDgcProbe;

impl DgcProbe for NvDgcProbe {
    fn probe_dgc(&self) -> DgcExtension {
        DgcExtension::Nv
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DgcExtension, DgcLayoutDesc, DgcLayoutToken, DgcProbe, DgcTokenKind, NvDgcProbe,
        StubDgcProbe,
    };

    #[test]
    fn extension_strings() {
        assert_eq!(DgcExtension::Nv.as_str(), "VK_NV_device_generated_commands");
        assert_eq!(
            DgcExtension::Ext.as_str(),
            "VK_EXT_device_generated_commands"
        );
        assert_eq!(DgcExtension::None.as_str(), "none");
    }

    #[test]
    fn extension_supported_flag() {
        assert!(DgcExtension::Nv.is_supported());
        assert!(DgcExtension::Ext.is_supported());
        assert!(!DgcExtension::None.is_supported());
    }

    #[test]
    fn token_kind_wire_size() {
        assert_eq!(DgcTokenKind::Dispatch.wire_size(), 12);
        assert_eq!(DgcTokenKind::DispatchMesh.wire_size(), 12);
        assert_eq!(DgcTokenKind::Pipeline.wire_size(), 4);
        assert_eq!(DgcTokenKind::Draw.wire_size(), 16);
    }

    #[test]
    fn token_kind_tag_kebab() {
        for k in [
            DgcTokenKind::Pipeline,
            DgcTokenKind::PushConstant,
            DgcTokenKind::Dispatch,
            DgcTokenKind::DispatchMesh,
        ] {
            let s = k.as_str();
            assert!(!s.is_empty());
            assert!(!s.contains('_'));
        }
    }

    #[test]
    fn layout_token_stride_matches_wire_size() {
        let t = DgcLayoutToken::new(DgcTokenKind::Dispatch, 0);
        assert_eq!(t.stride, 12);
    }

    #[test]
    fn layout_desc_round_trips_label() {
        let d = DgcLayoutDesc::new()
            .with_label("test")
            .with_max_sequence_count(64);
        assert_eq!(d.label.as_deref(), Some("test"));
        assert_eq!(d.max_sequence_count, 64);
    }

    #[test]
    fn layout_desc_token_count_grows() {
        let mut d = DgcLayoutDesc::new();
        d.push(DgcLayoutToken::new(DgcTokenKind::Pipeline, 0));
        d.push(DgcLayoutToken::new(DgcTokenKind::Dispatch, 4));
        assert_eq!(d.token_count(), 2);
    }

    #[test]
    fn layout_desc_stream_size() {
        let mut d = DgcLayoutDesc::new();
        d.push(DgcLayoutToken::new(DgcTokenKind::Pipeline, 0));
        d.push(DgcLayoutToken::new(DgcTokenKind::Dispatch, 4));
        // 4 (Pipeline) + 12 (Dispatch) = 16 bytes
        assert_eq!(d.stream_size_bytes(), 16);
    }

    #[test]
    fn empty_desc_is_empty() {
        let d = DgcLayoutDesc::new();
        assert!(d.is_empty());
        assert_eq!(d.stream_size_bytes(), 0);
    }

    #[test]
    fn stub_probe_returns_none() {
        let p = StubDgcProbe;
        assert_eq!(p.probe_dgc(), DgcExtension::None);
    }

    #[test]
    fn nv_probe_returns_nv() {
        let p = NvDgcProbe;
        assert_eq!(p.probe_dgc(), DgcExtension::Nv);
    }
}
