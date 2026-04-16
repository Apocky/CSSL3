//! Differential-backend oracle (`@differential`) — R18 central-test.
//!
//! § SPEC   : `specs/23_TESTING.csl` § oracle-modes • differential.
//! § GATE   : T28 (OG8) ship-gate — Vulkan × Level-Zero bit-exact on same SPIR-V.
//! § ROLE   : runs same compiled kernel on multiple backends with seeded inputs;
//!            compares outputs bit-exact for `{PureDet}`-tagged fns, with ULP-tolerance
//!            for non-PureDet.
//! § STATUS : T10 stub — implementation pending (blocks on `cssl-host-*` crates).

/// Backend registered for differential comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Backend {
    /// Vulkan 1.4.333 via `cssl-host-vulkan`.
    Vulkan,
    /// Intel Level-Zero compute via `cssl-host-level-zero`.
    LevelZero,
    /// D3D12 via `cssl-host-d3d12` (Windows).
    D3d12,
    /// Metal via `cssl-host-metal` (Apple).
    Metal,
    /// WebGPU via `cssl-host-webgpu` (browser + native).
    WebGpu,
}

/// Config for the `@differential` oracle.
#[derive(Debug, Clone)]
pub struct Config {
    /// Backends to compare (minimum length 2 at T10; at T1 this list is empty stub).
    pub backends: Vec<Backend>,
    /// If `true`, require byte-for-byte equality (used with `{PureDet}` tag).
    /// If `false`, `ulp_tolerance` bounds allowed float divergence.
    pub pure_det: bool,
    /// ULPs of allowed float divergence when `pure_det == false`.
    pub ulp_tolerance: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            backends: Vec::new(),
            pure_det: true,
            ulp_tolerance: 0,
        }
    }
}

/// Outcome of running the `@differential` oracle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Stage0 stub — body populated at T10.
    Stage0Unimplemented,
    /// All backends produced matching output within tolerance.
    Ok,
    /// Backend `Backend` diverged at the described input.
    Divergence {
        backend: Backend,
        delta: String,
        message: String,
    },
}

/// Dispatcher trait for `@differential` oracle.
pub trait Dispatcher {
    /// Execute the oracle for the configured backend matrix.
    fn run(&self, config: &Config) -> Outcome;
}

/// Stage0 stub dispatcher — always returns `Stage0Unimplemented`.
#[derive(Debug, Default, Clone, Copy)]
pub struct Stage0Stub;

impl Dispatcher for Stage0Stub {
    fn run(&self, _config: &Config) -> Outcome {
        Outcome::Stage0Unimplemented
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, Dispatcher, Outcome, Stage0Stub};

    #[test]
    fn stub_returns_unimplemented() {
        assert_eq!(
            Stage0Stub.run(&Config::default()),
            Outcome::Stage0Unimplemented
        );
    }
}
