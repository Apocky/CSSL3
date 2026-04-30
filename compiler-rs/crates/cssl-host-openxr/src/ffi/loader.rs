//! § ffi::loader : `xrInitializeLoaderKHR` + dispatch-table.
//!
//! § SPEC : OpenXR 1.0 § 9 (Loader). The loader is dlopen'd at runtime.
//!          On Windows the canonical library is `openxr_loader.dll` ;
//!          on Linux it is `libopenxr_loader.so.1`. The loader exposes
//!          a single entry-point — `xrGetInstanceProcAddr` — through which
//!          every other API function pointer is resolved.
//!
//! § STAGE-0 ZERO-EXTERNAL POSTURE
//!   We do not link the loader directly ; we expose a `DispatchTable`
//!   with raw function-pointer slots that the engine populates after
//!   dlopen (the engine owns the dlopen handle). On platforms without
//!   a runtime present the dispatch table stays in the `Unloaded`
//!   state and every call returns `Err(LoaderError::Unloaded)`.
//!
//! § QUEST-3S
//!   Apocky's primary VR target. The standalone Quest 3s runs the
//!   Meta OpenXR runtime (Horizon OS). Loader path :
//!     `/system/lib64/libopenxr_loader.so` (Quest)
//!     `C:\Windows\System32\openxr_loader.dll` (PCVR via Link/Air-Link)
//!   The Meta runtime advertises `XR_KHR_vulkan_enable2` + the Quest-3s
//!   passthrough / body-tracking / face-tracking extension family.

use core::ptr;

use super::types::Atom;

/// `XR_KHR_loader_init` extension struct (`XrLoaderInitInfoBaseHeaderKHR`).
/// On Android this is the `XrLoaderInitInfoAndroidKHR` chain ; we keep
/// the base-header only here and extend per-platform out-of-band.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct LoaderInitInfo {
    pub ty: i32,
    pub next: *const core::ffi::c_void,
}

// SAFETY : LoaderInitInfo carries a raw `next` pointer the caller owns.
// We don't deref it inside this crate ; the runtime does. Send/Sync
// auto-derive is correct for the `i32` discriminator.

/// Errors surfaced by the loader / dispatch-table population path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoaderError {
    /// The dispatch-table has not been populated (no `xrGetInstanceProcAddr`
    /// resolved).
    Unloaded,
    /// `xrGetInstanceProcAddr` returned null for a function we asked for.
    SymbolMissing,
    /// The runtime API-version does not match what we requested.
    ApiVersionUnsupported,
    /// `xrInitializeLoaderKHR` failed for a platform-specific reason.
    InitializationFailed,
}

