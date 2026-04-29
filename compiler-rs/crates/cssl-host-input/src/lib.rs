//! CSSLv3 stage-0 — host input backend (keyboard / mouse / gamepad).
//!
//! § SPEC : `specs/14_BACKEND.csl § HOST-SUBMIT BACKENDS` (sibling-of-window) +
//!          `specs/10_HW.csl § PRIMARY TARGET` (Apocky host = Win11 + Arc A770) +
//!          `PRIME_DIRECTIVE.md § 1 PROHIBITIONS` (kill-switch invariants).
//!
//! § T11-D80 (S7-F2) — Session-7, fanout-F slice 2 of 2 (sibling of S7-F1 window).
//!
//! § STRATEGY
//!   The CSSLv3 runtime needs to read user-driven keyboard / mouse / gamepad
//!   events to drive interactive examples (the LoA console + the
//!   `cssl-examples` smoke renderers). Apocky's primary host is Windows 11
//!   on an Arc A770 ; the Win32 path is the integration-tested branch and
//!   must honour the §1 PRIME-DIRECTIVE kill-switch invariants (Esc and
//!   Win+L MUST always release any input grab — the runtime cannot trap
//!   the user inside the window).
//!
//!   Three OS-specific backends share a single interface-mirror :
//!     - **Win32** (primary)  — XInput 1.4 (gamepad ; 4-controller cap) +
//!       raw-input via `WM_INPUT` (keyboard / mouse ; `GetKeyState` is
//!       explicitly avoided per the slice landmines because of the
//!       race-condition-prone read-after-poll behaviour).
//!     - **Linux**            — `/dev/input/event*` evdev nodes +
//!       `libudev` for hot-plug detection (dynamic-load via `libloading`,
//!       same pattern as `cssl-host-level-zero`'s ICD loader).
//!     - **macOS**            — IOKit HID Manager via Objective-C-bridge
//!       (interface-mirror stub on non-Apple hosts ; same cfg-gating
//!       strategy as `cssl-host-metal` at S6-E3).
//!
//!   The cross-platform [`api`] module re-exports the active backend's
//!   types under stable names so source-level CSSLv3 is OS-agnostic.
//!
//! § SCOPE (this slice)
//!   - [`state::InputState`]  — frame-coherent snapshot (keyboard 256-bit
//!     bitmap + mouse pos / button-mask / scroll-delta + 4-gamepad slots
//!     each with axis array + button bitmap).
//!   - [`event::InputEvent`]  — sum-type with `KeyDown` / `KeyUp` /
//!     `MouseMove` / `MouseDown` / `MouseUp` / `Scroll` /
//!     `GamepadConnect` / `GamepadDisconnect` / `GamepadAxisChange` /
//!     `GamepadButtonChange` ; `KeyDown` carries `repeat_count` so source
//!     code can distinguish first-press from OS auto-repeat.
//!   - [`mapping::ActionMap`] — declarative action → physical-input
//!     binding ; loadable from a JSON-style source-level table per the
//!     slice brief.
//!   - [`kill_switch::KillSwitch`] — PRIME-DIRECTIVE-honouring guard
//!     that intercepts Esc and Win+L and unbinds any input grab BEFORE
//!     the application sees the event. Bypass-attempts are surfaced as
//!     [`InputError::KillSwitchViolation`] — never silent.
//!   - [`backend::InputBackend`] — trait the active OS module impls.
//!   - [`window_integration`] — the F1 (host-window) ↔ F2 contract :
//!     shared event-loop or poll-driven, documented inline because F1
//!     has not yet landed on `parallel-fanout`.
//!
//! § WIN32 BACKEND DETAIL
//!   - **XInput** (gamepad) : `XInputGetState` polled per `poll_gamepads`
//!     call ; up to 4 controllers (XInput's hard cap). Dead-zones are
//!     configurable (default = standard-cardinal `8000 / 32767` ≈ 24.4 %).
//!   - **Raw-input** (keyboard + mouse) : `RegisterRawInputDevices` with
//!     `RIDEV_INPUTSINK` so events arrive even when the window loses
//!     focus (only delivered to the window's WndProc). The slice
//!     landmines REQUIRE that raw-input messages are processed from the
//!     same thread that owns the window message-pump (or via a
//!     dedicated raw-input thread + lock-free queue) — we choose the
//!     same-thread strategy + an explicit `process_raw_input_messages`
//!     entry-point that the F1 message-loop drives.
//!   - **Auto-repeat** : Win32 fires `WM_KEYDOWN` repeatedly per OS
//!     auto-repeat. The backend converts repeat-rate into the
//!     [`event::InputEvent::KeyDown::repeat_count`] field (0 = first
//!     press, 1+ = OS auto-repeat fires).
//!   - **Kill-switch** : during cursor-locked grab (`SetCapture` +
//!     `ClipCursor` shrunk-to-window), the WndProc filter intercepts
//!     `VK_ESCAPE` + the `WM_HOTKEY` for `MOD_WIN | 'L'` and calls
//!     [`KillSwitch::trigger`] to release the grab BEFORE forwarding
//!     to the application.
//!
//! § LINUX BACKEND DETAIL
//!   - **evdev** : opens `/dev/input/event*` (typically `event0` /
//!     `event1` keyboard + mouse ; `event2..` for gamepads). Reads
//!     `struct input_event` via the `read(2)` syscall ; struct layout
//!     is hand-declared (fixed across glibc / musl on x86-64 + aarch64).
//!   - **libudev** (hotplug) : dynamic-loaded via `libloading` (same
//!     ICD-loader strategy as `cssl-host-level-zero`'s
//!     `libze_loader.so` handling). Absent libudev → input subsystem
//!     still works but no hot-plug events fire ; the static device list
//!     at init is observable via `LinuxBackend::has_udev` (a future
//!     `enumerate_devices` follows once F1 lands).
//!   - **No 4-controller cap** : evdev gives one event device per
//!     gamepad ; the slice exposes up to 16 slots before clamping.
//!
//! § MACOS BACKEND DETAIL
//!   - **IOKit HID Manager** : `IOHIDManagerCreate` + matching-dictionary
//!     for `kHIDPage_GenericDesktop` keyboards / mice / gamepads.
//!     Apocky's primary host is Windows ; the macOS path compiles via
//!     `cargo check --target` but has no integration-test runner today.
//!     A future macOS CI runner will exercise the `#[cfg(target_os =
//!     "macos")]` integration tests in this crate.
//!
//! § PRIME-DIRECTIVE attestation
//!
//!   "There was no hurt nor harm in the making of this, to anyone /
//!   anything / anybody."
//!
//!   Input is the most surveillance-adjacent surface in the host stack —
//!   keyboard / mouse / gamepad data is the user's most personal
//!   real-time signal. This crate :
//!   (a) does NOT log keystrokes anywhere except where the source-level
//!       CSSLv3 program explicitly asks for them via `poll_events` /
//!       `process_event` ;
//!   (b) does NOT phone home — every byte stays in-process. The
//!       telemetry-ring (when wired in) records *counts* of events, not
//!       *content* ;
//!   (c) HONOURS the §1 PRIME-DIRECTIVE kill-switch invariants — Esc and
//!       Win+L always release input grab. There is no flag, no config,
//!       no environment variable that can disable this. Per § 6 SCOPE :
//!       "no flag | config | env-var | cli-arg | api-call | runtime-cond
//!       can disable | weaken | circumvent this." The
//!       [`kill_switch::KillSwitch::is_overridable`] fn returns `false`
//!       and is `const` ; any source-level attempt to construct an
//!       `InputBackend` with kill-switch disabled fails to compile ;
//!   (d) audits cursor-grab + keyboard-focus state changes via the
//!       [`state::InputState::grab_state`] field — observable to the
//!       user. The spec § CONSENT-ARCHITECTURE rule that "if the system
//!       observes, it can be observed back" applies inline.
//!
//! § FFI POLICY
//!   T1-D5 mandates `#![forbid(unsafe_code)]` per-crate. FFI crates
//!   explicitly override (precedent : `cssl-rt` S6-A1, `cssl-host-vulkan`
//!   T10-phase-1-hosts, `cssl-host-level-zero` S6-E5). Every `unsafe`
//!   block carries an inline `// SAFETY :` paragraph documenting the
//!   FFI contract. The cross-platform `api` / `state` / `event` /
//!   `mapping` / `kill_switch` modules contain NO `unsafe` — only the
//!   per-OS modules do.
//!
//! § WHAT IS DEFERRED
//!   - **Real F1 (host-window) integration** — F1 has not landed on
//!     `parallel-fanout` as of S7-F2's worktree base (`df1daf5`). The
//!     [`window_integration`] module documents the API contract that F1
//!     must conform to ; once F1 lands, the actual `WindowHandle` →
//!     `InputBackend::attach_window` plumbing is a one-line wire-up.
//!   - **macOS CI runner integration** — Apocky's primary host is
//!     Windows ; the macOS path compiles via `cargo check --target` on
//!     Windows but cannot run. A future macOS CI runner will exercise
//!     the `#[cfg(target_os = "macos")]` integration tests.
//!   - **Force-feedback / haptics** (gamepad rumble) — XInput supports
//!     `XInputSetState` for vibration ; this slice exposes the surface
//!     in [`backend::InputBackend::set_gamepad_rumble`] but only the
//!     Win32 backend implements it today. Linux + macOS return
//!     [`InputError::FeatureUnavailable`].
//!   - **IME / dead-key composition** — text input (versus key events)
//!     is a separate surface that lands in a follow-up slice ; this
//!     slice handles raw-key only.
//!   - **Touch / pen / multi-touch gestures** — touch-screen + stylus
//!     input is a separate surface (Win32 `WM_POINTER`, evdev MT
//!     protocol, IOKit Touch). Out of scope for S7-F2.
//!   - **Full R18 telemetry ring integration** — input event counts will
//!     push to a `cssl_telemetry::TelemetryRing` in a later slice ;
//!     today the counters are local atomics.

