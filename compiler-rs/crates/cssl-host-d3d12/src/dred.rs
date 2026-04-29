//! D3D12 diagnostic capture (DRED + ID3D12InfoQueue).
//!
//! § DESIGN
//!   `DredCapture` is a host-side ring buffer of diagnostic messages drained
//!   from `ID3D12InfoQueue` (when the debug layer is on) and DRED breadcrumbs
//!   (when the device removed and DRED was enabled).
//!
//! § INTEGRATION
//!   This module is wired into `cssl-rt` telemetry via the `tap_into` /
//!   `drain` pair. Per `specs/22_TELEMETRY § R18`, host-side diagnostic
//!   messages flow through the same ring as runtime + GPU sysman data.
//!   The full integration is gated on cssl-rt's host-diagnostics-channel
//!   landing in a follow-up slice ; for S6-E2 the capture exposes the
//!   data as `Vec<DiagnosticMessage>` consumable by anyone.

// (re-imported inside cfg-gated `imp` modules)

/// Severity of a diagnostic message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DiagnosticSeverity {
    /// Corruption — heap / pointer / lifetime violation.
    Corruption,
    /// Error — invalid state / failed call.
    Error,
    /// Warning — suboptimal but not invalid.
    Warning,
    /// Info — operational fact.
    Info,
    /// Message — verbose / debug.
    Message,
}

impl DiagnosticSeverity {
    /// Short name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Corruption => "corruption",
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
            Self::Message => "message",
        }
    }
}

/// A single diagnostic message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticMessage {
    /// Severity.
    pub severity: DiagnosticSeverity,
    /// Category (free-form ; e.g., "Execution", "State Creation").
    pub category: String,
    /// Free-form description.
    pub description: String,
}

