#!/usr/bin/env bash
# § build.sh — pure-CSSL LoA.exe build pipeline
# ════════════════════════════════════════════════════════════════════════════
#
# § T11-LOA-PURE-CSSL (W-LOA-pure-cssl-engine)
#
# § PIPELINE
#   1. Build cssl-rt + loa-host staticlibs via cargo (MSVC toolchain on Windows).
#   2. Build csslc compiler binary.
#   3. Compile main.cssl via csslc — auto-discovers + links cssl-rt + loa-host
#      staticlibs · produces LoA.exe.
#   4. Print READY banner.
#
# § ENV CONTROLS
#   `CSSL_LOA_PROFILE`     — `release` (default) or `debug`. Affects cargo + path.
#   `CSSL_LOA_NO_CARGO=1`  — skip cargo step (use pre-built staticlibs).
#   `CSSL_LOA_TOOLCHAIN`   — cargo toolchain. Default: `+stable-x86_64-pc-windows-msvc`
#                            on Windows, system default elsewhere.
#
# § PRIME-DIRECTIVE
#   Build is local-only ; no off-machine relay, no telemetry leak. Cargo
#   downloads only what Cargo.toml declares (already vetted). Staticlibs
#   are produced into `compiler-rs/target/<profile>/` next to csslc.exe.
#
# § OUTPUT
#   LoA.exe in this directory · LoA.lib (windows export-stub) · LoA.exp · LoA.pdb (debug)
#   logs/loa_runtime.log + logs/loa_telemetry.csv populated on first run.

set -euo pipefail

# ─────────────────────────────────────────────────────────────────────────
# § resolve repo root + paths
# ─────────────────────────────────────────────────────────────────────────

LOA_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${LOA_DIR}/.." && pwd)"
COMPILER_RS_DIR="${REPO_ROOT}/compiler-rs"
PROFILE="${CSSL_LOA_PROFILE:-release}"

if [[ ! -d "${COMPILER_RS_DIR}" ]]; then
  echo "ERROR: compiler-rs directory not found at ${COMPILER_RS_DIR}" >&2
  echo "       expected layout : <repo>/Labyrinth\\ of\\ Apocalypse/build.sh" >&2
  echo "                          <repo>/compiler-rs/Cargo.toml" >&2
  exit 1
fi

# Detect host OS for toolchain default + filename conventions.
case "$(uname -s)" in
  *_NT-* | MINGW* | MSYS* | CYGWIN*) HOST_OS="windows" ;;
  Darwin*)                            HOST_OS="macos"   ;;
  Linux*)                             HOST_OS="linux"   ;;
  *)                                  HOST_OS="other"   ;;
esac

if [[ -z "${CSSL_LOA_TOOLCHAIN+x}" ]]; then
  if [[ "${HOST_OS}" == "windows" ]]; then
    CSSL_LOA_TOOLCHAIN="+stable-x86_64-pc-windows-msvc"
  else
    CSSL_LOA_TOOLCHAIN=""
  fi
fi

CARGO_PROFILE_FLAG=""
if [[ "${PROFILE}" == "release" ]]; then
  CARGO_PROFILE_FLAG="--release"
fi

CSSLC_EXE="${COMPILER_RS_DIR}/target/${PROFILE}/csslc"
if [[ "${HOST_OS}" == "windows" ]]; then
  CSSLC_EXE="${CSSLC_EXE}.exe"
fi

OUTPUT_EXE="${LOA_DIR}/LoA"
if [[ "${HOST_OS}" == "windows" ]]; then
  OUTPUT_EXE="${OUTPUT_EXE}.exe"
fi

echo "§ build.sh · pure-CSSL LoA.exe pipeline"
echo "  repo-root       : ${REPO_ROOT}"
echo "  compiler-rs     : ${COMPILER_RS_DIR}"
echo "  profile         : ${PROFILE}"
echo "  host-os         : ${HOST_OS}"
echo "  toolchain       : ${CSSL_LOA_TOOLCHAIN:-<system-default>}"
echo "  csslc-exe       : ${CSSLC_EXE}"
echo "  output-exe      : ${OUTPUT_EXE}"
echo

# ─────────────────────────────────────────────────────────────────────────
# § sibling-module banner (POD-3 wave · grep-discoverable)
# ─────────────────────────────────────────────────────────────────────────
#
# These 10 .csl files are TRACKED IN GIT alongside main.cssl + declare the
# host-side FFI contracts for the POD-3 staticlibs. They are NOT yet ingested
# at compile-time : csslc currently accepts a SINGLE positional <input> (see
# compiler-rs/crates/csslc/src/cli.rs § parse_build). Once POD-4-D3 lands
# csslc multi-module-compile, this loop will evolve to feed each sibling
# into the csslc invocation. Until then : grep-discoverable scaffolding.

