//! § Legacy bridge — `LegacyTensor<T,R>` alias + `OmegaTensor::to_field` adapter.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Preserves the surface of [`cssl_substrate_omega_tensor::OmegaTensor`]
//!   verbatim so the S8-H1..H6 test-suite is unbroken, AND provides a
//!   migration adapter `to_field()` that lifts a scalar OmegaTensor into a
//!   facet of a new OmegaField.
//!
//! § SPEC
//!   - `Omniverse/02_CSSL/06_SUBSTRATE_EVOLUTION.csl` § IV.10 LegacyTensor.
//!   - The deprecation window per § I.evolution-trigger : T11-D113..D129
//!     full-coexistence, removal scheduled at T11-D130.
//!
//! § DESIGN
//!   `LegacyTensor<T, R>` is a strict type-alias for the S8-H1
//!   `OmegaTensor<T, R>` from `cssl-substrate-omega-tensor`. Existing
//!   surface (`new`, `get`, `set`, `iter`, `slice_along`, `reshape`, `add`,
//!   etc.) is preserved verbatim ; downstream callers do not need to
//!   change a single import.
//!
//! § ADAPTER
//!   The new `LegacyTensor::to_field` migration adapter (extension trait
//!   below) lifts a rank-3 scalar tensor into a fresh
//!   [`crate::omega_field::OmegaField`] by writing each tensor entry to the
//!   FieldCell facet implied by the chosen [`ScalarFacet`].
//!
//! § STABILITY
//!   - The alias name is `LegacyTensor` ; the old name `OmegaTensor` is also
//!     re-exported as `pub type OmegaTensor<T, R> = LegacyTensor<T, R>`. This
//!     gives downstream callers TWO ways to refer to the same type, both
//!     equally supported during the deprecation window.
//!   - The migration adapter is on a fresh trait `LegacyTensorMigration`
//!     to keep the adapter explicit (no method shadowing on the original
//!     surface).

pub use cssl_substrate_omega_tensor::OmegaScalar;
pub use cssl_substrate_omega_tensor::OmegaTensor;

/// Strict alias for the legacy [`cssl_substrate_omega_tensor::OmegaTensor`]
/// during the T11-D113..D129 deprecation window. Removal slated for
/// T11-D130 ; downstream callers M? migrate to [`crate::OmegaField`] via
/// [`LegacyTensorMigration::to_field`] before that.
pub type LegacyTensor<T, const R: usize> = OmegaTensor<T, R>;

/// Which facet of [`crate::FieldCell`] the scalar value lands into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarFacet {
    /// Write into [`FieldCell::density`] (S-facet ρ).
    Density,
    /// Write into [`FieldCell::enthalpy`] (S-facet H).
    Enthalpy,
    /// Write into the Ψ-overlay (Wigner-negativity scalar).
    PsiNegativity,
    /// Write into [`FieldCell::velocity`] (uniform-x channel).
    VelocityX,
    /// Write into [`FieldCell::velocity`] (uniform-y channel).
    VelocityY,
    /// Write into [`FieldCell::velocity`] (uniform-z channel).
    VelocityZ,
}

impl ScalarFacet {
    /// Stable canonical name (for telemetry + audit).
    #[must_use]
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::Density => "density",
            Self::Enthalpy => "enthalpy",
            Self::PsiNegativity => "psi_negativity",
            Self::VelocityX => "velocity_x",
            Self::VelocityY => "velocity_y",
            Self::VelocityZ => "velocity_z",
        }
    }
}

/// Migration adapter trait. Provides the `to_field()` method on legacy
/// rank-3 OmegaTensors that lifts the scalar tensor into a new
/// [`crate::OmegaField`] by writing each tensor entry to the chosen facet
/// of the FieldCell at the corresponding Morton-key.
pub trait LegacyTensorMigration<T: OmegaScalar> {
    /// Lift this rank-3 scalar tensor into a fresh [`crate::OmegaField`].
    /// The (i, j, k) tensor index becomes the (x, y, z) Morton-axis indices ;
    /// the scalar value is written to the chosen [`ScalarFacet`].
    ///
    /// The bootstrap path is used for cell-stamping (Σ-check bypassed since
    /// this is scene-load time).
    ///
    /// # Errors
    /// Returns [`MigrationError`] on Morton-encoding failure (axis out of
    /// 21-bit range) or grid-saturation.
    fn to_field(self, facet: ScalarFacet) -> Result<crate::OmegaField, MigrationError>;
}

