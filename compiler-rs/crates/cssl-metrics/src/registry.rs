//! `MetricRegistry` — per-subsystem namespace + cross-crate registration.
//!
//! § SPEC : `_drafts/phase_j/06_l2_telemetry_spec.md` § II.7 + § IV.
//!
//! § DESIGN
//!   - The registry is a `Mutex<HashMap<&'static str, RegistryEntry>>` keyed by
//!     fully-qualified metric-name (`"engine.frame_n"` / `"render.stage_time_ns"`).
//!   - `register(name, kind, schema_id)` is idempotent for matching schema-id ;
//!     a mismatched schema-id triggers [`MetricError::SchemaCollision`].
//!   - The global `global()` instance is process-wide ; per-subsystem
//!     [`SubsystemRegistry`] thin-wrappers prepend a stable namespace prefix
//!     (e.g., `"engine."`) so cross-crate code organizes naturally.
//!   - `completeness_check(catalog)` validates that every catalog-entry has
//!     a matching registration ; missing entries are reported in the result.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::error::{MetricError, MetricResult};

/// Type of metric registered (mirrors [`crate::Counter`] / [`crate::Gauge`] / etc).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetricKind {
    /// Counter (monotonic u64).
    Counter,
    /// Gauge (f64 set/get).
    Gauge,
    /// Histogram (bucketed distribution).
    Histogram,
    /// Timer (RAII duration).
    Timer,
}

impl MetricKind {
    /// Short-name (for diagnostic messages).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Counter => "counter",
            Self::Gauge => "gauge",
            Self::Histogram => "histogram",
            Self::Timer => "timer",
        }
    }
}

/// Registered-metric metadata (name + kind + schema-id).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RegistryEntry {
    /// Fully-qualified metric-name.
    pub name: &'static str,
    /// Kind.
    pub kind: MetricKind,
    /// Schema-id (FNV1a of name + tag-keys).
    pub schema_id: u64,
}

/// Process-wide metric registry.
#[derive(Debug, Default)]
pub struct MetricRegistry {
    inner: Mutex<HashMap<&'static str, RegistryEntry>>,
}

impl MetricRegistry {
    /// New empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a metric ; idempotent for matching schema-id.
    ///
    /// # Errors
    /// Returns [`MetricError::SchemaCollision`] when `name` is already present
    /// with a different `schema_id`.
    pub fn register(
        &self,
        name: &'static str,
        kind: MetricKind,
        schema_id: u64,
    ) -> MetricResult<()> {
        let mut guard = self.inner.lock().expect("registry mutex poisoned");
        match guard.get(name) {
            Some(existing) => {
                if existing.schema_id != schema_id || existing.kind != kind {
                    return Err(MetricError::SchemaCollision {
                        existing: existing.name,
                        new: name,
                    });
                }
                Ok(())
            }
            None => {
                guard.insert(
                    name,
                    RegistryEntry {
                        name,
                        kind,
                        schema_id,
                    },
                );
                Ok(())
            }
        }
    }

    /// Lookup a metric.
    #[must_use]
    pub fn lookup(&self, name: &str) -> Option<RegistryEntry> {
        let guard = self.inner.lock().expect("registry mutex poisoned");
        guard.get(name).cloned()
    }

    /// True iff `name` is registered.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        let guard = self.inner.lock().expect("registry mutex poisoned");
        guard.contains_key(name)
    }

    /// Number of registered metrics.
    #[must_use]
    pub fn len(&self) -> usize {
        let guard = self.inner.lock().expect("registry mutex poisoned");
        guard.len()
    }

    /// True iff no metrics registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        let guard = self.inner.lock().expect("registry mutex poisoned");
        guard.is_empty()
    }

    /// Snapshot of all registered entries (sorted by name for determinism).
    #[must_use]
    pub fn entries(&self) -> Vec<RegistryEntry> {
        let guard = self.inner.lock().expect("registry mutex poisoned");
        let mut v: Vec<_> = guard.values().cloned().collect();
        v.sort_by_key(|e| e.name);
        v
    }

    /// Filter to entries whose name starts with `prefix` (sorted).
    #[must_use]
    pub fn entries_with_prefix(&self, prefix: &str) -> Vec<RegistryEntry> {
        let guard = self.inner.lock().expect("registry mutex poisoned");
        let mut v: Vec<_> = guard
            .values()
            .filter(|e| e.name.starts_with(prefix))
            .cloned()
            .collect();
        v.sort_by_key(|e| e.name);
        v
    }

    /// Walk a catalog of expected `(name, kind)` pairs ; return any not-yet-registered.
    ///
    /// § DISCIPLINE : the catalog is the canonical inventory from § III of the
    /// spec ; a non-empty `missing` is a build-fail-condition for downstream
    /// integration tests.
    #[must_use]
    pub fn completeness_check(
        &self,
        catalog: &[(&'static str, MetricKind)],
    ) -> CompletenessReport {
        let guard = self.inner.lock().expect("registry mutex poisoned");
        let mut missing = Vec::new();
        let mut mismatched = Vec::new();
        let mut present = 0_usize;
        for (name, expected_kind) in catalog {
            match guard.get(*name) {
                None => missing.push(*name),
                Some(entry) => {
                    if entry.kind == *expected_kind {
                        present += 1;
                    } else {
                        mismatched.push((*name, entry.kind, *expected_kind));
                    }
                }
            }
        }
        CompletenessReport {
            total: catalog.len(),
            present,
            missing,
            mismatched,
        }
    }

    /// Clear ; useful for tests that want a fresh registry.
    pub fn clear(&self) {
        let mut guard = self.inner.lock().expect("registry mutex poisoned");
        guard.clear();
    }
}

/// Result of a [`MetricRegistry::completeness_check`].
#[derive(Debug, Clone, Default)]
pub struct CompletenessReport {
    /// Catalog size.
    pub total: usize,
    /// Number of catalog-entries present in the registry with the right kind.
    pub present: usize,
    /// Catalog-entries not registered at all.
    pub missing: Vec<&'static str>,
    /// Catalog-entries registered with the wrong kind.
    pub mismatched: Vec<(&'static str, MetricKind, MetricKind)>,
}

impl CompletenessReport {
    /// True iff every catalog entry is present with the right kind.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.missing.is_empty() && self.mismatched.is_empty()
    }

    /// Coverage as a fraction (0.0..=1.0). Returns 1.0 for empty catalog.
    #[must_use]
    pub fn coverage_fraction(&self) -> f64 {
        if self.total == 0 {
            return 1.0;
        }
        (self.present as f64) / (self.total as f64)
    }
}

/// Process-wide singleton registry.
///
/// § DISCIPLINE : avoid double-init with `OnceLock` ; the registry is created
/// on first use and lives for the life of the process.
#[must_use]
pub fn global() -> &'static MetricRegistry {
    static GLOBAL: OnceLock<MetricRegistry> = OnceLock::new();
    GLOBAL.get_or_init(MetricRegistry::new)
}

