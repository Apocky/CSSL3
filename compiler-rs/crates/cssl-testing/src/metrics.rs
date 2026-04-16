//! Observability metric data-structures — frequency, latency, dispatch histograms.
//!
//! § SPEC : `specs/22_TELEMETRY.csl` ring-buffer schema +
//!          `specs/23_TESTING.csl` § oracle-modes • frequency-stability + latency-percentile.
//! § ROLE : not an oracle mode itself; data-structs consumed by `@bench`,
//!          `@power_bench`, `@thermal_stress`, `@latency_percentile` oracle invocations.
//! § STATUS : T11 stub — structs wired; sampling pipeline pending.

/// Frequency-stability sample derived from `zesFrequencyGetState` at 100Hz.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FrequencySample {
    /// Mean GPU clock over the sampling window.
    pub mean_mhz: f32,
    /// Standard deviation (stability metric; §§ 23 target stdev/mean < 0.05).
    pub stdev_mhz: f32,
    /// Minimum observed clock (§§ 23 target > 90% of nominal base-clock).
    pub min_mhz: f32,
    /// Maximum observed clock.
    pub max_mhz: f32,
}

/// Dispatch-latency histogram samples from `{Telemetry<DispatchLatency>}`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LatencyPercentiles {
    /// 50th percentile.
    pub p50_ns: u64,
    /// 90th percentile.
    pub p90_ns: u64,
    /// 99th percentile (§§ 23 target < 1µs for L0-immediate-command-lists).
    pub p99_ns: u64,
    /// 99.9th percentile.
    pub p99_9_ns: u64,
}

/// Combined metrics snapshot consumed by bench/stress oracle modes.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MetricsSnapshot {
    /// Frequency-stability readings.
    pub frequency: FrequencySample,
    /// Latency percentile readings.
    pub latency: LatencyPercentiles,
}

#[cfg(test)]
mod tests {
    use super::{FrequencySample, LatencyPercentiles, MetricsSnapshot};

    #[test]
    fn defaults_are_zero() {
        // compare f32 via bit-pattern to sidestep clippy::float_cmp
        assert_eq!(
            FrequencySample::default().mean_mhz.to_bits(),
            0.0_f32.to_bits()
        );
        assert_eq!(LatencyPercentiles::default().p99_ns, 0);
        let snap = MetricsSnapshot::default();
        assert_eq!(snap.frequency.mean_mhz.to_bits(), 0.0_f32.to_bits());
        assert_eq!(snap.latency.p99_ns, 0);
    }
}
