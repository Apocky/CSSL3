//! Central error type for the D3D12 host backend.
//!
//! § DESIGN
//!   `D3d12Error` is the canonical error type for every fallible call in this
//!   crate. It folds three sources :
//!     - `Loader` — the FFI layer is unwired (non-Windows targets).
//!     - `Hresult` — a real D3D12 / DXGI HRESULT failure ; carries the raw value
//!       + the call site's free-form context for diagnostics.
//!     - `NotSupported` — a feature-level, capability, or descriptor-heap shape
//!       wasn't supported by the chosen adapter.
//!     - `InvalidArgument` — a builder-side precondition failed (zero-sized
//!       resource, root-signature with zero parameters, etc).
//!
//! § INTEGRATION
//!   Per `specs/14_BACKEND.csl § HOST-SUBMIT BACKENDS § D3D12`, the host-FFI
//!   error surface must be diagnosable independent of the underlying DXGI
//!   transport. The `Hresult` variant therefore carries both the raw `i32`
//!   HRESULT and a contextual string assembled at the call site — this means
//!   a caller can match on the kind without losing the location.

use thiserror::Error;

/// Errors returned by the D3D12 host backend.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum D3d12Error {
    /// Underlying D3D12 / DXGI loader is missing or this is a non-Windows target.
    #[error("D3D12 loader unavailable — {detail}")]
    LoaderMissing { detail: String },

    /// Stub probe used (no real FFI active).
    #[error("D3D12 FFI not wired (stub probe in use)")]
    FfiNotWired,

    /// A D3D12 / DXGI call returned a failing HRESULT.
    #[error("D3D12 call `{context}` failed (HRESULT 0x{hresult:08x}): {message}")]
    Hresult {
        /// Free-form description of which call failed (e.g., `"D3D12CreateDevice"`).
        context: String,
        /// Raw HRESULT integer.
        hresult: i32,
        /// Human-readable message (system-format if available).
        message: String,
    },

    /// Adapter / device does not support the requested feature.
    #[error("D3D12 feature unsupported: {feature}")]
    NotSupported {
        /// Which feature / capability is missing.
        feature: String,
    },

    /// Builder-side argument precondition failed.
    #[error("D3D12 invalid argument for `{site}`: {reason}")]
    InvalidArgument {
        /// Where the invalid argument was detected.
        site: String,
        /// Why it was rejected.
        reason: String,
    },

    /// No suitable adapter was found (e.g., no hardware adapter and `prefer_hardware=true`).
    #[error("no suitable D3D12 adapter found ({reason})")]
    AdapterNotFound {
        /// Why no adapter passed.
        reason: String,
    },

    /// A timeout elapsed while waiting on a fence / event.
    #[error("D3D12 fence wait timed out after {millis} ms")]
    FenceTimeout {
        /// Timeout that elapsed.
        millis: u32,
    },
}

impl D3d12Error {
    /// Build a `Hresult` variant. Convenience for FFI sites.
    #[must_use]
    pub fn hresult(context: impl Into<String>, hresult: i32, message: impl Into<String>) -> Self {
        Self::Hresult {
            context: context.into(),
            hresult,
            message: message.into(),
        }
    }

    /// Build a `LoaderMissing` variant.
    #[must_use]
    pub fn loader(detail: impl Into<String>) -> Self {
        Self::LoaderMissing {
            detail: detail.into(),
        }
    }

    /// Build a `NotSupported` variant.
    #[must_use]
    pub fn unsupported(feature: impl Into<String>) -> Self {
        Self::NotSupported {
            feature: feature.into(),
        }
    }

    /// Build an `InvalidArgument` variant.
    #[must_use]
    pub fn invalid(site: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::InvalidArgument {
            site: site.into(),
            reason: reason.into(),
        }
    }

    /// Build an `AdapterNotFound` variant.
    #[must_use]
    pub fn no_adapter(reason: impl Into<String>) -> Self {
        Self::AdapterNotFound {
            reason: reason.into(),
        }
    }

    /// Is this error a "host doesn't have D3D12" condition (skip-test territory)
    /// rather than a real bug ? Per S6-A4 BinaryMissing-skip precedent.
    #[must_use]
    pub const fn is_loader_missing(&self) -> bool {
        matches!(self, Self::LoaderMissing { .. } | Self::FfiNotWired)
    }
}

/// Crate-wide `Result` alias.
pub type Result<T> = core::result::Result<T, D3d12Error>;

#[cfg(test)]
mod tests {
    use super::D3d12Error;

    #[test]
    fn hresult_constructor_renders_human_message() {
        // 0x887a0005 = DXGI_ERROR_DEVICE_REMOVED ; cast through i32 via i32::from_ne_bytes.
        let raw_hresult = i32::from_ne_bytes(0x887a0005_u32.to_ne_bytes());
        let e = D3d12Error::hresult("D3D12CreateDevice", raw_hresult, "device removed");
        assert!(matches!(e, D3d12Error::Hresult { .. }));
        let s = format!("{e}");
        assert!(s.contains("D3D12CreateDevice"));
        assert!(s.contains("0x887a0005"));
        assert!(s.contains("device removed"));
    }

    #[test]
    fn loader_constructor_carries_detail() {
        let e = D3d12Error::loader("d3d12.dll absent on PATH");
        match e {
            D3d12Error::LoaderMissing { detail } => assert!(detail.contains("d3d12.dll")),
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn unsupported_constructor() {
        let e = D3d12Error::unsupported("MeshShaderTier1");
        let s = format!("{e}");
        assert!(s.contains("MeshShaderTier1"));
    }

    #[test]
    fn invalid_constructor() {
        let e = D3d12Error::invalid("ResourceDesc", "width=0");
        let s = format!("{e}");
        assert!(s.contains("ResourceDesc"));
        assert!(s.contains("width=0"));
    }

    #[test]
    fn no_adapter_constructor() {
        let e = D3d12Error::no_adapter("only WARP available; prefer_hardware=true");
        assert!(format!("{e}").contains("WARP"));
    }

    #[test]
    fn is_loader_missing_classification() {
        assert!(D3d12Error::loader("x").is_loader_missing());
        assert!(D3d12Error::FfiNotWired.is_loader_missing());
        assert!(!D3d12Error::unsupported("x").is_loader_missing());
        assert!(!D3d12Error::invalid("a", "b").is_loader_missing());
    }

    #[test]
    fn fence_timeout_renders_millis() {
        let e = D3d12Error::FenceTimeout { millis: 5_000 };
        assert!(format!("{e}").contains("5000"));
    }
}
