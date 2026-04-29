//! Telemetry scope + kind enumerations.
//!
//! § SPEC : `specs/22_TELEMETRY.csl` § TELEMETRY-SCOPE TAXONOMY.

use core::fmt;

/// 26-variant telemetry-scope taxonomy mirroring `specs/22` § TAXONOMY.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TelemetryScope {
    // § CPU-domain (8)
    /// Wallclock time (monotonic).
    WallClock,
    /// CPU-cycles counter (RDTSC / PAPI).
    CpuCycles,
    /// Instructions-retired.
    CpuInstRetired,
    /// Cache-miss events (L1 / L2 / L3).
    CacheMisses,
    /// Branch-mispredict events.
    BranchMisses,
    /// TLB miss events.
    TlbMisses,
    /// Page-fault events.
    PageFaults,
    /// Context-switch events.
    CtxSwitches,
    // § GPU-domain (6)
    /// Dispatch-latency (pre/post timestamp delta).
    DispatchLatency,
    /// Kernel occupancy estimate.
    KernelOccupancy,
    /// Shader invocations (pipeline-statistics).
    ShaderInvocations,
    /// Ray-tracing rays-per-second.
    RtRaysPerSec,
    /// Memory bandwidth.
    MemBandwidth,
    /// XMX engine utilization.
    XmxUtilization,
    // § Power / Thermal / Frequency (R18 primary) (4)
    /// Energy-counter (Joules cumulative).
    Power,
    /// Die-temperature (Celsius).
    Thermal,
    /// GPU frequency (MHz).
    Frequency,
    /// Fan speed (RPM).
    FanSpeed,
    // § RAS (reliability) (2)
    /// ECC errors (corrected + uncorrected).
    EccErrors,
    /// PCIe link-replay counters.
    PcieReplay,
    // § App-semantic (custom) (4)
    /// User-defined event-counters.
    Counters,
    /// OpenTelemetry-style spans.
    Spans,
    /// Structured event records.
    Events,
    /// Audit-chain entries (§§ 11).
    Audit,
    // § Compound (1)
    /// All-of-above (maximum overhead).
    Full,
    // § PRIME-DIRECTIVE diagnostic (T11-D132)
    /// **Diagnostic-only** scope used by the biometric-egress-refusal
    /// boundary in [`crate::TelemetrySlot::record_labeled`] to mark the
    /// audit-chain entry that records a refused egress attempt.
    ///
    /// This scope is **never** used to log actual biometric data — it
    /// records *the refusal itself* (timestamp + refusal-reason + domain)
    /// so PRIME-DIRECTIVE §11 attestation has a permanent signed witness
    /// of every biometric-data-leak attempt. The variant is named
    /// `BiometricRefused` to make the audit-chain self-documenting.
    BiometricRefused,
}