impl LegacyTensorMigration<f32> for LegacyTensor<f32, 3> {
    fn to_field(self, facet: ScalarFacet) -> Result<crate::OmegaField, MigrationError> {
        migrate_rank3_to_field(self, facet)
    }
}

impl LegacyTensorMigration<f64> for LegacyTensor<f64, 3> {
    fn to_field(self, facet: ScalarFacet) -> Result<crate::OmegaField, MigrationError> {
        // Lift f64 → f32 lossy here ; the FieldCell facets are f32.
        let shape = self.shape();
        let mut field = crate::OmegaField::new();
        for i in 0..shape[0] {
            for j in 0..shape[1] {
                for k in 0..shape[2] {
                    let v = self.get([i, j, k]).unwrap_or(0.0);
                    let key = crate::MortonKey::encode(i, j, k).map_err(MigrationError::Morton)?;
                    apply_facet(&mut field, key, facet, v as f32)?;
                }
            }
        }
        Ok(field)
    }
}

fn migrate_rank3_to_field(
    tensor: LegacyTensor<f32, 3>,
    facet: ScalarFacet,
) -> Result<crate::OmegaField, MigrationError> {
    let shape = tensor.shape();
    let mut field = crate::OmegaField::new();
    for i in 0..shape[0] {
        for j in 0..shape[1] {
            for k in 0..shape[2] {
                let v = tensor.get([i, j, k]).unwrap_or(0.0);
                let key = crate::MortonKey::encode(i, j, k).map_err(MigrationError::Morton)?;
                apply_facet(&mut field, key, facet, v)?;
            }
        }
    }
    Ok(field)
}

fn apply_facet(
    field: &mut crate::OmegaField,
    key: crate::MortonKey,
    facet: ScalarFacet,
    value: f32,
) -> Result<(), MigrationError> {
    match facet {
        ScalarFacet::Density => {
            let mut c = field.cell(key);
            c.density = value;
            field
                .stamp_cell_bootstrap(key, c)
                .map_err(MigrationError::Mutation)
        }
        ScalarFacet::Enthalpy => {
            let mut c = field.cell(key);
            c.enthalpy = value.max(0.0); // refinement clamp.
            field
                .stamp_cell_bootstrap(key, c)
                .map_err(MigrationError::Mutation)
        }
        ScalarFacet::PsiNegativity => {
            field.set_psi(key, value);
            Ok(())
        }
        ScalarFacet::VelocityX => {
            let mut c = field.cell(key);
            c.velocity[0] = value;
            field
                .stamp_cell_bootstrap(key, c)
                .map_err(MigrationError::Mutation)
        }
        ScalarFacet::VelocityY => {
            let mut c = field.cell(key);
            c.velocity[1] = value;
            field
                .stamp_cell_bootstrap(key, c)
                .map_err(MigrationError::Mutation)
        }
        ScalarFacet::VelocityZ => {
            let mut c = field.cell(key);
            c.velocity[2] = value;
            field
                .stamp_cell_bootstrap(key, c)
                .map_err(MigrationError::Mutation)
        }
    }
}

/// Failure modes for the `LegacyTensor::to_field` migration adapter.
#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("OF0030 — Morton-encoding failed during migration : {0}")]
    Morton(crate::morton::MortonError),
    #[error("OF0031 — OmegaField mutation failed during migration : {0}")]
    Mutation(crate::omega_field::MutationError),
}

#[cfg(test)]
mod tests {
    use super::{LegacyTensor, LegacyTensorMigration, ScalarFacet};
    use crate::MortonKey;

    // ── Type-alias surface preservation ────────────────────────

    #[test]
    fn legacy_tensor_alias_constructs() {
        let t = LegacyTensor::<f32, 3>::new([2, 2, 2]);
        assert_eq!(t.shape(), [2, 2, 2]);
        assert_eq!(t.numel(), 8);
    }

    #[test]
    fn legacy_tensor_get_set_roundtrip() {
        let mut t = LegacyTensor::<f32, 3>::new([2, 2, 2]);
        assert!(t.set([0, 0, 0], 1.5));
        assert!((t.get([0, 0, 0]).unwrap() - 1.5).abs() < 1e-6);
    }

