//! D3D12 device + DXGI factory + adapter enumeration.
//!
//! § SCOPE
//!   - [`Factory`] — wraps `IDXGIFactory6` (1.6 needed for `EnumAdapterByGpuPreference`).
//!   - [`Device`]  — wraps `ID3D12Device`. Created with the highest negotiable
//!     `D3D_FEATURE_LEVEL` ≥ 12.0.
//!   - [`AdapterPreference`] — `Hardware` / `LowPower` / `MinimumPower` / `Software` (WARP).
//!   - [`AdapterRecord`] — sample of `DXGI_ADAPTER_DESC3` for diagnostics.
//!
//! § STRATEGY
//!   On Windows targets, the impl uses `windows-rs 0.58` `IDXGIFactory6` +
//!   `D3D12CreateDevice`. On non-Windows targets, every constructor returns
//!   [`D3d12Error::LoaderMissing`] so callers can gracefully fall back without
//!   the crate failing to compile.
//!
//! § DEBUG-LAYER
//!   Per `specs/14_BACKEND.csl § HOST-SUBMIT BACKENDS § D3D12`, the debug layer
//!   is opt-in via [`Factory::new_with_debug`]. DRED auto-enables when the
//!   debug layer is on (see `dred.rs`).

use crate::adapter::FeatureLevel;
use crate::error::D3d12Error;

/// Adapter selection preference for `EnumAdapterByGpuPreference`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdapterPreference {
    /// Prefer the highest-performance hardware GPU.
    Hardware,
    /// Prefer integrated GPU for thermal-friendliness.
    LowPower,
    /// Equivalent to LowPower but on system without iGPU just picks something.
    MinimumPower,
    /// Software adapter (WARP) only.
    Software,
}

impl AdapterPreference {
    /// Short name for diagnostics.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hardware => "hardware",
            Self::LowPower => "low-power",
            Self::MinimumPower => "minimum-power",
            Self::Software => "software",
        }
    }
}

/// Identification record sampled from `DXGI_ADAPTER_DESC3`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterRecord {
    /// Adapter description string.
    pub description: String,
    /// PCI vendor ID.
    pub vendor_id: u32,
    /// PCI device ID.
    pub device_id: u32,
    /// PCI sub-system ID.
    pub sub_sys_id: u32,
    /// Hardware revision.
    pub revision: u32,
    /// Dedicated video memory in bytes.
    pub dedicated_video_memory: u64,
    /// Dedicated system memory in bytes.
    pub dedicated_system_memory: u64,
    /// Shared system memory in bytes.
    pub shared_system_memory: u64,
    /// Highest negotiated feature level (only known after device creation).
    pub feature_level: FeatureLevel,
    /// Is this a software adapter (WARP / reference) ?
    pub is_software: bool,
}

impl AdapterRecord {
    /// Returns `true` if this looks like an Intel adapter (vendor 0x8086).
    #[must_use]
    pub const fn is_intel(&self) -> bool {
        self.vendor_id == 0x8086
    }

    /// Returns `true` if this looks like an NVIDIA adapter (vendor 0x10DE).
    #[must_use]
    pub const fn is_nvidia(&self) -> bool {
        self.vendor_id == 0x10DE
    }

    /// Returns `true` if this looks like an AMD adapter (vendor 0x1002).
    #[must_use]
    pub const fn is_amd(&self) -> bool {
        self.vendor_id == 0x1002
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Windows impl
// ═══════════════════════════════════════════════════════════════════════

#[cfg(target_os = "windows")]
mod imp {
    use super::{AdapterPreference, AdapterRecord, FeatureLevel};
    use crate::error::{D3d12Error, Result};
    use windows::core::Interface;
    use windows::Win32::Graphics::Direct3D::{
        D3D_FEATURE_LEVEL_12_0, D3D_FEATURE_LEVEL_12_1, D3D_FEATURE_LEVEL_12_2,
    };
    use windows::Win32::Graphics::Direct3D12::{
        D3D12CreateDevice, D3D12GetDebugInterface, ID3D12Debug, ID3D12Device,
    };
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory2, IDXGIAdapter4, IDXGIFactory6, DXGI_ADAPTER_DESC3,
        DXGI_ADAPTER_FLAG3_SOFTWARE, DXGI_CREATE_FACTORY_DEBUG, DXGI_CREATE_FACTORY_FLAGS,
        DXGI_ERROR_NOT_FOUND, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE,
        DXGI_GPU_PREFERENCE_MINIMUM_POWER, DXGI_GPU_PREFERENCE_UNSPECIFIED,
    };

    /// DXGI factory wrapper.
    pub struct Factory {
        pub(crate) factory: IDXGIFactory6,
        pub(crate) debug_layer_enabled: bool,
    }

    impl core::fmt::Debug for Factory {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.debug_struct("Factory")
                .field("debug_layer_enabled", &self.debug_layer_enabled)
                .finish_non_exhaustive()
        }
    }

