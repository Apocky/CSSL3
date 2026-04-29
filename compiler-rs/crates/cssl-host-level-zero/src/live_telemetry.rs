//! Live sysman R18 telemetry probe — emits samples through the
//! [`cssl_telemetry::TelemetryRing`] placeholder per the S6-E5 brief.
//!
//! § SPEC : `specs/22_TELEMETRY.csl § R18 OBSERVABILITY-FIRST-CLASS` +
//!          `specs/10_HW.csl § SYSMAN-AVAILABILITY-TABLE`.
//!
//! § DESIGN
//!   - The companion [`crate::sysman::StubTelemetryProbe`] returns canonical
//!     Arc A770 sample-values without any FFI call (used on bare CI runners).
//!   - This module's [`LiveTelemetryProbe`] uses a real [`crate::loader::L0Loader`]
//!     to read sysman metrics via `zes*Get*` entry-points and pushes each
//!     sample into a caller-supplied [`TelemetryRingHandle`].
//!   - The full R18 plumbing (sampling-thread, audit-chain, OTLP exporter) is
//!     deferred per the handoff S6-E5 brief — this slice only wires the probe
//!     into the existing ring API.
//!
//! § PRIME-DIRECTIVE
//!   Telemetry samples here are LOCAL to the process ; nothing leaves the
//!   machine. Egress to OpenTelemetry collectors is gated by a separate
//!   `{Audit<"telemetry-egress">}` effect-row — out of scope for this slice.

use core::cell::Cell;

use cssl_telemetry::ring::{TelemetryRing, TelemetrySlot};
use cssl_telemetry::scope::{TelemetryKind, TelemetryScope};
use thiserror::Error;

use crate::ffi::{ZeResult, ZesDevice, ZesEnergyCounter, ZesFreqState, ZesPwr, ZesTemp};
use crate::loader::{L0Loader, LoaderError};
use crate::sysman::{
    SysmanCapture, SysmanMetric, SysmanMetricSet, SysmanSample, TelemetryError, TelemetryProbe,
};

/// Wrapper around a [`TelemetryRing`] threaded into [`LiveTelemetryProbe::capture`].
///
/// Stage-0 the ring is a single-thread `RefCell`-backed structure ; the
/// `'r` lifetime + `&Self` borrow-shape preserve the SPSC-with-borrow contract.
#[derive(Debug)]
pub struct TelemetryRingHandle<'r> {
    ring: &'r TelemetryRing,
    /// Monotonic timestamp counter (relative ; absolute timestamps require
    /// a clock-source which is host-specific — full R18 wires that).
    next_ts_ns: Cell<u64>,
    /// Most recently observed device-id (carried into ring slots).
    device_id: u32,
}

impl<'r> TelemetryRingHandle<'r> {
    /// Wrap a `TelemetryRing` for use by [`LiveTelemetryProbe`].
    #[must_use]
    pub const fn new(ring: &'r TelemetryRing, device_id: u32) -> Self {
        Self {
            ring,
            next_ts_ns: Cell::new(0),
            device_id,
        }
    }

    /// Push a sample as a [`TelemetrySlot`].
    ///
    /// Slot-encoding :
    ///   - `timestamp_ns` : the next monotonic counter
    ///   - `scope`        : maps `SysmanMetric` → `TelemetryScope` per category
    ///   - `kind`         : `Sample`
    ///   - `cpu_or_gpu_id`: device-id from the wrapper
    ///   - `payload`      : 8-byte little-endian f64 of the sample value
    ///
    /// # Errors
    /// Returns [`TelemetryEmitError::RingOverflow`] when the ring is full.
    /// Per the spec, callers tolerate overflow (lossy-non-blocking).
    pub fn push_sample(&self, sample: &SysmanSample) -> Result<(), TelemetryEmitError> {
        let ts = self.next_ts_ns.get();
        self.next_ts_ns.set(ts.saturating_add(1_000));

        let scope = scope_for_metric(sample.metric);
        let mut payload = [0u8; 40];
        let bytes = sample.value.to_le_bytes();
        payload[..bytes.len()].copy_from_slice(&bytes);

        let mut slot = TelemetrySlot::new(ts, scope, TelemetryKind::Sample);
        slot.cpu_or_gpu_id = self.device_id;
        slot.payload = payload;

        match self.ring.push(slot) {
            Ok(()) => Ok(()),
            Err(_) => Err(TelemetryEmitError::RingOverflow),
        }
    }

