//! § KanUpdateRule — canonical CFER iteration update rules.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Per the CFER iteration formulation
//!   (specs/36_CFER_RENDERER.csl § ALGORITHM) :
//!
//!       L_c^{(k+1)} = KAN_c( L_c^{(k)}, {L_n^{(k)} : n ∈ neighbors(c)},
//!                            material_c )
//!
//!   The full per-cell KAN_c factors as a *composition* of canonical
//!   physical-update primitives that match the radiance transport PDE :
//!
//!       ∂L_c/∂t = -∇·(L_c · v) + S_c + Σ_n K(c, n) · L_n
//!                  └ DiffusionRule ┘ └ EmissionRule ┘ └ InterCellTransportRule ┘
//!     plus      :  ScatteringRule (BSDF redistribution)
//!     and      :  AbsorptionRule (Beer-Lambert attenuation)
//!
//!   Each primitive implements the [`KanUpdateRule`] trait : given the
//!   current cell light-state, neighbor light-states, and material context,
//!   it produces a delta to apply. Composition is sum-of-rules with optional
//!   per-rule weights ; the composed update is the canonical KAN_c.
//!
//! § PRIME-DIRECTIVE
//!   - Update rules are *pure functions* of (cell-state, neighbors,
//!     material) — no hidden global state, deterministic, side-effect-free.
//!     Required for the adjoint backward-pass to be definable.
//!   - Per-rule weights respect Σ-mask consent : a Frozen cell refuses
//!     mutation entry, gated upstream by the cfer_iter driver.
//!   - Coefficient updates are bounded by [`COEF_BOUND`] post-update to
//!     keep the wave-solver stable.

use crate::kan_band::{KanBand, KanBandError, COEF_BOUND};

/// § Material context per cell : the parameters that shape how a cell
///   responds to incident light. Drives all five canonical update rules.
///
///   Values are normalized to [0.0, 1.0] except where explicitly noted.
///   The default context represents a perfect-absorber black-body (no
///   diffusion, no emission, full absorption, no scattering).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaterialContext {
    /// § Diffusion velocity (advection magnitude). 0.0 = static cell ;
    ///   1.0 = full diffusion to neighbors per timestep. Drives DiffusionRule.
    pub diffusion: f32,
    /// § Emission strength : self-emission added per timestep. Drives
    ///   EmissionRule.
    pub emission: f32,
    /// § Absorption : Beer-Lambert coefficient. 0.0 = no absorption ;
    ///   1.0 = full absorption per timestep. Drives AbsorptionRule.
    pub absorption: f32,
    /// § Scattering anisotropy g ∈ [-1.0, 1.0] : -1 = back-scatter, 0 =
    ///   isotropic, 1 = forward-scatter. Drives ScatteringRule.
    pub scattering: f32,
    /// § Inter-cell transport kernel coefficient. Drives the canonical
    ///   K(c, n) sum in InterCellTransportRule.
    pub transport_kernel: f32,
    /// § Sovereign-handle of the material's authoring author. Used by the
    ///   cfer_iter driver for consent gating ; pure-update primitives don't
    ///   inspect this directly.
    pub sovereign_handle: u16,
}

impl MaterialContext {
    /// § Default : perfect-absorber black body.
    #[must_use]
    pub const fn black_body() -> MaterialContext {
        MaterialContext {
            diffusion: 0.0,
            emission: 0.0,
            absorption: 1.0,
            scattering: 0.0,
            transport_kernel: 0.0,
            sovereign_handle: 0,
        }
    }

    /// § Lambertian-diffuser preset : isotropic scattering with mid
    ///   absorption.
    #[must_use]
    pub const fn lambertian() -> MaterialContext {
        MaterialContext {
            diffusion: 0.5,
            emission: 0.0,
            absorption: 0.3,
            scattering: 0.0,
            transport_kernel: 0.6,
            sovereign_handle: 0,
        }
    }

    /// § Emissive-light-source preset.
    #[must_use]
    pub const fn emissive(strength: f32) -> MaterialContext {
        MaterialContext {
            diffusion: 0.0,
            emission: strength,
            absorption: 0.0,
            scattering: 0.0,
            transport_kernel: 0.4,
            sovereign_handle: 0,
        }
    }

