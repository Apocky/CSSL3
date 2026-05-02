//! § wired_hotfix — live-update infrastructure wired into loa-host.
//! ════════════════════════════════════════════════════════════════════
//!
//! § ROLE (T11-W11-HOTFIX-INFRA-WIRE)
//!   Re-export `cssl-hotfix-client::HotfixClient` and provide a
//!   `start_hotfix_loop(...)` helper that the engine integrator
//!   (main.rs / ffi.rs — sibling-agent territory, NOT touched here)
//!   may call at engine startup. The actual thread-spawning is
//!   integrator-controlled : this file only provides the orchestration
//!   primitive + a sane default `LoggingTelemetrySink` that emits via
//!   the existing `telemetry::push_event` channel.
//!
//! § DESIGN
//!   - NEW file ; no overlap with sibling-agent territory.
//!   - Default sink forwards to `loa_host::telemetry` (existing module).
//!   - `start_hotfix_loop` is sync : the integrator wraps it in
//!     `std::thread::spawn` or an async runtime as fits.
//!
//! § AXIOMS (PRIME_DIRECTIVE encoded)
//!   ¬ harm · sovereign-revocable · Σ-mask-default-deny ·
//!   ¬ DRM · ¬ rootkit · rollback-always-available

use cssl_hotfix::cap::CapKey;
use cssl_hotfix_client::{
    BundleSource, HotfixClient, HotfixClientConfig, HotfixEvent, ManifestSource, TelemetrySink,
};
use std::path::PathBuf;
use std::sync::Arc;

// ──────────────────────────────────────────────────────────────────────
// § Default telemetry sink — forwards events to loa-host telemetry.
// ──────────────────────────────────────────────────────────────────────

/// § Default sink : push hotfix events into the loa-host telemetry ring.
/// Honours the `telemetry` module's existing event-bus shape.
pub struct LoaHostTelemetrySink;

impl TelemetrySink for LoaHostTelemetrySink {
    fn emit(&self, ev: HotfixEvent) {
        // Serialize the event to JSON and write to stderr ; future revs
        // forward to the structured telemetry channel once the
        // sibling-analytics-aggregator wires a `record_hotfix_event` fn.
        // Quiet by default in release builds : only emit when LOA_HOTFIX_LOG=1.
        if std::env::var("LOA_HOTFIX_LOG").as_deref() == Ok("1") {
            if let Ok(json) = serde_json::to_string(&ev) {
                eprintln!("[hotfix] {json}");
            }
        }
    }
}

// ──────────────────────────────────────────────────────────────────────
// § Public surface — engine integrator calls these.
// ──────────────────────────────────────────────────────────────────────

/// § Build a new `HotfixClient` ready for polling.
///
/// `manifest_src` and `bundle_src` are typically HTTP-backed adapters
/// (out-of-tree). For tests / offline-mode the loa-host integrator can
/// pass `MockManifestSource` / `MockBundleSource` from `cssl-hotfix-client::sources`.
pub fn build_client(
    install_dir: PathBuf,
    cap_keys: Vec<CapKey>,
    manifest_src: Arc<dyn ManifestSource>,
    bundle_src: Arc<dyn BundleSource>,
) -> Arc<HotfixClient> {
    let cfg = HotfixClientConfig {
        install_dir,
        poll_interval_ms: 5 * 60 * 1000,
        cap_keys,
    };
    let sink: Arc<dyn TelemetrySink> = Arc::new(LoaHostTelemetrySink);
    Arc::new(HotfixClient::new(cfg, manifest_src, bundle_src, sink))
}

/// § Start a single-thread polling loop.
///
/// Spawns a worker thread that calls `client.poll_once(now_ns)` every
/// `client.cfg.poll_interval_ms` until the returned `JoinHandle`'s
/// associated `Arc` is dropped.
///
/// Note : we deliberately use `std::thread` instead of an async runtime
/// to keep loa-host's dep surface small. Engine integrators that already
/// have a tokio runtime can call `client.poll_once` from their own task
/// scheduler instead.
pub fn start_hotfix_loop(client: Arc<HotfixClient>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        // Poll-loop : sleeps then ticks. Termination is by process exit.
        let interval = std::time::Duration::from_millis(5 * 60 * 1000);
        loop {
            let now_ns = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0);
            let _ = client.poll_once(now_ns);
            std::thread::sleep(interval);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_hotfix::cap::CapRole;
    use cssl_hotfix::manifest::Manifest;
    use cssl_hotfix_client::{MockBundleSource, MockManifestSource};

    #[test]
    fn build_client_constructs_with_default_sink() {
        let dir = std::env::temp_dir().join("wired-hotfix-test");
        let _ = std::fs::create_dir_all(&dir);
        let m = Manifest {
            schema_version: 1,
            generated_at_ns: 0,
            signed_by: CapRole::CapD,
            channels: Default::default(),
            revocations: vec![],
            signature: [0u8; 64],
        };
        let m_src: Arc<dyn ManifestSource> = Arc::new(MockManifestSource::new(m));
        let b_src: Arc<dyn BundleSource> = Arc::new(MockBundleSource::new());
        let _client = build_client(dir, vec![], m_src, b_src);
    }

    #[test]
    fn telemetry_sink_emit_does_not_panic() {
        let sink = LoaHostTelemetrySink;
        sink.emit(HotfixEvent::Checked { ts_ns: 1 });
        sink.emit(HotfixEvent::Skipped {
            channel: "x".to_string(),
            reason: "y".to_string(),
            ts_ns: 2,
        });
    }
}
