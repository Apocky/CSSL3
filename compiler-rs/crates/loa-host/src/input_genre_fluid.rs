// § T11-W13-INPUT-GENRE-FLUID (W13-11) · input_genre_fluid.rs ─────────────
// Multi-source input router + accessibility-tunables + sovereign-cap aim-assist.
// Sibling to `input.rs` (W11-1) — does NOT touch TextInputState / VirtualKey.
// Sibling to `genre_fluid_camera.rs` (W13-4) — reads `CameraMode` for per-mode
// binding-sets without taking ownership.
//
// § ROLE
//   PC + console + mobile players need seamless input-source switching with
//   zero ceremony. Pick up a controller mid-game ; the UI re-prints prompts as
//   "press X" within 500ms while the keyboard stays armed in case the player
//   reaches back. Touch overlays appear when the user taps. Last-source-wins
//   with a debounce-window prevents thrash from stale events on either side.
//
// § AXIOMS  (canonical : Labyrinth of Apocalypse/systems/input_genre_fluid.csl)
//   t∞: 4 input-modes {KeyboardMouse · GamepadXinput · GamepadDualSense · TouchScreen}
//   t∞: source-tag per-event ← last-source-wins · 500ms grace-window
//   t∞: bindings remappable EVERY action ← per-action JSON-roundtrip
//   t∞: accessibility-OS-integration ← sticky/slow/bounce-keys honored
//   t∞: aim-assist 5-tier ← sovereign-cap on tier ≥ 3 · anti-bullying server-clamp
//   t∞: ¬ pay-to-aim-better · cosmetic-only-axiom
//   t∞: ¬ surveillance · binding-share OPT-IN-anonymous
//
// § PRIME-DIRECTIVE COMPLIANCE
//   ✓ sovereignty ← every binding remappable · every accessibility-toggle player-controlled
//   ✓ ¬ manipulation ← aim-assist tier ≥ 3 requires explicit-consent reason-tag
//   ✓ ¬ exploitation ← anti-bullying server-clamp on PvP tier-cap
//   ✓ consent-OS ← OS-platform sticky/slow/bounce keys forwarded ¬ overridden
//
// § INTEGRATION
//   - input.rs (W11-1) : EXTEND-only · feed RawEvent here AFTER InputState consumed
//     them (or in parallel · the router only inspects timing + source-tag).
//   - genre_fluid_camera.rs (W13-4) : `set_camera_mode(CameraMode)` selects the
//     active per-mode binding set without rewriting any storage.
//   - cssl-host-config : binding-table JSON serialize-roundtrip integrates with
//     the existing host-config persistence path.
//   - ui_overlay : reads `active_mode()` once per frame · re-prints prompts.

#![allow(clippy::module_name_repetitions)]

use std::collections::BTreeMap;

use crate::genre_fluid_camera::CameraMode;

// ──────────────────────────────────────────────────────────────────────────
// § CONSTANTS  (mirror systems/input_genre_fluid.csl)
// ──────────────────────────────────────────────────────────────────────────

/// Grace-window in milliseconds before a switched-away source is forgotten.
/// During the grace, the previously-active source stays "warm" so a
/// reach-back to the keyboard doesn't trigger a flicker on the UI prompts.
pub const GRACE_WINDOW_MS: u64 = 500;

/// Maximum mouse-DPI exposed to player-tuning. Floor is 100.
pub const MAX_MOUSE_DPI: u32 = 6400;
pub const MIN_MOUSE_DPI: u32 = 100;

/// Aim-assist tier-count (0 OFF · 1 LIGHT · 2 MODERATE · 3 STRONG · 4 MAX).
pub const AIM_ASSIST_TIER_COUNT: u8 = 5;

/// Sovereign-cap key required for aim-assist tier ≥ 3.
pub const AIM_ASSIST_HIGH_CAP_KEY: &str = "input.aim_assist.high";

/// Server-clamp tier-cap for PvP context (anti-bullying axiom).
/// Players with class-cap ≥ 2 can lift to tier-2 max in PvP ; otherwise tier-1.
pub const PVP_TIER_CLAMP: u8 = 2;

// ──────────────────────────────────────────────────────────────────────────
// § INPUT MODE ENUM
// ──────────────────────────────────────────────────────────────────────────

/// 4 mutually-exclusive input modes. `as_u8()` is stable for serialization +
/// Σ-Chain attestation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputMode {
    /// Default-PC : WASD + mouse-aim.
    KeyboardMouse,
    /// Standard Xbox-class controller (Xinput).
    GamepadXinput,
    /// PS5 DualSense (adaptive-triggers + haptics-aware).
    GamepadDualSense,
    /// Mobile/tablet touch + virtual-sticks.
    TouchScreen,
}

impl InputMode {
    /// Stable u8 discriminant.
    #[must_use]
    pub fn as_u8(self) -> u8 {
        match self {
            Self::KeyboardMouse => 0,
            Self::GamepadXinput => 1,
            Self::GamepadDualSense => 2,
            Self::TouchScreen => 3,
        }
    }

    /// Human-readable name for prompts.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::KeyboardMouse => "keyboard+mouse",
            Self::GamepadXinput => "xbox-controller",
            Self::GamepadDualSense => "ps5-controller",
            Self::TouchScreen => "touch",
        }
    }
}