/// Function-pointer-table populated post-dlopen. Every slot is
/// `Option<unsafe extern "system" fn(...)>` so an unpopulated slot is
/// `None` and a missing call surfaces as `LoaderError::SymbolMissing`
/// rather than an opaque crash.
///
/// The slots covered here are the canonical 1.0 baseline + a small
/// Quest-3s-relevant subset of KHR/EXT/META extensions. The engine
/// extends the table by appending fields and resolving via
/// `xrGetInstanceProcAddr` against the live instance.
#[derive(Default)]
pub struct DispatchTable {
    /// `xrGetInstanceProcAddr` — bootstrapping symbol. Resolved by
    /// `dlsym(handle, "xrGetInstanceProcAddr")` ; every other slot
    /// is then resolved via this pointer.
    pub get_instance_proc_addr: Option<extern "system" fn(u64, *const u8, *mut *const ()) -> i32>,
    /// `xrEnumerateApiLayerProperties`.
    pub enumerate_api_layer_properties:
        Option<extern "system" fn(u32, *mut u32, *mut core::ffi::c_void) -> i32>,
    /// `xrEnumerateInstanceExtensionProperties`.
    pub enumerate_instance_extension_properties:
        Option<extern "system" fn(*const u8, u32, *mut u32, *mut core::ffi::c_void) -> i32>,
    /// `xrCreateInstance`.
    pub create_instance:
        Option<extern "system" fn(*const core::ffi::c_void, *mut u64) -> i32>,
    /// `xrDestroyInstance`.
    pub destroy_instance: Option<extern "system" fn(u64) -> i32>,
    /// `xrGetSystem`.
    pub get_system: Option<extern "system" fn(u64, *const core::ffi::c_void, *mut u64) -> i32>,
    /// `xrGetSystemProperties`.
    pub get_system_properties:
        Option<extern "system" fn(u64, u64, *mut core::ffi::c_void) -> i32>,
    /// `xrCreateSession`.
    pub create_session:
        Option<extern "system" fn(u64, *const core::ffi::c_void, *mut u64) -> i32>,
    /// `xrDestroySession`.
    pub destroy_session: Option<extern "system" fn(u64) -> i32>,
    /// `xrBeginSession`.
    pub begin_session: Option<extern "system" fn(u64, *const core::ffi::c_void) -> i32>,
    /// `xrEndSession`.
    pub end_session: Option<extern "system" fn(u64) -> i32>,
    /// `xrRequestExitSession`.
    pub request_exit_session: Option<extern "system" fn(u64) -> i32>,
    /// `xrCreateSwapchain`.
    pub create_swapchain:
        Option<extern "system" fn(u64, *const core::ffi::c_void, *mut u64) -> i32>,
    /// `xrDestroySwapchain`.
    pub destroy_swapchain: Option<extern "system" fn(u64) -> i32>,
    /// `xrAcquireSwapchainImage`.
    pub acquire_swapchain_image:
        Option<extern "system" fn(u64, *const core::ffi::c_void, *mut u32) -> i32>,
    /// `xrWaitSwapchainImage`.
    pub wait_swapchain_image:
        Option<extern "system" fn(u64, *const core::ffi::c_void) -> i32>,
    /// `xrReleaseSwapchainImage`.
    pub release_swapchain_image:
        Option<extern "system" fn(u64, *const core::ffi::c_void) -> i32>,
    /// `xrCreateReferenceSpace`.
    pub create_reference_space:
        Option<extern "system" fn(u64, *const core::ffi::c_void, *mut u64) -> i32>,
    /// `xrDestroySpace`.
    pub destroy_space: Option<extern "system" fn(u64) -> i32>,
    /// `xrLocateSpace`.
    pub locate_space:
        Option<extern "system" fn(u64, u64, i64, *mut core::ffi::c_void) -> i32>,
    /// `xrCreateActionSet`.
    pub create_action_set:
        Option<extern "system" fn(u64, *const core::ffi::c_void, *mut u64) -> i32>,
    /// `xrCreateAction`.
    pub create_action:
        Option<extern "system" fn(u64, *const core::ffi::c_void, *mut u64) -> i32>,
    /// `xrSuggestInteractionProfileBindings`.
    pub suggest_interaction_profile_bindings:
        Option<extern "system" fn(u64, *const core::ffi::c_void) -> i32>,
    /// `xrAttachSessionActionSets`.
    pub attach_session_action_sets:
        Option<extern "system" fn(u64, *const core::ffi::c_void) -> i32>,
    /// `xrSyncActions`.
    pub sync_actions: Option<extern "system" fn(u64, *const core::ffi::c_void) -> i32>,
    /// `xrGetActionStateBoolean`.
    pub get_action_state_boolean:
        Option<extern "system" fn(u64, *const core::ffi::c_void, *mut core::ffi::c_void) -> i32>,
    /// `xrGetActionStateFloat`.
    pub get_action_state_float:
        Option<extern "system" fn(u64, *const core::ffi::c_void, *mut core::ffi::c_void) -> i32>,
    /// `xrGetActionStatePose`.
    pub get_action_state_pose:
        Option<extern "system" fn(u64, *const core::ffi::c_void, *mut core::ffi::c_void) -> i32>,
    /// `xrApplyHapticFeedback`.
    pub apply_haptic_feedback: Option<
        extern "system" fn(u64, *const core::ffi::c_void, *const core::ffi::c_void) -> i32,
    >,
    /// `xrStringToPath`.
    pub string_to_path: Option<extern "system" fn(u64, *const u8, *mut u64) -> i32>,
    /// `xrPathToString`.
    pub path_to_string:
        Option<extern "system" fn(u64, u64, u32, *mut u32, *mut u8) -> i32>,
}

impl core::fmt::Debug for DispatchTable {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DispatchTable")
            .field("populated_slots", &self.populated_count())
            .finish()
    }
}

impl DispatchTable {
    /// Empty dispatch-table (every slot `None`). Pre-loader state.
    #[must_use]
    pub const fn unloaded() -> Self {
        Self {
            get_instance_proc_addr: None,
            enumerate_api_layer_properties: None,
            enumerate_instance_extension_properties: None,
            create_instance: None,
            destroy_instance: None,
            get_system: None,
            get_system_properties: None,
            create_session: None,
            destroy_session: None,
            begin_session: None,
            end_session: None,
            request_exit_session: None,
            create_swapchain: None,
            destroy_swapchain: None,
            acquire_swapchain_image: None,
            wait_swapchain_image: None,
            release_swapchain_image: None,
            create_reference_space: None,
            destroy_space: None,
            locate_space: None,
            create_action_set: None,
            create_action: None,
            suggest_interaction_profile_bindings: None,
            attach_session_action_sets: None,
            sync_actions: None,
            get_action_state_boolean: None,
            get_action_state_float: None,
            get_action_state_pose: None,
            apply_haptic_feedback: None,
            string_to_path: None,
            path_to_string: None,
        }
    }

