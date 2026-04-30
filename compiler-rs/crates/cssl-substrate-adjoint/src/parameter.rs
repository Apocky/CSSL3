//! § parameter — `Parameter` trait + `ParameterSet` registry.
//!
//! Abstracts over ALL gradient-able quantities :
//!   - KAN-cell-weights      (per-cell spline-basis coefficients)
//!   - material-coefs        (BRDF / impedance / spectral coefficients)
//!   - vertex-positions      (mesh / SDF control-points)
//!   - NPC-traits            (utility-fn-weights · personality-vectors)
//!   - audio-coefs           (wave-substrate synthesis coefficients)
//!
//! Each parameter registers a `ParameterId`, a `ParameterShape` (count of
//! scalar components), a `ParameterKind` discriminator, and an optional
//! `frozen` flag (Σ-mask gating from PRIME_DIRECTIVE).
//!
//! The `ParameterSet` is a registry that lets the optimizer iterate uniformly
//! over heterogeneous parameter-types, accumulate gradients, and apply
//! updates. Gradients are stored as `f32` slices keyed by `ParameterId`.

use smallvec::SmallVec;
use thiserror::Error;

/// Globally-unique identifier for a registered parameter.
///
/// Newtype around `u32` ; deterministic ordering ; assigned by `ParameterSet`
/// on registration.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ParameterId(pub u32);

impl ParameterId {
    /// Sentinel used for "not-yet-assigned".
    pub const SENTINEL: Self = Self(u32::MAX);

    /// Underlying numeric id.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// Discriminator describing what a parameter represents.
///
/// Used by the optimizer to apply per-kind learning-rate scales and
/// constraint-projection (e.g. positions stay within scene bounds, audio
/// coefs stay within Nyquist band).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ParameterKind {
    /// KAN-cell weights — spline-basis coefficients of a per-cell update-rule.
    /// Shape : N coefficients per spline-edge × E edges per cell.
    KanCellWeights,
    /// Material coefficient — BRDF / impedance / spectral-band weights.
    /// Shape : N spectral bands × M angular components.
    MaterialCoefs,
    /// Vertex / SDF control-point position.
    /// Shape : 3 × N (vec3 per vertex).
    VertexPositions,
    /// NPC-trait vector — utility-fn-weights or personality-axis values.
    /// Shape : N trait dimensions (typically 6-32).
    NpcTraits,
    /// Audio-substrate synthesis coefficient.
    /// Shape : N spectral bands of wave-substrate-coupled synth.
    AudioCoefs,
    /// Custom / experimental parameter (does not match any canonical kind).
    Custom,
}

impl ParameterKind {
    /// Default learning-rate scale per kind. Optimizers multiply the
    /// configured base-rate by this value.
    #[must_use]
    pub const fn default_lr_scale(self) -> f32 {
        match self {
            Self::KanCellWeights => 1.0,
            Self::MaterialCoefs => 0.5,    // tighter — physically constrained
            Self::VertexPositions => 0.1,  // very tight — geometry must stay coherent
            Self::NpcTraits => 0.25,       // moderate — behavioral stability
            Self::AudioCoefs => 0.5,       // tight — Nyquist / harmonic stability
            Self::Custom => 1.0,
        }
    }
}

/// Shape descriptor : flat scalar count + optional 2-axis hint for
/// reshape-on-display. Adjoint-kernel only cares about the flat count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParameterShape {
    /// Total number of scalar components (always ≥ 1).
    pub count: u32,
    /// Optional rows × cols hint for callers that want to reshape ; 0/0 means
    /// "treat as flat vector".
    pub rows: u32,
    /// See `rows`.
    pub cols: u32,
}

impl ParameterShape {
    /// Flat shape with `count` scalars.
    #[must_use]
    pub const fn flat(count: u32) -> Self {
        Self { count, rows: 0, cols: 0 }
    }

    /// 2-axis shape (rows × cols).
    #[must_use]
    pub const fn matrix(rows: u32, cols: u32) -> Self {
        Self {
            count: rows * cols,
            rows,
            cols,
        }
    }
}

