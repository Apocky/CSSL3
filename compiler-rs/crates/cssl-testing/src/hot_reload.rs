//! Hot-reload invariance oracle (`@hot_reload_test`) — `cssl-persist` + schema-migration.
//!
//! § SPEC   : `specs/23_TESTING.csl` § oracle-modes • hot-reload-invariance +
//!            `specs/18_ORTHOPERSIST.csl` schema-migration.
//! § ROLE   : record state S0 → hot-reload → record state S1; assert S0 ≡ S1
//!            modulo-schema-migration. Failure ⇒ migration-incorrect or `@transient`-misapplied.
//! § STATUS : T11 stub — implementation pending.

/// Config for the `@hot_reload_test` oracle.
#[derive(Debug, Default, Clone)]
pub struct Config {
    /// Path to the reload-target file (new schema version under test).
    pub reload_source: String,
}

/// Outcome of running the `@hot_reload_test` oracle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Stage0 stub — body populated at T11 (requires `cssl-persist`).
    Stage0Unimplemented,
    /// State preserved across reload.
    Ok,
    /// State diverged; migration or `@transient` misapplied.
    StateDiverged {
        field_path: String,
        before: String,
        after: String,
    },
}

/// Dispatcher trait for `@hot_reload_test` oracle.
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