    /// Count how many slots are populated. Useful for diagnostics +
    /// the smoke-test that confirms the engine wired the loader.
    /// Sawyer-pattern : flatten the slot-checks into a fixed-length
    /// `bool` array + sum, so the function stays cyclomatic-1.
    #[must_use]
    pub fn populated_count(&self) -> u32 {
        let slots: [bool; 25] = [
            self.get_instance_proc_addr.is_some(),
            self.enumerate_api_layer_properties.is_some(),
            self.enumerate_instance_extension_properties.is_some(),
            self.create_instance.is_some(),
            self.destroy_instance.is_some(),
            self.get_system.is_some(),
            self.get_system_properties.is_some(),
            self.create_session.is_some(),
            self.destroy_session.is_some(),
            self.begin_session.is_some(),
            self.end_session.is_some(),
            self.create_swapchain.is_some(),
            self.destroy_swapchain.is_some(),
            self.acquire_swapchain_image.is_some(),
            self.wait_swapchain_image.is_some(),
            self.release_swapchain_image.is_some(),
            self.create_reference_space.is_some(),
            self.locate_space.is_some(),
            self.create_action_set.is_some(),
            self.create_action.is_some(),
            self.suggest_interaction_profile_bindings.is_some(),
            self.sync_actions.is_some(),
            self.get_action_state_pose.is_some(),
            self.apply_haptic_feedback.is_some(),
            self.string_to_path.is_some(),
        ];
        slots.iter().filter(|b| **b).count() as u32
    }

    /// Resolve the bootstrap `xrGetInstanceProcAddr` from a raw symbol-pointer
    /// the engine retrieved via `dlsym`. The symbol is opaque to this crate ;
    /// we only typecheck the signature.
    ///
    /// # Safety
    /// `proc_addr_symbol` MUST be the result of `dlsym(handle,
    /// "xrGetInstanceProcAddr")` against a live OpenXR loader. If the
    /// pointer is null or points at a different symbol the runtime will
    /// crash on first invocation. The caller asserts this precondition.
    pub unsafe fn populate_bootstrap(
        &mut self,
        proc_addr_symbol: *const (),
    ) -> Result<(), LoaderError> {
        if proc_addr_symbol.is_null() {
            return Err(LoaderError::SymbolMissing);
        }
        // SAFETY : caller asserts proc_addr_symbol came from dlsym for
        // "xrGetInstanceProcAddr". The cast is a transparent retag from
        // `*const ()` to a function-pointer of matching ABI.
        self.get_instance_proc_addr =
            Some(unsafe { core::mem::transmute::<*const (), extern "system" fn(u64, *const u8, *mut *const ()) -> i32>(proc_addr_symbol) });
        Ok(())
    }

    /// `true` iff the bootstrap symbol has been wired ; everything else
    /// can be resolved from this point via `xrGetInstanceProcAddr`.
    #[must_use]
    pub fn is_bootstrapped(&self) -> bool {
        self.get_instance_proc_addr.is_some()
    }
}

/// In-memory mock of the dispatch table for headless testing. Records
/// every "call" that would have been routed to an FFI-pointer. Used by
/// `tests/ffi_instance_mock.rs` etc.
#[derive(Debug, Clone, Default)]
pub struct MockDispatch {
    pub instances_created: u32,
    pub instances_destroyed: u32,
    pub sessions_created: u32,
    pub swapchains_created: u32,
    pub action_sets_created: u32,
    pub actions_created: u32,
    pub sync_action_calls: u32,
    pub haptic_calls: u32,
    pub last_path_atom: Atom,
}

impl MockDispatch {
    /// Construct a fresh mock dispatch.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Pretend to call `xrStringToPath` ; assigns deterministic atoms by
    /// hashing the input bytes (FNV-1a 64).
    pub fn fake_string_to_path(&mut self, s: &[u8]) -> Atom {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in s {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        let a = Atom(h | 1); // never NULL_HANDLE
        self.last_path_atom = a;
        a
    }
}

/// Sentinel pointer used in `LoaderInitInfo::next` when no platform
/// extension chain is present. Re-exported for convenience.
pub const NULL_NEXT: *const core::ffi::c_void = ptr::null();

#[cfg(test)]
mod tests {
    use super::{DispatchTable, LoaderError, MockDispatch};

    #[test]
    fn unloaded_dispatch_table_has_no_slots() {
        let dt = DispatchTable::unloaded();
        assert_eq!(dt.populated_count(), 0);
        assert!(!dt.is_bootstrapped());
    }

    #[test]
    fn bootstrap_with_null_returns_symbol_missing() {
        let mut dt = DispatchTable::unloaded();
        // SAFETY : passing null is the explicit sentinel-test path ;
        // the function checks for null before transmuting.
        let r = unsafe { dt.populate_bootstrap(core::ptr::null()) };
        assert_eq!(r, Err(LoaderError::SymbolMissing));
        assert!(!dt.is_bootstrapped());
    }

    #[test]
    fn mock_dispatch_string_to_path_is_deterministic() {
        let mut d1 = MockDispatch::new();
        let mut d2 = MockDispatch::new();
        let a = d1.fake_string_to_path(b"/user/hand/left");
        let b = d2.fake_string_to_path(b"/user/hand/left");
        let c = d1.fake_string_to_path(b"/user/hand/right");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_ne!(a.0, 0);
    }
}