/// Errors surfaced by parameter-set operations.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ParameterError {
    #[error("parameter-id {0} out of range (set has {1} entries)")]
    OutOfRange(u32, u32),
    #[error("dimension-mismatch on parameter {param_id}: expected {expected}, got {actual}")]
    DimensionMismatch {
        param_id: u32,
        expected: u32,
        actual: u32,
    },
    #[error("parameter {0} is frozen (Σ-mask gating) ; gradient update refused")]
    Frozen(u32),
    #[error("parameter-set is full ; cannot register more (cap = {0})")]
    SetFull(u32),
}

/// One entry in the parameter registry.
#[derive(Clone, Debug)]
pub struct Parameter {
    /// Parameter identity (assigned at registration ; stable for the life of the set).
    pub id: ParameterId,
    /// Kind discriminator (for per-kind optimizer behavior).
    pub kind: ParameterKind,
    /// Scalar layout.
    pub shape: ParameterShape,
    /// Current values (length = shape.count).
    pub values: Vec<f32>,
    /// Σ-mask freeze flag : when `true`, the optimizer refuses updates and
    /// surfaces a `ParameterError::Frozen` rather than silently writing.
    pub frozen: bool,
    /// Optional human-readable label for telemetry / debugging.
    pub label: SmallVec<[u8; 32]>,
}

impl Parameter {
    /// Construct a new parameter with values pre-filled by `init`.
    #[must_use]
    pub fn new(kind: ParameterKind, shape: ParameterShape, init: f32, label: &str) -> Self {
        let mut lb = SmallVec::new();
        lb.extend_from_slice(label.as_bytes());
        Self {
            id: ParameterId::SENTINEL,
            kind,
            shape,
            values: vec![init; shape.count as usize],
            frozen: false,
            label: lb,
        }
    }

    /// Convenience : KAN-cell weights with N coefficients, zero-initialized.
    #[must_use]
    pub fn kan_cell_weights(n: u32, label: &str) -> Self {
        Self::new(ParameterKind::KanCellWeights, ParameterShape::flat(n), 0.0, label)
    }

    /// Convenience : material-coef vector with N components.
    #[must_use]
    pub fn material_coefs(n: u32, label: &str) -> Self {
        Self::new(ParameterKind::MaterialCoefs, ParameterShape::flat(n), 0.5, label)
    }

    /// Convenience : 3 × N vertex-position matrix.
    #[must_use]
    pub fn vertex_positions(num_vertices: u32, label: &str) -> Self {
        Self::new(
            ParameterKind::VertexPositions,
            ParameterShape::matrix(num_vertices, 3),
            0.0,
            label,
        )
    }

    /// Convenience : NPC trait-vector of N dimensions.
    #[must_use]
    pub fn npc_traits(n: u32, label: &str) -> Self {
        Self::new(ParameterKind::NpcTraits, ParameterShape::flat(n), 0.0, label)
    }

    /// Convenience : audio-coef vector of N spectral bands.
    #[must_use]
    pub fn audio_coefs(n: u32, label: &str) -> Self {
        Self::new(ParameterKind::AudioCoefs, ParameterShape::flat(n), 0.0, label)
    }

    /// Number of scalar components.
    #[must_use]
    pub fn len(&self) -> u32 {
        self.shape.count
    }

    /// `true` when there are no scalar components.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.shape.count == 0
    }

    /// Mark this parameter frozen (Σ-mask refusal). Idempotent.
    pub fn freeze(&mut self) {
        self.frozen = true;
    }

    /// Mark this parameter mutable again. Use only when consent has been
    /// re-established at the substrate level.
    pub fn unfreeze(&mut self) {
        self.frozen = false;
    }
}

/// Hard cap on registered parameters. Set high enough for whole-scene fits.
pub const PARAMETER_SET_CAP: u32 = 65_536;