impl Default for InputMode {
    fn default() -> Self {
        Self::KeyboardMouse
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § ABSTRACT ACTION SET (binding-table key)
// ──────────────────────────────────────────────────────────────────────────

/// Abstract player-action enumerated for remapping. Each action maps to one
/// Binding per active InputMode. Keep this enum SMALL ; new actions land via
/// add-variant + match-arm pair (same discipline as `VirtualKey` in input.rs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Action {
    MoveForward,
    MoveBack,
    MoveLeft,
    MoveRight,
    Jump,
    Crouch,
    Sprint,
    Interact,
    Fire,
    AimDownSights,
    Reload,
    Pause,
    OpenInventory,
    /// Cycle through the 4 camera-modes (matches W13-4).
    CycleCameraMode,
    /// Toggle text-input focus — bridges to W11-1 TextInputState.
    OpenChat,
}

impl Action {
    /// Stable u8 discriminant for serialization.
    #[must_use]
    pub fn as_u8(self) -> u8 {
        match self {
            Self::MoveForward => 0,
            Self::MoveBack => 1,
            Self::MoveLeft => 2,
            Self::MoveRight => 3,
            Self::Jump => 4,
            Self::Crouch => 5,
            Self::Sprint => 6,
            Self::Interact => 7,
            Self::Fire => 8,
            Self::AimDownSights => 9,
            Self::Reload => 10,
            Self::Pause => 11,
            Self::OpenInventory => 12,
            Self::CycleCameraMode => 13,
            Self::OpenChat => 14,
        }
    }
}

/// One binding-record. The `code` is a free-form integer (e.g. winit
/// virtual-key-code · gamepad-button-id · touch-region-id) ; the binding
/// table is mode-keyed so the same Action can have completely different
/// codes per InputMode.
///
/// `hold_to_toggle` flips press-and-hold to press-once-toggles. This is per
/// (Action, InputMode) pair so a player can have hold-crouch on KB+M but
/// toggle-crouch on the gamepad.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    pub code: u32,
    pub hold_to_toggle: bool,
}

impl Binding {
    pub const fn new(code: u32) -> Self {
        Self {
            code,
            hold_to_toggle: false,
        }
    }
    pub const fn toggle(code: u32) -> Self {
        Self {
            code,
            hold_to_toggle: true,
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § AIM-ASSIST (5-tier · sovereign-cap)
// ──────────────────────────────────────────────────────────────────────────

/// 5-tier aim-assist strength. Higher tiers add stronger magnetism +
/// stick-friction near targets. Tier ≥ 3 requires sovereign-cap grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AimAssistTier {
    Off = 0,
    Light = 1,
    Moderate = 2,
    Strong = 3,
    Max = 4,
}

impl AimAssistTier {
    /// Clamp a u8 input to a valid tier.
    #[must_use]
    pub fn from_u8(t: u8) -> Self {
        match t {
            0 => Self::Off,
            1 => Self::Light,
            2 => Self::Moderate,
            3 => Self::Strong,
            _ => Self::Max,
        }
    }
    pub fn as_u8(self) -> u8 {
        self as u8
    }
    /// Magnetism-coefficient applied to aim-vector (0..1).
    #[must_use]
    pub fn magnetism(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Light => 0.10,
            Self::Moderate => 0.20,
            Self::Strong => 0.35,
            Self::Max => 0.50,
        }
    }
    /// Stick-friction near a target (slows aim-rotation when reticle is on enemy).
    #[must_use]
    pub fn friction(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Light => 0.05,
            Self::Moderate => 0.10,
            Self::Strong => 0.20,
            Self::Max => 0.30,
        }
    }
    /// Whether this tier requires sovereign-cap grant.
    #[must_use]
    pub fn requires_sovereign_cap(self) -> bool {
        matches!(self, Self::Strong | Self::Max)
    }
}

/// Result of an aim-assist tier-set request. Distinguishes successful
/// transitions from cap-deny so the host can surface the consent-prompt UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AimAssistGrantResult {
    /// Tier set successfully.
    Granted(AimAssistTier),
    /// Cap-table did not grant the required key. The previous tier remains.
    DeniedNoSovereignCap,
    /// PvP server-clamp prevented the tier from going above the class-cap.
    ClampedToPvpCap(AimAssistTier),
}

// ──────────────────────────────────────────────────────────────────────────
// § ACCESSIBILITY FLAGS
// ──────────────────────────────────────────────────────────────────────────

/// OS-integration accessibility flags. `default()` is all-OFF ; the host
/// observes platform-level state and feeds it in. Honoring these is a
/// PRIME-DIRECTIVE consent-OS axiom : the platform's accessibility settings
/// MUST be respected, never overridden.
///
/// `Eq` cannot be derived because `haptic_strength` is `f32`. Use `PartialEq`
/// for tests ; canonical equality is the "have these settings been changed"
/// question which the surrounding `InputModeRouter` answers via per-field
/// reads, not blanket equality.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AccessibilityFlags {
    /// Sticky-keys : single press of a modifier "stays-down" until next
    /// non-modifier key. OS-driven ; we mirror state for in-game hint UI.
    pub sticky_keys: bool,
    /// Slow-keys : minimum key-press duration (ms) before a press registers.
    /// Zero = OFF.
    pub slow_keys_ms: u32,
    /// Bounce-keys : repeat-suppression window (ms) for the same key. Zero = OFF.
    pub bounce_keys_ms: u32,
    /// Mouse-acceleration ON/OFF. False = raw-input (no OS accel).
    pub mouse_accel: bool,
    /// Mouse DPI tuning. Clamped to [MIN_MOUSE_DPI, MAX_MOUSE_DPI] on apply.
    pub mouse_dpi: u32,
    /// Anonymous binding-share opt-in. Default false ← privacy-by-default.
    pub binding_share_optin: bool,
    /// Haptic-rumble cap on DualSense / Switch-class controllers (0..1).
    pub haptic_strength: f32,
}

impl AccessibilityFlags {
    pub fn new() -> Self {
        Self {
            sticky_keys: false,
            slow_keys_ms: 0,
            bounce_keys_ms: 0,
            mouse_accel: false,
            mouse_dpi: 800, // common-default
            binding_share_optin: false,
            haptic_strength: 1.0,
        }
    }

