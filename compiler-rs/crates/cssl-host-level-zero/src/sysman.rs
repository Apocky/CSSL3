//! Sysman R18 telemetry metric catalog + capture trait.
//!
//! § SPEC : `specs/10_HW.csl` § SYSMAN AVAILABILITY TABLE + `specs/22_TELEMETRY.csl` (R18).

use core::fmt;
use std::collections::BTreeSet;

use thiserror::Error;

use crate::driver::L0Device;

/// Sysman metric catalog (11 variants covering the R18 probe matrix).
///
/// Each metric maps 1:1 to a `zes*Get*` API. The L0 column is `direct` everywhere
/// per `specs/10` § SYSMAN-AVAILABILITY-TABLE ; CSSLv3 targets L0 as the canonical
/// R18 telemetry backing on Intel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SysmanMetric {
    /// `zesPowerGetEnergyCounter` — accumulated energy (millijoules).
    PowerEnergyCounter,
    /// `zesPowerGetLimits` / `zesPowerSetLimits` — TDP envelope.
    PowerLimits,
    /// `zesTemperatureGetState` — current die temperature (°C).
    TemperatureCurrent,
    /// `zesTemperatureGetMaxRange` — rated thermal envelope.
    TemperatureMaxRange,
    /// `zesFrequencyGetState` — current GPU frequency (MHz).
    FrequencyCurrent,
    /// `zesFrequencyGetRange` — min/max supported frequency.
    FrequencyRange,
    /// `zesFrequencyOcGet` — overclock state.
    FrequencyOverclock,
    /// `zesEngineGetActivity` — per-engine busy-time (cumulative).
    EngineActivity,
    /// `zesRasGetState` — reliability / availability / serviceability events.
    RasEvents,
    /// `zesDeviceProcessesGetState` — running-process count.
    ProcessList,
    /// `zesPerformanceFactorGetConfig` — perf-factor hint.
    PerformanceFactor,
}

impl SysmanMetric {
    /// Canonical metric name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PowerEnergyCounter => "power.energy_counter_mj",
            Self::PowerLimits => "power.limits_w",
            Self::TemperatureCurrent => "temperature.current_c",
            Self::TemperatureMaxRange => "temperature.max_range_c",
            Self::FrequencyCurrent => "frequency.current_mhz",
            Self::FrequencyRange => "frequency.range_mhz",
            Self::FrequencyOverclock => "frequency.overclock",
            Self::EngineActivity => "engine.activity_us",
            Self::RasEvents => "ras.events",
            Self::ProcessList => "processes.list",
            Self::PerformanceFactor => "performance.factor",
        }
    }

    /// Category — used by telemetry aggregation + R18 effect-row discharge.
    #[must_use]
    pub const fn category(self) -> MetricCategory {
        match self {
            Self::PowerEnergyCounter | Self::PowerLimits => MetricCategory::Power,
            Self::TemperatureCurrent | Self::TemperatureMaxRange => MetricCategory::Thermal,
            Self::FrequencyCurrent | Self::FrequencyRange | Self::FrequencyOverclock => {
                MetricCategory::Frequency
            }
            Self::EngineActivity => MetricCategory::EngineActivity,
            Self::RasEvents => MetricCategory::Ras,
            Self::ProcessList => MetricCategory::Processes,
            Self::PerformanceFactor => MetricCategory::Performance,
        }
    }

    /// All 11 metrics.
    pub const ALL_METRICS: [Self; 11] = [
        Self::PowerEnergyCounter,
        Self::PowerLimits,
        Self::TemperatureCurrent,
        Self::TemperatureMaxRange,
        Self::FrequencyCurrent,
        Self::FrequencyRange,
        Self::FrequencyOverclock,
        Self::EngineActivity,
        Self::RasEvents,
        Self::ProcessList,
        Self::PerformanceFactor,
    ];
}

impl fmt::Display for SysmanMetric {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Metric category group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetricCategory {
    Power,
    Thermal,
    Frequency,
    EngineActivity,
    Ras,
    Processes,
    Performance,
}

/// Set of metrics to capture on each probe iteration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SysmanMetricSet {
    metrics: BTreeSet<SysmanMetric>,
}