impl TelemetryScope {
    /// Short-name (stable diagnostic form).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WallClock => "wallclock",
            Self::CpuCycles => "cpu-cycles",
            Self::CpuInstRetired => "cpu-inst-retired",
            Self::CacheMisses => "cache-misses",
            Self::BranchMisses => "branch-misses",
            Self::TlbMisses => "tlb-misses",
            Self::PageFaults => "page-faults",
            Self::CtxSwitches => "ctx-switches",
            Self::DispatchLatency => "dispatch-latency",
            Self::KernelOccupancy => "kernel-occupancy",
            Self::ShaderInvocations => "shader-invocations",
            Self::RtRaysPerSec => "rt-rays-per-sec",
            Self::MemBandwidth => "mem-bandwidth",
            Self::XmxUtilization => "xmx-utilization",
            Self::Power => "power",
            Self::Thermal => "thermal",
            Self::Frequency => "frequency",
            Self::FanSpeed => "fan-speed",
            Self::EccErrors => "ecc-errors",
            Self::PcieReplay => "pcie-replay",
            Self::Counters => "counters",
            Self::Spans => "spans",
            Self::Events => "events",
            Self::Audit => "audit",
            Self::Full => "full",
            Self::BiometricRefused => "biometric-refused",
        }
    }

    /// Domain-category.
    #[must_use]
    pub const fn domain(self) -> ScopeDomain {
        match self {
            Self::WallClock
            | Self::CpuCycles
            | Self::CpuInstRetired
            | Self::CacheMisses
            | Self::BranchMisses
            | Self::TlbMisses
            | Self::PageFaults
            | Self::CtxSwitches => ScopeDomain::Cpu,
            Self::DispatchLatency
            | Self::KernelOccupancy
            | Self::ShaderInvocations
            | Self::RtRaysPerSec
            | Self::MemBandwidth
            | Self::XmxUtilization => ScopeDomain::Gpu,
            Self::Power | Self::Thermal | Self::Frequency | Self::FanSpeed => {
                ScopeDomain::PowerThermal
            }
            Self::EccErrors | Self::PcieReplay => ScopeDomain::Ras,
            Self::Counters | Self::Spans | Self::Events | Self::Audit => ScopeDomain::AppSemantic,
            Self::Full => ScopeDomain::Compound,
            Self::BiometricRefused => ScopeDomain::PrimeDirective,
        }
    }

    /// Stable 16-bit encoding for serialization.
    #[must_use]
    pub const fn as_u16(self) -> u16 {
        match self {
            Self::WallClock => 0,
            Self::CpuCycles => 1,
            Self::CpuInstRetired => 2,
            Self::CacheMisses => 3,
            Self::BranchMisses => 4,
            Self::TlbMisses => 5,
            Self::PageFaults => 6,
            Self::CtxSwitches => 7,
            Self::DispatchLatency => 8,
            Self::KernelOccupancy => 9,
            Self::ShaderInvocations => 10,
            Self::RtRaysPerSec => 11,
            Self::MemBandwidth => 12,
            Self::XmxUtilization => 13,
            Self::Power => 14,
            Self::Thermal => 15,
            Self::Frequency => 16,
            Self::FanSpeed => 17,
            Self::EccErrors => 18,
            Self::PcieReplay => 19,
            Self::Counters => 20,
            Self::Spans => 21,
            Self::Events => 22,
            Self::Audit => 23,
            Self::Full => 255,
            Self::BiometricRefused => 254,
        }
    }

    /// All 26 scopes (25 telemetry + 1 PRIME-DIRECTIVE diagnostic).
    pub const ALL_SCOPES: [Self; 26] = [
        Self::WallClock,
        Self::CpuCycles,
        Self::CpuInstRetired,
        Self::CacheMisses,
        Self::BranchMisses,
        Self::TlbMisses,
        Self::PageFaults,
        Self::CtxSwitches,
        Self::DispatchLatency,
        Self::KernelOccupancy,
        Self::ShaderInvocations,
        Self::RtRaysPerSec,
        Self::MemBandwidth,
        Self::XmxUtilization,
        Self::Power,
        Self::Thermal,
        Self::Frequency,
        Self::FanSpeed,
        Self::EccErrors,
        Self::PcieReplay,
        Self::Counters,
        Self::Spans,
        Self::Events,
        Self::Audit,
        Self::Full,
        Self::BiometricRefused,
    ];
}

impl fmt::Display for TelemetryScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Scope-domain category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScopeDomain {
    /// CPU counters.
    Cpu,
    /// GPU performance counters.
    Gpu,
    /// Power / thermal / frequency (R18 primary).
    PowerThermal,
    /// Reliability-Availability-Serviceability.
    Ras,
    /// App-semantic (counters / spans / events / audit).
    AppSemantic,
    /// Compound (all-of-above).
    Compound,
    /// PRIME-DIRECTIVE diagnostic (biometric-refusal etc).
    PrimeDirective,
}

impl ScopeDomain {
    /// Short-name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
            Self::PowerThermal => "power-thermal",
            Self::Ras => "ras",
            Self::AppSemantic => "app-semantic",
            Self::Compound => "compound",
            Self::PrimeDirective => "prime-directive",
        }
    }
}