    impl Factory {
        /// Create a non-debug DXGI factory.
        pub fn new() -> Result<Self> {
            // SAFETY : `CreateDXGIFactory2` is a stable Win32 export ; the GUID
            // comes from windows-rs. The factory pointer is owned (RAII) and
            // released on `Drop`.
            #[allow(unsafe_code)]
            let factory: IDXGIFactory6 =
                unsafe { CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)) }
                    .map_err(|e| map_hresult("CreateDXGIFactory2", e))?;
            Ok(Self {
                factory,
                debug_layer_enabled: false,
            })
        }

        /// Create a DXGI factory with the D3D12 debug layer enabled. The debug
        /// layer must be installed (Graphics Tools optional Windows feature).
        pub fn new_with_debug() -> Result<Self> {
            // SAFETY : `D3D12GetDebugInterface` returns a COM interface owned
            // by windows-rs ; calling `EnableDebugLayer` is documented stable.
            #[allow(unsafe_code)]
            unsafe {
                let mut debug: Option<ID3D12Debug> = None;
                D3D12GetDebugInterface(&mut debug)
                    .map_err(|e| map_hresult("D3D12GetDebugInterface", e))?;
                if let Some(d) = debug {
                    d.EnableDebugLayer();
                }
            }
            #[allow(unsafe_code)]
            let factory: IDXGIFactory6 =
                unsafe { CreateDXGIFactory2(DXGI_CREATE_FACTORY_DEBUG) }
                    .map_err(|e| map_hresult("CreateDXGIFactory2(DEBUG)", e))?;
            Ok(Self {
                factory,
                debug_layer_enabled: true,
            })
        }

        /// Enumerate adapters in preference-order.
        ///
        /// On `prefer_hardware=true` the result list excludes `DXGI_ADAPTER_FLAG3_SOFTWARE`
        /// adapters (WARP).
        pub fn enumerate_adapters(
            &self,
            preference: AdapterPreference,
            prefer_hardware: bool,
        ) -> Result<Vec<AdapterRecord>> {
            let dxgi_pref = match preference {
                AdapterPreference::Hardware => DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE,
                AdapterPreference::LowPower | AdapterPreference::MinimumPower => {
                    DXGI_GPU_PREFERENCE_MINIMUM_POWER
                }
                AdapterPreference::Software => DXGI_GPU_PREFERENCE_UNSPECIFIED,
            };
            let mut out = Vec::new();
            for index in 0u32.. {
                #[allow(unsafe_code)]
                let r: windows::core::Result<IDXGIAdapter4> =
                    unsafe { self.factory.EnumAdapterByGpuPreference(index, dxgi_pref) };
                let adapter = match r {
                    Ok(a) => a,
                    Err(e) if e.code() == DXGI_ERROR_NOT_FOUND => break,
                    Err(e) => return Err(map_hresult("EnumAdapterByGpuPreference", e)),
                };
                #[allow(unsafe_code)]
                let desc: DXGI_ADAPTER_DESC3 =
                    unsafe { adapter.GetDesc3() }.map_err(|e| map_hresult("GetDesc3", e))?;
                let is_software = (desc.Flags.0 & DXGI_ADAPTER_FLAG3_SOFTWARE.0) != 0;
                if prefer_hardware && is_software {
                    continue;
                }
                let description = String::from_utf16_lossy(
                    &desc.Description[..desc
                        .Description
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(desc.Description.len())],
                );
                out.push(AdapterRecord {
                    description,
                    vendor_id: desc.VendorId,
                    device_id: desc.DeviceId,
                    sub_sys_id: desc.SubSysId,
                    revision: desc.Revision,
                    dedicated_video_memory: desc.DedicatedVideoMemory as u64,
                    dedicated_system_memory: desc.DedicatedSystemMemory as u64,
                    shared_system_memory: desc.SharedSystemMemory as u64,
                    feature_level: FeatureLevel::Fl12_0, // placeholder ; updated post-device-create
                    is_software,
                });
            }
            if out.is_empty() {
                return Err(D3d12Error::no_adapter(format!(
                    "preference={}, prefer_hardware={}",
                    preference.as_str(),
                    prefer_hardware
                )));
            }
            Ok(out)
        }

        /// Pick the first adapter satisfying `prefer_hardware` and return both
        /// the record and the underlying COM interface for `Device::new`.
        pub fn pick_adapter(
            &self,
            preference: AdapterPreference,
            prefer_hardware: bool,
        ) -> Result<(IDXGIAdapter4, AdapterRecord)> {
            let dxgi_pref = match preference {
                AdapterPreference::Hardware => DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE,
                AdapterPreference::LowPower | AdapterPreference::MinimumPower => {
                    DXGI_GPU_PREFERENCE_MINIMUM_POWER
                }
                AdapterPreference::Software => DXGI_GPU_PREFERENCE_UNSPECIFIED,
            };
            for index in 0u32.. {
                #[allow(unsafe_code)]
                let r: windows::core::Result<IDXGIAdapter4> =
                    unsafe { self.factory.EnumAdapterByGpuPreference(index, dxgi_pref) };
                let adapter = match r {
                    Ok(a) => a,
                    Err(e) if e.code() == DXGI_ERROR_NOT_FOUND => break,
                    Err(e) => return Err(map_hresult("EnumAdapterByGpuPreference", e)),
                };
                #[allow(unsafe_code)]
                let desc: DXGI_ADAPTER_DESC3 =
                    unsafe { adapter.GetDesc3() }.map_err(|e| map_hresult("GetDesc3", e))?;
                let is_software = (desc.Flags.0 & DXGI_ADAPTER_FLAG3_SOFTWARE.0) != 0;
                if prefer_hardware && is_software {
                    continue;
                }
                let description = String::from_utf16_lossy(
                    &desc.Description[..desc
                        .Description
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(desc.Description.len())],
                );
                return Ok((
                    adapter,
                    AdapterRecord {
                        description,
                        vendor_id: desc.VendorId,
                        device_id: desc.DeviceId,
                        sub_sys_id: desc.SubSysId,
                        revision: desc.Revision,
                        dedicated_video_memory: desc.DedicatedVideoMemory as u64,
                        dedicated_system_memory: desc.DedicatedSystemMemory as u64,
                        shared_system_memory: desc.SharedSystemMemory as u64,
                        feature_level: FeatureLevel::Fl12_0,
                        is_software,
                    },
                ));
            }
            Err(D3d12Error::no_adapter(format!(
                "preference={}, prefer_hardware={}",
                preference.as_str(),
                prefer_hardware
            )))
        }
    }

    /// D3D12 device wrapper.
    pub struct Device {
        pub(crate) device: ID3D12Device,
        pub(crate) feature_level: FeatureLevel,
        pub(crate) adapter: AdapterRecord,
    }

    impl core::fmt::Debug for Device {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.debug_struct("Device")
                .field("feature_level", &self.feature_level)
                .field("adapter", &self.adapter)
                .finish_non_exhaustive()
        }
    }

    impl Device {
        /// Create a `Device` from the given factory using the requested adapter
        /// preference. The highest negotiable feature level is selected.
        pub fn new(factory: &Factory, preference: AdapterPreference) -> Result<Self> {
            Self::new_with_options(factory, preference, true)
        }

        /// Like `new` but lets the caller decide whether to allow software adapters.
        pub fn new_with_options(
            factory: &Factory,
            preference: AdapterPreference,
            prefer_hardware: bool,
        ) -> Result<Self> {
            let (adapter_iface, mut record) = factory.pick_adapter(preference, prefer_hardware)?;
            // Try descending feature levels until D3D12CreateDevice accepts.
            let level_chain = [
                (D3D_FEATURE_LEVEL_12_2, FeatureLevel::Fl12_2),
                (D3D_FEATURE_LEVEL_12_1, FeatureLevel::Fl12_1),
                (D3D_FEATURE_LEVEL_12_0, FeatureLevel::Fl12_0),
            ];
            let mut last_err = None;
            for (raw_level, our_level) in level_chain {
                let mut device: Option<ID3D12Device> = None;
                #[allow(unsafe_code)]
                let r = unsafe { D3D12CreateDevice(&adapter_iface, raw_level, &mut device) };
                match r {
                    Ok(()) => {
                        if let Some(d) = device {
                            record.feature_level = our_level;
                            return Ok(Self {
                                device: d,
                                feature_level: our_level,
                                adapter: record,
                            });
                        }
                    }
                    Err(e) => last_err = Some(e),
                }
            }
            Err(last_err.map_or_else(
                || D3d12Error::unsupported("any feature level >= 12.0"),
                |e| map_hresult("D3D12CreateDevice", e),
            ))
        }

        /// Negotiated feature level.
        #[must_use]
        pub const fn feature_level(&self) -> FeatureLevel {
            self.feature_level
        }

        /// Adapter record.
        #[must_use]
        pub const fn adapter(&self) -> &AdapterRecord {
            &self.adapter
        }

        /// Get the underlying `ID3D12Device` clone for downstream wrappers.
        #[must_use]
        pub fn raw(&self) -> ID3D12Device {
            self.device.cast::<ID3D12Device>().expect("device cast")
        }
    }

    #[allow(clippy::redundant_pub_crate)]
    pub(crate) fn map_hresult(context: &str, e: windows::core::Error) -> D3d12Error {
        D3d12Error::hresult(context, e.code().0, e.message())
    }
}