impl SysmanMetricSet {
    /// Empty.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Canonical R18 full-fidelity set (all 11 metrics).
    #[must_use]
    pub fn full_r18() -> Self {
        Self::from_iter(SysmanMetric::ALL_METRICS)
    }

    /// "Advisory" subset (power + temp + frequency — non-privileged).
    #[must_use]
    pub fn advisory() -> Self {
        Self::from_iter([
            SysmanMetric::PowerEnergyCounter,
            SysmanMetric::TemperatureCurrent,
            SysmanMetric::FrequencyCurrent,
        ])
    }

    /// Add a metric.
    pub fn add(&mut self, m: SysmanMetric) {
        self.metrics.insert(m);
    }

    /// Present check.
    #[must_use]
    pub fn contains(&self, m: SysmanMetric) -> bool {
        self.metrics.contains(&m)
    }

    /// Iterate in stable order.
    pub fn iter(&self) -> impl Iterator<Item = SysmanMetric> + '_ {
        self.metrics.iter().copied()
    }

    /// Size.
    #[must_use]
    pub fn len(&self) -> usize {
        self.metrics.len()
    }

    /// Empty check.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.metrics.is_empty()
    }
}

impl FromIterator<SysmanMetric> for SysmanMetricSet {
    fn from_iter<I: IntoIterator<Item = SysmanMetric>>(iter: I) -> Self {
        let mut s = Self::new();
        for m in iter {
            s.add(m);
        }
        s
    }
}

/// One captured sample.
#[derive(Debug, Clone, PartialEq)]
pub struct SysmanSample {
    /// Metric captured.
    pub metric: SysmanMetric,
    /// Captured value (metric-type-specific semantics ; callers interpret per-variant).
    pub value: f64,
    /// Sample timestamp (microseconds since arbitrary epoch).
    pub timestamp_us: u64,
}

/// A full capture round — one sample per declared metric.
#[derive(Debug, Clone, PartialEq)]
pub struct SysmanCapture {
    /// Samples in the declared metric-set order.
    pub samples: Vec<SysmanSample>,
    /// Source device index.
    pub device_index: u32,
}

/// Failure modes for telemetry.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TelemetryError {
    /// Sysman subsystem not initialized (`zesInit` never called).
    #[error("Sysman not initialized — call `zesInit` first")]
    SysmanNotInitialized,
    /// Device does not support the requested metric.
    #[error("device {device_index} does not expose metric `{metric}`")]
    UnsupportedMetric {
        device_index: u32,
        metric: SysmanMetric,
    },
    /// FFI not wired at stage-0.
    #[error(
        "FFI backend not wired at stage-0 (T10-phase-2 delivers `level-zero-sys` integration)"
    )]
    FfiNotWired,
}

/// Trait for sysman-backed telemetry probes (stage-0 stub + phase-2 real impl).
pub trait TelemetryProbe {
    /// Capture one round — one sample per metric in `metrics`.
    ///
    /// # Errors
    /// Returns [`TelemetryError::SysmanNotInitialized`] if sysman isn't ready,
    /// [`TelemetryError::UnsupportedMetric`] per-metric if unavailable, or
    /// [`TelemetryError::FfiNotWired`] if backed by the stub.
    fn capture(
        &self,
        device: &L0Device,
        metrics: &SysmanMetricSet,
    ) -> Result<SysmanCapture, TelemetryError>;
}

/// Stage-0 stub probe — returns canonical Arc A770 sample values.
#[derive(Debug, Clone, Default)]
pub struct StubTelemetryProbe {
    /// Monotonic timestamp counter.
    pub next_timestamp_us: core::cell::Cell<u64>,
}

impl StubTelemetryProbe {
    /// New probe.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            next_timestamp_us: core::cell::Cell::new(0),
        }
    }
}

impl TelemetryProbe for StubTelemetryProbe {
    fn capture(
        &self,
        device: &L0Device,
        metrics: &SysmanMetricSet,
    ) -> Result<SysmanCapture, TelemetryError> {
        let ts = self.next_timestamp_us.get();
        self.next_timestamp_us.set(ts.saturating_add(1000));

        let samples: Vec<SysmanSample> = metrics
            .iter()
            .map(|metric| SysmanSample {
                metric,
                value: stub_value(metric),
                timestamp_us: ts,
            })
            .collect();

        Ok(SysmanCapture {
            samples,
            device_index: device.device_index,
        })
    }
}

