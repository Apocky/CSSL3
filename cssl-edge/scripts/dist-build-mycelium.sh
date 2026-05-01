#!/usr/bin/env bash
# dist-build-mycelium.sh
# Distribution-build pipeline for Mycelium-Desktop v0.1.0-alpha (Win-x64).
#
# Mode-default :: --apocky-action-pending (the Tauri toolchain + frontend deps
#                are NOT yet wired into the workspace; cargo-tauri + the
#                frontend's npm tree are Apocky-action items per spec/23).
#                This default mode prints a clear checklist + describes what
#                each step WILL do once enabled, then exits 0.
#
# Mode-live    :: --live (or pass --no-pending) actually invokes
#                npm install + cargo tauri build + stages the NSIS installer
#                + computes hashes + emits manifest JSON.
#
# Sovereignty discipline (per Apocky PRIME-DIRECTIVE) :
#   - NO telemetry. NO third-party API calls. NO analytics.
#   - All build output stays local until Apocky explicitly publishes.
#   - All cap-bits + audit emissions happen in-binary, not in this script.
#
# Usage :
#   bash cssl-edge/scripts/dist-build-mycelium.sh                       # checklist mode
#   bash cssl-edge/scripts/dist-build-mycelium.sh --apocky-action-pending  # explicit checklist
#   bash cssl-edge/scripts/dist-build-mycelium.sh --live                # real build (after Apocky enables Tauri-dep)
#
# Exit codes :
#   0  success (or pending-mode complete)
#   1  generic failure
#   2  prerequisite missing (cargo / cargo-tauri / npm)
#   3  source path missing
#   4  build step failed
#   5  staging / hashing failed

set -euo pipefail

# ---------------------------------------------------------------------------
# § Resolve repo root (script lives at cssl-edge/scripts/, repo is two-up).
# ---------------------------------------------------------------------------

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
CRATE_DIR="${REPO_ROOT}/compiler-rs/crates/cssl-host-mycelium-desktop"
FRONTEND_DIR="${CRATE_DIR}/frontend"
DOWNLOADS_DIR="${REPO_ROOT}/cssl-edge/public/downloads"

VERSION="0.1.0-alpha"
TARGET="windows-x64"
ARTIFACT_NAME="Mycelium-v${VERSION}-${TARGET}.exe"
MANIFEST_NAME="Mycelium-v${VERSION}-${TARGET}.manifest.json"
ARTIFACT_PATH="${DOWNLOADS_DIR}/${ARTIFACT_NAME}"
MANIFEST_PATH="${DOWNLOADS_DIR}/${MANIFEST_NAME}"

# ---------------------------------------------------------------------------
# § Mode parsing (default :: pending).
# ---------------------------------------------------------------------------

MODE="pending"
for arg in "$@"; do
  case "${arg}" in
    --apocky-action-pending) MODE="pending" ;;
    --live|--no-pending)     MODE="live" ;;
    -h|--help)
      cat <<HELP
Mycelium-Desktop dist-build pipeline (v${VERSION}, ${TARGET})

Default mode is --apocky-action-pending (prints checklist; safe to run anytime).

Flags :
  --apocky-action-pending  print Apocky's pre-flight checklist + dry-run plan (default)
  --live                   actually run the full Tauri build pipeline
  -h, --help               this help

The live build needs : Rust toolchain, cargo-tauri (>=2.0), npm/Node, and the
Tauri dep enabled in compiler-rs/crates/cssl-host-mycelium-desktop/Cargo.toml.
See cssl-edge/scripts/README-dist-build-mycelium.md for the full checklist.
HELP
      exit 0
      ;;
    *)
      echo "[dist-build-mycelium] unknown flag: ${arg}" >&2
      echo "  use --help for available options." >&2
      exit 1
      ;;
  esac
done

# ---------------------------------------------------------------------------
# § Logging helpers.
# ---------------------------------------------------------------------------