/// Re-exposed HRESULT mapper for sibling submodules. Windows-only.
#[cfg(target_os = "windows")]
pub(crate) fn imp_map_hresult(context: &str, e: windows::core::Error) -> D3d12Error {
    imp::map_hresult(context, e)
}

// ═══════════════════════════════════════════════════════════════════════
// Non-Windows stub impl
// ═══════════════════════════════════════════════════════════════════════

#[cfg(not(target_os = "windows"))]
mod imp {
    use super::{AdapterPreference, AdapterRecord, FeatureLevel};
    use crate::error::{D3d12Error, Result};

    /// DXGI factory stub on non-Windows.
    #[derive(Debug)]
    pub struct Factory;

    impl Factory {
        /// Always returns `LoaderMissing` on non-Windows targets.
        pub fn new() -> Result<Self> {
            Err(D3d12Error::loader("non-Windows target"))
        }

        /// Always returns `LoaderMissing` on non-Windows targets.
        pub fn new_with_debug() -> Result<Self> {
            Err(D3d12Error::loader("non-Windows target"))
        }

        /// Always returns `LoaderMissing`.
        pub fn enumerate_adapters(
            &self,
            _preference: AdapterPreference,
            _prefer_hardware: bool,
        ) -> Result<Vec<AdapterRecord>> {
            Err(D3d12Error::loader("non-Windows target"))
        }
    }