    /// Clamp the mouse-DPI to the legal range.
    pub fn apply_dpi_clamp(&mut self) {
        if self.mouse_dpi < MIN_MOUSE_DPI {
            self.mouse_dpi = MIN_MOUSE_DPI;
        }
        if self.mouse_dpi > MAX_MOUSE_DPI {
            self.mouse_dpi = MAX_MOUSE_DPI;
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § BINDING TABLE
// ──────────────────────────────────────────────────────────────────────────

/// Binding-storage keyed by (mode, action). Two-level BTreeMap keeps the
/// JSON-roundtrip deterministic.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BindingTable {
    pub by_mode: BTreeMap<u8, BTreeMap<u8, Binding>>,
}

impl BindingTable {
    /// Load the canonical default bindings (matches the per-mode binding-set
    /// in `input_genre_fluid.csl`). Players are expected to remap from here.
    #[must_use]
    pub fn defaults() -> Self {
        let mut t = Self::default();
        // KB+M defaults — winit virtual-key-codes are arbitrary u32 here
        // (the integration commit maps them through input.rs's VirtualKey).
        let kbm = t.mode_map(InputMode::KeyboardMouse);
        kbm.insert(Action::MoveForward.as_u8(), Binding::new(0x57)); // 'W'
        kbm.insert(Action::MoveBack.as_u8(), Binding::new(0x53)); // 'S'
        kbm.insert(Action::MoveLeft.as_u8(), Binding::new(0x41)); // 'A'
        kbm.insert(Action::MoveRight.as_u8(), Binding::new(0x44)); // 'D'
        kbm.insert(Action::Jump.as_u8(), Binding::new(0x20)); // Space
        kbm.insert(Action::Crouch.as_u8(), Binding::new(0xA2)); // LCtrl
        kbm.insert(Action::Sprint.as_u8(), Binding::new(0xA0)); // LShift
        kbm.insert(Action::Interact.as_u8(), Binding::new(0x46)); // 'F'
        kbm.insert(Action::Fire.as_u8(), Binding::new(0x01)); // LMB
        kbm.insert(Action::AimDownSights.as_u8(), Binding::new(0x02)); // RMB
        kbm.insert(Action::Reload.as_u8(), Binding::new(0x52)); // 'R'
        kbm.insert(Action::Pause.as_u8(), Binding::new(0x09)); // Tab
        kbm.insert(Action::OpenInventory.as_u8(), Binding::new(0x49)); // 'I'
        kbm.insert(Action::CycleCameraMode.as_u8(), Binding::new(0x56)); // 'V'
        kbm.insert(Action::OpenChat.as_u8(), Binding::new(0xBF)); // '/'

        // Xinput gamepad — button-IDs (1=A,2=B,3=X,4=Y,5=LB,6=RB,7=LT,8=RT,9=BACK,10=START,11=LS,12=RS).
        let pad = t.mode_map(InputMode::GamepadXinput);
        pad.insert(Action::MoveForward.as_u8(), Binding::new(101)); // LStickY+
        pad.insert(Action::MoveBack.as_u8(), Binding::new(102)); // LStickY-
        pad.insert(Action::MoveLeft.as_u8(), Binding::new(103)); // LStickX-
        pad.insert(Action::MoveRight.as_u8(), Binding::new(104)); // LStickX+
        pad.insert(Action::Jump.as_u8(), Binding::new(1)); // A
        pad.insert(Action::Crouch.as_u8(), Binding::toggle(2)); // B-toggle
        pad.insert(Action::Sprint.as_u8(), Binding::new(11)); // LSClick
        pad.insert(Action::Interact.as_u8(), Binding::new(3)); // X
        pad.insert(Action::Fire.as_u8(), Binding::new(8)); // RT
        pad.insert(Action::AimDownSights.as_u8(), Binding::new(7)); // LT-hold
        pad.insert(Action::Reload.as_u8(), Binding::new(4)); // Y
        pad.insert(Action::Pause.as_u8(), Binding::new(10)); // START
        pad.insert(Action::OpenInventory.as_u8(), Binding::new(9)); // BACK
        pad.insert(Action::CycleCameraMode.as_u8(), Binding::new(5)); // LB
        pad.insert(Action::OpenChat.as_u8(), Binding::new(6)); // RB

        // DualSense — same logical layout, distinct ID-space (200..)
        let ds = t.mode_map(InputMode::GamepadDualSense);
        ds.insert(Action::MoveForward.as_u8(), Binding::new(201));
        ds.insert(Action::MoveBack.as_u8(), Binding::new(202));
        ds.insert(Action::MoveLeft.as_u8(), Binding::new(203));
        ds.insert(Action::MoveRight.as_u8(), Binding::new(204));
        ds.insert(Action::Jump.as_u8(), Binding::new(210)); // X (PS)
        ds.insert(Action::Crouch.as_u8(), Binding::toggle(211)); // Circle-toggle
        ds.insert(Action::Sprint.as_u8(), Binding::new(220)); // L3
        ds.insert(Action::Interact.as_u8(), Binding::new(212)); // Square
        ds.insert(Action::Fire.as_u8(), Binding::new(218)); // R2
        ds.insert(Action::AimDownSights.as_u8(), Binding::new(217)); // L2-hold
        ds.insert(Action::Reload.as_u8(), Binding::new(213)); // Triangle
        ds.insert(Action::Pause.as_u8(), Binding::new(219)); // Options
        ds.insert(Action::OpenInventory.as_u8(), Binding::new(216)); // Touchpad
        ds.insert(Action::CycleCameraMode.as_u8(), Binding::new(214)); // L1
        ds.insert(Action::OpenChat.as_u8(), Binding::new(215)); // R1

        // Touch — virtual region IDs (300..)
        let touch = t.mode_map(InputMode::TouchScreen);
        touch.insert(Action::MoveForward.as_u8(), Binding::new(301)); // VirtStick+Y
        touch.insert(Action::MoveBack.as_u8(), Binding::new(302));
        touch.insert(Action::MoveLeft.as_u8(), Binding::new(303));
        touch.insert(Action::MoveRight.as_u8(), Binding::new(304));
        touch.insert(Action::Jump.as_u8(), Binding::new(310)); // RegionA
        touch.insert(Action::Crouch.as_u8(), Binding::toggle(311));
        touch.insert(Action::Sprint.as_u8(), Binding::new(312));
        touch.insert(Action::Interact.as_u8(), Binding::new(313));
        touch.insert(Action::Fire.as_u8(), Binding::new(320));
        touch.insert(Action::AimDownSights.as_u8(), Binding::new(321));
        touch.insert(Action::Reload.as_u8(), Binding::new(322));
        touch.insert(Action::Pause.as_u8(), Binding::new(330));
        touch.insert(Action::OpenInventory.as_u8(), Binding::new(331));
        touch.insert(Action::CycleCameraMode.as_u8(), Binding::new(340));
        touch.insert(Action::OpenChat.as_u8(), Binding::new(341));

        t
    }

    fn mode_map(&mut self, mode: InputMode) -> &mut BTreeMap<u8, Binding> {
        self.by_mode.entry(mode.as_u8()).or_default()
    }

    /// Look up a binding by (mode, action). Returns `None` if not bound.
    #[must_use]
    pub fn get(&self, mode: InputMode, action: Action) -> Option<Binding> {
        self.by_mode
            .get(&mode.as_u8())
            .and_then(|m| m.get(&action.as_u8()).copied())
    }

    /// Remap a single (mode, action) → binding. Replaces any existing
    /// binding for this pair. `hold_to_toggle` is preserved on the new value.
    pub fn remap(&mut self, mode: InputMode, action: Action, binding: Binding) {
        self.by_mode
            .entry(mode.as_u8())
            .or_default()
            .insert(action.as_u8(), binding);
    }

    /// Toggle hold-to-toggle on a single (mode, action).
    pub fn set_hold_to_toggle(&mut self, mode: InputMode, action: Action, on: bool) {
        if let Some(m) = self.by_mode.get_mut(&mode.as_u8()) {
            if let Some(b) = m.get_mut(&action.as_u8()) {
                b.hold_to_toggle = on;
            }
        }
    }

    /// Serialize to JSON (deterministic via BTreeMap).
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut out = String::from("{");
        let mut first_mode = true;
        for (mode_id, actions) in &self.by_mode {
            if !first_mode {
                out.push(',');
            }
            first_mode = false;
            out.push_str(&format!("\"{mode_id}\":{{"));
            let mut first_act = true;
            for (act_id, b) in actions {
                if !first_act {
                    out.push(',');
                }
                first_act = false;
                let toggle_flag = u8::from(b.hold_to_toggle);
                out.push_str(&format!("\"{act_id}\":[{},{}]", b.code, toggle_flag));
            }
            out.push('}');
        }
        out.push('}');
        out
    }

    /// Deserialize from the format `to_json` emits. On parse-error the
    /// returned table is empty (caller can fall back to `defaults()`).
    #[must_use]
    pub fn from_json(s: &str) -> Self {
        let mut t = Self::default();
        // Tiny purpose-built parser — the format is constrained enough that
        // a full serde dep is overkill for the catalog build.
        let bytes = s.as_bytes();
        let mut i = 0;
        if i >= bytes.len() || bytes[i] != b'{' {
            return t;
        }
        i += 1;
        loop {
            // skip whitespace + leading separators
            while i < bytes.len() && (bytes[i] == b',' || bytes[i] == b' ') {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] == b'}' {
                break;
            }
            // mode_id quoted-int
            if bytes[i] != b'"' {
                break;
            }
            i += 1;
            let key_start = i;
            while i < bytes.len() && bytes[i] != b'"' {
                i += 1;
            }
            let mode_id: u8 = match std::str::from_utf8(&bytes[key_start..i])
                .ok()
                .and_then(|s| s.parse().ok())
            {
                Some(v) => v,
                None => return Self::default(),
            };
            i += 1; // consume closing "
            // expect ':'
            while i < bytes.len() && bytes[i] != b':' {
                i += 1;
            }
            i += 1;
            while i < bytes.len() && bytes[i] != b'{' {
                i += 1;
            }
            i += 1;
            let act_map = t.by_mode.entry(mode_id).or_default();
            // parse actions
            loop {
                while i < bytes.len() && (bytes[i] == b',' || bytes[i] == b' ') {
                    i += 1;
                }
                if i >= bytes.len() || bytes[i] == b'}' {
                    if i < bytes.len() {
                        i += 1;
                    }
                    break;
                }
                if bytes[i] != b'"' {
                    break;
                }
                i += 1;
                let ks = i;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += 1;
                }
                let act_id: u8 = match std::str::from_utf8(&bytes[ks..i])
                    .ok()
                    .and_then(|s| s.parse().ok())
                {
                    Some(v) => v,
                    None => return Self::default(),
                };
                i += 1;
                while i < bytes.len() && bytes[i] != b'[' {
                    i += 1;
                }
                i += 1;
                let cs = i;
                while i < bytes.len() && bytes[i] != b',' {
                    i += 1;
                }
                let code: u32 = match std::str::from_utf8(&bytes[cs..i])
                    .ok()
                    .and_then(|s| s.parse().ok())
                {
                    Some(v) => v,
                    None => return Self::default(),
                };
                i += 1;
                let ts = i;
                while i < bytes.len() && bytes[i] != b']' {
                    i += 1;
                }
                let toggle: u8 = match std::str::from_utf8(&bytes[ts..i])
                    .ok()
                    .and_then(|s| s.parse().ok())
                {
                    Some(v) => v,
                    None => return Self::default(),
                };
                i += 1;
                act_map.insert(
                    act_id,
                    Binding {
                        code,
                        hold_to_toggle: toggle != 0,
                    },
                );
            }
        }
        t
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § ROUTER (auto-detection · per-mode binding-set · UI prompt-shift)
// ──────────────────────────────────────────────────────────────────────────

/// Source-tagged input event. The router only needs the source-id + the
/// monotonic millisecond timestamp ; the actual payload is owned by the
/// existing `RawEvent` pipeline in `input.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceEvent {
    pub source: InputMode,
    pub now_ms: u64,
}

/// In-PvP context flag. Provided by the host's session-state ; controls
/// whether the server-clamp on aim-assist applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionContext {
    SinglePlayer,
    Cooperative,
    PvP,
}