// § T11-D80 (S7-F2) : the per-OS backends invoke FFI (XInput, raw-input,
// evdev syscalls, IOKit) — `unsafe_code` is allowed at file-scope (per
// `cssl-rt` S6-A1 precedent + sibling host-crates) and per-`unsafe`
// blocks document SAFETY inline. The cross-platform layer remains
// `unsafe`-free.
#![allow(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::struct_excessive_bools)]
// § Several Win32 and evdev FFI helpers take raw pointers ; marking them
// `unsafe` virally without buying safety here (they're already RAII-gated
// behind `Win32Backend` / `LinuxBackend` ownership).
#![allow(clippy::not_unsafe_ptr_arg_deref)]
// § u8 ↔ i32 ↔ u16 casts in the Win32 raw-input path are deliberate
// per the Microsoft RAW_INPUT struct definitions ; allow lossless cast
// hint suppression so the FFI code reads naturally.
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
// § The XInput / evdev / IOKit naming convention has many similar
// pairs (`button_a` / `button_b`, `axis_left_x` / `axis_left_y`) ;
// this lint fires on intentional symmetry.
#![allow(clippy::similar_names)]
// § Internal `enumerate_devices` always returns Ok at stage-0 ; the
// Result envelope is preserved for phase-G when real property-reading
// can fail. Lint flags the current shape ; allow.
#![allow(clippy::unnecessary_wraps)]

