// § T11-W5-SPECTRAL-GRADER (cssl/session-15/W-W5-spectral)
// § I> crate-root · re-exports + crate-level docs
// § I> stdlib-only · stage-0 host crate

#![forbid(unsafe_code)]

//! # cssl-host-spectral-grader
//!
//! `RGB` → 16-band spectral SPD upsampler for the LoA-v13 game-engine asset
//! pipeline. Takes sRGB/BT.709 colors (assumed linearized — caller controls
//! input gamma) and produces 16-banded reflectance SPDs that re-project to
//! the same RGB triple under the D65 illuminant + CIE 1931 2-deg observer.
//!
//! ## Pairing
//!
//! Output `Spd` instances are the input format consumed by the existing
//! `cssl-spectral-render` crate (16-band Mueller matrices + 4 illuminants).
//! Wave-5b will wire this crate's outputs into the renderer's reflectance
//! channels at asset-load time.
//!
//! ## Methods
//!
//! - [`grader::GraderMethod::SmitsLike`] : 7-basis-SPD positive-weighted
//!   reconstruction (white-bias + CMY + RGB Gaussian-bell curves). Round-trip
//!   error ≤ 0.05 for white/red/green/blue/gray.
//! - [`grader::GraderMethod::JakobSimplified`] : 3-Gaussian sum keyed on
//!   R/G/B amplitude. Cheaper but looser round-trip (≤ 0.10 typical).
//! - [`grader::GraderMethod::FlatGray`] : returns a flat luminance-matched SPD.
//!   Reference / fallback.
//!
//! ## Determinism
//!
//! All routines are pure functions of their inputs. No PRNG, no global state,
//! no allocation in `rgb_to_spd_*`. The bulk image API allocates a single
//! `Vec<Spd>` of known capacity.

pub mod cmf;
pub mod grader;
pub mod spd;
pub mod upsample;

pub use cmf::{spd_to_xyz, srgb_to_xyz_d65, xyz_to_srgb_d65, CMF_X, CMF_Y, CMF_Z};
pub use grader::{GraderMethod, SpectralGrader};
pub use spd::{Spd, BAND_WAVELENGTHS_NM, N_BANDS};
pub use upsample::{rgb_to_spd_jakob_simplified, rgb_to_spd_smits_like, roundtrip_error};
