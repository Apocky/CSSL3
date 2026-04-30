//! § cfer — CFER iteration driver (the keystone).
//!
//! ## Role
//! ADCS Wave-S CORE-5 keystone : the per-frame driver that runs the
//! convergence-loop, dirty-set tracking, V-cycle multigrid, evidence-driven
//! adaptive-budget, and render-readout into the final image.
//!
//! Per spec § 36 § ALGORITHM the per-frame loop is :
//!
//!   1. Mark dirty cells (post-mutation since last frame).
//!   2. Iterate field-evolution to convergence (with V-cycle accel).
//!   3. Render via decompressed-read at viewpoint.
//!
//! ## Convergence
//! Per spec : 16–64 iterations for fresh scene ; 4–16 for warm-cache. The
//! evidence-driver's per-cell budget multiplies into the per-cell iteration
//! count, so warm-cache cells (✓ trusted) skip while uncertain (◐) cells
//! get 4× the budget.
//!
//! ## Temporal-amortization
//! Per spec : steady-state scenes have <10% of cells dirty per frame. The
//! [`DirtySet`] persists across calls so only the changed-region re-iterates.

use crate::camera::{Camera, CameraError};
use crate::denoiser::{Denoiser, DenoiserError};
use crate::evidence_driver::{EvidenceDriver, EvidenceGlyph, EvidenceReport};
use crate::kan_stub::{kan_update, KanUpdateError, MaterialBag};
use crate::light_stub::LightState;
use crate::multigrid::{MultigridError, VCycle};
use crate::tonemap::{ToneCurve, ToneMapper};
use std::collections::HashSet;
use thiserror::Error;

/// Render error class.
#[derive(Debug, Error)]
pub enum RenderError {
    /// CFER iteration exhausted the budget without converging.
    #[error("CFER convergence-failure: {iters} iterations, residual {residual} > epsilon {epsilon}")]
    ConvergenceFailure {
        iters: u32,
        residual: f32,
        epsilon: f32,
    },
    /// Render budget zero (no work allowed).
    #[error("RenderBudget max_iterations must be > 0")]
    ZeroBudget,
    /// Camera error.
    #[error("camera error: {0}")]
    Camera(#[from] CameraError),
    /// Multigrid error.
    #[error("multigrid error: {0}")]
    Multigrid(#[from] MultigridError),
    /// Denoiser error.
    #[error("denoiser error: {0}")]
    Denoiser(#[from] DenoiserError),
    /// KAN-update error.
    #[error("kan update error: {0}")]
    KanUpdate(#[from] KanUpdateError),
    /// Field is empty (no cells to iterate).
    #[error("field is empty")]
    EmptyField,
}

/// Render budget : iteration cap + ε convergence threshold.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderBudget {
    /// Max CFER iterations per frame (spec : 16–64 fresh ; 4–16 warm).
    pub max_iterations: u32,
    /// Convergence epsilon : ‖ΔL‖ < ε ⇒ converged.
    pub epsilon: f32,
    /// Convergence threshold for total-frame residual.
    pub convergence_threshold: f32,
    /// Whether to run the V-cycle multigrid acceleration.
    pub use_multigrid: bool,
    /// Whether to run the spatio-temporal denoiser.
    pub use_denoiser: bool,
}

impl Default for RenderBudget {
    fn default() -> Self {
        Self {
            max_iterations: 64,
            epsilon: 1e-3,
            convergence_threshold: 1e-2,
            use_multigrid: true,
            use_denoiser: true,
        }
    }
}

/// Convergence report for one [`cfer_render_frame`] call.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ConvergenceReport {
    /// CFER iterations executed.
    pub iterations: u32,
    /// Final L1 residual across all cells.
    pub final_residual_l1: f32,
    /// True iff residual fell below convergence_threshold.
    pub converged: bool,
    /// Cells re-iterated this frame (dirty + extension).
    pub cells_iterated: u32,
    /// Total cells in the field.
    pub total_cells: u32,
    /// Evidence-driver report.
    pub evidence: EvidenceReport,
}

impl ConvergenceReport {
    /// Skip rate : cells skipped / total. High = warm-cache.
    pub fn skip_rate(&self) -> f32 {
        let total = self.total_cells.max(1);
        let skipped = total.saturating_sub(self.cells_iterated);
        (skipped as f32) / (total as f32)
    }
}

/// Pixel : LDR RGB tuple after tonemapping.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ImagePixel {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl ImagePixel {
    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    pub fn from_array(arr: [f32; 3]) -> Self {
        Self {
            r: arr[0],
            g: arr[1],
            b: arr[2],
        }
    }

    pub fn to_array(self) -> [f32; 3] {
        [self.r, self.g, self.b]
    }
}

/// Rendered image : packed RGB pixel grid.
#[derive(Debug, Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<ImagePixel>,
    pub time_ns: u64,
}