/// Telemetry event-kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TelemetryKind {
    /// Numeric sample (metric reading).
    Sample,
    /// Span begin.
    SpanBegin,
    /// Span end.
    SpanEnd,
    /// Counter increment.
    Counter,
    /// Audit-chain entry.
    Audit,
}

impl TelemetryKind {
    /// Short-name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sample => "sample",
            Self::SpanBegin => "span-begin",
            Self::SpanEnd => "span-end",
            Self::Counter => "counter",
            Self::Audit => "audit",
        }
    }

    /// Stable 16-bit encoding.
    #[must_use]
    pub const fn as_u16(self) -> u16 {
        match self {
            Self::Sample => 0,
            Self::SpanBegin => 1,
            Self::SpanEnd => 2,
            Self::Counter => 3,
            Self::Audit => 4,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ScopeDomain, TelemetryKind, TelemetryScope};

    #[test]
    fn scope_count() {
        assert_eq!(TelemetryScope::ALL_SCOPES.len(), 26);
    }

    #[test]
    fn scope_names() {
        assert_eq!(TelemetryScope::Power.as_str(), "power");
        assert_eq!(TelemetryScope::DispatchLatency.as_str(), "dispatch-latency");
        assert_eq!(TelemetryScope::Audit.as_str(), "audit");
    }

    #[test]
    fn scope_u16_unique_non_full() {
        let mut seen = std::collections::HashSet::new();
        for s in TelemetryScope::ALL_SCOPES {
            if s != TelemetryScope::Full && s != TelemetryScope::BiometricRefused {
                seen.insert(s.as_u16());
            }
        }
        assert_eq!(seen.len(), 24);
    }

    #[test]
    fn scope_u16_full_is_255() {
        assert_eq!(TelemetryScope::Full.as_u16(), 255);
    }

    #[test]
    fn scope_u16_biometric_refused_is_254() {
        assert_eq!(TelemetryScope::BiometricRefused.as_u16(), 254);
    }

    #[test]
    fn scope_biometric_refused_canonical_name() {
        assert_eq!(
            TelemetryScope::BiometricRefused.as_str(),
            "biometric-refused"
        );
    }

    #[test]
    fn scope_biometric_refused_domain_is_prime_directive() {
        assert_eq!(
            TelemetryScope::BiometricRefused.domain(),
            ScopeDomain::PrimeDirective
        );
        assert_eq!(ScopeDomain::PrimeDirective.as_str(), "prime-directive");
    }

    #[test]
    fn scope_domain_grouping() {
        assert_eq!(TelemetryScope::CpuCycles.domain(), ScopeDomain::Cpu);
        assert_eq!(TelemetryScope::XmxUtilization.domain(), ScopeDomain::Gpu);
        assert_eq!(TelemetryScope::Power.domain(), ScopeDomain::PowerThermal);
        assert_eq!(TelemetryScope::EccErrors.domain(), ScopeDomain::Ras);
        assert_eq!(TelemetryScope::Audit.domain(), ScopeDomain::AppSemantic);
        assert_eq!(TelemetryScope::Full.domain(), ScopeDomain::Compound);
    }

    #[test]
    fn scope_domain_names() {
        assert_eq!(ScopeDomain::Cpu.as_str(), "cpu");
        assert_eq!(ScopeDomain::PowerThermal.as_str(), "power-thermal");
    }

    #[test]
    fn kind_names() {
        assert_eq!(TelemetryKind::Sample.as_str(), "sample");
        assert_eq!(TelemetryKind::SpanBegin.as_str(), "span-begin");
        assert_eq!(TelemetryKind::Audit.as_str(), "audit");
    }

    #[test]
    fn kind_u16_sequential() {
        let values: Vec<_> = [
            TelemetryKind::Sample,
            TelemetryKind::SpanBegin,
            TelemetryKind::SpanEnd,
            TelemetryKind::Counter,
            TelemetryKind::Audit,
        ]
        .iter()
        .map(|k| k.as_u16())
        .collect();
        assert_eq!(values, vec![0, 1, 2, 3, 4]);
    }
}