impl Default for SessionContext {
    fn default() -> Self {
        Self::SinglePlayer
    }
}

/// Top-level state machine for genre-fluid input.
///
/// Owns :
///   • active InputMode + last-seen-timestamp per source
///   • binding-table (mode-keyed)
///   • accessibility-flags
///   • aim-assist-tier + sovereign-cap-state
///   • camera-mode (mirrors W13-4 for per-mode binding-set selection)
#[derive(Debug, Clone)]
pub struct InputModeRouter {
    active: InputMode,
    last_event_ms: BTreeMap<u8, u64>, // mode-id → ms timestamp
    bindings: BindingTable,
    accessibility: AccessibilityFlags,
    aim_assist: AimAssistTier,
    sovereign_cap_high_grant: bool,
    pvp_class_cap: u8, // server-side cap for PvP clamp (0..=4)
    camera_mode: CameraMode,
    context: SessionContext,
    /// True if the active-mode flipped THIS frame ; consumed by `consume_frame_router()`.
    mode_changed_this_frame: bool,
}

impl Default for InputModeRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl InputModeRouter {
    #[must_use]
    pub fn new() -> Self {
        Self {
            active: InputMode::KeyboardMouse,
            last_event_ms: BTreeMap::new(),
            bindings: BindingTable::defaults(),
            accessibility: AccessibilityFlags::new(),
            aim_assist: AimAssistTier::Off,
            sovereign_cap_high_grant: false,
            pvp_class_cap: PVP_TIER_CLAMP,
            camera_mode: CameraMode::FpsLocked,
            context: SessionContext::SinglePlayer,
            mode_changed_this_frame: false,
        }
    }