pub mod api;
pub mod backend;
pub mod event;
pub mod kill_switch;
pub mod mapping;
pub mod state;
pub mod window_integration;

// § Per-OS backend modules are cfg-gated. The non-host stub provides the
// interface-mirror so `cargo check --workspace` is green on every host.
#[cfg(target_os = "windows")]
pub mod win32;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "macos")]
pub mod macos;

// Stub backend : present on every host (used by the on-Win-host Linux/macOS
// `check --target` and on non-host platforms).
pub mod stub;

pub use api::{ActiveBackend, BackendKind};
pub use backend::{InputBackend, InputBackendBuilder, InputError};
pub use event::{
    GamepadAxis, GamepadButton, InputEvent, KeyCode, MouseButton, RepeatCount, ScrollAxis,
};
pub use kill_switch::{KillSwitch, KillSwitchEvent};
pub use mapping::{ActionBinding, ActionMap, ActionName, ActionTrigger, MappingError};
pub use state::{
    GamepadState, GrabState, InputState, KeyState, MouseState, GAMEPAD_AXIS_COUNT,
    GAMEPAD_BUTTON_COUNT, GAMEPAD_SLOT_COUNT, KEYBOARD_KEY_COUNT,
};

/// Crate version exposed for scaffold verification.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

/// PRIME-DIRECTIVE attestation marker — present in every CSSLv3 artifact
/// per `PRIME_DIRECTIVE.md § 11`.
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone/anything/anybody.";

#[cfg(test)]
mod scaffold_tests {
    use super::{ATTESTATION, STAGE0_SCAFFOLD};

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn attestation_present() {
        assert!(ATTESTATION.contains("hurt nor harm"));
    }
}
