#!/usr/bin/env bash
# dist-build-apocrypha-desktop.sh
#
# Builds the Apocrypha desktop client (Windows x64), stages the NSIS installer
# into cssl-edge/public/downloads/, and writes the release manifest the
# /download/apocrypha page reads.
#
# Follows the shape of dist-build-mycelium.sh, with one difference: this one is
# live by default, because its toolchain is present and its crate builds. The
# manifest it writes is the only thing that makes a download appear on the site,
# and `loadDesktopRelease` re-checks the hash at page-build time — so a manifest
# written here is a claim the site verifies rather than trusts.
#
# Sovereignty discipline (per Apocky PRIME-DIRECTIVE):
#   · no telemetry · no third-party calls · every hash computed locally
#   · nothing is published until Apocky deploys the site
#
# Usage:
#   bash cssl-edge/scripts/dist-build-apocrypha-desktop.sh            # build + stage
#   bash cssl-edge/scripts/dist-build-apocrypha-desktop.sh --stage-only
#   bash cssl-edge/scripts/dist-build-apocrypha-desktop.sh --help
#
# Exit codes: 0 ok · 2 prerequisite missing · 3 source missing · 4 build failed
#             5 staging/hashing failed

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
CRATE_DIR="${REPO_ROOT}/apps/apocrypha-desktop"
FRONTEND_DIR="${CRATE_DIR}/frontend"
DOWNLOADS_DIR="${REPO_ROOT}/cssl-edge/public/downloads"
RELEASE_DIR="${REPO_ROOT}/cssl-edge/public/releases/apocrypha-desktop"

VERSION="$(sed -n 's/^version *= *"\([0-9.]*\)".*/\1/p' "${CRATE_DIR}/Cargo.toml" | head -n 1)"
TARGET="windows-x64"
ARTIFACT_NAME="Apocrypha-Desktop-${VERSION}-${TARGET}.exe"
ARTIFACT_PATH="${DOWNLOADS_DIR}/${ARTIFACT_NAME}"
MANIFEST_PATH="${RELEASE_DIR}/manifest.json"

MODE="build"
for arg in "$@"; do
  case "${arg}" in
    --stage-only) MODE="stage" ;;
    -h|--help)
      sed -n '2,28p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *) echo "[dist-apocrypha-desktop] unknown flag: ${arg}" >&2; exit 1 ;;
  esac
done

log() { printf '[dist-apocrypha-desktop] %s\n' "$*"; }
err() { printf '[dist-apocrypha-desktop] ERROR · %s\n' "$*" >&2; }
ok()  { printf '[dist-apocrypha-desktop] OK    · %s\n' "$*"; }
hr()  { printf '[dist-apocrypha-desktop] %s\n' '──────────────────────────────────────────────'; }

check_prereqs() {
  local missing=0
  for tool in cargo npm; do
    if command -v "${tool}" >/dev/null 2>&1; then
      ok "found ${tool} :: $(command -v "${tool}")"
    else
      err "missing prerequisite: '${tool}' not on PATH."
      missing=1
    fi
  done
  if ! cargo --list 2>/dev/null | grep -q '^ *tauri$'; then
    err "missing prerequisite: 'cargo tauri' subcommand not registered."
    err "  install hint: cargo install tauri-cli --version \"^2.0\""
    missing=1
  else
    ok "found cargo tauri"
  fi
  [ "${missing}" -eq 0 ] || exit 2
  [ -f "${CRATE_DIR}/Cargo.toml" ] || { err "crate not found: ${CRATE_DIR}"; exit 3; }
  [ -d "${FRONTEND_DIR}" ] || { err "frontend not found: ${FRONTEND_DIR}"; exit 3; }
}

run_tests() {
  hr; log "Step · tests (offline)"
  ( cd "${FRONTEND_DIR}" && npm run --silent test ) || { err "frontend tests failed."; exit 4; }
  ( cd "${CRATE_DIR}" && cargo test --release --quiet ) || { err "crate tests failed."; exit 4; }
  ok "tests passed."
}

run_build() {
  hr; log "Step · npm install"
  ( cd "${FRONTEND_DIR}" && npm install --no-audit --no-fund --loglevel=error ) || { err "npm install failed."; exit 4; }
  hr; log "Step · cargo tauri build"
  ( cd "${CRATE_DIR}" && cargo tauri build ) || { err "cargo tauri build failed."; exit 4; }
  ok "installer built."
}