/// Canonical stub sample value for a given metric (Arc A770 typical values).
const fn stub_value(metric: SysmanMetric) -> f64 {
    match metric {
        SysmanMetric::PowerEnergyCounter => 100_000.0, // mJ
        SysmanMetric::PowerLimits => 225.0,            // W
        SysmanMetric::TemperatureCurrent => 55.0,      // °C
        SysmanMetric::TemperatureMaxRange => 95.0,     // °C
        SysmanMetric::FrequencyCurrent => 1800.0,      // MHz
        SysmanMetric::FrequencyRange => 2100.0,        // MHz (max)
        SysmanMetric::FrequencyOverclock => 0.0,       // factor
        SysmanMetric::EngineActivity => 1_000_000.0,   // µs accumulated
        SysmanMetric::RasEvents => 0.0,                // count
        SysmanMetric::ProcessList => 1.0,              // process count
        SysmanMetric::PerformanceFactor => 100.0,      // %
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MetricCategory, StubTelemetryProbe, SysmanMetric, SysmanMetricSet, TelemetryProbe,
    };
    use crate::driver::L0Driver;

    #[test]
    fn metric_names() {
        assert_eq!(
            SysmanMetric::PowerEnergyCounter.as_str(),
            "power.energy_counter_mj"
        );
        assert_eq!(
            SysmanMetric::TemperatureCurrent.as_str(),
            "temperature.current_c"
        );
    }

    #[test]
    fn metric_category_maps() {
        assert_eq!(
            SysmanMetric::PowerEnergyCounter.category(),
            MetricCategory::Power
        );
        assert_eq!(
            SysmanMetric::FrequencyCurrent.category(),
            MetricCategory::Frequency
        );
        assert_eq!(SysmanMetric::RasEvents.category(), MetricCategory::Ras);
    }

    #[test]
    fn metric_all_count() {
        assert_eq!(SysmanMetric::ALL_METRICS.len(), 11);
    }

    #[test]
    fn full_r18_has_all_metrics() {
        let s = SysmanMetricSet::full_r18();
        assert_eq!(s.len(), 11);
        for m in SysmanMetric::ALL_METRICS {
            assert!(s.contains(m));
        }
    }

    #[test]
    fn advisory_has_subset() {
        let s = SysmanMetricSet::advisory();
        assert_eq!(s.len(), 3);
        assert!(s.contains(SysmanMetric::PowerEnergyCounter));
        assert!(s.contains(SysmanMetric::TemperatureCurrent));
        assert!(s.contains(SysmanMetric::FrequencyCurrent));
        assert!(!s.contains(SysmanMetric::RasEvents));
    }

    #[test]
    fn stub_probe_captures_declared_metrics() {
        let driver = L0Driver::stub_arc_a770();
        let dev = &driver.devices[0];
        let probe = StubTelemetryProbe::new();
        let set = SysmanMetricSet::advisory();
        let cap = probe.capture(dev, &set).unwrap();
        assert_eq!(cap.samples.len(), 3);
        assert_eq!(cap.device_index, 0);
    }

    #[test]
    fn stub_probe_advances_timestamp() {
        let driver = L0Driver::stub_arc_a770();
        let dev = &driver.devices[0];
        let probe = StubTelemetryProbe::new();
        let set = SysmanMetricSet::advisory();
        let a = probe.capture(dev, &set).unwrap();
        let b = probe.capture(dev, &set).unwrap();
        assert!(b.samples[0].timestamp_us > a.samples[0].timestamp_us);
    }

    #[test]
    fn stub_captures_arc_canonical_tdp() {
        let driver = L0Driver::stub_arc_a770();
        let dev = &driver.devices[0];
        let probe = StubTelemetryProbe::new();
        let set = SysmanMetricSet::from_iter([SysmanMetric::PowerLimits]);
        let cap = probe.capture(dev, &set).unwrap();
        // Arc A770 TDP = 225 W per `specs/10`.
        assert!((cap.samples[0].value - 225.0).abs() < f64::EPSILON);
    }
}