LOA_SIBLING_MODULES=(
  "systems/combat.csl"
  "systems/inventory.csl"
  "systems/crafting.csl"
  "systems/alchemy.csl"
  "systems/magic.csl"
  "systems/run.csl"
  "systems/npc.csl"
  "systems/multiplayer.csl"
  "scenes/city_central_hub.csl"
  "scenes/dungeon_template.csl"
)

echo "§ POD-3 sibling-modules (forward-declared · not-yet-compiled)"
for SIBLING in "${LOA_SIBLING_MODULES[@]}"; do
  SIBLING_PATH="${LOA_DIR}/${SIBLING}"
  if [[ -f "${SIBLING_PATH}" ]]; then
    echo "  ✓ ${SIBLING}"
  else
    echo "  ✗ ${SIBLING}  (MISSING — expected in git)"
  fi
done
echo "  (csslc multi-module-compile lands in POD-4-D3 ; until then these"
echo "   serve as grep-discoverable host-FFI contracts — see main.cssl)"
echo

# ─────────────────────────────────────────────────────────────────────────
# § Step 1 : build cssl-rt + loa-host staticlibs (+ csslc binary)
# ─────────────────────────────────────────────────────────────────────────

if [[ "${CSSL_LOA_NO_CARGO:-}" == "1" ]]; then
  echo "[1/3] cargo build · SKIPPED (CSSL_LOA_NO_CARGO=1)"
else
  echo "[1/3] cargo build · cssl-rt staticlib + loa-host staticlib + csslc"
  pushd "${COMPILER_RS_DIR}" > /dev/null

  # Build cssl-rt staticlib (rlib + staticlib via crate-type list).
  echo "  → cargo build -p cssl-rt ${CARGO_PROFILE_FLAG}"
  cargo ${CSSL_LOA_TOOLCHAIN} build -p cssl-rt ${CARGO_PROFILE_FLAG}

  # Build loa-host staticlib (with `runtime` feature so the engine actually
  # opens the window). The `runtime` feature pulls winit + wgpu + pollster.
  echo "  → cargo build -p loa-host --features runtime ${CARGO_PROFILE_FLAG}"
  cargo ${CSSL_LOA_TOOLCHAIN} build -p loa-host --features runtime ${CARGO_PROFILE_FLAG}

  # Build csslc compiler.
  echo "  → cargo build -p csslc ${CARGO_PROFILE_FLAG}"
  cargo ${CSSL_LOA_TOOLCHAIN} build -p csslc ${CARGO_PROFILE_FLAG}

  popd > /dev/null
fi

if [[ ! -x "${CSSLC_EXE}" ]]; then
  echo "ERROR: csslc binary not found at ${CSSLC_EXE}" >&2
  echo "       set CSSL_LOA_NO_CARGO=0 (default) to build it via cargo," >&2
  echo "       or build it manually + place it at the expected path." >&2
  exit 1
fi

# ─────────────────────────────────────────────────────────────────────────
# § Step 2 : compile main.cssl → LoA.exe via csslc
# ─────────────────────────────────────────────────────────────────────────

echo
echo "[2/3] csslc compile · main.cssl → LoA.exe"
echo "  → ${CSSLC_EXE} build ${LOA_DIR}/main.cssl --emit=exe -o ${OUTPUT_EXE}"

# Verbose mode shows which staticlibs csslc auto-discovers.
export CSSL_RT_VERBOSE="${CSSL_RT_VERBOSE:-1}"

"${CSSLC_EXE}" build "${LOA_DIR}/main.cssl" --emit=exe -o "${OUTPUT_EXE}"

if [[ ! -f "${OUTPUT_EXE}" ]]; then
  echo "ERROR: csslc reported success but ${OUTPUT_EXE} was not produced" >&2
  exit 1
fi

# ─────────────────────────────────────────────────────────────────────────
# § Step 3 : ready
# ─────────────────────────────────────────────────────────────────────────

echo
echo "[3/3] READY · LoA.exe = pure-CSSL navigatable engine"
echo "  output : ${OUTPUT_EXE}"
echo "  source : ${LOA_DIR}/main.cssl"
echo "  link   : cssl-rt staticlib + loa-host staticlib (auto-discovered)"
echo
echo "  next   : ./LoA.exe   (or double-click the .exe in Explorer)"
echo "           the engine opens a borderless-fullscreen window at native"
echo "           resolution + captures input + serves MCP on localhost:3001"
echo "           Esc opens menu · F11 toggles fullscreen · Tab pauses"
echo