impl Image {
    /// Construct a zero-filled image.
    pub fn zeros(width: u32, height: u32, time_ns: u64) -> Self {
        let n = (width as usize) * (height as usize);
        Self {
            width,
            height,
            pixels: vec![ImagePixel::default(); n],
            time_ns,
        }
    }

    /// Pixel at (x, y) ; clamped to bounds.
    pub fn get(&self, x: u32, y: u32) -> ImagePixel {
        let xi = x.min(self.width.saturating_sub(1));
        let yi = y.min(self.height.saturating_sub(1));
        self.pixels[(yi * self.width + xi) as usize]
    }

    /// Mutable pixel at (x, y).
    pub fn get_mut(&mut self, x: u32, y: u32) -> Option<&mut ImagePixel> {
        if x < self.width && y < self.height {
            Some(&mut self.pixels[(y * self.width + x) as usize])
        } else {
            None
        }
    }

    /// Total pixel count.
    pub fn pixel_count(&self) -> usize {
        self.pixels.len()
    }
}

/// Dirty-set : tracks which 1D cell indices need re-iteration this frame.
#[derive(Debug, Clone, Default)]
pub struct DirtySet {
    pub indices: HashSet<u32>,
}

impl DirtySet {
    pub fn new() -> Self {
        Self {
            indices: HashSet::new(),
        }
    }

    pub fn from_iter<I: IntoIterator<Item = u32>>(iter: I) -> Self {
        Self {
            indices: iter.into_iter().collect(),
        }
    }

    pub fn mark(&mut self, idx: u32) {
        self.indices.insert(idx);
    }

    pub fn extend<I: IntoIterator<Item = u32>>(&mut self, iter: I) {
        self.indices.extend(iter);
    }