log()  { printf '[dist-build-mycelium] %s\n' "$*"; }
warn() { printf '[dist-build-mycelium] WARN  · %s\n' "$*" >&2; }
err()  { printf '[dist-build-mycelium] ERROR · %s\n' "$*" >&2; }
ok()   { printf '[dist-build-mycelium] OK    · %s\n' "$*"; }
hr()   { printf '[dist-build-mycelium] %s\n' '──────────────────────────────────────────────────────────────'; }

# ---------------------------------------------------------------------------
# § Checklist mode (default).
# ---------------------------------------------------------------------------

print_checklist() {
  hr
  log "Mycelium-Desktop dist-build · MODE = apocky-action-pending"
  log "Repo root  : ${REPO_ROOT}"
  log "Crate dir  : ${CRATE_DIR}"
  log "Frontend   : ${FRONTEND_DIR}"
  log "Downloads  : ${DOWNLOADS_DIR}"
  log "Artifact   : ${ARTIFACT_NAME} (target staging)"
  log "Manifest   : ${MANIFEST_NAME} (target staging)"
  hr
  log "Sovereignty pledge :"
  log "  · NO telemetry · NO third-party tracking · NO analytics callouts."
  log "  · This script never phones home. All hashes computed locally."
  log "  · Apocky-master-key-revoke-anytime is preserved in-binary."
  hr
  log "Apocky-action checklist (must complete BEFORE --live build) :"
  log "  [ ] 1. Uncomment in ${CRATE_DIR}/Cargo.toml :"
  log "          tauri = { version = \"2\", optional = true }"
  log "          tauri-build = { version = \"2\", optional = true }"
  log "  [ ] 2. Update the [features] tauri-shell entry to include :"
  log "          tauri-shell = [\"dep:tauri\", \"dep:tauri-build\"]"
  log "  [ ] 3. Install cargo-tauri CLI :"
  log "          cargo install tauri-cli --version \"^2.0\""
  log "  [ ] 4. Install frontend deps (Vite + React 19 + TS5) :"
  log "          (cd ${FRONTEND_DIR} && npm install)"
  log "  [ ] 5. Validate Tauri compile-pass :"
  log "          (cd ${CRATE_DIR} && cargo build --features tauri-shell --release)"
  log "  [ ] 6. (post-W10) Acquire DigiCert / Sectigo code-signing-cert (~\$300/yr)"
  log "          and configure tauri.conf.json signingIdentity."
  log "  [ ] 7. Re-run :"
  log "          bash cssl-edge/scripts/dist-build-mycelium.sh --live"
  hr
  log "What --live mode WILL do once enabled :"
  log "  step-1 : prereq check (cargo / cargo-tauri / npm) — exit 2 if any missing."
  log "  step-2 : (cd ${FRONTEND_DIR} && npm install)"
  log "  step-3 : (cd ${CRATE_DIR} && cargo tauri build --features tauri-shell)"
  log "           [bundles WebView2 frontend + Rust backend → NSIS .exe installer, ~30-50 MB]"
  log "  step-4 : stage installer → ${ARTIFACT_PATH}"
  log "  step-5 : compute SHA-256 (always) + BLAKE3 (best-effort, optional binary)"
  log "  step-6 : write manifest JSON → ${MANIFEST_PATH}"
  log "  step-7 : print human-readable summary."
  hr
  log "Status : pending-Apocky-action · exit 0 (this is the safe default)."
  ok "Checklist printed. No build attempted."
}

# ---------------------------------------------------------------------------
# § Prereq check (live mode only).
# ---------------------------------------------------------------------------

require_cmd() {
  local name="$1"
  local hint="$2"
  if ! command -v "${name}" >/dev/null 2>&1; then
    err "missing prerequisite : '${name}' not on PATH."
    err "  install hint : ${hint}"
    return 2
  fi
  ok "found ${name} :: $(command -v "${name}")"
  return 0
}

