//! Mixed-reality passthrough composition-layer.
//!
//! § SPEC : `07_AESTHETIC/05_VR_RENDERING.csl` § IX.
//!
//! § DESIGN
//!   - § IX : "passthrough = composition-LAYER ⊗ N! 'render real-world ourselves'"
//!     ⊗ runtime owns-camera-image ⊗ engine declares-want + alpha-blend-with.
//!   - `XR_FB_passthrough` (Quest3) + `XR_META_environment_depth` for
//!     depth-aware occlusion + `XR_FB_passthrough_keyboard_hands` for
//!     hand-cutout.
//!   - `XR_HTC_passthrough` (Vive XR Elite, Vive Focus Vision).
//!   - `Compositor-Services` (visionOS) : passthrough is the default
//!     background ; engine submits content-layer + alpha.

use crate::error::XRFailure;

/// Alpha-blend mode for passthrough composition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlphaMode {
    /// `ADDITIVE` : content + passthrough (passthrough always visible).
    Additive,
    /// `OVER` : content over passthrough (content occludes passthrough).
    Over,
    /// `UNDER` : content under passthrough (passthrough occludes content).
    Under,
}

impl AlphaMode {
    /// Display-name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Additive => "additive",
            Self::Over => "over",
            Self::Under => "under",
        }
    }
}

/// Runtime-side passthrough provider. § IX.A + § IX.B.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PassthroughProvider {
    /// `XR_FB_passthrough` (Meta Quest 3 / Quest Pro).
    MetaFb,
    /// `XR_HTC_passthrough` (Vive XR Elite, Vive Focus Vision).
    HtcVive,
    /// Apple Vision Pro Compositor-Services (passthrough = default background).
    AppleCompositorServices,
    /// No passthrough (e.g. flat-monitor or device without RGB cameras).
    None,
}

impl PassthroughProvider {
    /// Display-name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MetaFb => "meta-fb",
            Self::HtcVive => "htc-vive",
            Self::AppleCompositorServices => "apple-compositor-services",
            Self::None => "none",
        }
    }

    /// `true` iff this provider supports environment-depth (depth-aware
    /// occlusion of virtual by real).
    #[must_use]
    pub const fn supports_environment_depth(self) -> bool {
        matches!(self, Self::MetaFb | Self::AppleCompositorServices)
    }

    /// `true` iff this provider supports hand-cutout (runtime
    /// segmentation of hands @ passthrough-layer).
    #[must_use]
    pub const fn supports_hand_cutout(self) -> bool {
        matches!(self, Self::MetaFb)
    }
}

/// Engine-side passthrough config. § IX.C.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PassthroughConfig {
    /// Whether passthrough is enabled.
    pub enabled: bool,
    /// Alpha-blend mode.
    pub alpha_mode: AlphaMode,
    /// `true` ⇒ engage `XR_META_environment_depth` (occlusion of
    /// virtual by real environment).
    pub environment_depth: bool,
    /// `true` ⇒ engage `XR_FB_passthrough_keyboard_hands`
    /// (Meta-only ; runtime cuts out hands).
    pub hand_cutout: bool,
    /// Provider in use.
    pub provider: PassthroughProvider,
}

impl PassthroughConfig {
    /// Disabled / no-op config.
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            enabled: false,
            alpha_mode: AlphaMode::Over,
            environment_depth: false,
            hand_cutout: false,
            provider: PassthroughProvider::None,
        }
    }

    /// Quest 3 default : enabled + ADDITIVE + env-depth + hand-cutout.
    #[must_use]
    pub const fn quest3_default() -> Self {
        Self {
            enabled: true,
            alpha_mode: AlphaMode::Additive,
            environment_depth: true,
            hand_cutout: true,
            provider: PassthroughProvider::MetaFb,
        }
    }

    /// Vision Pro default : compositor-managed background ; engine just
    /// submits content-layer + alpha.
    #[must_use]
    pub const fn vision_pro_default() -> Self {
        Self {
            enabled: true,
            alpha_mode: AlphaMode::Over,
            environment_depth: true,
            hand_cutout: false, // Apple handles internally
            provider: PassthroughProvider::AppleCompositorServices,
        }
    }

    /// HTC Vive XR Elite default.
    #[must_use]
    pub const fn htc_vive_default() -> Self {
        Self {
            enabled: true,
            alpha_mode: AlphaMode::Additive,
            environment_depth: false,
            hand_cutout: false,
            provider: PassthroughProvider::HtcVive,
        }
    }

    /// Validate that requested features match the provider's capabilities.
    pub fn validate(&self) -> Result<(), XRFailure> {
        if !self.enabled {
            return Ok(());
        }
        if self.environment_depth && !self.provider.supports_environment_depth() {
            return Err(XRFailure::PassthroughCreate { code: -20 });
        }
        if self.hand_cutout && !self.provider.supports_hand_cutout() {
            return Err(XRFailure::PassthroughCreate { code: -21 });
        }
        Ok(())
    }
}

