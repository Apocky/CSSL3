//! § ffi::result : `XrResult` enum + `XR_SUCCEEDED` / `XR_FAILED` helpers.
//!
//! § SPEC : OpenXR 1.0 § 2.4 (Result Codes). Result-codes are `i32` ;
//!          success-codes are `>= 0` ; failure-codes are `< 0`. The
//!          spec mandates that `XR_SUCCESS` is canonical 0.

use core::fmt;

/// OpenXR result code (mirrors `XrResult` from `openxr.h`). Repr-i32
/// because the spec defines result-codes as `int32_t` and we need the
/// FFI ABI to match exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct XrResult(pub i32);

impl XrResult {
    // ── Success codes (>= 0) ────────────────────────────────────────
    pub const SUCCESS: Self = Self(0);
    pub const TIMEOUT_EXPIRED: Self = Self(1);
    pub const SESSION_LOSS_PENDING: Self = Self(3);
    pub const EVENT_UNAVAILABLE: Self = Self(4);
    pub const SPACE_BOUNDS_UNAVAILABLE: Self = Self(7);
    pub const SESSION_NOT_FOCUSED: Self = Self(8);
    pub const FRAME_DISCARDED: Self = Self(9);

    // ── Failure codes (< 0) ─────────────────────────────────────────
    pub const ERROR_VALIDATION_FAILURE: Self = Self(-1);
    pub const ERROR_RUNTIME_FAILURE: Self = Self(-2);
    pub const ERROR_OUT_OF_MEMORY: Self = Self(-3);
    pub const ERROR_API_VERSION_UNSUPPORTED: Self = Self(-4);
    pub const ERROR_INITIALIZATION_FAILED: Self = Self(-6);
    pub const ERROR_FUNCTION_UNSUPPORTED: Self = Self(-7);
    pub const ERROR_FEATURE_UNSUPPORTED: Self = Self(-8);
    pub const ERROR_EXTENSION_NOT_PRESENT: Self = Self(-9);
    pub const ERROR_LIMIT_REACHED: Self = Self(-10);
    pub const ERROR_SIZE_INSUFFICIENT: Self = Self(-11);
    pub const ERROR_HANDLE_INVALID: Self = Self(-12);
    pub const ERROR_INSTANCE_LOST: Self = Self(-13);
    pub const ERROR_SESSION_RUNNING: Self = Self(-14);
    pub const ERROR_SESSION_NOT_RUNNING: Self = Self(-16);
    pub const ERROR_SESSION_NOT_READY: Self = Self(-17);
    pub const ERROR_SESSION_NOT_STOPPING: Self = Self(-18);
    pub const ERROR_TIME_INVALID: Self = Self(-19);
    pub const ERROR_REFERENCE_SPACE_UNSUPPORTED: Self = Self(-20);
    pub const ERROR_FILE_ACCESS_ERROR: Self = Self(-21);
    pub const ERROR_FILE_CONTENTS_INVALID: Self = Self(-22);
    pub const ERROR_FORM_FACTOR_UNSUPPORTED: Self = Self(-23);
    pub const ERROR_FORM_FACTOR_UNAVAILABLE: Self = Self(-24);
    pub const ERROR_API_LAYER_NOT_PRESENT: Self = Self(-25);
    pub const ERROR_CALL_ORDER_INVALID: Self = Self(-26);
    pub const ERROR_GRAPHICS_DEVICE_INVALID: Self = Self(-27);
    pub const ERROR_POSE_INVALID: Self = Self(-28);
    pub const ERROR_INDEX_OUT_OF_RANGE: Self = Self(-29);
    pub const ERROR_VIEW_CONFIGURATION_TYPE_UNSUPPORTED: Self = Self(-30);
    pub const ERROR_ENVIRONMENT_BLEND_MODE_UNSUPPORTED: Self = Self(-31);
    pub const ERROR_NAME_DUPLICATED: Self = Self(-44);
    pub const ERROR_NAME_INVALID: Self = Self(-45);
    pub const ERROR_ACTIONSET_NOT_ATTACHED: Self = Self(-46);
    pub const ERROR_ACTIONSETS_ALREADY_ATTACHED: Self = Self(-47);
    pub const ERROR_LOCALIZED_NAME_DUPLICATED: Self = Self(-48);
    pub const ERROR_LOCALIZED_NAME_INVALID: Self = Self(-49);

    /// `true` if the result is in the success-half (>= 0).
    #[must_use]
    pub const fn is_success(self) -> bool {
        self.0 >= 0
    }

    /// `true` if the result is a hard-failure (< 0).
    #[must_use]
    pub const fn is_failure(self) -> bool {
        self.0 < 0
    }
}

