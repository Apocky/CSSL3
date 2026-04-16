//! Thermal-stress oracle (`@thermal_stress`) — R18 sysman backed sustained sampling.
//!
//! § SPEC    : `specs/23_TESTING.csl` § oracle-modes • thermal-stress +
//!             `specs/22_TELEMETRY.csl` `{Telemetry<Thermal>}` scope.
//! § BACKING : `zesTemperatureGetState` via `cssl-host-level-zero` sysman.
//! § POLICY  : 5 min sustained workload; steady-state reached < 120s; max-temp < thermal-limit - 5°C.
//! § STATUS  : T11 stub — implementation pending.

use core::time::Duration;

/// Config for the `@thermal_stress` oracle.
#[derive(Debug, Clone)]
pub struct Config {
    /// Duration of sustained workload (§§ 23 default 5 min).
    pub duration: Duration,
    /// Sampling interval (§§ 23 default 100ms).
    pub sample_interval: Duration,
    /// Thermal hard limit in Celsius; steady-state must stay below this minus safety-margin.
    pub limit_c: f32,
    /// Safety margin below hard limit (default 5°C).
    pub safety_margin_c: f32,
    /// Maximum steady-state convergence time (§§ 23 default 120s).
    pub steady_state_max: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            duration: Duration::from_secs(5 * 60),
            sample_interval: Duration::from_millis(100),
            limit_c: 100.0,
            safety_margin_c: 5.0,
            steady_state_max: Duration::from_secs(120),
        }
    }
}

/// Outcome of running the `@thermal_stress` oracle.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Stage0 stub — body populated at T11.
    Stage0Unimplemented,
    /// Workload stayed within thermal envelope.
    Ok { steady_c: f32, peak_c: f32 },
    /// Thermal limit breached.
    LimitBreached { peak_c: f32, limit_c: f32 },
    /// Steady-state not reached within `steady_state_max`.
    NoSteadyState { final_c: f32 },
    /// Sysman unavailable on this platform (non-Intel).
    SysmanUnavailable,
}

/// Dispatcher trait for `@thermal_stress` oracle.
pub trait Dispatcher {
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