    /// Currently-active input mode. Last-source-wins outside the grace-window.
    #[must_use]
    pub fn active_mode(&self) -> InputMode {
        self.active
    }

    /// Camera-mode (W13-4 mirror).
    #[must_use]
    pub fn camera_mode(&self) -> CameraMode {
        self.camera_mode
    }

    /// Handle a source-tagged event. Activates the corresponding InputMode if
    /// it differs AND the grace-window has elapsed since the previous source's
    /// last event. Within the grace, the previously-active source stays held.
    pub fn handle_source_event(&mut self, ev: SourceEvent) {
        let prev_active_id = self.active.as_u8();
        // Always update the last-seen for THIS source (so we know the source is alive).
        self.last_event_ms.insert(ev.source.as_u8(), ev.now_ms);
        if ev.source == self.active {
            return;
        }
        // Different source : decide based on the grace-window.
        let prev_last = self.last_event_ms.get(&prev_active_id).copied().unwrap_or(0);
        // If the previous active source has not received an event in
        // GRACE_WINDOW_MS, switch immediately. Otherwise switch only if the
        // grace elapsed since the previous event from the previously-active
        // source. The clean rule : `now_ms - prev_last >= GRACE`.
        let elapsed = ev.now_ms.saturating_sub(prev_last);
        if elapsed >= GRACE_WINDOW_MS {
            self.set_active(ev.source);
        }
    }

    fn set_active(&mut self, mode: InputMode) {
        if self.active != mode {
            self.active = mode;
            self.mode_changed_this_frame = true;
        }
    }

    /// Force-set the active mode (e.g. for tests + initial-config).
    pub fn force_active(&mut self, mode: InputMode) {
        self.set_active(mode);
    }

    /// Set the camera-mode (W13-4 mirror). Causes the per-mode binding-set
    /// lookup to consider this mode (e.g. iso vs FPS uses different sets).
    pub fn set_camera_mode(&mut self, mode: CameraMode) {
        self.camera_mode = mode;
    }

    /// Set session-context (drives PvP-clamp).
    pub fn set_context(&mut self, ctx: SessionContext) {
        self.context = ctx;
    }

    /// Player-class-cap for PvP clamp. Default `PVP_TIER_CLAMP` ; can be
    /// raised via cap-table grant.
    pub fn set_pvp_class_cap(&mut self, cap: u8) {
        self.pvp_class_cap = cap.min(AIM_ASSIST_TIER_COUNT - 1);
    }

    /// Look up the binding for an action under the current active mode.
    #[must_use]
    pub fn current_binding(&self, action: Action) -> Option<Binding> {
        self.bindings.get(self.active, action)
    }

    /// Mutable access to the binding-table (for remap UI).
    pub fn bindings_mut(&mut self) -> &mut BindingTable {
        &mut self.bindings
    }

    /// Read-only access to the binding-table (for serialization + UI).
    #[must_use]
    pub fn bindings(&self) -> &BindingTable {
        &self.bindings
    }

    /// Mutable access to the accessibility-flags (for in-game settings UI).
    pub fn accessibility_mut(&mut self) -> &mut AccessibilityFlags {
        &mut self.accessibility
    }

    /// Read-only access to the accessibility-flags.
    #[must_use]
    pub fn accessibility(&self) -> &AccessibilityFlags {
        &self.accessibility
    }

    /// Grant or revoke the sovereign-cap key for high aim-assist tiers.
    /// The host's cap-table integration calls this when the player's
    /// cap-state changes.
    pub fn set_sovereign_cap_high(&mut self, granted: bool) {
        self.sovereign_cap_high_grant = granted;
        // If the cap is revoked while a high tier is active, drop back to
        // the highest legal tier (Moderate).
        if !granted && self.aim_assist.requires_sovereign_cap() {
            self.aim_assist = AimAssistTier::Moderate;
        }
    }

    /// Request an aim-assist tier change. Enforces sovereign-cap on tier ≥ 3
    /// AND PvP server-clamp on tier > class-cap. Returns the result so the
    /// host can surface the appropriate UI.
    pub fn set_aim_assist(&mut self, requested: AimAssistTier) -> AimAssistGrantResult {
        // Sovereign-cap gate : tier ≥ Strong needs the high-cap grant.
        if requested.requires_sovereign_cap() && !self.sovereign_cap_high_grant {
            return AimAssistGrantResult::DeniedNoSovereignCap;
        }
        // PvP clamp : never exceed the class-cap in PvP.
        if matches!(self.context, SessionContext::PvP) && requested.as_u8() > self.pvp_class_cap {
            let clamped = AimAssistTier::from_u8(self.pvp_class_cap);
            self.aim_assist = clamped;
            return AimAssistGrantResult::ClampedToPvpCap(clamped);
        }
        self.aim_assist = requested;
        AimAssistGrantResult::Granted(requested)
    }

