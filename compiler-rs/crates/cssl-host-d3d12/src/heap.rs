//! D3D12 heap + command-list + descriptor-heap enumerations.

/// `D3D12_COMMAND_LIST_TYPE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandListType {
    /// `D3D12_COMMAND_LIST_TYPE_DIRECT` — 3D + compute + copy.
    Direct,
    /// `D3D12_COMMAND_LIST_TYPE_COMPUTE`.
    Compute,
    /// `D3D12_COMMAND_LIST_TYPE_COPY`.
    Copy,
    /// `D3D12_COMMAND_LIST_TYPE_BUNDLE`.
    Bundle,
    /// `D3D12_COMMAND_LIST_TYPE_VIDEO_DECODE`.
    VideoDecode,
    /// `D3D12_COMMAND_LIST_TYPE_VIDEO_PROCESS`.
    VideoProcess,
    /// `D3D12_COMMAND_LIST_TYPE_VIDEO_ENCODE`.
    VideoEncode,
}

impl CommandListType {
    /// Short name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Compute => "compute",
            Self::Copy => "copy",
            Self::Bundle => "bundle",
            Self::VideoDecode => "video-decode",
            Self::VideoProcess => "video-process",
            Self::VideoEncode => "video-encode",
        }
    }

    /// All 7 list types.
    pub const ALL_TYPES: [Self; 7] = [
        Self::Direct,
        Self::Compute,
        Self::Copy,
        Self::Bundle,
        Self::VideoDecode,
        Self::VideoProcess,
        Self::VideoEncode,
    ];
}

/// `D3D12_DESCRIPTOR_HEAP_TYPE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DescriptorHeapType {
    /// CBV + SRV + UAV.
    CbvSrvUav,
    /// Sampler.
    Sampler,
    /// RTV.
    Rtv,
    /// DSV.
    Dsv,
}

impl DescriptorHeapType {
    /// Short name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CbvSrvUav => "cbv-srv-uav",
            Self::Sampler => "sampler",
            Self::Rtv => "rtv",
            Self::Dsv => "dsv",
        }
    }
}

/// `D3D12_HEAP_TYPE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HeapType {
    /// GPU-local memory.
    Default,
    /// CPU-write / GPU-read staging.
    Upload,
    /// CPU-read / GPU-write staging.
    Readback,
    /// Custom — CPU + GPU page properties specified manually.
    Custom,
}

impl HeapType {
    /// Short name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Upload => "upload",
            Self::Readback => "readback",
            Self::Custom => "custom",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CommandListType, DescriptorHeapType, HeapType};

    #[test]
    fn command_list_type_count() {
        assert_eq!(CommandListType::ALL_TYPES.len(), 7);
    }

    #[test]
    fn command_list_type_names() {
        assert_eq!(CommandListType::Direct.as_str(), "direct");
        assert_eq!(CommandListType::VideoEncode.as_str(), "video-encode");
    }

    #[test]
    fn descriptor_heap_type_names() {
        assert_eq!(DescriptorHeapType::CbvSrvUav.as_str(), "cbv-srv-uav");
        assert_eq!(DescriptorHeapType::Sampler.as_str(), "sampler");
    }

    #[test]
    fn heap_type_names() {
        assert_eq!(HeapType::Default.as_str(), "default");
        assert_eq!(HeapType::Upload.as_str(), "upload");
        assert_eq!(HeapType::Readback.as_str(), "readback");
    }
}