/// Per-subsystem registry view : prefixes names with `<subsystem>.`.
///
/// § USAGE
/// ```rust,ignore
/// let r = SubsystemRegistry::new("engine");
/// // r.register("frame_n", ...) actually registers "engine.frame_n"
/// ```
#[derive(Debug)]
pub struct SubsystemRegistry {
    /// Subsystem prefix (without trailing dot).
    pub subsystem: &'static str,
    /// Underlying registry.
    pub inner: &'static MetricRegistry,
}

impl SubsystemRegistry {
    /// View into the global registry for `subsystem`.
    #[must_use]
    pub fn new(subsystem: &'static str) -> Self {
        Self {
            subsystem,
            inner: global(),
        }
    }

    /// Subsystem-prefix.
    #[must_use]
    pub fn prefix(&self) -> &'static str {
        self.subsystem
    }

    /// All entries with this subsystem's prefix.
    #[must_use]
    pub fn entries(&self) -> Vec<RegistryEntry> {
        // Build the full prefix on the fly.
        let buf = format!("{}.", self.subsystem);
        self.inner.entries_with_prefix(&buf)
    }

    /// Number of entries with this subsystem's prefix.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries().len()
    }

    /// True iff no entries with this subsystem's prefix.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        global, CompletenessReport, MetricKind, MetricRegistry, RegistryEntry, SubsystemRegistry,
    };
    use crate::error::MetricError;

    fn fresh_registry() -> MetricRegistry {
        MetricRegistry::new()
    }

    #[test]
    fn empty_registry_is_empty() {
        let r = fresh_registry();
        assert!(r.is_empty());
        assert_eq!(r.len(), 0);
    }

    #[test]
    fn register_one() {
        let r = fresh_registry();
        r.register("a", MetricKind::Counter, 1).unwrap();
        assert_eq!(r.len(), 1);
        assert!(r.contains("a"));
    }

    #[test]
    fn register_idempotent_for_matching_schema() {
        let r = fresh_registry();
        r.register("a", MetricKind::Counter, 1).unwrap();
        r.register("a", MetricKind::Counter, 1).unwrap();
        r.register("a", MetricKind::Counter, 1).unwrap();
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn register_collision_on_schema_id() {
        let r = fresh_registry();
        r.register("a", MetricKind::Counter, 1).unwrap();
        let res = r.register("a", MetricKind::Counter, 2);
        assert!(matches!(res, Err(MetricError::SchemaCollision { .. })));
    }

    #[test]
    fn register_collision_on_kind_mismatch() {
        let r = fresh_registry();
        r.register("a", MetricKind::Counter, 1).unwrap();
        let res = r.register("a", MetricKind::Gauge, 1);
        assert!(matches!(res, Err(MetricError::SchemaCollision { .. })));
    }

    #[test]
    fn lookup_returns_entry() {
        let r = fresh_registry();
        r.register("a", MetricKind::Counter, 1).unwrap();
        let entry = r.lookup("a").unwrap();
        assert_eq!(entry.name, "a");
        assert_eq!(entry.kind, MetricKind::Counter);
        assert_eq!(entry.schema_id, 1);
    }

    #[test]
    fn lookup_missing_returns_none() {
        let r = fresh_registry();
        assert!(r.lookup("nope").is_none());
    }

    #[test]
    fn entries_sorted_by_name() {
        let r = fresh_registry();
        r.register("b", MetricKind::Counter, 1).unwrap();
        r.register("a", MetricKind::Counter, 2).unwrap();
        r.register("c", MetricKind::Counter, 3).unwrap();
        let names: Vec<_> = r.entries().iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["a", "b", "c"]);
    }

    #[test]
    fn entries_with_prefix_filters() {
        let r = fresh_registry();
        r.register("engine.frame_n", MetricKind::Counter, 1).unwrap();
        r.register("engine.tick", MetricKind::Gauge, 2).unwrap();
        r.register("render.stage_time", MetricKind::Timer, 3).unwrap();
        let engine = r.entries_with_prefix("engine.");
        assert_eq!(engine.len(), 2);
        let render = r.entries_with_prefix("render.");
        assert_eq!(render.len(), 1);
    }

    #[test]
    fn completeness_check_all_present() {
        let r = fresh_registry();
        r.register("a", MetricKind::Counter, 1).unwrap();
        r.register("b", MetricKind::Gauge, 2).unwrap();
        let cat = [("a", MetricKind::Counter), ("b", MetricKind::Gauge)];
        let report = r.completeness_check(&cat);
        assert!(report.is_complete());
        assert_eq!(report.coverage_fraction(), 1.0);
    }

    #[test]
    fn completeness_check_missing_reported() {
        let r = fresh_registry();
        r.register("a", MetricKind::Counter, 1).unwrap();
        let cat = [("a", MetricKind::Counter), ("b", MetricKind::Gauge)];
        let report = r.completeness_check(&cat);
        assert!(!report.is_complete());
        assert_eq!(report.missing, vec!["b"]);
    }

    #[test]
    fn completeness_check_mismatched_kind_reported() {
        let r = fresh_registry();
        r.register("a", MetricKind::Counter, 1).unwrap();
        let cat = [("a", MetricKind::Gauge)];
        let report = r.completeness_check(&cat);
        assert!(!report.is_complete());
        assert_eq!(report.mismatched.len(), 1);
        assert_eq!(report.mismatched[0].0, "a");
    }

    #[test]
    fn completeness_check_partial_coverage_fraction() {
        let r = fresh_registry();
        r.register("a", MetricKind::Counter, 1).unwrap();
        let cat = [("a", MetricKind::Counter), ("b", MetricKind::Gauge)];
        let report = r.completeness_check(&cat);
        assert_eq!(report.coverage_fraction(), 0.5);
    }

    #[test]
    fn completeness_empty_catalog_full_coverage() {
        let r = fresh_registry();
        let cat: [(&'static str, MetricKind); 0] = [];
        let report = r.completeness_check(&cat);
        assert_eq!(report.coverage_fraction(), 1.0);
        assert!(report.is_complete());
    }

    #[test]
    fn clear_resets_registry() {
        let r = fresh_registry();
        r.register("a", MetricKind::Counter, 1).unwrap();
        r.clear();
        assert!(r.is_empty());
    }

    #[test]
    fn metric_kind_as_str() {
        assert_eq!(MetricKind::Counter.as_str(), "counter");
        assert_eq!(MetricKind::Gauge.as_str(), "gauge");
        assert_eq!(MetricKind::Histogram.as_str(), "histogram");
        assert_eq!(MetricKind::Timer.as_str(), "timer");
    }

    #[test]
    fn registry_entry_clonable() {
        let e = RegistryEntry {
            name: "x",
            kind: MetricKind::Counter,
            schema_id: 1,
        };
        let _e2 = e.clone();
    }

    #[test]
    fn global_singleton_is_stable() {
        let a = global() as *const MetricRegistry;
        let b = global() as *const MetricRegistry;
        assert_eq!(a, b);
    }

    #[test]
    fn subsystem_registry_prefix_observable() {
        let s = SubsystemRegistry::new("engine");
        assert_eq!(s.prefix(), "engine");
    }

    #[test]
    fn subsystem_registry_filters_entries() {
        let r = global();
        // Use unique names to avoid collisions across tests.
        let _ = r.register("subsystest_engine.frame_x", MetricKind::Counter, 99);
        let _ = r.register("subsystest_render.stage_x", MetricKind::Timer, 100);
        // Constructed view ; the SubsystemRegistry uses prefix("subsystest_engine") which
        // does not match "subsystest_render", so we only see one entry from this batch.
        let view = SubsystemRegistry::new("subsystest_engine");
        let entries = view.entries();
        // Could be ≥ 1 if other tests registered "subsystest_engine.*" too — count the
        // exact match instead to keep the assertion stable.
        assert!(entries.iter().any(|e| e.name == "subsystest_engine.frame_x"));
    }

    #[test]
    fn completeness_report_default() {
        let r: CompletenessReport = Default::default();
        assert_eq!(r.total, 0);
        assert!(r.is_complete());
    }

    #[test]
    fn metric_kind_eq() {
        assert_eq!(MetricKind::Counter, MetricKind::Counter);
        assert_ne!(MetricKind::Counter, MetricKind::Gauge);
    }
}
