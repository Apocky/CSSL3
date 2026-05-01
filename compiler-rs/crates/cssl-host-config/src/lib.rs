//! § cssl-host-config — typed runtime-config loader for the LoA host.
//!
//! § I> LOAD ORDER (host bootstrap)
//!   1. start with `LoaConfig::default()`                    ← in-memory defaults
//!   2. `let mut cfg = LoaConfig::load_from_file("loa.config.json")?` ← optional
//!   3. `cfg.apply_env_overrides()`                          ← LOA_* env-vars
//!   4. `cfg.validate()?`                                    ← aggregate errors
//!   5. consume sub-configs : `cfg.render`, `cfg.network`, `cfg.policy`
//!
//! § sections
//!   - `render`  : window resolution, MSAA, HDR exposure, ACES, CFER alpha, FPS, vsync
//!   - `network` : MCP port, HTTP capability bitset, Supabase URL+anon-key
//!   - `policy`  : sovereign-cap hex, audit + telemetry log dirs, license-policy
//!
//! § env-override surface
//!   `LOA_MCP_PORT`, `LOA_HDR_EXPOSURE`, `LOA_TARGET_FPS`, `LOA_VSYNC`,
//!   `LOA_HTTP_CAPS`, `LOA_AUDIT_LOG_DIR`, `LOA_SUPABASE_URL`,
//!   `LOA_SUPABASE_ANON_KEY`.
//!
//! § JSON-only (NO toml/yaml dep)
//! Per workspace-policy : serde_json is already pinned ; toml/yaml would add
//! additional surface area. JSON is sufficient for human + machine editing.
//!
//! § non-panic guarantee
//! Library never panics under valid input. Validation failures return
//! `ConfigErr` ; serde-failures wrap into `ConfigErr::JsonParse` ; io
//! failures wrap into `ConfigErr::Io`.

#![forbid(unsafe_code)]

pub mod loader;
pub mod network;
pub mod policy;
pub mod render;

pub use loader::{ConfigErr, LoaConfig};
pub use network::NetworkConfig;
pub use policy::{LicensePolicyChoice, PolicyConfig};
pub use render::RenderConfig;
