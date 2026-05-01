# dist-build-mycelium · Mycelium-Desktop alpha-installer pipeline

Build, hash, and stage the Mycelium-Desktop v0.1.0-alpha NSIS installer for
Windows-x64 onto the `cssl-edge/public/downloads/` tree (which Vercel serves
under `apocky.com/downloads/`).

> Spec reference: `specs/grand-vision/23_MYCELIUM_DESKTOP.csl` § ROADMAP § wave-10-C2 + § DISTRIBUTION

## What this script does

The pipeline (live mode) runs seven steps end-to-end:

1. **Pre-requirement check** — verifies `cargo`, `cargo tauri`, and `npm` are
   on `PATH`. Emits a clear install hint per missing tool and exits with code
   `2` if anything is missing.
2. **Frontend deps** — `npm install --no-audit --no-fund` inside
   `compiler-rs/crates/cssl-host-mycelium-desktop/frontend/`.
3. **Tauri build** — `cargo tauri build --features tauri-shell` from the crate
   root. Tauri 2.x bundles the Vite-built React frontend with the Rust
   backend into a single NSIS installer.
4. **Stage installer** — copies
   `target/release/bundle/nsis/*-setup.exe` to
   `cssl-edge/public/downloads/Mycelium-v0.1.0-alpha-windows-x64.exe`.
5. **Hashes** — computes SHA-256 (always) and BLAKE3 (best-effort, skipped
   with a warning if `b3sum`/`blake3sum` is not on `PATH`). Emits
   `<artifact>.sha256` and `<artifact>.blake3` sidecar files.
6. **Manifest JSON** — writes
   `Mycelium-v0.1.0-alpha-windows-x64.manifest.json` with `name`, `version`,
   `target`, `sha256`, `blake3`, `size_bytes`, `build_time_iso`,
   `signing_status` (`unsigned-alpha`), and explicit sovereignty-pledge keys.
7. **Summary** — human-readable recap of the artifact paths and hashes.

Default mode is `--apocky-action-pending`: prints a checklist and exits `0`
without attempting a build. This is intentional — neither `cargo-tauri` nor
the `tauri` Cargo dep is wired into the workspace yet (per spec/23 wave-10-C2:
the Tauri runtime is feature-gated and Apocky enables it manually).

## Apocky-actions-pending checklist

The live build needs each of these completed (in order):

- [ ] **1. Enable the Tauri Cargo dep** — uncomment in
      `compiler-rs/crates/cssl-host-mycelium-desktop/Cargo.toml`:
      ```toml
      tauri       = { version = "2", optional = true }
      tauri-build = { version = "2", optional = true }
      ```
- [ ] **2. Wire the feature flag** — update the `[features]` block:
      ```toml
      tauri-shell = ["dep:tauri", "dep:tauri-build"]
      ```
- [ ] **3. Install the Tauri CLI** —
      ```sh
      cargo install tauri-cli --version "^2.0"
      ```
- [ ] **4. Install frontend deps** —
      ```sh
      cd compiler-rs/crates/cssl-host-mycelium-desktop/frontend
      npm install
      ```
      (Vite 5 + React 19 + TypeScript 5 — versions per spec/23 § TECH-STACK.)
- [ ] **5. Validate Tauri compile-pass** —
      ```sh
      cd compiler-rs/crates/cssl-host-mycelium-desktop
      cargo build --features tauri-shell --release
      ```
- [ ] **6. (post-W10) Acquire a code-signing certificate** —
      DigiCert or Sectigo OV-cert (~$300/year). Without one, Windows shows
      the SmartScreen "Unknown publisher" warning at install time. Alpha
      testers can click-through; v1.0 should ship signed.
- [ ] **7. (post-W10) Configure NSIS signing** — set `signingIdentity` in
      `compiler-rs/crates/cssl-host-mycelium-desktop/tauri.conf.json` and
      pass `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
      via env-var at build time (never commit to git).

## Build commands

```sh
# Default — prints checklist (safe; no build attempted):
bash cssl-edge/scripts/dist-build-mycelium.sh

# Explicit checklist mode:
bash cssl-edge/scripts/dist-build-mycelium.sh --apocky-action-pending

# Real build (after the checklist above is complete):
bash cssl-edge/scripts/dist-build-mycelium.sh --live