    /// § Specular-reflector preset (forward-scattering, low absorption).
    #[must_use]
    pub const fn specular() -> MaterialContext {
        MaterialContext {
            diffusion: 0.1,
            emission: 0.0,
            absorption: 0.05,
            scattering: 0.9,
            transport_kernel: 0.8,
            sovereign_handle: 0,
        }
    }

    /// § Bounds-check : returns true iff all coefficients lie in their
    ///   canonical ranges.
    #[must_use]
    pub fn in_bounds(&self) -> bool {
        (0.0..=1.0).contains(&self.diffusion)
            && self.emission >= 0.0
            && (0.0..=1.0).contains(&self.absorption)
            && (-1.0..=1.0).contains(&self.scattering)
            && self.transport_kernel >= 0.0
            && self.transport_kernel <= 4.0
    }
}

impl Default for MaterialContext {
    fn default() -> Self {
        Self::black_body()
    }
}

/// § A neighbor entry : the neighbor's KanBand reference paired with its
///   geometric weight (typically inverse distance + cosine-weight). Used
///   uniformly across all update rules that consult neighbors.
#[derive(Debug, Clone, Copy)]
pub struct Neighbor<'a> {
    /// § Reference to the neighbor's KanBand (no copy ; iteration is
    ///   embarrassingly parallel ; this slice is read-only during a phase).
    pub band: &'a KanBand,
    /// § Geometric weight : kernel value K(c, n). Caller pre-computes from
    ///   geometry + view direction.
    pub weight: f32,
}

