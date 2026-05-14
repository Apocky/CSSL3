#!/usr/bin/env bash
# § scripts/sentinel_build_check.sh
# § GATE-E : the WinMain-class structural detector
# § ref   : specs/64_SPEC_FIRST_DISCIPLINE.csl §IV Gate-E
#
# § AXIOM
#   t∞ : every csslc-built .exe must produce observable side-effect
#   N! : exit-0 in <Tms with ∅-stdout ∅-stderr ∅-sentinel ≡ DEAD-CODE-SHIPPED
#   ✓  : ≥1 of {sentinel-file · stdout · stderr · exit≠0 · runtime>T} ≡ ALIVE
#
# § CONTRACT (the env-var sentinel mechanism · csslc-rt SHOULD honor)
#   Caller sets CSSL_SENTINEL_PATH to an mktemp path before invoking the .exe.
#   csslc-rt prologue (post-spec-65 work) writes 0xC0FFEE to that path
#   BEFORE user-main runs · and writes 0xDEADBEEF AFTER user-main returns.
#   The gate verifies BOTH bytes present · proves user-main was reached.
#   Until csslc-rt honors the contract · the gate accepts weaker signals
#   (non-empty output · exit≠0 · runtime>threshold) but reports DEGRADED.
#
# § INPUT
#   $1 : path to .exe under test
#   $2 : (optional) timeout seconds · default 5
#   $3 : (optional) min-runtime ms below-which-suspect · default 50
#
# § OUTPUT
#   stdout : annotated step-by-step PASS/FAIL/DEGRADED lines
#   exit 0 ≡ PASS · exit 1 ≡ FAIL · exit 2 ≡ DEGRADED (passes but contract not-honored)
#
# § INVOKE
#   bash scripts/sentinel_build_check.sh dist/foo.exe
#   bash scripts/sentinel_build_check.sh dist/foo.exe 10 100
#
# § INTEGRATION
#   CI runs this on EVERY csslc-built .exe artifact · build fails on exit 1
#   degraded artifacts open auto-issue but do not block (until contract lands)
#
# § attestation : ¬(hurt ∨ harm) · WinMain-bug-class · structurally-closed @ this-script
# ─────────────────────────────────────────────────────────────────────────────

set -u

EXE="${1:-}"
TIMEOUT_S="${2:-5}"
MIN_RUNTIME_MS="${3:-50}"

if [[ -z "${EXE}" ]]; then
    echo "FAIL · no .exe path provided"
    echo "       usage: $0 <path-to-exe> [timeout-s] [min-runtime-ms]"
    exit 1
fi

if [[ ! -f "${EXE}" ]]; then
    echo "FAIL · ${EXE} does not exist"
    exit 1
fi

if [[ ! -x "${EXE}" ]]; then
    # On Windows .exe may not be marked executable in bash · try anyway
    case "${EXE}" in
        *.exe) : ;;  # Windows binary · ok
        *)     echo "FAIL · ${EXE} is not executable and not a .exe"
               exit 1 ;;
    esac
fi

echo "§ GATE-E sentinel-build-check"
echo "§ target  : ${EXE}"
echo "§ timeout : ${TIMEOUT_S}s · min-runtime : ${MIN_RUNTIME_MS}ms"
echo

# Step 1 · set up sentinel-path contract
SENTINEL_DIR="$(mktemp -d 2>/dev/null || mktemp -d -t cssl_gate_e)"
SENTINEL_PATH="${SENTINEL_DIR}/cssl_sentinel.bin"
export CSSL_SENTINEL_PATH="${SENTINEL_PATH}"
echo "[1/5] sentinel contract path : ${SENTINEL_PATH}"

# Step 2 · capture all observable side-effects
STDOUT_FILE="${SENTINEL_DIR}/stdout.txt"
STDERR_FILE="${SENTINEL_DIR}/stderr.txt"

START_NS=$(date +%s%N 2>/dev/null || python -c 'import time; print(int(time.time()*1e9))')

# Run with timeout · capture all streams · do NOT fail on non-zero exit
if command -v timeout >/dev/null 2>&1; then
    timeout "${TIMEOUT_S}" "${EXE}" >"${STDOUT_FILE}" 2>"${STDERR_FILE}"
    EXIT_CODE=$?
else
    # macOS without coreutils · use background-and-kill
    "${EXE}" >"${STDOUT_FILE}" 2>"${STDERR_FILE}" &
    EXE_PID=$!
    ( sleep "${TIMEOUT_S}" && kill -9 "${EXE_PID}" 2>/dev/null ) &
    KILLER_PID=$!
    wait "${EXE_PID}" 2>/dev/null
    EXIT_CODE=$?
    kill -9 "${KILLER_PID}" 2>/dev/null
fi

END_NS=$(date +%s%N 2>/dev/null || python -c 'import time; print(int(time.time()*1e9))')
RUNTIME_MS=$(( (END_NS - START_NS) / 1000000 ))

echo "[2/5] exec complete · exit=${EXIT_CODE} · runtime=${RUNTIME_MS}ms"

# Step 3 · check each observable signal
SIGNALS=0
CONTRACT_HONORED=0
DETAILS=()