# Turns the two mechanical release checks into observations rather than claims.
# `launch` and `service_configuration` are the pair the distribution contract
# insists on before a build may be offered, so they are measured here every run.
LAUNCH_CHECK="pending"
SERVICE_CHECK="pending"

observe_runtime() {
  hr; log "Step · runtime observation"
  local exe="${CRATE_DIR}/target/release/apocrypha-desktop.exe"
  [ -f "${exe}" ] || { err "built executable not found: ${exe}"; exit 5; }

  if ( cd "${CRATE_DIR}" && cargo test --release --quiet -- --ignored ) >/dev/null 2>&1; then
    SERVICE_CHECK="passed"
    ok "service configuration : the live contract check passed"
  else
    err "the live service configuration check did not pass"
  fi

  "${exe}" &
  local pid=$!
  sleep 8
  if kill -0 "${pid}" 2>/dev/null; then
    LAUNCH_CHECK="passed"
    ok "launch : the application stayed running"
  else
    err "the application exited during the launch check"
  fi
  kill "${pid}" 2>/dev/null || true
  wait "${pid}" 2>/dev/null || true

  if [ "${LAUNCH_CHECK}" != "passed" ] || [ "${SERVICE_CHECK}" != "passed" ]; then
    err "refusing to publish a build that did not start or could not reach the service."
    exit 4
  fi
}

stage_installer() {
  hr; log "Step · stage installer → ${ARTIFACT_PATH}"
  mkdir -p "${DOWNLOADS_DIR}" "${RELEASE_DIR}"
  local nsis_dir="${CRATE_DIR}/target/release/bundle/nsis"
  local found
  found="$(find "${nsis_dir}" -maxdepth 1 -type f -name '*-setup.exe' 2>/dev/null | head -n 1)"
  [ -n "${found}" ] || { err "no *-setup.exe under ${nsis_dir}"; exit 5; }
  cp -f "${found}" "${ARTIFACT_PATH}"
  ok "staged from $(basename "${found}")"
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | awk '{print $1}'
  else err "no SHA-256 tool found."; return 1
  fi
}

emit_manifest() {
  hr; log "Step · hashes + manifest"
  local digest bytes
  digest="$(sha256_of "${ARTIFACT_PATH}")" || exit 5
  bytes="$(wc -c < "${ARTIFACT_PATH}" | tr -d ' \r\n')"
  # The sidecar format is asserted byte-for-byte by loadDesktopRelease.
  printf '%s  %s\n' "${digest}" "${ARTIFACT_NAME}" > "${ARTIFACT_PATH}.sha256"

  # Verification results are carried forward from the existing manifest when one
  # is present, so a rebuild never silently upgrades a check nobody re-ran.
  local sign_in="pending" install_check="pending"
  if [ -f "${MANIFEST_PATH}" ]; then
    sign_in="$(sed -n 's/.*"account_sign_in_and_chat" *: *"\([a-z_]*\)".*/\1/p' "${MANIFEST_PATH}" | head -n 1)"
    install_check="$(sed -n 's/.*"installer_install_and_uninstall" *: *"\([a-z_]*\)".*/\1/p' "${MANIFEST_PATH}" | head -n 1)"
    [ -n "${sign_in}" ] || sign_in="pending"
    [ -n "${install_check}" ] || install_check="pending"
  fi

  cat > "${MANIFEST_PATH}" <<JSON
{
  "schema_version": "apocky.desktop-release.v1",
  "access": "account",
  "channel": "preview",
  "version": "${VERSION}",
  "windows": {
    "state": "ready",
    "artifact": {
      "href": "/downloads/${ARTIFACT_NAME}",
      "sha256": "${digest}",
      "bytes": ${bytes},
      "format": "nsis-installer"
    },
    "signing": "unsigned",
    "verification": {
      "launch": "${LAUNCH_CHECK}",
      "service_configuration": "${SERVICE_CHECK}",
      "account_sign_in_and_chat": "${sign_in}",
      "installer_install_and_uninstall": "${install_check}"
    }
  }
}
JSON
  ok "sha256   : ${digest}"
  ok "bytes    : ${bytes}"
  ok "manifest : ${MANIFEST_PATH}"
}

main() {
  hr
  log "Apocrypha desktop · v${VERSION} · ${TARGET} · mode=${MODE}"
  log "signing  : unsigned (Windows will warn; the page says so)"
  check_prereqs
  if [ "${MODE}" = "build" ]; then
    run_tests
    run_build
  fi
  observe_runtime
  stage_installer
  emit_manifest
  hr
  ok "done. Review cssl-edge/public/downloads/ and deploy the site to publish."
}

main "$@"