# Windows .cmd wrapper (auto-detects Git-Bash, falls back to WSL):
cssl-edge\scripts\dist-build-mycelium.cmd --live
```

## Expected outputs

After a successful `--live` run:

| Path                                                                                     | Contents                                                          |
| ---------------------------------------------------------------------------------------- | ----------------------------------------------------------------- |
| `cssl-edge/public/downloads/Mycelium-v0.1.0-alpha-windows-x64.exe`                       | NSIS installer (~30–50 MB)                                        |
| `cssl-edge/public/downloads/Mycelium-v0.1.0-alpha-windows-x64.exe.sha256`                | SHA-256 sidecar (`<hex>  <filename>`)                             |
| `cssl-edge/public/downloads/Mycelium-v0.1.0-alpha-windows-x64.exe.blake3`                | BLAKE3 sidecar (only if `b3sum`/`blake3sum` is installed)         |
| `cssl-edge/public/downloads/Mycelium-v0.1.0-alpha-windows-x64.manifest.json`             | JSON manifest with name/version/target/hashes/size/build-time     |

The manifest JSON shape:

```jsonc
{
  "name": "Mycelium-Desktop",
  "version": "0.1.0-alpha",
  "target": "windows-x64",
  "artifact": "Mycelium-v0.1.0-alpha-windows-x64.exe",
  "size_bytes": 12345678,
  "sha256": "...",
  "blake3": "...",
  "build_time_iso": "2026-05-01T12:34:56Z",
  "signing_status": "unsigned-alpha",
  "sovereignty_pledge": "no-telemetry · no-third-party-tracking · no-analytics · no-update-callback-at-install",
  "auto_update": "opt-in-only-via-cssl-host-hotfix-stream"
}
```

## Troubleshooting

### `cargo tauri build` fails with "no `tauri` dep found"

You skipped checklist step 1 or 2. Re-check `Cargo.toml`.

### `cargo tauri` not found

```sh
cargo install tauri-cli --version "^2.0"
```

If the install itself fails, your toolchain is too old — `cargo update` and
ensure `rustc --version` is at least 1.85.

### `npm install` errors in `frontend/`

- Confirm Node LTS (20.x or 22.x). Older Node breaks Vite 5.
- Delete `frontend/node_modules` and `frontend/package-lock.json`, retry.

### Tauri build complains about WebView2

WebView2 runtime is preinstalled on Windows 10 1803+ and Windows 11. Tauri's
NSIS bundle declares it as a dependency and downloads the Evergreen runtime
at install time on hosts that lack it. Build-time errors usually mean the
**WebView2 SDK headers** are missing — run `cargo tauri info` and follow
the prompts.

### NSIS not on PATH

Tauri 2.x downloads NSIS automatically on first build. If your network is
restricted, install NSIS manually from <https://nsis.sourceforge.io/> and
ensure `makensis` is on `PATH`.

### `blake3sum` / `b3sum` not found

Optional. Install with `cargo install b3sum`. The script gracefully
continues with SHA-256 only and warns visibly.

### "Unknown publisher" SmartScreen warning at install time

Expected for unsigned alpha builds (`signing_status: unsigned-alpha`). The
fix is checklist step 6 (acquire a code-signing certificate). Alpha testers
can click "More info → Run anyway".

## Sovereignty discipline

Per Apocky's `PRIME_DIRECTIVE`:

- **No telemetry.** This script never phones home, never reports build stats,
  never pings analytics. All hashes are computed locally.
- **No third-party tracking.** The installer itself ships with the same
  pledge — no Sentry, no Segment, no Mixpanel, no Google Analytics, no
  Amplitude. The runtime audit log is written **locally** via
  `cssl-host-audit-emit` and never transmitted.
- **No update-server-callback at install.** The NSIS installer does NOT
  contact any update server. Auto-update is **opt-in only**, gated through
  `cssl-host-hotfix-stream` which Apocky controls; a fresh-install does not
  even know an update server exists.
- **Apocky-master-key revoke-anytime.** All cap-bits in the runtime honor
  the master-key revocation primitive; this is a build-time invariant and
  is asserted by the test suite in `cssl-host-mycelium-desktop`.
- **Signed only when Apocky owns the cert.** Code-signing is post-W10 and
  uses a cert in Apocky's name (no Anthropic / no third-party signer).

## File inventory

- `cssl-edge/scripts/dist-build-mycelium.sh` — the pipeline (Bash, ~300 LOC).
- `cssl-edge/scripts/dist-build-mycelium.cmd` — Windows wrapper (~30 LOC).
- `cssl-edge/scripts/README-dist-build-mycelium.md` — this file.

## Related

- `specs/grand-vision/23_MYCELIUM_DESKTOP.csl` — full Mycelium design spec.
- `specs/grand-vision/25_W10_MYCELIUM_RETRO.csl` — wave-10 retrospective.
- `compiler-rs/crates/cssl-host-mycelium-desktop/` — the Tauri crate (parallel agent).
- `cssl-edge/pages/mycelium/` — the apocky.com/mycelium landing + download routes.