/// Composition-layer for passthrough output. Submitted to `xrEndFrame`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PassthroughLayer {
    /// Config that produced this layer.
    pub config: PassthroughConfig,
    /// Layer flags (e.g. blend-mode bits ; runtime-specific).
    pub flags: u32,
    /// Z-order of this layer relative to other composition-layers.
    /// Smaller = behind, larger = front.
    pub z_order: i32,
}

impl PassthroughLayer {
    /// Construct from config. Layer flags + z-order default to zero
    /// (back-most = passthrough behind virtual content with Over).
    pub fn from_config(config: PassthroughConfig) -> Result<Self, XRFailure> {
        config.validate()?;
        Ok(Self {
            config,
            flags: 0,
            z_order: i32::MIN, // back-most
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{AlphaMode, PassthroughConfig, PassthroughLayer, PassthroughProvider};
    use crate::error::XRFailure;

    #[test]
    fn alpha_mode_as_str() {
        assert_eq!(AlphaMode::Additive.as_str(), "additive");
        assert_eq!(AlphaMode::Over.as_str(), "over");
        assert_eq!(AlphaMode::Under.as_str(), "under");
    }

    #[test]
    fn provider_capabilities_meta_fb() {
        let p = PassthroughProvider::MetaFb;
        assert!(p.supports_environment_depth());
        assert!(p.supports_hand_cutout());
    }

    #[test]
    fn provider_capabilities_compositor_services() {
        let p = PassthroughProvider::AppleCompositorServices;
        assert!(p.supports_environment_depth());
        assert!(!p.supports_hand_cutout()); // Apple handles internally
    }

    #[test]
    fn provider_capabilities_htc_vive() {
        let p = PassthroughProvider::HtcVive;
        assert!(!p.supports_environment_depth());
        assert!(!p.supports_hand_cutout());
    }

    #[test]
    fn provider_none_capabilities_zero() {
        let p = PassthroughProvider::None;
        assert!(!p.supports_environment_depth());
        assert!(!p.supports_hand_cutout());
    }

    #[test]
    fn quest3_default_full_features() {
        let cfg = PassthroughConfig::quest3_default();
        assert!(cfg.enabled);
        assert_eq!(cfg.alpha_mode, AlphaMode::Additive);
        assert!(cfg.environment_depth);
        assert!(cfg.hand_cutout);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn vision_pro_default_no_hand_cutout() {
        let cfg = PassthroughConfig::vision_pro_default();
        assert!(cfg.enabled);
        assert!(cfg.environment_depth);
        assert!(!cfg.hand_cutout);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn htc_vive_default_minimal() {
        let cfg = PassthroughConfig::htc_vive_default();
        assert!(cfg.enabled);
        assert!(!cfg.environment_depth);
        assert!(!cfg.hand_cutout);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn disabled_validates_trivially() {
        let cfg = PassthroughConfig::disabled();
        assert!(!cfg.enabled);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn validate_rejects_env_depth_without_provider_support() {
        let mut cfg = PassthroughConfig::htc_vive_default();
        cfg.environment_depth = true;
        let err = cfg.validate().unwrap_err();
        assert!(matches!(err, XRFailure::PassthroughCreate { .. }));
    }

    #[test]
    fn validate_rejects_hand_cutout_without_provider_support() {
        let mut cfg = PassthroughConfig::vision_pro_default();
        cfg.hand_cutout = true;
        let err = cfg.validate().unwrap_err();
        assert!(matches!(err, XRFailure::PassthroughCreate { .. }));
    }

    #[test]
    fn layer_from_config_succeeds() {
        let cfg = PassthroughConfig::quest3_default();
        let layer = PassthroughLayer::from_config(cfg).unwrap();
        assert_eq!(layer.z_order, i32::MIN);
        assert_eq!(layer.config, cfg);
    }

    #[test]
    fn layer_from_invalid_config_fails() {
        let mut cfg = PassthroughConfig::htc_vive_default();
        cfg.environment_depth = true;
        assert!(PassthroughLayer::from_config(cfg).is_err());
    }
}