    /// Total samples successfully emitted to the ring (excludes overflows).
    /// Useful for tests + cap-budget enforcement.
    #[must_use]
    pub fn samples_emitted(&self) -> u64 {
        self.ring.total_pushed() - self.ring.overflow_count()
    }
}

/// Live sysman probe — reads R18 metrics via L0 and pushes samples into a
/// [`TelemetryRingHandle`].
pub struct LiveTelemetryProbe<'l, 'r> {
    /// Loader for `zes*` entry-point dispatch.
    loader: &'l L0Loader,
    /// Sysman device handle to query.
    sysman_device: ZesDevice,
    /// Ring to push samples into.
    ring: &'r TelemetryRingHandle<'r>,
}

impl<'l, 'r> LiveTelemetryProbe<'l, 'r> {
    /// Construct a live probe.
    #[must_use]
    pub const fn new(
        loader: &'l L0Loader,
        sysman_device: ZesDevice,
        ring: &'r TelemetryRingHandle<'r>,
    ) -> Self {
        Self {
            loader,
            sysman_device,
            ring,
        }
    }

    /// Read sysman metrics + emit one ring slot per sample.
    ///
    /// # Errors
    /// Mirrors [`TelemetryProbe::capture`] : returns
    /// [`TelemetryError::SysmanNotInitialized`] / [`TelemetryError::FfiNotWired`]
    /// / [`TelemetryError::UnsupportedMetric`] as appropriate. Sample push to
    /// the ring is best-effort ; overflows are counted but do NOT fail the
    /// capture.
    pub fn capture_and_emit(
        &self,
        metrics: &SysmanMetricSet,
    ) -> Result<SysmanCapture, TelemetryError> {
        if !self.loader.has_sysman() {
            return Err(TelemetryError::SysmanNotInitialized);
        }
        let mut samples = Vec::with_capacity(metrics.len());
        for metric in metrics.iter() {
            let value = self.read_metric(metric)?;
            let sample = SysmanSample {
                metric,
                value,
                timestamp_us: 0,
            };
            // Best-effort emit ; overflow does not fail the capture.
            let _ = self.ring.push_sample(&sample);
            samples.push(sample);
        }
        Ok(SysmanCapture {
            samples,
            device_index: self.ring.device_id,
        })
    }

    /// Read one metric — internal dispatcher.
    fn read_metric(&self, metric: SysmanMetric) -> Result<f64, TelemetryError> {
        match metric {
            SysmanMetric::PowerEnergyCounter => self.read_power_energy(),
            SysmanMetric::TemperatureCurrent => self.read_temperature(),
            SysmanMetric::FrequencyCurrent => self.read_frequency(),
            // The remaining metrics are deferred to a later slice — the FFI
            // shapes for `zesPowerGetLimits` / `zesEngineGetActivity` /
            // `zesRasGetState` differ enough that a phase-F refinement
            // covers them. Until then they surface as UnsupportedMetric.
            other => Err(TelemetryError::UnsupportedMetric {
                device_index: self.ring.device_id,
                metric: other,
            }),
        }
    }