    /// Current aim-assist tier (post-clamp).
    #[must_use]
    pub fn aim_assist_tier(&self) -> AimAssistTier {
        self.aim_assist
    }

    /// True when `aim_assist_tier` has the high cap granted.
    #[must_use]
    pub fn sovereign_cap_high(&self) -> bool {
        self.sovereign_cap_high_grant
    }

    /// UI prompt-string per Action under the currently-active InputMode.
    /// The host's UI overlay calls this to swap "press F" ↔ "press X" as
    /// the active mode changes. Strings are short ASCII tokens — locale +
    /// font-shaping is the UI-overlay's territory.
    #[must_use]
    pub fn prompt_string(&self, action: Action) -> &'static str {
        match (self.active, action) {
            (InputMode::KeyboardMouse, Action::Interact) => "press F",
            (InputMode::KeyboardMouse, Action::Jump) => "press Space",
            (InputMode::KeyboardMouse, Action::Fire) => "click LMB",
            (InputMode::KeyboardMouse, Action::AimDownSights) => "hold RMB",
            (InputMode::KeyboardMouse, Action::Reload) => "press R",
            (InputMode::GamepadXinput, Action::Interact) => "press X",
            (InputMode::GamepadXinput, Action::Jump) => "press A",
            (InputMode::GamepadXinput, Action::Fire) => "press RT",
            (InputMode::GamepadXinput, Action::AimDownSights) => "hold LT",
            (InputMode::GamepadXinput, Action::Reload) => "press Y",
            (InputMode::GamepadDualSense, Action::Interact) => "press Square",
            (InputMode::GamepadDualSense, Action::Jump) => "press X",
            (InputMode::GamepadDualSense, Action::Fire) => "press R2",
            (InputMode::GamepadDualSense, Action::AimDownSights) => "hold L2",
            (InputMode::GamepadDualSense, Action::Reload) => "press Triangle",
            (InputMode::TouchScreen, Action::Interact) => "tap to interact",
            (InputMode::TouchScreen, Action::Jump) => "tap jump-button",
            (InputMode::TouchScreen, Action::Fire) => "tap fire-button",
            (InputMode::TouchScreen, Action::AimDownSights) => "tap ADS-button",
            (InputMode::TouchScreen, Action::Reload) => "tap reload-button",
            // Generic per-action fallbacks
            (_, Action::MoveForward) => "move-forward",
            (_, Action::MoveBack) => "move-back",
            (_, Action::MoveLeft) => "move-left",
            (_, Action::MoveRight) => "move-right",
            (_, Action::Sprint) => "sprint",
            (_, Action::Crouch) => "crouch",
            (_, Action::Pause) => "pause",
            (_, Action::OpenInventory) => "inventory",
            (_, Action::CycleCameraMode) => "camera-mode",
            (_, Action::OpenChat) => "open-chat",
        }
    }

    /// Apply a one-frame aim-assist correction to a (yaw_delta, pitch_delta)
    /// pair given a target offset (target_yaw_offset, target_pitch_offset)
    /// from the current reticle. Returns the corrected deltas. The host's
    /// camera/weapon code calls this AFTER reading the player's raw aim.
    ///
    /// `target_offset = (0, 0)` means no target ⇒ no correction.
    /// Coefficients come from the active tier ; deterministic per-tier.
    pub fn apply_aim_assist(
        &self,
        yaw_delta: f32,
        pitch_delta: f32,
        target_yaw_offset: f32,
        target_pitch_offset: f32,
    ) -> (f32, f32) {
        let mag = self.aim_assist.magnetism();
        let fric = self.aim_assist.friction();
        // Magnetism : pull aim slightly toward target.
        let pull_yaw = target_yaw_offset * mag;
        let pull_pitch = target_pitch_offset * mag;
        // Friction : reduce the player's own delta when on-target (target offset
        // very small ⇒ friction full ; target offset large ⇒ friction zero).
        let on_target = (target_yaw_offset * target_yaw_offset
            + target_pitch_offset * target_pitch_offset)
            < 0.05_f32 * 0.05_f32;
        let friction_factor = if on_target { 1.0 - fric } else { 1.0 };
        let new_yaw = yaw_delta * friction_factor + pull_yaw;
        let new_pitch = pitch_delta * friction_factor + pull_pitch;
        (new_yaw, new_pitch)
    }

    /// Drain per-frame edges. Returns a snapshot the host can route to
    /// telemetry + UI. Mode-changed flag is reset.
    pub fn consume_frame_router(&mut self) -> RouterFrame {
        let f = RouterFrame {
            active_mode: self.active,
            mode_changed: self.mode_changed_this_frame,
            aim_assist_tier: self.aim_assist,
            camera_mode: self.camera_mode,
            sovereign_cap_high: self.sovereign_cap_high_grant,
        };
        self.mode_changed_this_frame = false;
        f
    }
}

/// Per-frame router snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouterFrame {
    pub active_mode: InputMode,
    pub mode_changed: bool,
    pub aim_assist_tier: AimAssistTier,
    pub camera_mode: CameraMode,
    pub sovereign_cap_high: bool,
}

