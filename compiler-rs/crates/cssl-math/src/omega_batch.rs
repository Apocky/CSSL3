//! § omega_batch — feature-gated Ω-tensor interop
//!
//! Convertors between `&[Vec3]` slices and `OmegaTensor<f32, 2>` of
//! shape `[N, 3]`. Used by renderer batch-skinning, physics broadphase,
//! and particle systems that want to push their position arrays through
//! the substrate-level tensor surface.
//!
//! § COPY DISCIPLINE
//!   At this slice the conversion is a copy in both directions. The
//!   tensor surface does not yet expose stride-permutation views ; once
//!   that lands we can offer a zero-copy `Vec3Slice -> View<f32, 2>`
//!   adaptor. The copy cost is `O(N)` in fp32 elements ; for typical
//!   batch sizes (10k-100k vertices) this is fast relative to the
//!   downstream GPU upload latency.
//!
//! § FEATURE GATE
//!   Only built when `--features omega-batch` is enabled. The default
//!   feature set is empty so pure-math consumers never pull in
//!   `cssl-substrate-omega-tensor` and its `cssl-rt`-allocator path.

use cssl_substrate_omega_tensor::OmegaTensor;

use crate::vec3::Vec3;

/// Convert a slice of `Vec3` into an `OmegaTensor<f32, 2>` of shape
/// `[N, 3]`. Returns the tensor with the same N positions, copied
/// element-by-element.
#[must_use]
pub fn vec3_slice_to_omega(points: &[Vec3]) -> OmegaTensor<f32, 2> {
    let n = points.len() as u64;
    let mut tensor = OmegaTensor::<f32, 2>::new([n, 3]);
    for (i, p) in points.iter().enumerate() {
        let idx_u64 = i as u64;
        tensor.set([idx_u64, 0], p.x);
        tensor.set([idx_u64, 1], p.y);
        tensor.set([idx_u64, 2], p.z);
    }
    tensor
}

/// Convert an `OmegaTensor<f32, 2>` of shape `[N, 3]` back into a `Vec<Vec3>`.
/// Returns an empty Vec for any tensor whose shape does not match `[?, 3]`.
#[must_use]
pub fn omega_to_vec3_vec(tensor: &OmegaTensor<f32, 2>) -> Vec<Vec3> {
    let shape = tensor.shape();
    if shape[1] != 3 {
        return Vec::new();
    }
    let n = shape[0] as usize;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let idx_u64 = i as u64;
        let x = tensor.get([idx_u64, 0]).unwrap_or(0.0);
        let y = tensor.get([idx_u64, 1]).unwrap_or(0.0);
        let z = tensor.get([idx_u64, 2]).unwrap_or(0.0);
        out.push(Vec3::new(x, y, z));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{omega_to_vec3_vec, vec3_slice_to_omega};
    use crate::vec3::Vec3;

    #[test]
    fn vec3_slice_round_trip_through_tensor() {
        let pts = vec![
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::new(4.0, 5.0, 6.0),
            Vec3::new(7.0, 8.0, 9.0),
        ];
        let tensor = vec3_slice_to_omega(&pts);
        assert_eq!(tensor.shape(), [3, 3]);
        let back = omega_to_vec3_vec(&tensor);
        assert_eq!(pts, back);
    }

    #[test]
    fn empty_slice_round_trips_to_empty_tensor() {
        let pts: Vec<Vec3> = Vec::new();
        let tensor = vec3_slice_to_omega(&pts);
        assert_eq!(tensor.shape(), [0, 3]);
        let back = omega_to_vec3_vec(&tensor);
        assert!(back.is_empty());
    }
}
