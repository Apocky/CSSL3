//! Telemetry schema metadata + scope-set helpers.

use std::collections::BTreeSet;

use crate::scope::TelemetryScope;

/// Set of declared telemetry scopes (used by fat-binary [telemetry-schema] section).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TelemetryScopeSet {
    scopes: BTreeSet<TelemetryScope>,
}

impl TelemetryScopeSet {
    /// Empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a scope.
    pub fn add(&mut self, s: TelemetryScope) {
        self.scopes.insert(s);
    }

    /// Present check.
    #[must_use]
    pub fn contains(&self, s: TelemetryScope) -> bool {
        self.scopes.contains(&s)
    }

    /// Iter (sorted).
    pub fn iter(&self) -> impl Iterator<Item = TelemetryScope> + '_ {
        self.scopes.iter().copied()
    }

    /// Size.
    #[must_use]
    pub fn len(&self) -> usize {
        self.scopes.len()
    }

    /// Empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.scopes.is_empty()
    }

    /// True iff this set is a subset of `other` (scope narrowing invariant per `specs/22`
    /// § "callee's scope ⊑ caller's scope").
    #[must_use]
    pub fn is_subset_of(&self, other: &Self) -> bool {
        self.scopes.is_subset(&other.scopes)
    }

    /// Full-scope set shortcut (everything including `Full`).
    #[must_use]
    pub fn full() -> Self {
        Self::from_iter(TelemetryScope::ALL_SCOPES)
    }
}

impl FromIterator<TelemetryScope> for TelemetryScopeSet {
    fn from_iter<I: IntoIterator<Item = TelemetryScope>>(iter: I) -> Self {
        let mut s = Self::new();
        for x in iter {
            s.add(x);
        }
        s
    }
}

/// Telemetry schema metadata — embedded in `.cssl-bin` fat-binary `[telemetry-schema]` section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetrySchema {
    /// Schema version (monotonic ; bump on incompatible changes).
    pub version: u32,
    /// Module name (from source `module com.apocky.loa`).
    pub module: String,
    /// Declared scopes for this module.
    pub scopes: TelemetryScopeSet,
    /// Ring-buffer size declared via `@telemetry_ring_size<N>`.
    pub ring_size: usize,
    /// Sampling-rate (Hz) for periodic samplers.
    pub sampling_hz: u32,
}

impl TelemetrySchema {
    /// Default schema : empty-scopes + 2^20 ring + 100 Hz sysman sampling (per `specs/22` defaults).
    #[must_use]
    pub fn defaults_for(module: impl Into<String>) -> Self {
        Self {
            version: 1,
            module: module.into(),
            scopes: TelemetryScopeSet::new(),
            ring_size: 1 << 20,
            sampling_hz: 100,
        }
    }

    /// Short diagnostic summary.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "schema v{} / module={} / {} scopes / ring={} / sampling={}Hz",
            self.version,
            self.module,
            self.scopes.len(),
            self.ring_size,
            self.sampling_hz,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{TelemetrySchema, TelemetryScopeSet};
    use crate::scope::TelemetryScope;

    #[test]
    fn empty_scope_set_is_empty() {
        let s = TelemetryScopeSet::new();
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn scope_set_add_contains() {
        let mut s = TelemetryScopeSet::new();
        s.add(TelemetryScope::Power);
        s.add(TelemetryScope::Thermal);
        assert!(s.contains(TelemetryScope::Power));
        assert!(s.contains(TelemetryScope::Thermal));
        assert!(!s.contains(TelemetryScope::Full));
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn scope_set_subset_check() {
        let a = TelemetryScopeSet::from_iter([TelemetryScope::Power, TelemetryScope::Thermal]);
        let b = TelemetryScopeSet::from_iter([
            TelemetryScope::Power,
            TelemetryScope::Thermal,
            TelemetryScope::Frequency,
        ]);
        assert!(a.is_subset_of(&b));
        assert!(!b.is_subset_of(&a));
    }

    #[test]
    fn scope_set_full_has_all_25() {
        let s = TelemetryScopeSet::full();
        assert_eq!(s.len(), 25);
    }

    #[test]
    fn schema_defaults_canonical() {
        let s = TelemetrySchema::defaults_for("mymod");
        assert_eq!(s.version, 1);
        assert_eq!(s.module, "mymod");
        assert_eq!(s.ring_size, 1 << 20);
        assert_eq!(s.sampling_hz, 100);
        assert!(s.scopes.is_empty());
    }

    #[test]
    fn schema_summary_has_module_and_ring_size() {
        let s = TelemetrySchema::defaults_for("mymod");
        let sum = s.summary();
        assert!(sum.contains("mymod"));
        assert!(sum.contains("ring=1048576"));
        assert!(sum.contains("sampling=100Hz"));
    }
}