    /// D3D12 device stub on non-Windows.
    #[derive(Debug)]
    pub struct Device {
        feature_level: FeatureLevel,
        adapter: AdapterRecord,
    }

    impl Device {
        /// Always returns `LoaderMissing`.
        pub fn new(_factory: &Factory, _preference: AdapterPreference) -> Result<Self> {
            Err(D3d12Error::loader("non-Windows target"))
        }

        /// Always returns `LoaderMissing`.
        pub fn new_with_options(
            _factory: &Factory,
            _preference: AdapterPreference,
            _prefer_hardware: bool,
        ) -> Result<Self> {
            Err(D3d12Error::loader("non-Windows target"))
        }

        /// Negotiated feature level (always 12.0 for stub).
        #[must_use]
        pub const fn feature_level(&self) -> FeatureLevel {
            self.feature_level
        }

        /// Adapter record.
        #[must_use]
        pub const fn adapter(&self) -> &AdapterRecord {
            &self.adapter
        }
    }
}

pub use imp::{Device, Factory};

#[cfg(test)]
mod tests {
    use super::{AdapterPreference, AdapterRecord, FeatureLevel};

    #[test]
    fn adapter_preference_names() {
        assert_eq!(AdapterPreference::Hardware.as_str(), "hardware");
        assert_eq!(AdapterPreference::LowPower.as_str(), "low-power");
        assert_eq!(AdapterPreference::Software.as_str(), "software");
        assert_eq!(AdapterPreference::MinimumPower.as_str(), "minimum-power");
    }