/// Registry of all parameters being optimized in a single adjoint job.
///
/// Owns the values + maintains a parallel gradient-buffer keyed by `ParameterId`.
#[derive(Clone, Debug, Default)]
pub struct ParameterSet {
    params: Vec<Parameter>,
    gradients: Vec<Vec<f32>>,
    /// Monotonic step-counter ; bumped by `clear_gradients` so telemetry can
    /// correlate optimizer-steps with FieldCell.epoch advances.
    step: u64,
}

impl ParameterSet {
    /// Construct an empty set.
    #[must_use]
    pub fn new() -> Self {
        Self {
            params: Vec::new(),
            gradients: Vec::new(),
            step: 0,
        }
    }

    /// Register a parameter ; returns its assigned `ParameterId`.
    pub fn register(&mut self, mut p: Parameter) -> Result<ParameterId, ParameterError> {
        if self.params.len() as u32 >= PARAMETER_SET_CAP {
            return Err(ParameterError::SetFull(PARAMETER_SET_CAP));
        }
        let id = ParameterId(self.params.len() as u32);
        p.id = id;
        let grads = vec![0.0_f32; p.shape.count as usize];
        self.params.push(p);
        self.gradients.push(grads);
        Ok(id)
    }

    /// Total number of registered parameters.
    #[must_use]
    pub fn len(&self) -> usize {
        self.params.len()
    }