/// § Update-rule errors. Threaded through the canonical thiserror chain.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum UpdateRuleError {
    /// § A material-context coefficient is out of canonical range.
    #[error("update_rule: material context out of bounds : {0}")]
    MaterialOutOfBounds(&'static str),
    /// § Composition error from KanBand.
    #[error("update_rule: kan_band error : {0}")]
    KanBand(#[from] KanBandError),
    /// § Cell + neighbor band rank mismatch.
    #[error("update_rule: rank mismatch ; cell {cell} != neighbor {neighbor}")]
    RankMismatch { cell: usize, neighbor: usize },
    /// § Cell + neighbor basis-kind mismatch.
    #[error("update_rule: basis mismatch ; cell {cell} != neighbor {neighbor}")]
    BasisMismatch { cell: u8, neighbor: u8 },
}

/// § Canonical CFER per-cell update rule. Each implementation captures a
///   single physical-update primitive ; full KAN_c is the weighted sum of
///   these.
///
/// § CONTRACT
///   - PURE : same input → same output ; no hidden state.
///   - INCREMENTAL : `apply` returns a *delta* coefficient vector (caller
///     accumulates into the cell's KanBand).
///   - BOUNDED : delta entries are clamped within [`COEF_BOUND`] before
///     return, but the full update may grow beyond this — the cfer_iter
///     driver re-clamps post-composition.
pub trait KanUpdateRule: Send + Sync {
    /// § Canonical name for telemetry + audit.
    fn canonical_name(&self) -> &'static str;

    /// § Compute the per-coefficient delta this rule contributes for the
    ///   given (cell-state, neighbors, material). Output buffer length =
    ///   cell.rank().
    ///
    /// # Errors
    /// - [`UpdateRuleError::MaterialOutOfBounds`] when material context is
    ///   out-of-canonical-range.
    /// - [`UpdateRuleError::RankMismatch`] when neighbor ranks differ.
    /// - [`UpdateRuleError::BasisMismatch`] when neighbor basis differs.
    fn apply(
        &self,
        cell: &KanBand,
        neighbors: &[Neighbor<'_>],
        material: &MaterialContext,
        out: &mut [f32],
    ) -> Result<(), UpdateRuleError>;
}

// ── DiffusionRule ──────────────────────────────────────────────────

/// § Diffusion update : light flows out of the cell to neighbors at rate
///   proportional to material.diffusion. Implements the -∇·(L_c · v_propagation)
///   advection term of the radiance transport PDE.
#[derive(Debug, Clone, Copy, Default)]
pub struct DiffusionRule;

impl KanUpdateRule for DiffusionRule {
    fn canonical_name(&self) -> &'static str {
        "diffusion"
    }

    fn apply(
        &self,
        cell: &KanBand,
        _neighbors: &[Neighbor<'_>],
        material: &MaterialContext,
        out: &mut [f32],
    ) -> Result<(), UpdateRuleError> {
        if !material.in_bounds() {
            return Err(UpdateRuleError::MaterialOutOfBounds("diffusion"));
        }
        if out.len() < cell.rank() {
            return Err(UpdateRuleError::RankMismatch {
                cell: cell.rank(),
                neighbor: out.len(),
            });
        }
        // Diffusion : delta = -material.diffusion * cell.coefs (out-flow).
        let d = material.diffusion;
        for (i, &c) in cell.coefs.iter().enumerate() {
            out[i] = (-d * c).clamp(-COEF_BOUND, COEF_BOUND);
        }
        Ok(())
    }
}

// ── EmissionRule ───────────────────────────────────────────────────

/// § Emission update : material self-emission adds light per timestep.
///   Implements the +S_c source term of the radiance transport PDE.
///
///   Emission is added to the lowest-frequency basis coefficients in
///   order, which corresponds to broadband + low-spatial-frequency
///   self-emission for the canonical Gaussian-mix / Hat / Cosine bases.
#[derive(Debug, Clone, Copy, Default)]
pub struct EmissionRule;

impl KanUpdateRule for EmissionRule {
    fn canonical_name(&self) -> &'static str {
        "emission"
    }

    fn apply(
        &self,
        cell: &KanBand,
        _neighbors: &[Neighbor<'_>],
        material: &MaterialContext,
        out: &mut [f32],
    ) -> Result<(), UpdateRuleError> {
        if !material.in_bounds() {
            return Err(UpdateRuleError::MaterialOutOfBounds("emission"));
        }
        if out.len() < cell.rank() {
            return Err(UpdateRuleError::RankMismatch {
                cell: cell.rank(),
                neighbor: out.len(),
            });
        }
        // Spread emission across the first 4 (or rank) lowest-frequency
        // coefs at decaying intensities. Total integrated injection =
        // material.emission.
        let strength = material.emission;
        let n = cell.rank().min(4);
        for i in 0..cell.rank() {
            out[i] = 0.0;
        }
        if n == 0 {
            return Ok(());
        }
        // Decay weights : w_i = 1/2^i normalized.
        let mut total = 0.0_f32;
        let mut w = [0.0_f32; 4];
        for i in 0..n {
            w[i] = 1.0 / (1u32 << i) as f32;
            total += w[i];
        }
        for i in 0..n {
            let v = strength * (w[i] / total);
            out[i] = v.clamp(-COEF_BOUND, COEF_BOUND);
        }
        Ok(())
    }
}

// ── AbsorptionRule ─────────────────────────────────────────────────

/// § Absorption update : light is attenuated per Beer-Lambert. delta_i
///   = -material.absorption * cell.coefs[i]. Wavelength-uniform absorption
///   in the canonical formulation ; per-wavelength variation is captured
///   by the basis-fn weights at higher rank.
#[derive(Debug, Clone, Copy, Default)]
pub struct AbsorptionRule;

impl KanUpdateRule for AbsorptionRule {
    fn canonical_name(&self) -> &'static str {
        "absorption"
    }

    fn apply(
        &self,
        cell: &KanBand,
        _neighbors: &[Neighbor<'_>],
        material: &MaterialContext,
        out: &mut [f32],
    ) -> Result<(), UpdateRuleError> {
        if !material.in_bounds() {
            return Err(UpdateRuleError::MaterialOutOfBounds("absorption"));
        }
        if out.len() < cell.rank() {
            return Err(UpdateRuleError::RankMismatch {
                cell: cell.rank(),
                neighbor: out.len(),
            });
        }
        let a = material.absorption;
        for (i, &c) in cell.coefs.iter().enumerate() {
            out[i] = (-a * c).clamp(-COEF_BOUND, COEF_BOUND);
        }
        Ok(())
    }
}

// ── ScatteringRule ─────────────────────────────────────────────────

/// § Scattering update : redistributes coefficients via a phase-function
///   weighting (anisotropy `g` from material.scattering). g = 0 means no
///   scattering (delta = 0 ; pass-through). g > 0 = forward-bias (delta
///   amplifies coef relative to mean). g < 0 = back-scatter (delta
///   inverts coef relative to mean). Magnitude |g| is the scattering
///   strength.
///
///   Per the canonical Henyey-Greenstein phase fn projected onto the basis :
///       delta_i = g · (cell.coefs[i] - mean(cell.coefs))
///   At g = 0 this is exactly zero (no scatter) ; at g = 1 it's the
///   forward-bias relative-to-mean ; at g = -1 it's the inverse.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScatteringRule;

impl KanUpdateRule for ScatteringRule {
    fn canonical_name(&self) -> &'static str {
        "scattering"
    }

    fn apply(
        &self,
        cell: &KanBand,
        _neighbors: &[Neighbor<'_>],
        material: &MaterialContext,
        out: &mut [f32],
    ) -> Result<(), UpdateRuleError> {
        if !material.in_bounds() {
            return Err(UpdateRuleError::MaterialOutOfBounds("scattering"));
        }
        if out.len() < cell.rank() {
            return Err(UpdateRuleError::RankMismatch {
                cell: cell.rank(),
                neighbor: out.len(),
            });
        }
        let g = material.scattering;
        let n = cell.rank();
        if n == 0 {
            return Ok(());
        }
        let mut mean = 0.0_f32;
        for &c in cell.coefs.iter() {
            mean += c;
        }
        mean /= n as f32;
        for (i, &c) in cell.coefs.iter().enumerate() {
            let v = g * (c - mean);
            out[i] = v.clamp(-COEF_BOUND, COEF_BOUND);
        }
        Ok(())
    }
}

// ── InterCellTransportRule ─────────────────────────────────────────

/// § Inter-cell transport : Σ_n K(c, n) · L_n. Each neighbor contributes
///   its weighted band into the cell. The neighbor.weight encodes the
///   geometric K(c, n) kernel ; material.transport_kernel scales the
///   total in-flow.
#[derive(Debug, Clone, Copy, Default)]
pub struct InterCellTransportRule;

impl KanUpdateRule for InterCellTransportRule {
    fn canonical_name(&self) -> &'static str {
        "inter_cell_transport"
    }

    fn apply(
        &self,
        cell: &KanBand,
        neighbors: &[Neighbor<'_>],
        material: &MaterialContext,
        out: &mut [f32],
    ) -> Result<(), UpdateRuleError> {
        if !material.in_bounds() {
            return Err(UpdateRuleError::MaterialOutOfBounds("inter_cell_transport"));
        }
        let n_rank = cell.rank();
        if out.len() < n_rank {
            return Err(UpdateRuleError::RankMismatch {
                cell: n_rank,
                neighbor: out.len(),
            });
        }
        for v in out.iter_mut().take(n_rank) {
            *v = 0.0;
        }
        for nb in neighbors {
            // Rank + basis must match for canonical inter-cell transport.
            if nb.band.rank() != n_rank {
                return Err(UpdateRuleError::RankMismatch {
                    cell: n_rank,
                    neighbor: nb.band.rank(),
                });
            }
            if nb.band.basis_kind != cell.basis_kind {
                return Err(UpdateRuleError::BasisMismatch {
                    cell: cell.basis_kind.to_u8(),
                    neighbor: nb.band.basis_kind.to_u8(),
                });
            }
            let scale = material.transport_kernel * nb.weight;
            for i in 0..n_rank {
                out[i] += scale * nb.band.coefs[i];
            }
        }
        for v in out.iter_mut().take(n_rank) {
            *v = v.clamp(-COEF_BOUND, COEF_BOUND);
        }
        Ok(())
    }
}

// ── Composition helper ──────────────────────────────────────────────

/// § Compose multiple rules into a single weighted-sum update : sum the
///   deltas from each rule into one output buffer. Caller-provided weight
///   per rule lets the cfer_iter driver tune the relative contribution.
///
/// # Errors
///   Propagates any rule's apply error.
pub fn compose_rules(
    rules: &[(&dyn KanUpdateRule, f32)],
    cell: &KanBand,
    neighbors: &[Neighbor<'_>],
    material: &MaterialContext,
    out: &mut [f32],
) -> Result<(), UpdateRuleError> {
    let n = cell.rank().min(out.len());
    for v in out.iter_mut().take(n) {
        *v = 0.0;
    }
    let mut scratch = vec![0.0_f32; n];
    for (rule, weight) in rules {
        for v in scratch.iter_mut() {
            *v = 0.0;
        }
        rule.apply(cell, neighbors, material, &mut scratch)?;
        for i in 0..n {
            out[i] += weight * scratch[i];
        }
    }
    for v in out.iter_mut().take(n) {
        *v = v.clamp(-COEF_BOUND, COEF_BOUND);
    }
    Ok(())
}

/// § Convenience : the canonical 5-rule composition (Diffusion +
///   Scattering + Emission + Absorption + InterCellTransport) with unit
///   weights. Drives the typical CFER iteration step.
pub struct CanonicalRuleSet {
    pub diffusion: DiffusionRule,
    pub scattering: ScatteringRule,
    pub emission: EmissionRule,
    pub absorption: AbsorptionRule,
    pub transport: InterCellTransportRule,
}

impl CanonicalRuleSet {
    /// § Construct the canonical set.
    #[must_use]
    pub const fn new() -> CanonicalRuleSet {
        CanonicalRuleSet {
            diffusion: DiffusionRule,
            scattering: ScatteringRule,
            emission: EmissionRule,
            absorption: AbsorptionRule,
            transport: InterCellTransportRule,
        }
    }

    /// § Apply all 5 rules at unit weights. Output = sum of all deltas.
    ///
    /// # Errors
    ///   Propagates any rule's apply error.
    pub fn apply_all(
        &self,
        cell: &KanBand,
        neighbors: &[Neighbor<'_>],
        material: &MaterialContext,
        out: &mut [f32],
    ) -> Result<(), UpdateRuleError> {
        let rules: [(&dyn KanUpdateRule, f32); 5] = [
            (&self.diffusion, 1.0),
            (&self.scattering, 1.0),
            (&self.emission, 1.0),
            (&self.absorption, 1.0),
            (&self.transport, 1.0),
        ];
        compose_rules(&rules, cell, neighbors, material, out)
    }
}

impl Default for CanonicalRuleSet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kan_band::BasisKind;

    fn band(rank: usize, val: f32) -> KanBand {
        let coefs: Vec<f32> = (0..rank).map(|_| val).collect();
        KanBand::from_slice(&coefs, BasisKind::GaussianMix).unwrap()
    }

    // ── MaterialContext ─────────────────────────────────────────────

    #[test]
    fn material_black_body_in_bounds() {
        assert!(MaterialContext::black_body().in_bounds());
    }

    #[test]
    fn material_lambertian_in_bounds() {
        assert!(MaterialContext::lambertian().in_bounds());
    }

    #[test]
    fn material_default_is_black_body() {
        let m = MaterialContext::default();
        assert_eq!(m, MaterialContext::black_body());
    }

    #[test]
    fn material_out_of_bounds_caught() {
        let mut m = MaterialContext::lambertian();
        m.diffusion = 5.0;
        assert!(!m.in_bounds());
    }

    // ── DiffusionRule ───────────────────────────────────────────────

    #[test]
    fn diffusion_rule_pulls_energy_out() {
        let rule = DiffusionRule;
        let cell = band(3, 1.0);
        let mut m = MaterialContext::default();
        m.diffusion = 0.5;
        let mut out = vec![0.0_f32; 3];
        rule.apply(&cell, &[], &m, &mut out).unwrap();
        // delta = -0.5 * 1.0 = -0.5 per coef.
        for &v in out.iter() {
            assert!((v + 0.5).abs() < 1e-6);
        }
    }

    #[test]
    fn diffusion_rule_zero_when_diffusion_zero() {
        let rule = DiffusionRule;
        let cell = band(4, 1.0);
        let m = MaterialContext::default();
        let mut out = vec![0.0_f32; 4];
        rule.apply(&cell, &[], &m, &mut out).unwrap();
        for &v in out.iter() {
            assert_eq!(v, 0.0);
        }
    }

    #[test]
    fn diffusion_rule_oob_material_errors() {
        let rule = DiffusionRule;
        let cell = band(2, 0.0);
        let mut m = MaterialContext::lambertian();
        m.diffusion = 5.0;
        let mut out = vec![0.0_f32; 2];
        let r = rule.apply(&cell, &[], &m, &mut out);
        assert!(matches!(r, Err(UpdateRuleError::MaterialOutOfBounds(_))));
    }

    // ── EmissionRule ────────────────────────────────────────────────

    #[test]
    fn emission_rule_zero_strength_yields_zero() {
        let rule = EmissionRule;
        let cell = band(4, 0.0);
        let m = MaterialContext::default();
        let mut out = vec![0.0_f32; 4];
        rule.apply(&cell, &[], &m, &mut out).unwrap();
        for &v in out.iter() {
            assert_eq!(v, 0.0);
        }
    }

    #[test]
    fn emission_rule_total_injection_equals_strength() {
        let rule = EmissionRule;
        let cell = band(4, 0.0);
        let m = MaterialContext::emissive(2.0);
        let mut out = vec![0.0_f32; 4];
        rule.apply(&cell, &[], &m, &mut out).unwrap();
        let sum: f32 = out.iter().sum();
        // Spread across 4 weights : 1 + 1/2 + 1/4 + 1/8 = 15/8 normalized.
        // Sum of decayed contributions equals the strength.
        assert!((sum - 2.0).abs() < 1e-3);
    }

    #[test]
    fn emission_rule_first_coef_largest() {
        let rule = EmissionRule;
        let cell = band(4, 0.0);
        let m = MaterialContext::emissive(1.0);
        let mut out = vec![0.0_f32; 4];
        rule.apply(&cell, &[], &m, &mut out).unwrap();
        assert!(out[0] > out[1]);
        assert!(out[1] > out[2]);
        assert!(out[2] > out[3]);
    }

    // ── AbsorptionRule ──────────────────────────────────────────────

    #[test]
    fn absorption_rule_full_absorption_negates() {
        let rule = AbsorptionRule;
        let cell = band(3, 1.0);
        let m = MaterialContext::black_body(); // absorption = 1.0
        let mut out = vec![0.0_f32; 3];
        rule.apply(&cell, &[], &m, &mut out).unwrap();
        for &v in out.iter() {
            assert!((v + 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn absorption_rule_zero_no_change() {
        let rule = AbsorptionRule;
        let cell = band(3, 1.0);
        let m = MaterialContext::specular(); // absorption ~0.05, so non-zero
        let mut m_zero = m;
        m_zero.absorption = 0.0;
        let mut out = vec![0.0_f32; 3];
        rule.apply(&cell, &[], &m_zero, &mut out).unwrap();
        for &v in out.iter() {
            assert_eq!(v, 0.0);
        }
    }

    // ── ScatteringRule ──────────────────────────────────────────────

    #[test]
    fn scattering_rule_zero_g_no_scatter() {
        // g = 0 ⇒ no scatter (delta = 0) for every coef.
        let rule = ScatteringRule;
        let cell = KanBand::from_slice(&[1.0, 2.0, 3.0], BasisKind::GaussianMix).unwrap();
        let mut m = MaterialContext::default();
        m.scattering = 0.0;
        m.diffusion = 0.0;
        let mut out = vec![0.0_f32; 3];
        rule.apply(&cell, &[], &m, &mut out).unwrap();
        for &v in out.iter() {
            assert!(v.abs() < 1e-6);
        }
    }

    #[test]
    fn scattering_rule_forward_amplifies_relative_to_mean() {
        // g = 1 forward ⇒ delta = cell - mean. mean = 2 for [1,2,3].
        let rule = ScatteringRule;
        let cell = KanBand::from_slice(&[1.0, 2.0, 3.0], BasisKind::GaussianMix).unwrap();
        let mut m = MaterialContext::default();
        m.scattering = 1.0;
        let mut out = vec![0.0_f32; 3];
        rule.apply(&cell, &[], &m, &mut out).unwrap();
        let mean = 2.0;
        for (i, &v) in out.iter().enumerate() {
            let expected = cell.coefs[i] - mean;
            assert!((v - expected).abs() < 1e-5);
        }
    }

    // ── InterCellTransportRule ──────────────────────────────────────

    #[test]
    fn inter_cell_transport_no_neighbors_yields_zero() {
        let rule = InterCellTransportRule;
        let cell = band(3, 1.0);
        let m = MaterialContext::lambertian();
        let mut out = vec![0.0_f32; 3];
        rule.apply(&cell, &[], &m, &mut out).unwrap();
        for &v in out.iter() {
            assert_eq!(v, 0.0);
        }
    }

    #[test]
    fn inter_cell_transport_adds_weighted_neighbors() {
        let rule = InterCellTransportRule;
        let cell = band(3, 0.0);
        let nb1 = band(3, 1.0);
        let nb2 = band(3, 2.0);
        let neighbors = [
            Neighbor { band: &nb1, weight: 1.0 },
            Neighbor { band: &nb2, weight: 0.5 },
        ];
        let mut m = MaterialContext::lambertian();
        m.transport_kernel = 1.0;
        let mut out = vec![0.0_f32; 3];
        rule.apply(&cell, &neighbors, &m, &mut out).unwrap();
        // 1 * 1.0 + 2 * 0.5 = 2.0 per coef.
        for &v in out.iter() {
            assert!((v - 2.0).abs() < 1e-5);
        }
    }

    #[test]
    fn inter_cell_transport_rank_mismatch_errors() {
        let rule = InterCellTransportRule;
        let cell = band(3, 0.0);
        let nb_wrong = band(2, 1.0);
        let neighbors = [Neighbor { band: &nb_wrong, weight: 1.0 }];
        let m = MaterialContext::lambertian();
        let mut out = vec![0.0_f32; 3];
        let r = rule.apply(&cell, &neighbors, &m, &mut out);
        assert!(matches!(r, Err(UpdateRuleError::RankMismatch { .. })));
    }

    #[test]
    fn inter_cell_transport_basis_mismatch_errors() {
        let rule = InterCellTransportRule;
        let cell = KanBand::from_slice(&[1.0, 1.0, 1.0], BasisKind::GaussianMix).unwrap();
        let nb_wrong = KanBand::from_slice(&[1.0, 1.0, 1.0], BasisKind::Hat).unwrap();
        let neighbors = [Neighbor { band: &nb_wrong, weight: 1.0 }];
        let m = MaterialContext::lambertian();
        let mut out = vec![0.0_f32; 3];
        let r = rule.apply(&cell, &neighbors, &m, &mut out);
        assert!(matches!(r, Err(UpdateRuleError::BasisMismatch { .. })));
    }

    // ── Composition + canonical-set ─────────────────────────────────

    #[test]
    fn compose_rules_sums_individual_deltas() {
        let cell = band(3, 1.0);
        let m = MaterialContext::lambertian();
        let neighbors: [Neighbor<'_>; 0] = [];
        // Compose Diffusion + Absorption only.
        let d = DiffusionRule;
        let a = AbsorptionRule;
        let mut out = vec![0.0_f32; 3];
        let rules: [(&dyn KanUpdateRule, f32); 2] =
            [(&d, 1.0), (&a, 1.0)];
        compose_rules(&rules, &cell, &neighbors, &m, &mut out).unwrap();
        // Sum : -0.5 (diffusion) + -0.3 (absorption) = -0.8 per coef.
        for &v in out.iter() {
            assert!((v + 0.8).abs() < 1e-5);
        }
    }

    #[test]
    fn canonical_rule_set_runs_without_error() {
        let cell = band(4, 0.5);
        let m = MaterialContext::lambertian();
        let nb = band(4, 0.3);
        let neighbors = [Neighbor { band: &nb, weight: 1.0 }];
        let set = CanonicalRuleSet::new();
        let mut out = vec![0.0_f32; 4];
        set.apply_all(&cell, &neighbors, &m, &mut out).unwrap();
        // Bounded delta : every coef in [-COEF_BOUND, COEF_BOUND].
        for &v in out.iter() {
            assert!(v.abs() <= COEF_BOUND);
        }
    }

    #[test]
    fn canonical_rule_set_emissive_grows_first_coef() {
        let cell = band(4, 0.0);
        let m = MaterialContext::emissive(1.0);
        let neighbors: [Neighbor<'_>; 0] = [];
        let set = CanonicalRuleSet::new();
        let mut out = vec![0.0_f32; 4];
        set.apply_all(&cell, &neighbors, &m, &mut out).unwrap();
        // For a cell at zero with emission but no transport, delta[0] > 0.
        assert!(out[0] > 0.0);
    }

    // ── Dispatch via trait object ───────────────────────────────────

    #[test]
    fn rule_canonical_names_unique() {
        let names = [
            DiffusionRule.canonical_name(),
            ScatteringRule.canonical_name(),
            EmissionRule.canonical_name(),
            AbsorptionRule.canonical_name(),
            InterCellTransportRule.canonical_name(),
        ];
        let mut s = names.to_vec();
        s.sort_unstable();
        let pre = s.len();
        s.dedup();
        assert_eq!(s.len(), pre);
    }

    #[test]
    fn kan_band_table_unused_path_alive() {
        let _ = crate::kan_band::KanBandTable::canonical();
    }
}