    fn intel_record() -> AdapterRecord {
        AdapterRecord {
            description: "Intel(R) Arc(TM) A770".into(),
            vendor_id: 0x8086,
            device_id: 0x56A0,
            sub_sys_id: 0,
            revision: 0,
            dedicated_video_memory: 16 << 30,
            dedicated_system_memory: 0,
            shared_system_memory: 16 << 30,
            feature_level: FeatureLevel::Fl12_2,
            is_software: false,
        }
    }

    #[test]
    fn adapter_record_intel_classifier() {
        let r = intel_record();
        assert!(r.is_intel());
        assert!(!r.is_nvidia());
        assert!(!r.is_amd());
    }

    #[test]
    fn adapter_record_nvidia_classifier() {
        let r = AdapterRecord {
            vendor_id: 0x10DE,
            ..intel_record()
        };
        assert!(r.is_nvidia());
        assert!(!r.is_intel());
        assert!(!r.is_amd());
    }

    #[test]
    fn adapter_record_amd_classifier() {
        let r = AdapterRecord {
            vendor_id: 0x1002,
            ..intel_record()
        };
        assert!(r.is_amd());
        assert!(!r.is_intel());
        assert!(!r.is_nvidia());
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn factory_new_returns_loader_missing_on_non_windows() {
        use super::Factory;
        let r = Factory::new();
        assert!(r.is_err());
        let err = r.unwrap_err();
        assert!(err.is_loader_missing());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn factory_new_succeeds_or_loader_missing() {
        use super::Factory;
        match Factory::new() {
            Ok(_f) => {}
            Err(e) => {
                // CI runners without DXGI runtime — skip-test territory.
                assert!(
                    e.is_loader_missing() || matches!(e, crate::error::D3d12Error::Hresult { .. })
                );
            }
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn factory_enumerate_returns_at_least_one_adapter_or_skips() {
        use super::{AdapterPreference, Factory};
        match Factory::new() {
            Ok(f) => match f.enumerate_adapters(AdapterPreference::Hardware, false) {
                Ok(adapters) => {
                    assert!(!adapters.is_empty());
                    for a in &adapters {
                        // Every adapter description must have at least one char.
                        assert!(!a.description.is_empty());
                    }
                }
                Err(e) => {
                    // On a CI host with no GPU at all, this can fail with NotFound.
                    assert!(
                        e.is_loader_missing()
                            || matches!(e, crate::error::D3d12Error::AdapterNotFound { .. })
                            || matches!(e, crate::error::D3d12Error::Hresult { .. })
                    );
                }
            },
            Err(e) => assert!(e.is_loader_missing()),
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn device_creation_on_real_hardware_or_skips() {
        use super::{AdapterPreference, Device, Factory};
        match Factory::new() {
            Ok(f) => match Device::new(&f, AdapterPreference::Hardware) {
                Ok(d) => {
                    eprintln!(
                        "[S6-E2] D3D12 device created : feature_level={}, adapter={}, vendor=0x{:04x}, device=0x{:04x}",
                        d.feature_level().dotted(),
                        d.adapter().description,
                        d.adapter().vendor_id,
                        d.adapter().device_id
                    );
                    assert!(d.feature_level() >= FeatureLevel::Fl12_0);
                }
                Err(e) => {
                    assert!(
                        e.is_loader_missing()
                            || matches!(e, crate::error::D3d12Error::AdapterNotFound { .. })
                            || matches!(e, crate::error::D3d12Error::Hresult { .. })
                            || matches!(e, crate::error::D3d12Error::NotSupported { .. })
                    );
                }
            },
            Err(e) => assert!(e.is_loader_missing()),
        }
    }
}
