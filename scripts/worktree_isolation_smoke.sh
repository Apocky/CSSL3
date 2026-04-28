#!/usr/bin/env bash
# § scripts/worktree_isolation_smoke.sh
# § GATE-ZERO S6-A0 verification : Windows-safe parallel worktree isolation
# § ref : HANDOFF_SESSION_6.csl § GATE-ZERO • DECISIONS.md § T11-D51
#
# § AXIOM
#   t∞: parallel-agent-fanout R! worktree-isolation
#   N!  cross-worktree-leakage @ NTFS+core.autocrlf=true
#   ✓   .gitattributes(eol=lf) + git-config(autocrlf=false,eol=lf) → isolated
#
# § CHECKS  (4 verifications • exit-code 0 ≡ PASS • non-zero ≡ FAIL)
#   1 worktree-A creation + commit file-A  ← positive-control
#   2 worktree-B does NOT see file-A       ← absence-of-checkout-leakage
#   3 worktree-A modify + worktree-B status untouched  ← edit-leak-block
#   4 LF-normalization on commit (no \r in committed bytes)
#
# § OUT
#   stdout = step-by-step PASS/FAIL annotated lines
#   exit 0 ≡ all-green ; exit 1 ≡ any-FAIL
#
# § INVOKE
#   bash scripts/worktree_isolation_smoke.sh
#   (Windows : Git Bash • Linux/macOS : default bash)
#
# § ¬ run-on-main-without-clean-status : the script makes commits on
#   throwaway branches in dedicated worktrees, never touches the calling
#   worktree's tracked files. Untracked entries in the calling worktree
#   are unaffected.
# § attestation : ¬(hurt ∨ harm) .making-of-this @ (anyone ∨ anything ∨ anybody)

set -euo pipefail

# === locate repo root ===
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

# === paths + branch names ===
SMOKE_BASE=".claude/worktrees/_smoke"
WT_A="$SMOKE_BASE/A"
WT_B="$SMOKE_BASE/B"
BR_A="cssl/_smoke/wt-A"
BR_B="cssl/_smoke/wt-B"
CANARY="smoke-canary-A.txt"

TS="$(date -u +%Y-%m-%dT%H-%M-%SZ)"

# === cleanup helper (idempotent ; safe to invoke twice) ===
cleanup() {
    git worktree remove --force "$WT_A" >/dev/null 2>&1 || true
    git worktree remove --force "$WT_B" >/dev/null 2>&1 || true
    git branch -D "$BR_A" >/dev/null 2>&1 || true
    git branch -D "$BR_B" >/dev/null 2>&1 || true
    git worktree prune >/dev/null 2>&1 || true
    rm -rf "$SMOKE_BASE" >/dev/null 2>&1 || true
}

# always cleanup on exit (success or failure)
trap cleanup EXIT

# === pre-cleanup (defensive — handles aborted prior runs) ===
cleanup
mkdir -p "$SMOKE_BASE"

# === resolve baseline branch (prefer main • else current HEAD) ===
if git show-ref --verify --quiet refs/heads/main; then
    BASELINE_BRANCH="main"
elif git show-ref --verify --quiet refs/heads/master; then
    BASELINE_BRANCH="master"
else
    BASELINE_BRANCH="$(git rev-parse --abbrev-ref HEAD)"
fi

echo "[smoke] baseline-branch = $BASELINE_BRANCH"
echo "[smoke] timestamp       = $TS"
echo ""

# ---------------------------------------------------------------------------
# CHECK 1 : worktree-A creation + commit file-A
# ---------------------------------------------------------------------------
echo "[1/4] create worktree-A on branch $BR_A from $BASELINE_BRANCH..."
git worktree add -b "$BR_A" "$WT_A" "$BASELINE_BRANCH" >/dev/null 2>&1

(
    cd "$WT_A"
    printf 'smoke-canary-A @ %s\nline2\nline3\n' "$TS" > "$CANARY"
    git add "$CANARY"
    git -c user.email=smoke@cssl.local \
        -c user.name=smoke \
        commit -m "smoke: add canary $CANARY (worktree-A)" >/dev/null
)
echo "[1/4] PASS — worktree-A committed canary"

# ---------------------------------------------------------------------------
# CHECK 2 : worktree-B does NOT see file-A
# ---------------------------------------------------------------------------
echo "[2/4] create worktree-B on branch $BR_B from $BASELINE_BRANCH..."
git worktree add -b "$BR_B" "$WT_B" "$BASELINE_BRANCH" >/dev/null 2>&1

if [[ -f "$WT_B/$CANARY" ]]; then
    echo "[2/4] FAIL — canary leaked into worktree-B (file exists)"
    exit 1
fi
echo "[2/4] PASS — worktree-B isolated (canary absent)"

# ---------------------------------------------------------------------------
# CHECK 3 : worktree-A modify file-A → worktree-B git status unaffected
# ---------------------------------------------------------------------------
echo "[3/4] modify canary in worktree-A → re-check worktree-B status..."
(
    cd "$WT_A"
    printf 'modified @ %s\n' "$TS" >> "$CANARY"
)

# verify worktree-A sees its own modification
A_STATUS="$(cd "$WT_A" && git status --porcelain)"
if ! echo "$A_STATUS" | grep -q "$CANARY"; then
    echo "[3/4] FAIL — worktree-A doesn't show its own modification"
    echo "      A status: '$A_STATUS'"
    exit 1
fi

# verify worktree-B is clean (no spurious entries)
B_STATUS="$(cd "$WT_B" && git status --porcelain)"
if [[ -n "$B_STATUS" ]]; then
    echo "[3/4] FAIL — worktree-B status NOT clean after worktree-A edit:"
    echo "$B_STATUS"
    exit 1
fi
echo "[3/4] PASS — worktree-A edit isolated from worktree-B"

# ---------------------------------------------------------------------------
# CHECK 4 : LF-only on the committed canary (no \r bytes)
# ---------------------------------------------------------------------------
echo "[4/4] verify LF-only in committed canary (no \\r bytes)..."
CR_COUNT=$(cd "$WT_A" && git show "HEAD:$CANARY" | tr -cd '\r' | wc -c | tr -d '[:space:]')
if [[ "$CR_COUNT" -ne 0 ]]; then
    echo "[4/4] FAIL — committed canary has $CR_COUNT CR bytes (CRLF leakage)"
    exit 1
fi
echo "[4/4] PASS — committed canary is LF-only ($CR_COUNT CR bytes)"

# ---------------------------------------------------------------------------
# SUMMARY
# ---------------------------------------------------------------------------
echo ""
echo "================================================================"
echo "  S6-A0 GATE-ZERO : ALL 4 CHECKS PASSED"
echo "  Parallel-worktree fanout is SAFE on this clone."
echo "================================================================"
exit 0