check_prereqs() {
  hr
  log "Step 1/7 · pre-requirement check"
  local missing=0
  require_cmd cargo "https://rustup.rs/ then 'rustup default stable'" || missing=1
  if ! command -v cargo-tauri >/dev/null 2>&1; then
    if cargo --list 2>/dev/null | grep -q '^    tauri$'; then
      ok "found cargo-tauri (via cargo subcommand)"
    else
      err "missing prerequisite : 'cargo tauri' subcommand not registered."
      err "  install hint : cargo install tauri-cli --version \"^2.0\""
      missing=1
    fi
  else
    ok "found cargo-tauri :: $(command -v cargo-tauri)"
  fi
  require_cmd npm "https://nodejs.org/ (LTS 20.x or 22.x) — Node ships with npm" || missing=1
  if [ "${missing}" -ne 0 ]; then
    err "one-or-more prerequisites missing. Aborting (exit 2)."
    exit 2
  fi
  ok "all prerequisites present."
}

# ---------------------------------------------------------------------------
# § Source-tree check.
# ---------------------------------------------------------------------------

check_source_tree() {
  hr
  log "Step · source-tree validation"
  if [ ! -d "${CRATE_DIR}" ]; then
    err "crate dir not found : ${CRATE_DIR}"
    err "  the parallel agent should have scaffolded this. retry after sync."
    exit 3
  fi
  if [ ! -f "${CRATE_DIR}/Cargo.toml" ]; then
    err "Cargo.toml not found in : ${CRATE_DIR}"
    exit 3
  fi
  if [ ! -d "${FRONTEND_DIR}" ]; then
    err "frontend dir not found : ${FRONTEND_DIR}"
    err "  the Tauri scaffold needs frontend/ with Vite config."
    exit 3
  fi
  ok "source tree present."
}

# ---------------------------------------------------------------------------
# § Frontend deps.
# ---------------------------------------------------------------------------

run_npm_install() {
  hr
  log "Step 2/7 · npm install in ${FRONTEND_DIR}"
  if ! ( cd "${FRONTEND_DIR}" && npm install --no-audit --no-fund --loglevel=error ); then
    err "npm install failed in ${FRONTEND_DIR}"
    err "  check Node version (LTS 20.x / 22.x) + frontend/package.json validity."
    exit 4
  fi
  ok "frontend deps installed."
}

# ---------------------------------------------------------------------------
# § Tauri build.
# ---------------------------------------------------------------------------

run_tauri_build() {
  hr
  log "Step 3/7 · cargo tauri build --features tauri-shell"
  if ! ( cd "${CRATE_DIR}" && cargo tauri build --features tauri-shell ); then
    err "cargo tauri build failed."
    err "  common causes :"
    err "    · Tauri dep still commented out in Cargo.toml (see checklist step 1)"
    err "    · WebView2 SDK headers missing on host"
    err "    · NSIS not on PATH (Tauri auto-downloads on first run)"
    err "    · frontend/dist not produced by 'npm run build' (Vite config issue)"
    exit 4
  fi
  ok "Tauri build succeeded."
}

# ---------------------------------------------------------------------------
# § Stage installer.
# ---------------------------------------------------------------------------

stage_installer() {
  hr
  log "Step 4/7 · stage NSIS installer → ${ARTIFACT_PATH}"
  mkdir -p "${DOWNLOADS_DIR}"
  # Tauri 2.x bundle locations (NSIS on Windows) :
  #   target/release/bundle/nsis/<productName>_<version>_x64-setup.exe
  local nsis_dir="${CRATE_DIR}/target/release/bundle/nsis"
  if [ ! -d "${nsis_dir}" ]; then
    err "NSIS bundle dir not found : ${nsis_dir}"
    err "  cargo tauri build did not emit an NSIS installer."
    exit 5
  fi
  local found
  found="$(find "${nsis_dir}" -maxdepth 1 -type f -name '*-setup.exe' | head -n 1)"
  if [ -z "${found}" ]; then
    err "no *-setup.exe found in ${nsis_dir}"
    exit 5
  fi
  cp -f "${found}" "${ARTIFACT_PATH}"
  ok "staged : ${ARTIFACT_PATH}"
}

# ---------------------------------------------------------------------------
# § Hashes (SHA-256 mandatory · BLAKE3 best-effort).
# ---------------------------------------------------------------------------

