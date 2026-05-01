//! Cocreative-bias scoring trait abstraction + stage-0 dot-product + stage-1 KAN-stub impls.
//!
//! § T11-W6-KAN-BRIDGE — module 2/4
//!
//! § ROLE
//!   LoA's `cocreative-optimizer` ranks candidate completions against a
//!   feature-vector parameterization of the player's stylistic bias.
//!   Stage-0 today uses a simple θ·x dot-product ; stage-1+ replaces
//!   this with a KAN evaluator that learns nonlinear feature-mixing
//!   directly from the ω-field substrate. The trait below pins the
//!   `score_features` + `rank_candidates` contract so both backends
//!   plug in interchangeably.

/// Trait for any cocreative-bias scoring backend.
///
/// Object-safe : registry stores `Box<dyn CocreativeClassifier>`.
pub trait CocreativeClassifier: Send + Sync {
    /// Stable identifier (e.g. `"stage0-dot-product"`).
    fn name(&self) -> &'static str;

    /// Score a single feature-vector. Higher = more aligned with the
    /// player's cocreative bias. Implementations must return `0.0` for
    /// any malformed input (e.g. dim-mismatch) rather than panic.
    fn score_features(&self, features: &[f32]) -> f32;

    /// Rank a slice of candidate feature-vectors by descending score.
    /// Returns indices into the input slice. Stable for equal scores.
    fn rank_candidates(&self, candidates: &[Vec<f32>]) -> Vec<usize>;
}

/// Stage-0 dot-product classifier : `score(x) = θ · x`.
///
/// `theta.len()` must equal `dim` ; mismatched-dim inputs to
/// `score_features` return `0.0` rather than panic.
pub struct Stage0DotProductClassifier {
    pub theta: Vec<f32>,
    pub dim: usize,
}

impl Stage0DotProductClassifier {
    /// Construct from a pre-trained `theta` vector.
    #[must_use]
    pub fn new(theta: Vec<f32>) -> Self {
        let dim = theta.len();
        Self { theta, dim }
    }

    /// Construct with a default 4-dim θ = [0.5, 0.5, 0.5, 0.5] for tests.
    #[must_use]
    pub fn default_dim4() -> Self {
        Self::new(vec![0.5_f32, 0.5_f32, 0.5_f32, 0.5_f32])
    }
}

impl CocreativeClassifier for Stage0DotProductClassifier {
    fn name(&self) -> &'static str {
        "stage0-dot-product"
    }

    fn score_features(&self, features: &[f32]) -> f32 {
        if features.len() != self.dim {
            return 0.0;
        }
        self.theta
            .iter()
            .zip(features.iter())
            .map(|(t, x)| t * x)
            .sum()
    }

    fn rank_candidates(&self, candidates: &[Vec<f32>]) -> Vec<usize> {
        let mut indexed: Vec<(usize, f32)> = candidates
            .iter()
            .enumerate()
            .map(|(i, x)| (i, self.score_features(x)))
            .collect();
        // Stable sort : descending by score, ties preserve original order.
        indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        indexed.into_iter().map(|(i, _)| i).collect()
    }
}

/// Stage-1 KAN-stub classifier : same trait, opaque KAN handle, fallback.
///
/// When `kan_handle.is_some()` returns a canned mocked score (sum of
/// features clamped) ; when `None`, delegates to `fallback`.
pub struct Stage1KanStubClassifier {
    pub fallback: Box<dyn CocreativeClassifier>,
    pub kan_handle: Option<String>,
}

impl Stage1KanStubClassifier {
    /// Construct with fallback + no KAN handle.
    #[must_use]
    pub fn new(fallback: Box<dyn CocreativeClassifier>) -> Self {
        Self {
            fallback,
            kan_handle: None,
        }
    }

    /// Construct with fallback + mock KAN handle.
    #[must_use]
    pub fn with_handle(fallback: Box<dyn CocreativeClassifier>, handle: String) -> Self {
        Self {
            fallback,
            kan_handle: Some(handle),
        }
    }
}

impl CocreativeClassifier for Stage1KanStubClassifier {
    fn name(&self) -> &'static str {
        "stage1-kan-stub"
    }

    fn score_features(&self, features: &[f32]) -> f32 {
        if self.kan_handle.is_some() {
            // Mocked KAN response : tanh-clamped feature-sum.
            let s: f32 = features.iter().sum();
            s.tanh()
        } else {
            self.fallback.score_features(features)
        }
    }

    fn rank_candidates(&self, candidates: &[Vec<f32>]) -> Vec<usize> {
        if self.kan_handle.is_some() {
            // Mocked KAN ranking : same algorithm but using mocked scores.
            let mut indexed: Vec<(usize, f32)> = candidates
                .iter()
                .enumerate()
                .map(|(i, x)| (i, self.score_features(x)))
                .collect();
            indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            indexed.into_iter().map(|(i, _)| i).collect()
        } else {
            self.fallback.rank_candidates(candidates)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage0_dot_correct() {
        let c = Stage0DotProductClassifier::new(vec![1.0, 2.0, 3.0]);
        let s = c.score_features(&[1.0, 1.0, 1.0]);
        assert!((s - 6.0).abs() < 1e-6);
    }

    #[test]
    fn stage0_dim_mismatch_zero() {
        let c = Stage0DotProductClassifier::new(vec![1.0, 2.0, 3.0]);
        assert!(c.score_features(&[1.0, 1.0]).abs() < 1e-6);
        assert!(c.score_features(&[1.0, 1.0, 1.0, 1.0]).abs() < 1e-6);
    }

    #[test]
    fn stage0_rank_orders_correctly() {
        let c = Stage0DotProductClassifier::new(vec![1.0, 1.0]);
        let cands = vec![
            vec![1.0, 1.0], // score 2.0
            vec![5.0, 5.0], // score 10.0
            vec![3.0, 3.0], // score 6.0
        ];
        let ranked = c.rank_candidates(&cands);
        assert_eq!(ranked, vec![1, 2, 0]);
    }

    #[test]
    fn stage1_no_kan_falls_through() {
        let stage0 = Box::new(Stage0DotProductClassifier::default_dim4());
        let s1 = Stage1KanStubClassifier::new(stage0);
        let s = s1.score_features(&[1.0, 1.0, 1.0, 1.0]);
        // stage-0 with default θ=[0.5;4] · [1;4] = 2.0.
        assert!((s - 2.0).abs() < 1e-6);
    }

    #[test]
    fn stage1_with_kan_mocked() {
        let stage0 = Box::new(Stage0DotProductClassifier::default_dim4());
        let s1 = Stage1KanStubClassifier::with_handle(stage0, String::from("kan-mock"));
        let s = s1.score_features(&[10.0, 10.0, 10.0, 10.0]);
        // tanh(40.0) ~= 1.0
        assert!(s > 0.99);
    }

    #[test]
    fn trait_object_safe() {
        let v: Vec<Box<dyn CocreativeClassifier>> = vec![
            Box::new(Stage0DotProductClassifier::default_dim4()),
            Box::new(Stage1KanStubClassifier::new(Box::new(
                Stage0DotProductClassifier::default_dim4(),
            ))),
        ];
        assert_eq!(v.len(), 2);
    }
}