    /// `true` when no parameters are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.params.is_empty()
    }

    /// Borrow a parameter by id.
    pub fn get(&self, id: ParameterId) -> Result<&Parameter, ParameterError> {
        self.params
            .get(id.0 as usize)
            .ok_or(ParameterError::OutOfRange(id.0, self.params.len() as u32))
    }

    /// Mutably borrow a parameter by id.
    pub fn get_mut(&mut self, id: ParameterId) -> Result<&mut Parameter, ParameterError> {
        let n = self.params.len() as u32;
        self.params
            .get_mut(id.0 as usize)
            .ok_or(ParameterError::OutOfRange(id.0, n))
    }

    /// Read-only accessor for the parameter slice.
    #[must_use]
    pub fn params(&self) -> &[Parameter] {
        &self.params
    }

    /// Borrow the gradient buffer for `id`.
    pub fn gradient(&self, id: ParameterId) -> Result<&[f32], ParameterError> {
        self.gradients
            .get(id.0 as usize)
            .map(Vec::as_slice)
            .ok_or(ParameterError::OutOfRange(id.0, self.params.len() as u32))
    }

    /// Mutably borrow the gradient buffer for `id`.
    pub fn gradient_mut(&mut self, id: ParameterId) -> Result<&mut [f32], ParameterError> {
        let n = self.params.len() as u32;
        self.gradients
            .get_mut(id.0 as usize)
            .map(Vec::as_mut_slice)
            .ok_or(ParameterError::OutOfRange(id.0, n))
    }

    /// Accumulate a gradient slice into the buffer for `id`. Lengths must match.
    pub fn accumulate(&mut self, id: ParameterId, grad: &[f32]) -> Result<(), ParameterError> {
        let buf = self.gradient_mut(id)?;
        if buf.len() != grad.len() {
            return Err(ParameterError::DimensionMismatch {
                param_id: id.0,
                expected: buf.len() as u32,
                actual: grad.len() as u32,
            });
        }
        for (b, g) in buf.iter_mut().zip(grad.iter()) {
            *b += *g;
        }
        Ok(())
    }

    /// Reset all gradient buffers to zero. Bumps the step counter.
    pub fn clear_gradients(&mut self) {
        for g in &mut self.gradients {
            for x in g.iter_mut() {
                *x = 0.0;
            }
        }
        self.step += 1;
    }

    /// Current optimizer step counter.
    #[must_use]
    pub fn step(&self) -> u64 {
        self.step
    }

    /// L2 norm of the full concatenated gradient vector. Useful for clip-norm
    /// + convergence telemetry.
    #[must_use]
    pub fn gradient_norm(&self) -> f32 {
        let mut acc = 0.0_f64;
        for g in &self.gradients {
            for x in g {
                acc += (*x as f64) * (*x as f64);
            }
        }
        acc.sqrt() as f32
    }

    /// Apply an in-place values update. Refuses frozen parameters.
    pub fn apply_update(
        &mut self,
        id: ParameterId,
        update: &[f32],
    ) -> Result<(), ParameterError> {
        // Check frozen flag without holding the mutable borrow over the
        // dimension-mismatch error path.
        let is_frozen = self.get(id)?.frozen;
        if is_frozen {
            return Err(ParameterError::Frozen(id.0));
        }
        let p = self.get_mut(id)?;
        if p.values.len() != update.len() {
            return Err(ParameterError::DimensionMismatch {
                param_id: id.0,
                expected: p.values.len() as u32,
                actual: update.len() as u32,
            });
        }
        for (v, u) in p.values.iter_mut().zip(update.iter()) {
            *v += *u;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameter_id_sentinel_is_max() {
        assert_eq!(ParameterId::SENTINEL.raw(), u32::MAX);
    }

    #[test]
    fn parameter_kind_lr_scales_are_in_range() {
        for k in [
            ParameterKind::KanCellWeights,
            ParameterKind::MaterialCoefs,
            ParameterKind::VertexPositions,
            ParameterKind::NpcTraits,
            ParameterKind::AudioCoefs,
            ParameterKind::Custom,
        ] {
            let s = k.default_lr_scale();
            assert!(s > 0.0 && s <= 1.0, "lr-scale out of range for {k:?}");
        }
    }

    #[test]
    fn parameter_constructors_have_correct_shapes() {
        let kw = Parameter::kan_cell_weights(8, "k");
        assert_eq!(kw.len(), 8);
        let mc = Parameter::material_coefs(4, "m");
        assert_eq!(mc.len(), 4);
        let vp = Parameter::vertex_positions(10, "v");
        assert_eq!(vp.len(), 30);
        let nt = Parameter::npc_traits(6, "n");
        assert_eq!(nt.len(), 6);
        let ac = Parameter::audio_coefs(16, "a");
        assert_eq!(ac.len(), 16);
    }

    #[test]
    fn parameter_set_register_assigns_sequential_ids() {
        let mut s = ParameterSet::new();
        let id0 = s.register(Parameter::material_coefs(4, "a")).unwrap();
        let id1 = s.register(Parameter::material_coefs(4, "b")).unwrap();
        assert_eq!(id0.raw(), 0);
        assert_eq!(id1.raw(), 1);
    }

    #[test]
    fn parameter_set_accumulate_dimension_check() {
        let mut s = ParameterSet::new();
        let id = s.register(Parameter::material_coefs(4, "a")).unwrap();
        let bad = vec![0.0_f32; 3];
        let err = s.accumulate(id, &bad).unwrap_err();
        match err {
            ParameterError::DimensionMismatch { expected, actual, .. } => {
                assert_eq!(expected, 4);
                assert_eq!(actual, 3);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn parameter_set_apply_update_refuses_frozen() {
        let mut s = ParameterSet::new();
        let id = s.register(Parameter::material_coefs(2, "a")).unwrap();
        s.get_mut(id).unwrap().freeze();
        let upd = vec![0.1_f32; 2];
        assert!(matches!(s.apply_update(id, &upd), Err(ParameterError::Frozen(_))));
    }

    #[test]
    fn parameter_set_gradient_norm_accumulates() {
        let mut s = ParameterSet::new();
        let id = s.register(Parameter::material_coefs(3, "a")).unwrap();
        s.accumulate(id, &[3.0, 4.0, 0.0]).unwrap();
        let n = s.gradient_norm();
        assert!((n - 5.0).abs() < 1e-5, "expected 5.0, got {n}");
    }

    #[test]
    fn clear_gradients_bumps_step() {
        let mut s = ParameterSet::new();
        let id = s.register(Parameter::material_coefs(3, "a")).unwrap();
        s.accumulate(id, &[1.0, 2.0, 3.0]).unwrap();
        let before = s.step();
        s.clear_gradients();
        assert_eq!(s.step(), before + 1);
        for x in s.gradient(id).unwrap() {
            assert_eq!(*x, 0.0);
        }
    }
}