compute_sha256() {
  local target="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "${target}" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "${target}" | awk '{print $1}'
  elif command -v certutil >/dev/null 2>&1; then
    # Windows fallback (Git-Bash often has certutil via WinPATH).
    certutil -hashfile "${target}" SHA256 | sed -n '2p' | tr -d ' \r\n'
  else
    err "no SHA-256 tool found (sha256sum / shasum / certutil)."
    return 1
  fi
}

compute_blake3() {
  local target="$1"
  if command -v blake3sum >/dev/null 2>&1; then
    blake3sum "${target}" | awk '{print $1}'
  elif command -v b3sum >/dev/null 2>&1; then
    b3sum "${target}" | awk '{print $1}'
  else
    return 1
  fi
}

emit_hashes() {
  hr
  log "Step 5/7 · compute hashes"
  SHA256_HEX="$(compute_sha256 "${ARTIFACT_PATH}")" || {
    err "SHA-256 computation failed."
    exit 5
  }
  printf '%s  %s\n' "${SHA256_HEX}" "${ARTIFACT_NAME}" > "${ARTIFACT_PATH}.sha256"
  ok "SHA-256 : ${SHA256_HEX}"
  ok "wrote   : ${ARTIFACT_PATH}.sha256"
  if BLAKE3_HEX="$(compute_blake3 "${ARTIFACT_PATH}")"; then
    printf '%s  %s\n' "${BLAKE3_HEX}" "${ARTIFACT_NAME}" > "${ARTIFACT_PATH}.blake3"
    ok "BLAKE3  : ${BLAKE3_HEX}"
    ok "wrote   : ${ARTIFACT_PATH}.blake3"
  else
    BLAKE3_HEX=""
    warn "blake3sum / b3sum not found — skipping BLAKE3 (sha256 still emitted)."
    warn "  install hint : cargo install b3sum"
  fi
}

# ---------------------------------------------------------------------------
# § Manifest JSON.
# ---------------------------------------------------------------------------

emit_manifest() {
  hr
  log "Step 6/7 · emit manifest JSON → ${MANIFEST_PATH}"
  local size_bytes
  if [ -f "${ARTIFACT_PATH}" ]; then
    size_bytes="$(wc -c < "${ARTIFACT_PATH}" | tr -d ' \r\n')"
  else
    size_bytes="0"
  fi
  local build_time
  build_time="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  cat > "${MANIFEST_PATH}" <<JSON
{
  "name": "Mycelium-Desktop",
  "version": "${VERSION}",
  "target": "${TARGET}",
  "artifact": "${ARTIFACT_NAME}",
  "size_bytes": ${size_bytes},
  "sha256": "${SHA256_HEX}",
  "blake3": "${BLAKE3_HEX}",
  "build_time_iso": "${build_time}",
  "signing_status": "unsigned-alpha",
  "sovereignty_pledge": "no-telemetry · no-third-party-tracking · no-analytics · no-update-callback-at-install",
  "auto_update": "opt-in-only-via-cssl-host-hotfix-stream"
}
JSON
  ok "manifest written."
}

# ---------------------------------------------------------------------------
# § Summary.
# ---------------------------------------------------------------------------

print_summary() {
  hr
  log "Step 7/7 · summary"
  log "  artifact   : ${ARTIFACT_PATH}"
  log "  size       : $(wc -c < "${ARTIFACT_PATH}" 2>/dev/null || echo "?") bytes"
  log "  sha256     : ${SHA256_HEX}"
  log "  blake3     : ${BLAKE3_HEX:-<not-computed>}"
  log "  manifest   : ${MANIFEST_PATH}"
  log "  signing    : unsigned-alpha (cert-acquisition is post-W10 Apocky-action)"
  hr
  ok "Mycelium-v${VERSION}-${TARGET} dist-build COMPLETE."
}

# ---------------------------------------------------------------------------
# § main
# ---------------------------------------------------------------------------

main() {
  if [ "${MODE}" = "pending" ]; then
    print_checklist
    exit 0
  fi
  log "MODE = live · attempting full build pipeline"
  check_prereqs
  check_source_tree
  run_npm_install
  run_tauri_build
  stage_installer
  emit_hashes
  emit_manifest
  print_summary
}

main "$@"