    fn read_power_energy(&self) -> Result<f64, TelemetryError> {
        let enum_pwr = self
            .loader
            .zes_device_enum_power_domains
            .ok_or(TelemetryError::SysmanNotInitialized)?;
        let get_energy = self
            .loader
            .zes_power_get_energy_counter
            .ok_or(TelemetryError::SysmanNotInitialized)?;

        let mut count: u32 = 0;
        // SAFETY: documented count-query (null fill-ptr).
        let raw = unsafe { (enum_pwr)(self.sysman_device, &mut count, core::ptr::null_mut()) };
        if !ZeResult::from_raw(raw).is_success() || count == 0 {
            return Err(TelemetryError::UnsupportedMetric {
                device_index: self.ring.device_id,
                metric: SysmanMetric::PowerEnergyCounter,
            });
        }
        let mut domains = vec![ZesPwr(core::ptr::null_mut()); count as usize];
        // SAFETY: caller-owned buffer of `count` slots.
        let raw = unsafe { (enum_pwr)(self.sysman_device, &mut count, domains.as_mut_ptr()) };
        if !ZeResult::from_raw(raw).is_success() || count == 0 {
            return Err(TelemetryError::UnsupportedMetric {
                device_index: self.ring.device_id,
                metric: SysmanMetric::PowerEnergyCounter,
            });
        }
        domains.truncate(count as usize);

        let mut counter = ZesEnergyCounter::default();
        // SAFETY: domain handle is non-null (count>0 + Success).
        let raw = unsafe { (get_energy)(domains[0], &mut counter) };
        if !ZeResult::from_raw(raw).is_success() {
            return Err(TelemetryError::UnsupportedMetric {
                device_index: self.ring.device_id,
                metric: SysmanMetric::PowerEnergyCounter,
            });
        }
        // Convert to milli-Joules (raw is micro-Joules).
        Ok(counter.energy_uj as f64 / 1_000.0)
    }

    fn read_temperature(&self) -> Result<f64, TelemetryError> {
        let enum_temp = self
            .loader
            .zes_device_enum_temperature_sensors
            .ok_or(TelemetryError::SysmanNotInitialized)?;
        let get_state = self
            .loader
            .zes_temperature_get_state
            .ok_or(TelemetryError::SysmanNotInitialized)?;

        let mut count: u32 = 0;
        // SAFETY: documented count-query.
        let raw = unsafe { (enum_temp)(self.sysman_device, &mut count, core::ptr::null_mut()) };
        if !ZeResult::from_raw(raw).is_success() || count == 0 {
            return Err(TelemetryError::UnsupportedMetric {
                device_index: self.ring.device_id,
                metric: SysmanMetric::TemperatureCurrent,
            });
        }
        let mut sensors = vec![ZesTemp(core::ptr::null_mut()); count as usize];
        // SAFETY: caller-owned `count` slots.
        let raw = unsafe { (enum_temp)(self.sysman_device, &mut count, sensors.as_mut_ptr()) };
        if !ZeResult::from_raw(raw).is_success() || count == 0 {
            return Err(TelemetryError::UnsupportedMetric {
                device_index: self.ring.device_id,
                metric: SysmanMetric::TemperatureCurrent,
            });
        }
        sensors.truncate(count as usize);

        let mut state: f64 = 0.0;
        // SAFETY: sensor handle is the first element of a Success-filled buffer.
        let raw = unsafe { (get_state)(sensors[0], &mut state) };
        if !ZeResult::from_raw(raw).is_success() {
            return Err(TelemetryError::UnsupportedMetric {
                device_index: self.ring.device_id,
                metric: SysmanMetric::TemperatureCurrent,
            });
        }
        Ok(state)
    }

    fn read_frequency(&self) -> Result<f64, TelemetryError> {
        let enum_freq = self
            .loader
            .zes_device_enum_frequency_domains
            .ok_or(TelemetryError::SysmanNotInitialized)?;
        let get_state = self
            .loader
            .zes_frequency_get_state
            .ok_or(TelemetryError::SysmanNotInitialized)?;

        let mut count: u32 = 0;
        // SAFETY: documented count-query.
        let raw = unsafe { (enum_freq)(self.sysman_device, &mut count, core::ptr::null_mut()) };
        if !ZeResult::from_raw(raw).is_success() || count == 0 {
            return Err(TelemetryError::UnsupportedMetric {
                device_index: self.ring.device_id,
                metric: SysmanMetric::FrequencyCurrent,
            });
        }
        let mut domains = vec![crate::ffi::ZesFreq(core::ptr::null_mut()); count as usize];
        // SAFETY: caller-owned `count` slots.
        let raw = unsafe { (enum_freq)(self.sysman_device, &mut count, domains.as_mut_ptr()) };
        if !ZeResult::from_raw(raw).is_success() || count == 0 {
            return Err(TelemetryError::UnsupportedMetric {
                device_index: self.ring.device_id,
                metric: SysmanMetric::FrequencyCurrent,
            });
        }
        domains.truncate(count as usize);

        let mut state = ZesFreqState::default();
        // SAFETY: domain handle is the first non-null entry post-Success.
        let raw = unsafe { (get_state)(domains[0], &mut state) };
        if !ZeResult::from_raw(raw).is_success() {
            return Err(TelemetryError::UnsupportedMetric {
                device_index: self.ring.device_id,
                metric: SysmanMetric::FrequencyCurrent,
            });
        }
        Ok(state.actual_mhz)
    }
}