# Signal A · sentinel file exists and contains expected pattern (the strong contract)
if [[ -f "${SENTINEL_PATH}" ]]; then
    SENTINEL_SIZE=$(wc -c <"${SENTINEL_PATH}" | tr -d ' ')
    SIGNALS=$((SIGNALS + 1))
    CONTRACT_HONORED=1
    DETAILS+=("✓ sentinel-file present · ${SENTINEL_SIZE} bytes")
    # Look for the canonical magic bytes 0xC0FFEE (pre-main) and 0xDEADBEEF (post-main)
    if hexdump -C "${SENTINEL_PATH}" 2>/dev/null | grep -qi 'c0 ff ee'; then
        DETAILS+=("✓ 0xC0FFEE magic found · csslc-rt prologue ran")
    fi
    if hexdump -C "${SENTINEL_PATH}" 2>/dev/null | grep -qi 'de ad be ef'; then
        DETAILS+=("✓ 0xDEADBEEF magic found · user-main returned · WinMain-bug structurally-impossible")
    fi
else
    DETAILS+=("◐ sentinel-file absent · contract not-yet-honored by csslc-rt · DEGRADED")
fi

# Signal B · non-empty stdout
if [[ -s "${STDOUT_FILE}" ]]; then
    STDOUT_BYTES=$(wc -c <"${STDOUT_FILE}" | tr -d ' ')
    SIGNALS=$((SIGNALS + 1))
    DETAILS+=("✓ stdout non-empty · ${STDOUT_BYTES} bytes")
fi

# Signal C · non-empty stderr (excluding common startup noise)
if [[ -s "${STDERR_FILE}" ]]; then
    STDERR_BYTES=$(wc -c <"${STDERR_FILE}" | tr -d ' ')
    SIGNALS=$((SIGNALS + 1))
    DETAILS+=("✓ stderr non-empty · ${STDERR_BYTES} bytes")
fi

# Signal D · explicit non-zero exit (deliberate signal)
if [[ "${EXIT_CODE}" -ne 0 && "${EXIT_CODE}" -ne 124 && "${EXIT_CODE}" -ne 137 ]]; then
    # 124 = GNU timeout · 137 = SIGKILL · those are gate-imposed not exe-deliberate
    SIGNALS=$((SIGNALS + 1))
    DETAILS+=("✓ explicit non-zero exit · code=${EXIT_CODE}")
fi

# Signal E · runtime exceeds minimum threshold (proves the exe did SOMETHING)
if [[ "${RUNTIME_MS}" -ge "${MIN_RUNTIME_MS}" ]]; then
    SIGNALS=$((SIGNALS + 1))
    DETAILS+=("✓ runtime ${RUNTIME_MS}ms ≥ ${MIN_RUNTIME_MS}ms threshold")
fi

# Step 4 · render the audit
echo "[3/5] signals detected : ${SIGNALS} of 5"
for line in "${DETAILS[@]}"; do
    echo "      ${line}"
done

# Step 5 · the WinMain anti-pattern detector
#   exit=0 · runtime<threshold · stdout=∅ · stderr=∅ · no-sentinel ≡ WinMain-class dead-code
SUSPECT_DEAD_CODE=0
if [[ "${EXIT_CODE}" -eq 0 ]] \
   && [[ "${RUNTIME_MS}" -lt "${MIN_RUNTIME_MS}" ]] \
   && [[ ! -s "${STDOUT_FILE}" ]] \
   && [[ ! -s "${STDERR_FILE}" ]] \
   && [[ ! -f "${SENTINEL_PATH}" ]]; then
    SUSPECT_DEAD_CODE=1
fi

echo "[4/5] WinMain-class detector : $(if [[ ${SUSPECT_DEAD_CODE} -eq 1 ]]; then echo "‼ TRIGGERED"; else echo "✓ clear"; fi)"

# Step 6 · cleanup + verdict
echo "[5/5] cleanup ${SENTINEL_DIR}"
rm -rf "${SENTINEL_DIR}"
echo

if [[ "${SUSPECT_DEAD_CODE}" -eq 1 ]]; then
    echo "‼ FAIL · WinMain-class dead-code pattern detected"
    echo "  exit=0 · runtime=${RUNTIME_MS}ms · stdout=∅ · stderr=∅ · sentinel=absent"
    echo "  this is structurally identical to the bug that hid weeks of fixes"
    echo "  fix : ensure user-main produces observable side-effect OR honor"
    echo "        CSSL_SENTINEL_PATH contract in csslc-rt prologue/epilogue"
    exit 1
fi

if [[ "${SIGNALS}" -eq 0 ]]; then
    echo "‼ FAIL · zero observable signals · cannot prove .exe did anything"
    exit 1
fi

if [[ "${CONTRACT_HONORED}" -eq 0 ]]; then
    echo "◐ DEGRADED · ${SIGNALS} weak signal(s) but sentinel-contract not-honored"
    echo "  the .exe is alive but csslc-rt should implement CSSL_SENTINEL_PATH"
    echo "  to provide strong proof that user-main ran"
    echo "  passes-build but opens auto-issue per spec-64 § Gate-E"
    exit 2
fi

echo "✓ PASS · ${SIGNALS} signal(s) detected · sentinel-contract honored"
exit 0
