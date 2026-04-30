//! § loa-runtime — binary entry-point for the LoA-v13 stage-0 host.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-LOA-HOST-1 (W-LOA-host-render) : opens a window the user can
//! navigate. Pure-Rust stage-0 shell ; per Apocky greenlight :
//!   "do whatever we need to in order to get a working test room I can navigate."
//!
//! § ATTESTATION
//!   There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody.

#![forbid(unsafe_code)]

fn main() {
    eprintln!("§ LoA-host starting · winit + wgpu render");
    if let Err(e) = loa_host::run_engine() {
        eprintln!("§ loa-host : engine returned IO error : {e}");
        std::process::exit(1);
    }
}