impl<'l, 'r> TelemetryProbe for LiveTelemetryProbe<'l, 'r> {
    fn capture(
        &self,
        _device: &crate::driver::L0Device,
        metrics: &SysmanMetricSet,
    ) -> Result<SysmanCapture, TelemetryError> {
        self.capture_and_emit(metrics)
    }
}

/// Map a [`SysmanMetric`] to the corresponding [`TelemetryScope`] used in the ring.
#[must_use]
pub const fn scope_for_metric(m: SysmanMetric) -> TelemetryScope {
    match m {
        SysmanMetric::PowerEnergyCounter | SysmanMetric::PowerLimits => TelemetryScope::Power,
        SysmanMetric::TemperatureCurrent | SysmanMetric::TemperatureMaxRange => {
            TelemetryScope::Thermal
        }
        SysmanMetric::FrequencyCurrent
        | SysmanMetric::FrequencyRange
        | SysmanMetric::FrequencyOverclock => TelemetryScope::Frequency,
        SysmanMetric::EngineActivity => TelemetryScope::XmxUtilization,
        SysmanMetric::RasEvents => TelemetryScope::EccErrors,
        SysmanMetric::ProcessList | SysmanMetric::PerformanceFactor => TelemetryScope::Counters,
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Errors
// ───────────────────────────────────────────────────────────────────────────

/// Failure modes for the ring-emission path.
///
/// No `Eq` derive — `LoaderError::LoadFailed` carries a `String`. Tests
/// pattern-match instead of `assert_eq!`-ing.
#[derive(Debug, Error)]
pub enum TelemetryEmitError {
    /// Telemetry ring overflow — sample dropped, ring counter incremented.
    #[error("telemetry ring overflow — sample dropped (lossy-non-blocking per spec)")]
    RingOverflow,
    /// Loader subsystem error wrapped through.
    #[error(transparent)]
    Loader(#[from] LoaderError),
}

// ───────────────────────────────────────────────────────────────────────────
// § Tests
// ───────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_telemetry::ring::TelemetryRing;

    #[test]
    fn scope_mapping_power() {
        assert_eq!(
            scope_for_metric(SysmanMetric::PowerEnergyCounter),
            TelemetryScope::Power
        );
        assert_eq!(
            scope_for_metric(SysmanMetric::PowerLimits),
            TelemetryScope::Power
        );
    }

    #[test]
    fn scope_mapping_thermal() {
        assert_eq!(
            scope_for_metric(SysmanMetric::TemperatureCurrent),
            TelemetryScope::Thermal
        );
    }

    #[test]
    fn scope_mapping_frequency() {
        assert_eq!(
            scope_for_metric(SysmanMetric::FrequencyCurrent),
            TelemetryScope::Frequency
        );
    }

    #[test]
    fn ring_handle_pushes_sample_into_ring() {
        let ring = TelemetryRing::new(8);
        let h = TelemetryRingHandle::new(&ring, 0);
        let sample = SysmanSample {
            metric: SysmanMetric::PowerEnergyCounter,
            value: 12_345.0,
            timestamp_us: 0,
        };
        h.push_sample(&sample).unwrap();
        assert_eq!(ring.len(), 1);
        let slot = ring.peek().unwrap();
        assert_eq!(slot.cpu_or_gpu_id, 0);
        assert_eq!(slot.scope, TelemetryScope::Power.as_u16());
        assert_eq!(slot.kind, TelemetryKind::Sample.as_u16());
        // payload is 8-byte LE f64 of 12345.0
        let val_bytes = &slot.payload[..8];
        let mut buf = [0u8; 8];
        buf.copy_from_slice(val_bytes);
        let v = f64::from_le_bytes(buf);
        assert!((v - 12_345.0).abs() < f64::EPSILON);
    }

    #[test]
    fn ring_handle_advances_timestamp_monotonically() {
        let ring = TelemetryRing::new(8);
        let h = TelemetryRingHandle::new(&ring, 1);
        let s = SysmanSample {
            metric: SysmanMetric::TemperatureCurrent,
            value: 50.0,
            timestamp_us: 0,
        };
        h.push_sample(&s).unwrap();
        h.push_sample(&s).unwrap();
        let drained = ring.drain_all();
        assert!(drained[1].timestamp_ns > drained[0].timestamp_ns);
    }

    #[test]
    fn ring_handle_overflow_returns_error() {
        let ring = TelemetryRing::new(1);
        let h = TelemetryRingHandle::new(&ring, 0);
        let s = SysmanSample {
            metric: SysmanMetric::PowerEnergyCounter,
            value: 1.0,
            timestamp_us: 0,
        };
        h.push_sample(&s).unwrap();
        let err = h.push_sample(&s).unwrap_err();
        assert!(matches!(err, TelemetryEmitError::RingOverflow));
        assert_eq!(ring.overflow_count(), 1);
    }

    #[test]
    fn samples_emitted_excludes_overflow() {
        let ring = TelemetryRing::new(1);
        let h = TelemetryRingHandle::new(&ring, 7);
        let s = SysmanSample {
            metric: SysmanMetric::FrequencyCurrent,
            value: 2100.0,
            timestamp_us: 0,
        };
        h.push_sample(&s).unwrap();
        let _ = h.push_sample(&s); // overflow
        assert_eq!(h.samples_emitted(), 1);
    }

    #[test]
    fn ring_handle_records_device_id() {
        let ring = TelemetryRing::new(2);
        let h = TelemetryRingHandle::new(&ring, 99);
        let s = SysmanSample {
            metric: SysmanMetric::FrequencyCurrent,
            value: 1500.0,
            timestamp_us: 0,
        };
        h.push_sample(&s).unwrap();
        let slot = ring.peek().unwrap();
        assert_eq!(slot.cpu_or_gpu_id, 99);
    }

    #[test]
    fn telemetry_emit_error_display() {
        let _ = format!("{}", TelemetryEmitError::RingOverflow);
        let _ = format!("{}", TelemetryEmitError::Loader(LoaderError::NotFound));
    }

    /// Live probe gated to Apocky's host. Reads sysman metrics into the ring.
    #[test]
    #[ignore = "requires Intel L0 + sysman R18 (Arc A770 driver) — run with --ignored"]
    fn arc_a770_live_probe_emits_power_thermal_frequency() {
        use crate::session::DriverSession;
        let loader = L0Loader::open().expect("loader present");
        // sysman-init may differ from compute-init ; gracefully skip if missing
        if !loader.has_sysman() {
            eprintln!("sysman partially unavailable — skipping");
            return;
        }
        let _session = DriverSession::open(&loader).expect("session open");

        // Enumerate sysman drivers + devices.
        let zes_drivers = loader
            .enumerate_sysman_drivers()
            .expect("zesDriverGet succeeds");
        if zes_drivers.is_empty() {
            eprintln!("no sysman drivers — skipping");
            return;
        }
        let mut count: u32 = 0;
        // SAFETY: count-query.
        let raw = unsafe {
            (loader.zes_device_get.unwrap())(zes_drivers[0], &mut count, core::ptr::null_mut())
        };
        assert_eq!(ZeResult::from_raw(raw), ZeResult::Success);
        let mut sysman_devs = vec![ZesDevice(core::ptr::null_mut()); count as usize];
        // SAFETY: caller-owned buffer.
        let raw = unsafe {
            (loader.zes_device_get.unwrap())(zes_drivers[0], &mut count, sysman_devs.as_mut_ptr())
        };
        assert_eq!(ZeResult::from_raw(raw), ZeResult::Success);
        sysman_devs.truncate(count as usize);

        let ring = TelemetryRing::new(64);
        let handle = TelemetryRingHandle::new(&ring, 0);
        let probe = LiveTelemetryProbe::new(&loader, sysman_devs[0], &handle);
        let metrics = SysmanMetricSet::advisory();
        let cap = probe.capture_and_emit(&metrics).expect("capture");
        assert_eq!(cap.samples.len(), 3);
        // Some samples should have been emitted.
        assert!(handle.samples_emitted() >= 1);
    }
}