impl fmt::Display for XrResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::SUCCESS => f.write_str("XR_SUCCESS"),
            Self::TIMEOUT_EXPIRED => f.write_str("XR_TIMEOUT_EXPIRED"),
            Self::SESSION_LOSS_PENDING => f.write_str("XR_SESSION_LOSS_PENDING"),
            Self::EVENT_UNAVAILABLE => f.write_str("XR_EVENT_UNAVAILABLE"),
            Self::FRAME_DISCARDED => f.write_str("XR_FRAME_DISCARDED"),
            Self::ERROR_VALIDATION_FAILURE => f.write_str("XR_ERROR_VALIDATION_FAILURE"),
            Self::ERROR_RUNTIME_FAILURE => f.write_str("XR_ERROR_RUNTIME_FAILURE"),
            Self::ERROR_OUT_OF_MEMORY => f.write_str("XR_ERROR_OUT_OF_MEMORY"),
            Self::ERROR_HANDLE_INVALID => f.write_str("XR_ERROR_HANDLE_INVALID"),
            Self::ERROR_INSTANCE_LOST => f.write_str("XR_ERROR_INSTANCE_LOST"),
            Self::ERROR_SESSION_RUNNING => f.write_str("XR_ERROR_SESSION_RUNNING"),
            Self::ERROR_SESSION_NOT_RUNNING => f.write_str("XR_ERROR_SESSION_NOT_RUNNING"),
            Self::ERROR_SESSION_NOT_READY => f.write_str("XR_ERROR_SESSION_NOT_READY"),
            Self::ERROR_FORM_FACTOR_UNSUPPORTED => f.write_str("XR_ERROR_FORM_FACTOR_UNSUPPORTED"),
            Self::ERROR_FORM_FACTOR_UNAVAILABLE => f.write_str("XR_ERROR_FORM_FACTOR_UNAVAILABLE"),
            Self::ERROR_POSE_INVALID => f.write_str("XR_ERROR_POSE_INVALID"),
            Self::ERROR_INDEX_OUT_OF_RANGE => f.write_str("XR_ERROR_INDEX_OUT_OF_RANGE"),
            Self::ERROR_NAME_DUPLICATED => f.write_str("XR_ERROR_NAME_DUPLICATED"),
            Self::ERROR_NAME_INVALID => f.write_str("XR_ERROR_NAME_INVALID"),
            Self::ERROR_ACTIONSET_NOT_ATTACHED => f.write_str("XR_ERROR_ACTIONSET_NOT_ATTACHED"),
            Self::ERROR_API_VERSION_UNSUPPORTED => f.write_str("XR_ERROR_API_VERSION_UNSUPPORTED"),
            Self::ERROR_FUNCTION_UNSUPPORTED => f.write_str("XR_ERROR_FUNCTION_UNSUPPORTED"),
            Self::ERROR_FEATURE_UNSUPPORTED => f.write_str("XR_ERROR_FEATURE_UNSUPPORTED"),
            Self::ERROR_EXTENSION_NOT_PRESENT => f.write_str("XR_ERROR_EXTENSION_NOT_PRESENT"),
            Self::ERROR_INITIALIZATION_FAILED => f.write_str("XR_ERROR_INITIALIZATION_FAILED"),
            other => write!(f, "XrResult({})", other.0),
        }
    }
}

/// `XR_SUCCEEDED(r)` macro from the OpenXR spec, as a const-fn.
#[must_use]
#[inline]
pub const fn xr_succeeded(r: XrResult) -> bool {
    r.0 >= 0
}

/// `XR_FAILED(r)` macro from the OpenXR spec, as a const-fn.
#[must_use]
#[inline]
pub const fn xr_failed(r: XrResult) -> bool {
    r.0 < 0
}

#[cfg(test)]
mod tests {
    use super::{xr_failed, xr_succeeded, XrResult};

    #[test]
    fn success_codes_classify_correctly() {
        assert!(xr_succeeded(XrResult::SUCCESS));
        assert!(xr_succeeded(XrResult::TIMEOUT_EXPIRED));
        assert!(xr_succeeded(XrResult::SESSION_LOSS_PENDING));
        assert!(!xr_failed(XrResult::SUCCESS));
    }

    #[test]
    fn failure_codes_classify_correctly() {
        assert!(xr_failed(XrResult::ERROR_VALIDATION_FAILURE));
        assert!(xr_failed(XrResult::ERROR_RUNTIME_FAILURE));
        assert!(xr_failed(XrResult::ERROR_OUT_OF_MEMORY));
        assert!(xr_failed(XrResult::ERROR_HANDLE_INVALID));
        assert!(!xr_succeeded(XrResult::ERROR_VALIDATION_FAILURE));
    }

    #[test]
    fn display_renders_canonical_names() {
        assert_eq!(format!("{}", XrResult::SUCCESS), "XR_SUCCESS");
        assert_eq!(
            format!("{}", XrResult::ERROR_HANDLE_INVALID),
            "XR_ERROR_HANDLE_INVALID"
        );
        // Unknown codes fall through to the numeric form.
        assert_eq!(format!("{}", XrResult(-9999)), "XrResult(-9999)");
    }
}
