//! Schema migrations : (from, to, id) records composed into chains.

use crate::schema::SchemaVersion;

/// One migration step : from `before` → `after` tagged with an identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaMigration {
    /// Source schema version.
    pub before: SchemaVersion,
    /// Target schema version.
    pub after: SchemaVersion,
    /// Migration identifier (human-readable, e.g., `"add_energy_counter_field"`).
    pub id: String,
    /// Optional description.
    pub description: Option<String>,
}

impl SchemaMigration {
    /// Build a named migration.
    #[must_use]
    pub fn new(before: SchemaVersion, after: SchemaVersion, id: impl Into<String>) -> Self {
        Self {
            before,
            after,
            id: id.into(),
            description: None,
        }
    }

    /// Attach a description.
    #[must_use]
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }
}

/// Ordered list of migrations forming a chain between two schema versions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MigrationChain {
    migrations: Vec<SchemaMigration>,
}

impl MigrationChain {
    /// Empty chain.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a migration. Panics if the migration's `before` doesn't match the tail's `after`.
    pub fn push(&mut self, m: SchemaMigration) {
        if let Some(tail) = self.migrations.last() {
            assert_eq!(
                tail.after, m.before,
                "migration chain broken : tail.after ({}) != m.before ({})",
                tail.after, m.before
            );
        }
        self.migrations.push(m);
    }

    /// True iff empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.migrations.is_empty()
    }

    /// Number of migrations.
    #[must_use]
    pub fn len(&self) -> usize {
        self.migrations.len()
    }

    /// The first `before` version in the chain.
    #[must_use]
    pub fn start_version(&self) -> Option<SchemaVersion> {
        self.migrations.first().map(|m| m.before)
    }

    /// The last `after` version in the chain.
    #[must_use]
    pub fn end_version(&self) -> Option<SchemaVersion> {
        self.migrations.last().map(|m| m.after)
    }

    /// Iterate migrations in order.
    pub fn iter(&self) -> impl Iterator<Item = &SchemaMigration> {
        self.migrations.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::{MigrationChain, SchemaMigration};
    use crate::schema::SchemaVersion;

    #[test]
    fn migration_construct() {
        let a = SchemaVersion::new(1, 0);
        let b = SchemaVersion::new(1, 1);
        let m = SchemaMigration::new(a, b, "add_field").with_description("adds foo");
        assert_eq!(m.before, a);
        assert_eq!(m.after, b);
        assert_eq!(m.id, "add_field");
        assert_eq!(m.description.as_deref(), Some("adds foo"));
    }

    #[test]
    fn empty_chain_shape() {
        let c = MigrationChain::new();
        assert!(c.is_empty());
        assert_eq!(c.len(), 0);
        assert_eq!(c.start_version(), None);
        assert_eq!(c.end_version(), None);
    }

    #[test]
    fn chain_push_sequential() {
        let v0 = SchemaVersion::new(1, 0);
        let v1 = SchemaVersion::new(1, 1);
        let v2 = SchemaVersion::new(1, 2);
        let mut c = MigrationChain::new();
        c.push(SchemaMigration::new(v0, v1, "m1"));
        c.push(SchemaMigration::new(v1, v2, "m2"));
        assert_eq!(c.len(), 2);
        assert_eq!(c.start_version(), Some(v0));
        assert_eq!(c.end_version(), Some(v2));
    }

    #[test]
    #[should_panic(expected = "migration chain broken")]
    fn chain_push_broken_panics() {
        let v0 = SchemaVersion::new(1, 0);
        let v1 = SchemaVersion::new(1, 1);
        let v3 = SchemaVersion::new(1, 3);
        let mut c = MigrationChain::new();
        c.push(SchemaMigration::new(v0, v1, "m1"));
        // Skipping v2 : m2 should start at v1, not v3.
        c.push(SchemaMigration::new(v3, SchemaVersion::new(1, 4), "m2"));
    }

    #[test]
    fn chain_iter_preserves_order() {
        let v0 = SchemaVersion::new(1, 0);
        let v1 = SchemaVersion::new(1, 1);
        let v2 = SchemaVersion::new(1, 2);
        let mut c = MigrationChain::new();
        c.push(SchemaMigration::new(v0, v1, "m1"));
        c.push(SchemaMigration::new(v1, v2, "m2"));
        let ids: Vec<_> = c.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, vec!["m1", "m2"]);
    }
}