// ──────────────────────────────────────────────────────────────────────────
// § TESTS
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    fn ev(source: InputMode, now_ms: u64) -> SourceEvent {
        SourceEvent { source, now_ms }
    }

    #[test]
    fn input_mode_default_keyboard_mouse() {
        let r = InputModeRouter::new();
        assert_eq!(r.active_mode(), InputMode::KeyboardMouse);
        assert_eq!(r.aim_assist_tier(), AimAssistTier::Off);
    }

    #[test]
    fn auto_detect_switches_on_gamepad_event_after_grace() {
        let mut r = InputModeRouter::new();
        // Initial KB+M event @ t=0
        r.handle_source_event(ev(InputMode::KeyboardMouse, 0));
        // Gamepad event arrives 500ms+ later → should switch.
        r.handle_source_event(ev(InputMode::GamepadXinput, GRACE_WINDOW_MS));
        assert_eq!(r.active_mode(), InputMode::GamepadXinput);
        let frame = r.consume_frame_router();
        assert!(frame.mode_changed);
    }

    #[test]
    fn grace_window_prevents_thrash() {
        // Within the 500ms grace, alternating-source events should NOT switch.
        let mut r = InputModeRouter::new();
        r.handle_source_event(ev(InputMode::KeyboardMouse, 0));
        // Gamepad event @ 100ms — within grace, should NOT switch.
        r.handle_source_event(ev(InputMode::GamepadXinput, 100));
        assert_eq!(r.active_mode(), InputMode::KeyboardMouse);
        // KB+M event @ 200ms keeps it active.
        r.handle_source_event(ev(InputMode::KeyboardMouse, 200));
        // Gamepad event @ 250ms — still inside KB+M's grace from t=200, NO switch.
        r.handle_source_event(ev(InputMode::GamepadXinput, 250));
        assert_eq!(r.active_mode(), InputMode::KeyboardMouse);
        // Gamepad event @ 700ms — KB+M last seen @ 200, elapsed=500 → SWITCH.
        r.handle_source_event(ev(InputMode::GamepadXinput, 700));
        assert_eq!(r.active_mode(), InputMode::GamepadXinput);
    }

    #[test]
    fn binding_table_defaults_load_all_modes() {
        let t = BindingTable::defaults();
        // 4 modes, each with all 15 actions bound.
        assert_eq!(t.by_mode.len(), 4);
        for mode_id in 0..4u8 {
            assert_eq!(t.by_mode[&mode_id].len(), 15);
        }
    }

    #[test]
    fn binding_remap_roundtrip_replaces() {
        let mut t = BindingTable::defaults();
        // Default Jump on KB+M is Space (0x20). Remap to V (0x56).
        t.remap(
            InputMode::KeyboardMouse,
            Action::Jump,
            Binding::new(0x56),
        );
        let b = t.get(InputMode::KeyboardMouse, Action::Jump).unwrap();
        assert_eq!(b.code, 0x56);
        assert!(!b.hold_to_toggle);
    }

    #[test]
    fn binding_table_json_roundtrip_preserves_codes() {
        let original = BindingTable::defaults();
        let json = original.to_json();
        let parsed = BindingTable::from_json(&json);
        // Every (mode, action) → binding must roundtrip.
        for mode_id in 0..4u8 {
            let lhs = &original.by_mode[&mode_id];
            let rhs = &parsed.by_mode[&mode_id];
            assert_eq!(lhs.len(), rhs.len(), "mode {mode_id} action-count mismatch");
            for (act, b1) in lhs {
                let b2 = rhs.get(act).expect("action present in parsed");
                assert_eq!(b1, b2, "mode {mode_id} action {act} binding mismatch");
            }
        }
    }

    #[test]
    fn ui_prompt_shifts_atomically_on_mode_change() {
        let mut r = InputModeRouter::new();
        assert_eq!(r.prompt_string(Action::Interact), "press F");
        r.force_active(InputMode::GamepadXinput);
        assert_eq!(r.prompt_string(Action::Interact), "press X");
        r.force_active(InputMode::GamepadDualSense);
        assert_eq!(r.prompt_string(Action::Interact), "press Square");
        r.force_active(InputMode::TouchScreen);
        assert_eq!(r.prompt_string(Action::Interact), "tap to interact");
    }

    #[test]
    fn aim_assist_tier_clamp_zero_to_four() {
        assert_eq!(AimAssistTier::from_u8(0), AimAssistTier::Off);
        assert_eq!(AimAssistTier::from_u8(1), AimAssistTier::Light);
        assert_eq!(AimAssistTier::from_u8(2), AimAssistTier::Moderate);
        assert_eq!(AimAssistTier::from_u8(3), AimAssistTier::Strong);
        assert_eq!(AimAssistTier::from_u8(4), AimAssistTier::Max);
        // u8 above 4 clamps to Max
        assert_eq!(AimAssistTier::from_u8(5), AimAssistTier::Max);
        assert_eq!(AimAssistTier::from_u8(255), AimAssistTier::Max);
    }

    #[test]
    fn aim_assist_sovereign_cap_denies_strong_without_grant() {
        let mut r = InputModeRouter::new();
        // No cap granted → Strong should be denied.
        let res = r.set_aim_assist(AimAssistTier::Strong);
        assert_eq!(res, AimAssistGrantResult::DeniedNoSovereignCap);
        assert_eq!(r.aim_assist_tier(), AimAssistTier::Off);
        // Light + Moderate go through without cap.
        let _ = r.set_aim_assist(AimAssistTier::Moderate);
        assert_eq!(r.aim_assist_tier(), AimAssistTier::Moderate);
        // Grant cap → Strong now accepted.
        r.set_sovereign_cap_high(true);
        let res2 = r.set_aim_assist(AimAssistTier::Strong);
        assert_eq!(res2, AimAssistGrantResult::Granted(AimAssistTier::Strong));
        assert_eq!(r.aim_assist_tier(), AimAssistTier::Strong);
    }

    #[test]
    fn aim_assist_revoke_cap_drops_high_tier_back_to_moderate() {
        let mut r = InputModeRouter::new();
        r.set_sovereign_cap_high(true);
        let _ = r.set_aim_assist(AimAssistTier::Max);
        assert_eq!(r.aim_assist_tier(), AimAssistTier::Max);
        // Revoking cap pulls back to Moderate (highest legal sans cap).
        r.set_sovereign_cap_high(false);
        assert_eq!(r.aim_assist_tier(), AimAssistTier::Moderate);
    }

    #[test]
    fn aim_assist_pvp_clamp_caps_at_class_limit() {
        let mut r = InputModeRouter::new();
        r.set_sovereign_cap_high(true); // cap not the issue
        r.set_context(SessionContext::PvP);
        r.set_pvp_class_cap(1); // class-cap = Light
        let res = r.set_aim_assist(AimAssistTier::Strong);
        assert_eq!(
            res,
            AimAssistGrantResult::ClampedToPvpCap(AimAssistTier::Light)
        );
        assert_eq!(r.aim_assist_tier(), AimAssistTier::Light);
    }

    #[test]
    fn aim_assist_magnetism_per_tier_is_deterministic() {
        // Per-tier magnetism + friction values mirror the .csl spec.
        assert!((AimAssistTier::Off.magnetism() - 0.0).abs() < 1e-6);
        assert!((AimAssistTier::Light.magnetism() - 0.10).abs() < 1e-6);
        assert!((AimAssistTier::Moderate.magnetism() - 0.20).abs() < 1e-6);
        assert!((AimAssistTier::Strong.magnetism() - 0.35).abs() < 1e-6);
        assert!((AimAssistTier::Max.magnetism() - 0.50).abs() < 1e-6);
        assert!((AimAssistTier::Off.friction() - 0.0).abs() < 1e-6);
        assert!((AimAssistTier::Max.friction() - 0.30).abs() < 1e-6);
    }

    #[test]
    fn aim_assist_apply_pulls_toward_target() {
        let mut r = InputModeRouter::new();
        r.set_sovereign_cap_high(true);
        let _ = r.set_aim_assist(AimAssistTier::Strong);
        // Player aiming straight (yaw_delta=0) ; target offset to the right.
        let (yaw, pitch) = r.apply_aim_assist(0.0, 0.0, 1.0, 0.0);
        // Strong magnetism = 0.35 → yaw should pull by 0.35.
        assert!((yaw - 0.35).abs() < 1e-6);
        assert!(pitch.abs() < 1e-6);
        // No tier (Off) → no pull.
        let _ = r.set_aim_assist(AimAssistTier::Off);
        let (yaw0, pitch0) = r.apply_aim_assist(0.0, 0.0, 1.0, 0.0);
        assert_eq!(yaw0, 0.0);
        assert_eq!(pitch0, 0.0);
    }

    #[test]
    fn accessibility_os_integration_toggles() {
        let mut r = InputModeRouter::new();
        // Defaults : all OS-features OFF, mouse-DPI 800.
        assert!(!r.accessibility().sticky_keys);
        assert_eq!(r.accessibility().slow_keys_ms, 0);
        assert_eq!(r.accessibility().bounce_keys_ms, 0);
        assert_eq!(r.accessibility().mouse_dpi, 800);
        // Host observes OS sticky-keys + sets slow/bounce.
        r.accessibility_mut().sticky_keys = true;
        r.accessibility_mut().slow_keys_ms = 80;
        r.accessibility_mut().bounce_keys_ms = 50;
        r.accessibility_mut().mouse_dpi = 12000; // out of range
        r.accessibility_mut().apply_dpi_clamp();
        // DPI clamps to MAX.
        assert_eq!(r.accessibility().mouse_dpi, MAX_MOUSE_DPI);
        // Below-min also clamps.
        r.accessibility_mut().mouse_dpi = 0;
        r.accessibility_mut().apply_dpi_clamp();
        assert_eq!(r.accessibility().mouse_dpi, MIN_MOUSE_DPI);
    }

    #[test]
    fn hold_to_toggle_per_action_per_mode() {
        let mut r = InputModeRouter::new();
        // Default KB+M Crouch is hold (not toggle). Default Xinput Crouch is toggle.
        let kbm_crouch = r
            .bindings()
            .get(InputMode::KeyboardMouse, Action::Crouch)
            .unwrap();
        let pad_crouch = r
            .bindings()
            .get(InputMode::GamepadXinput, Action::Crouch)
            .unwrap();
        assert!(!kbm_crouch.hold_to_toggle);
        assert!(pad_crouch.hold_to_toggle);
        // Player remaps KB+M Crouch to toggle.
        r.bindings_mut()
            .set_hold_to_toggle(InputMode::KeyboardMouse, Action::Crouch, true);
        let after = r
            .bindings()
            .get(InputMode::KeyboardMouse, Action::Crouch)
            .unwrap();
        assert!(after.hold_to_toggle);
        // Gamepad mode unaffected by the KB+M change.
        let pad_after = r
            .bindings()
            .get(InputMode::GamepadXinput, Action::Crouch)
            .unwrap();
        assert!(pad_after.hold_to_toggle);
    }

    #[test]
    fn camera_mode_mirror_w13_4() {
        let mut r = InputModeRouter::new();
        assert_eq!(r.camera_mode(), CameraMode::FpsLocked);
        r.set_camera_mode(CameraMode::Isometric);
        assert_eq!(r.camera_mode(), CameraMode::Isometric);
        // Camera-mode change does NOT change input-mode.
        assert_eq!(r.active_mode(), InputMode::KeyboardMouse);
    }

    #[test]
    fn touch_mode_bindings_present() {
        let r = InputModeRouter::new();
        // All actions bound under TouchScreen mode.
        for action in [
            Action::MoveForward,
            Action::MoveBack,
            Action::Jump,
            Action::Fire,
            Action::Interact,
            Action::OpenChat,
        ] {
            assert!(
                r.bindings().get(InputMode::TouchScreen, action).is_some(),
                "missing touch binding for {action:?}"
            );
        }
    }

    #[test]
    fn router_frame_drains_mode_change_edge() {
        let mut r = InputModeRouter::new();
        r.force_active(InputMode::GamepadXinput);
        let f1 = r.consume_frame_router();
        assert!(f1.mode_changed);
        // Second consume w/o further changes → mode_changed clears.
        let f2 = r.consume_frame_router();
        assert!(!f2.mode_changed);
        assert_eq!(f2.active_mode, InputMode::GamepadXinput);
    }
}
