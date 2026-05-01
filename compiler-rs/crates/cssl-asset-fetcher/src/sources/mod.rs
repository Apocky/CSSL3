//! § sources — adapters per provider.
//! ════════════════════════════════════
//!
//! Each module implements [`crate::AssetSource`] for one provider. Stage-0
//! adapters ship :
//!   - sketchfab : mocked-with-real-license-shape (TLS/HTTP not yet wired)
//!   - polyhaven : mocked-with-real-license-shape
//!   - kenney    : static catalog of 100+ CC0 packs (truth-data : URLs +
//!                 author = Kenney Vleugels are real)
//!   - quaternius: static catalog of CC0 stylized model packs
//!   - opengameart: mocked CC0 + CC-BY representative entries
//!
//! When stage-0 promotes to wire-side fetch, the adapters' `fetch()`
//! routes through `cssl_rt::net::TcpStream` (already cap-gated) and the
//! TLS shim added in a follow-up slice. The trait surface is unchanged ;
//! only the body of `fetch()` evolves.

pub mod kenney;
pub mod opengameart;
pub mod polyhaven;
pub mod quaternius;
pub mod sketchfab;