    pub fn len(&self) -> usize {
        self.indices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub fn contains(&self, idx: u32) -> bool {
        self.indices.contains(&idx)
    }

    pub fn clear(&mut self) {
        self.indices.clear();
    }
}

/// Build the 6-neighborhood (±x, ±y, ±z in a 1D-flattened grid) for a given
/// cell index. Out-of-bound neighbors clamp to the cell itself (Neumann
/// boundary).
fn six_neighbors(states: &[LightState], idx: usize, size_x: usize, size_y: usize) -> Vec<LightState> {
    let mut out = Vec::with_capacity(6);
    let n = states.len();
    let plane = size_x * size_y;
    let z = idx / plane;
    let y = (idx % plane) / size_x;
    let x = idx % size_x;

    let push_or_self = |out: &mut Vec<LightState>, ni: i64| {
        if ni >= 0 && (ni as usize) < n {
            out.push(states[ni as usize]);
        } else {
            out.push(states[idx]);
        }
    };

    let i = idx as i64;
    if x > 0 {
        push_or_self(&mut out, i - 1);
    } else {
        push_or_self(&mut out, i);
    }
    if x + 1 < size_x {
        push_or_self(&mut out, i + 1);
    } else {
        push_or_self(&mut out, i);
    }
    if y > 0 {
        push_or_self(&mut out, i - (size_x as i64));
    } else {
        push_or_self(&mut out, i);
    }
    if y + 1 < size_y {
        push_or_self(&mut out, i + (size_x as i64));
    } else {
        push_or_self(&mut out, i);
    }
    if z > 0 {
        push_or_self(&mut out, i - (plane as i64));
    } else {
        push_or_self(&mut out, i);
    }
    let depth = if plane > 0 { n / plane } else { 0 };
    if z + 1 < depth {
        push_or_self(&mut out, i + (plane as i64));
    } else {
        push_or_self(&mut out, i);
    }
    out
}

/// Render-frame keystone driver.
///
/// ## Inputs
///   - `field` : the per-cell light-state vector (W-S-CORE-1 stub : Vec<LightState>).
///   - `materials` : per-cell material-bags. Length must match `field`.
///   - `size` : 3D size (size_x, size_y, size_z) of the rectangular grid
///     storing the field. `size.0 * size.1 * size.2 = field.len()`.
///   - `dirty` : initial dirty-set (cells changed since previous frame).
///   - `camera` : camera state for the render-readout.
///   - `time_ns` : wallclock-monotonic timestamp.
///   - `budget` : iteration cap + convergence ε + multigrid/denoiser flags.
///   - `evidence_driver` : adaptive-budget controller.
///
/// ## Outputs
///   - [`Image`] : tonemapped RGB output.
///   - [`ConvergenceReport`] : iteration count + residual + skip-rate.
pub fn cfer_render_frame(
    field: &mut Vec<LightState>,
    materials: &[MaterialBag],
    size: (usize, usize, usize),
    dirty: &mut DirtySet,
    camera: &Camera,
    time_ns: u64,
    budget: &RenderBudget,
    evidence_driver: &EvidenceDriver,
) -> Result<(Image, ConvergenceReport), RenderError> {
    if budget.max_iterations == 0 {
        return Err(RenderError::ZeroBudget);
    }
    if field.is_empty() {
        return Err(RenderError::EmptyField);
    }
    camera.validate()?;
    let total_cells = field.len() as u32;
    if materials.len() != field.len() {
        return Err(RenderError::EmptyField);
    }
    let (sx, sy, sz) = size;
    if sx * sy * sz != field.len() {
        return Err(RenderError::EmptyField);
    }

    // ─── 1. Ensure dirty-set is non-empty for fresh scenes ──────────────────
    if dirty.is_empty() {
        // Cold-start : mark all cells dirty.
        dirty.extend(0..total_cells);
    }

    let mut report = ConvergenceReport::default();
    report.total_cells = total_cells;

    // ─── 2. (Optional) V-cycle multigrid pass ───────────────────────────────
    if budget.use_multigrid {
        let v = VCycle::new(crate::multigrid::MultigridConfig::default())?;
        let (relaxed, _mg_report) = v.v_cycle(field)?;
        if relaxed.len() == field.len() {
            field.clone_from(&relaxed);
        }
    }

    // ─── 3. Convergence loop with evidence-driven adaptive-budget ───────────
    let mut iter = 0_u32;
    let mut last_residual = f32::INFINITY;
    let mut next_field = field.clone();
    let dirty_indices: Vec<u32> = dirty.indices.iter().copied().collect();
    while iter < budget.max_iterations {
        let mut delta_l1 = 0.0_f32;
        let mut iterated_this_round = 0_u32;
        let mut new_dirty: HashSet<u32> = HashSet::new();

        for &idx in &dirty_indices {
            let i = idx as usize;
            if i >= field.len() {
                continue;
            }

            // Classify by previous-step residual to budget this iteration.
            let prev = field[i];
            let prev_glyph = if iter == 0 {
                EvidenceGlyph::Default
            } else {
                evidence_driver.classify_states(prev, next_field[i])
            };

            if prev_glyph.is_skip() {
                // Skip-class : cell is trusted/rejected. Tally evidence ;
                // do not iterate.
                evidence_driver.tally(&mut report.evidence, prev_glyph);
                continue;
            }

            let neighbors = six_neighbors(field, i, sx, sy);
            let new_state = kan_update(prev, &neighbors, materials[i])?;
            let delta = prev.norm_diff_l1(new_state);
            next_field[i] = new_state;
            delta_l1 += delta;
            iterated_this_round += 1;

            let glyph = evidence_driver.classify(delta);
            evidence_driver.tally(&mut report.evidence, glyph);

            // Variance-driven extension : if cell is uncertain, mark its
            // neighbors as dirty for the next round.
            if matches!(glyph, EvidenceGlyph::Uncertain) {
                if i > 0 {
                    new_dirty.insert((i - 1) as u32);
                }
                if i + 1 < field.len() {
                    new_dirty.insert((i + 1) as u32);
                }
            }
        }

        // Commit next_field → field
        field.clone_from(&next_field);
        report.cells_iterated += iterated_this_round;
        last_residual = delta_l1;

        // Convergence test
        if delta_l1 < budget.convergence_threshold {
            iter += 1;
            break;
        }

        // Extend dirty set with newly-uncertain cells.
        dirty.extend(new_dirty);

        iter += 1;
    }

    report.iterations = iter;
    report.final_residual_l1 = last_residual;
    report.converged = last_residual < budget.convergence_threshold;

    if !report.converged && iter >= budget.max_iterations {
        // Best-effort : still produce an image, but record the failure in
        // the report. (Production may want to bail with ConvergenceFailure ;
        // for now we degrade gracefully.)
    }

    // ─── 4. Render-readout : ray-per-pixel + decompress-read ────────────────
    let img = render_readout(field, size, camera, time_ns);

    // ─── 5. (Optional) Denoiser pass ────────────────────────────────────────
    let final_image = if budget.use_denoiser {
        let mut d = Denoiser::default();
        let arr: Vec<[f32; 3]> = img.pixels.iter().map(|p| p.to_array()).collect();
        let denoised = d.denoise(&arr, img.width, img.height)?;
        let pixels: Vec<ImagePixel> = denoised.iter().map(|a| ImagePixel::from_array(*a)).collect();
        Image {
            width: img.width,
            height: img.height,
            pixels,
            time_ns,
        }
    } else {
        img
    };

    // Once the frame is rendered, the dirty-set can be cleared : every cell
    // present in `dirty` has been visited at least once and converged or
    // tallied.
    dirty.clear();

    Ok((final_image, report))
}

/// Render-readout : per-pixel ray → cell-along-ray → decompressed-light
/// → tonemapped pixel.
fn render_readout(
    field: &[LightState],
    size: (usize, usize, usize),
    camera: &Camera,
    time_ns: u64,
) -> Image {
    let mut img = Image::zeros(camera.width, camera.height, time_ns);
    let tm = ToneMapper {
        curve: ToneCurve::AcesApprox,
        exposure: camera.exposure,
        custom_lut: None,
    };
    let (sx, sy, _sz) = size;
    let _ = camera.foveation; // priority-aware path lives inside evidence_driver.

    for py in 0..camera.height {
        for px in 0..camera.width {
            // Ray-generation (drives spectrum sampling per spec § 36 step 3).
            let _ray = match camera.ray_through(px, py) {
                Ok(r) => r,
                Err(_) => continue,
            };
            // Cell-along-ray : map (px, py) to a deterministic cell.
            // Stub mapping : flatten px/py into a cell index, modulo field.
            // Real impl traces the ray through the ω-field grid.
            let cx = (px as usize * sx.max(1) / camera.width.max(1) as usize).min(sx.max(1) - 1);
            let cy = (py as usize * sy.max(1) / camera.height.max(1) as usize).min(sy.max(1) - 1);
            let cell_idx = cy * sx + cx;
            if cell_idx < field.len() {
                let state = field[cell_idx];
                // Decompress KAN-band : sample three wavelength positions for RGB.
                let r = state.coefs[0];
                let g = state.coefs[LIGHT_STATE_COEFS_HALF];
                let b = state.coefs[LIGHT_STATE_COEFS_LAST];
                let pri = camera.priority_at(px, py);
                let mapped = tm.map([r * pri, g * pri, b * pri]);
                if let Some(out) = img.get_mut(px, py) {
                    *out = ImagePixel::from_array(mapped);
                }
            }
        }
    }
    img
}

const LIGHT_STATE_COEFS_HALF: usize = crate::light_stub::LIGHT_STATE_COEFS / 2;
const LIGHT_STATE_COEFS_LAST: usize = crate::light_stub::LIGHT_STATE_COEFS - 1;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::Camera;