    // ── Migration adapter ────────────────────────────────────

    #[test]
    fn migrate_rank3_density_populates_field() {
        let mut t = LegacyTensor::<f32, 3>::new([2, 2, 2]);
        for i in 0..2_u64 {
            for j in 0..2_u64 {
                for k in 0..2_u64 {
                    t.set([i, j, k], (i + j + k) as f32 + 1.0);
                }
            }
        }
        let field = t.to_field(ScalarFacet::Density).unwrap();
        // Every (i, j, k) cell should have density (i+j+k+1).
        for i in 0..2_u64 {
            for j in 0..2_u64 {
                for k in 0..2_u64 {
                    let key = MortonKey::encode(i, j, k).unwrap();
                    let cell = field.cell(key);
                    let expected = (i + j + k) as f32 + 1.0;
                    assert!(
                        (cell.density - expected).abs() < 1e-5,
                        "cell ({i},{j},{k}) density = {}",
                        cell.density
                    );
                }
            }
        }
    }

    #[test]
    fn migrate_rank3_enthalpy_clamps_negative_to_zero() {
        let mut t = LegacyTensor::<f32, 3>::new([1, 1, 1]);
        t.set([0, 0, 0], -2.5);
        let field = t.to_field(ScalarFacet::Enthalpy).unwrap();
        let key = MortonKey::encode(0, 0, 0).unwrap();
        // Enthalpy is f32'pos → clamped to 0.
        assert!(field.cell(key).enthalpy >= 0.0);
    }

    #[test]
    fn migrate_rank3_psi_writes_overlay() {
        let mut t = LegacyTensor::<f32, 3>::new([2, 1, 1]);
        t.set([0, 0, 0], 0.5);
        t.set([1, 0, 0], -0.7);
        let field = t.to_field(ScalarFacet::PsiNegativity).unwrap();
        let k0 = MortonKey::encode(0, 0, 0).unwrap();
        let k1 = MortonKey::encode(1, 0, 0).unwrap();
        assert!((field.psi().at(k0) - 0.5).abs() < 1e-6);
        assert!((field.psi().at(k1) - (-0.7)).abs() < 1e-6);
    }

    /// Test-only free-fn helper since `to_field` consumes the tensor.
    /// (Inherent-impl for a foreign type is forbidden by orphan rules.)
    fn clone_then_to_field(src: &LegacyTensor<f32, 3>, facet: ScalarFacet) -> crate::OmegaField {
        let mut t = LegacyTensor::<f32, 3>::new(src.shape());
        for i in 0..src.shape()[0] {
            for j in 0..src.shape()[1] {
                for k in 0..src.shape()[2] {
                    t.set([i, j, k], src.get([i, j, k]).unwrap_or(0.0));
                }
            }
        }
        t.to_field(facet).unwrap()
    }

    #[test]
    fn migrate_rank3_velocity_x_y_z_components() {
        let mut t = LegacyTensor::<f32, 3>::new([1, 1, 1]);
        t.set([0, 0, 0], 0.7);
        let field_x = clone_then_to_field(&t, ScalarFacet::VelocityX);
        let field_y = clone_then_to_field(&t, ScalarFacet::VelocityY);
        let field_z = clone_then_to_field(&t, ScalarFacet::VelocityZ);
        let k = MortonKey::encode(0, 0, 0).unwrap();
        assert!((field_x.cell(k).velocity[0] - 0.7).abs() < 1e-6);
        assert!((field_y.cell(k).velocity[1] - 0.7).abs() < 1e-6);
        assert!((field_z.cell(k).velocity[2] - 0.7).abs() < 1e-6);
    }

    // ── Scalar-facet canonical names ─────────────────────────

    #[test]
    fn scalar_facet_canonical_names_unique() {
        let facets = [
            ScalarFacet::Density,
            ScalarFacet::Enthalpy,
            ScalarFacet::PsiNegativity,
            ScalarFacet::VelocityX,
            ScalarFacet::VelocityY,
            ScalarFacet::VelocityZ,
        ];
        let mut names: Vec<&'static str> = facets.iter().map(|f| f.canonical_name()).collect();
        names.sort_unstable();
        let original = names.len();
        names.dedup();
        assert_eq!(names.len(), original);
    }
}