impl DiagnosticMessage {
    /// New message constructor.
    #[must_use]
    pub fn new(
        severity: DiagnosticSeverity,
        category: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            category: category.into(),
            description: description.into(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Windows impl
// ═══════════════════════════════════════════════════════════════════════

#[cfg(target_os = "windows")]
mod imp {
    use super::{DiagnosticMessage, DiagnosticSeverity};
    use crate::device::Device;
    use crate::error::Result;
    use windows::core::Interface;
    use windows::Win32::Graphics::Direct3D12::{
        ID3D12DeviceRemovedExtendedData, ID3D12InfoQueue, D3D12_AUTO_BREADCRUMB_NODE,
        D3D12_MESSAGE_CATEGORY_EXECUTION, D3D12_MESSAGE_SEVERITY_CORRUPTION,
        D3D12_MESSAGE_SEVERITY_ERROR, D3D12_MESSAGE_SEVERITY_INFO, D3D12_MESSAGE_SEVERITY_MESSAGE,
        D3D12_MESSAGE_SEVERITY_WARNING,
    };

    /// Diagnostic capture wrapper.
    pub struct DredCapture {
        pub(crate) info_queue: Option<ID3D12InfoQueue>,
        pub(crate) dred: Option<ID3D12DeviceRemovedExtendedData>,
        pub(crate) max_messages: u64,
    }

    impl core::fmt::Debug for DredCapture {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.debug_struct("DredCapture")
                .field("info_queue_present", &self.info_queue.is_some())
                .field("dred_present", &self.dred.is_some())
                .field("max_messages", &self.max_messages)
                .finish_non_exhaustive()
        }
    }

    impl DredCapture {
        /// Tap into a device's debug surface. Returns a capture even if
        /// neither InfoQueue nor DRED is available — the `Vec<_>` from
        /// `drain` will simply be empty.
        pub fn tap_into(device: &Device) -> Result<Self> {
            // Try to fetch ID3D12InfoQueue (only present when debug layer enabled).
            // SAFETY : cast() is a stable COM operation ; failure → None.
            let info_queue: Option<ID3D12InfoQueue> = device.device.cast::<ID3D12InfoQueue>().ok();
            // Try DRED.
            let dred: Option<ID3D12DeviceRemovedExtendedData> =
                device.device.cast::<ID3D12DeviceRemovedExtendedData>().ok();
            Ok(Self {
                info_queue,
                dred,
                max_messages: 256,
            })
        }

        /// Set the maximum number of messages to drain per call.
        #[must_use]
        pub fn with_max_messages(mut self, max: u64) -> Self {
            self.max_messages = max;
            self
        }

        /// Drain all available diagnostic messages from the InfoQueue.
        /// Clears the InfoQueue's internal buffer as a side-effect.
        pub fn drain(&self) -> Result<Vec<DiagnosticMessage>> {
            let Some(iq) = self.info_queue.as_ref() else {
                return Ok(Vec::new());
            };
            // SAFETY : iq lives ; GetNumStoredMessages is a stable getter.
            let n = unsafe { iq.GetNumStoredMessages() };
            let n = n.min(self.max_messages);
            let mut out = Vec::with_capacity(n as usize);
            for i in 0..n {
                // First call gets length only.
                let mut size: usize = 0;
                // SAFETY : passing nullptr+size out is the documented length-probe form.
                if unsafe { iq.GetMessage(i, None, &mut size) }.is_err() {
                    continue;
                }
                if size == 0 {
                    continue;
                }
                // Allocate a Vec<u64> instead of Vec<u8> so the underlying
                // backing has 8-byte alignment matching D3D12_MESSAGE's needs.
                let words = size.div_ceil(8);
                let mut buf: Vec<u64> = vec![0u64; words];
                // SAFETY : buf has 8-aligned backing store of `words * 8 ≥ size`.
                let msg_ptr = buf
                    .as_mut_ptr()
                    .cast::<windows::Win32::Graphics::Direct3D12::D3D12_MESSAGE>();
                let r = unsafe { iq.GetMessage(i, Some(msg_ptr), &mut size) };
                if r.is_err() {
                    continue;
                }
                // SAFETY : the raw pointer points into our `buf` for at most `size`.
                let raw = unsafe { &*msg_ptr };
                let description_ptr = raw.pDescription;
                let description_len = raw.DescriptionByteLength;
                // SAFETY : description_ptr + len follow the D3D12 contract.
                let desc_slice = unsafe {
                    core::slice::from_raw_parts(description_ptr.cast::<u8>(), description_len)
                };
                let description = core::str::from_utf8(desc_slice)
                    .unwrap_or("<non-utf8>")
                    .trim_end_matches('\0')
                    .to_string();
                let severity = if raw.Severity == D3D12_MESSAGE_SEVERITY_CORRUPTION {
                    DiagnosticSeverity::Corruption
                } else if raw.Severity == D3D12_MESSAGE_SEVERITY_ERROR {
                    DiagnosticSeverity::Error
                } else if raw.Severity == D3D12_MESSAGE_SEVERITY_WARNING {
                    DiagnosticSeverity::Warning
                } else if raw.Severity == D3D12_MESSAGE_SEVERITY_INFO {
                    DiagnosticSeverity::Info
                } else if raw.Severity == D3D12_MESSAGE_SEVERITY_MESSAGE {
                    DiagnosticSeverity::Message
                } else {
                    DiagnosticSeverity::Info
                };
                let category = if raw.Category == D3D12_MESSAGE_CATEGORY_EXECUTION {
                    "Execution".to_string()
                } else {
                    format!("Category({})", raw.Category.0)
                };
                out.push(DiagnosticMessage {
                    severity,
                    category,
                    description,
                });
            }
            // SAFETY : iq lives ; ClearStoredMessages is a stable mutator.
            unsafe { iq.ClearStoredMessages() };
            Ok(out)
        }

        /// Drain DRED breadcrumbs. Empty unless device was removed AND DRED
        /// was enabled at device-creation time.
        pub fn drain_dred_breadcrumbs(&self) -> Result<Vec<DiagnosticMessage>> {
            let Some(dred) = self.dred.as_ref() else {
                return Ok(Vec::new());
            };
            let mut out = Vec::new();
            // SAFETY : dred lives ; GetAutoBreadcrumbsOutput is documented stable.
            // If DRED isn't enabled, the call returns DXGI_ERROR_NOT_SUPPORTED ;
            // we map that to "no breadcrumbs" rather than an error.
            let Ok(output) = (unsafe { dred.GetAutoBreadcrumbsOutput() }) else {
                return Ok(out);
            };
            // Walk the breadcrumb linked list. Each node carries a context
            // string + last-known op. We surface this as one message per node.
            let mut node_ptr: *const D3D12_AUTO_BREADCRUMB_NODE = output.pHeadAutoBreadcrumbNode;
            let mut steps = 0;
            while !node_ptr.is_null() && steps < self.max_messages {
                // SAFETY : node_ptr is either &head or &node->pNext.
                let node = unsafe { &*node_ptr };
                let ctx = if node.pCommandListDebugNameA.is_null() {
                    "<unnamed>".to_string()
                } else {
                    // SAFETY : pCommandListDebugNameA is a C-style string when non-null.
                    let cstr =
                        unsafe { core::ffi::CStr::from_ptr(node.pCommandListDebugNameA.cast()) };
                    cstr.to_string_lossy().into_owned()
                };
                out.push(DiagnosticMessage {
                    severity: DiagnosticSeverity::Corruption,
                    category: "DRED-Breadcrumb".to_string(),
                    description: format!("breadcrumb-node ctx={ctx}"),
                });
                node_ptr = node.pNext;
                steps += 1;
            }
            Ok(out)
        }

        /// Is the InfoQueue connected (debug layer enabled)?
        #[must_use]
        pub const fn has_info_queue(&self) -> bool {
            self.info_queue.is_some()
        }

        /// Is DRED connected?
        #[must_use]
        pub const fn has_dred(&self) -> bool {
            self.dred.is_some()
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Non-Windows stub impl
// ═══════════════════════════════════════════════════════════════════════

#[cfg(not(target_os = "windows"))]
mod imp {
    use super::DiagnosticMessage;
    use crate::device::Device;
    use crate::error::{D3d12Error, Result};

    /// Diagnostic capture stub.
    #[derive(Debug)]
    pub struct DredCapture {
        max_messages: u64,
    }

    impl DredCapture {
        /// Always returns `LoaderMissing`.
        pub fn tap_into(_device: &Device) -> Result<Self> {
            Err(D3d12Error::loader("non-Windows target"))
        }

        /// Stub setter.
        #[must_use]
        pub const fn with_max_messages(mut self, max: u64) -> Self {
            self.max_messages = max;
            self
        }

        /// Stub returns empty.
        pub fn drain(&self) -> Result<Vec<DiagnosticMessage>> {
            Ok(Vec::new())
        }

        /// Stub returns empty.
        pub fn drain_dred_breadcrumbs(&self) -> Result<Vec<DiagnosticMessage>> {
            Ok(Vec::new())
        }

        /// Stub.
        #[must_use]
        pub const fn has_info_queue(&self) -> bool {
            false
        }

        /// Stub.
        #[must_use]
        pub const fn has_dred(&self) -> bool {
            false
        }
    }
}

pub use imp::DredCapture;

#[cfg(test)]
mod tests {
    use super::{DiagnosticMessage, DiagnosticSeverity};

    #[test]
    fn severity_names() {
        assert_eq!(DiagnosticSeverity::Corruption.as_str(), "corruption");
        assert_eq!(DiagnosticSeverity::Error.as_str(), "error");
        assert_eq!(DiagnosticSeverity::Warning.as_str(), "warning");
        assert_eq!(DiagnosticSeverity::Info.as_str(), "info");
        assert_eq!(DiagnosticSeverity::Message.as_str(), "message");
    }

    #[test]
    fn severity_ordering_is_strict() {
        assert!(DiagnosticSeverity::Corruption < DiagnosticSeverity::Error);
        assert!(DiagnosticSeverity::Error < DiagnosticSeverity::Warning);
        assert!(DiagnosticSeverity::Warning < DiagnosticSeverity::Info);
        assert!(DiagnosticSeverity::Info < DiagnosticSeverity::Message);
    }

    #[test]
    fn message_constructor() {
        let m = DiagnosticMessage::new(
            DiagnosticSeverity::Error,
            "Execution",
            "command list closed without recording",
        );
        assert_eq!(m.severity, DiagnosticSeverity::Error);
        assert_eq!(m.category, "Execution");
        assert!(m.description.contains("command list"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn dred_capture_tap_or_skip() {
        use super::DredCapture;
        use crate::device::{AdapterPreference, Device, Factory};
        let Ok(factory) = Factory::new() else {
            return;
        };
        let Ok(device) = Device::new(&factory, AdapterPreference::Hardware) else {
            return;
        };
        // Tap-into should succeed even if neither InfoQueue nor DRED is connected.
        let cap = DredCapture::tap_into(&device).expect("tap_into should not fail");
        let messages = cap.drain().expect("drain should not fail");
        // Without the debug layer, expect no messages (or just a few benign info lines).
        assert!(messages.len() < 1000);
        let breadcrumbs = cap.drain_dred_breadcrumbs().expect("breadcrumbs drain ok");
        // Without a removed device, expect zero breadcrumbs.
        assert_eq!(breadcrumbs.len(), 0);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn dred_with_max_messages_setter() {
        use super::DredCapture;
        use crate::device::{AdapterPreference, Device, Factory};
        let Ok(factory) = Factory::new() else {
            return;
        };
        let Ok(device) = Device::new(&factory, AdapterPreference::Hardware) else {
            return;
        };
        let cap = DredCapture::tap_into(&device).unwrap().with_max_messages(8);
        // Just verify the type is constructed without panic.
        let _ = cap.has_info_queue();
        let _ = cap.has_dred();
    }
}