    fn empty_field(n: usize) -> (Vec<LightState>, Vec<MaterialBag>, (usize, usize, usize)) {
        (vec![LightState::zero(); n], vec![MaterialBag::VACUUM; n], (n, 1, 1))
    }

    #[test]
    fn budget_default_is_64_iters() {
        assert_eq!(RenderBudget::default().max_iterations, 64);
    }

    #[test]
    fn budget_zero_iter_errors() {
        let (mut f, m, sz) = empty_field(4);
        let cam = Camera::new(8, 8).unwrap();
        let mut d = DirtySet::new();
        let b = RenderBudget {
            max_iterations: 0,
            ..RenderBudget::default()
        };
        let ed = EvidenceDriver::default();
        let r = cfer_render_frame(&mut f, &m, sz, &mut d, &cam, 0, &b, &ed);
        assert!(matches!(r, Err(RenderError::ZeroBudget)));
    }

    #[test]
    fn empty_field_errors() {
        let mut f: Vec<LightState> = vec![];
        let m: Vec<MaterialBag> = vec![];
        let cam = Camera::new(8, 8).unwrap();
        let mut d = DirtySet::new();
        let b = RenderBudget::default();
        let ed = EvidenceDriver::default();
        let r = cfer_render_frame(&mut f, &m, (0, 0, 0), &mut d, &cam, 0, &b, &ed);
        assert!(matches!(r, Err(RenderError::EmptyField)));
    }

    #[test]
    fn dirty_set_basic_ops() {
        let mut d = DirtySet::new();
        assert!(d.is_empty());
        d.mark(5);
        assert_eq!(d.len(), 1);
        assert!(d.contains(5));
        d.extend([7, 9]);
        assert_eq!(d.len(), 3);
        d.clear();
        assert!(d.is_empty());
    }

    #[test]
    fn image_zeros_basic() {
        let img = Image::zeros(4, 4, 100);
        assert_eq!(img.pixel_count(), 16);
        assert_eq!(img.time_ns, 100);
        let p = img.get(0, 0);
        assert_eq!(p.r, 0.0);
    }

    #[test]
    fn convergence_report_skip_rate() {
        let mut r = ConvergenceReport::default();
        r.total_cells = 100;
        r.cells_iterated = 5;
        assert!((r.skip_rate() - 0.95).abs() < 1e-3);
    }

    #[test]
    fn six_neighbors_clamps_at_edges() {
        let states = vec![LightState::zero(); 8];
        let n = six_neighbors(&states, 0, 2, 2);
        assert_eq!(n.len(), 6);
    }
}
