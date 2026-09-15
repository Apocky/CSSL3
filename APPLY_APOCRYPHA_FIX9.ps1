# APPLY_APOCRYPHA_FIX9.ps1 - commit the applied worker changes and push to main, then deploy production.
# Standing directive: changes go live. Nothing here rewrites history; a conflict aborts cleanly.
$ErrorActionPreference = 'Continue'
$edge = 'C:\Users\Apocky\source\worktrees\apocky-contributor-node-prod-20260908\cssl-edge'
$repo = (git -C $edge rev-parse --show-toplevel 2>$null)
if (-not $repo) { throw "not a git worktree: $edge" }
$repo = $repo.Trim()
$branch = (git -C $repo rev-parse --abbrev-ref HEAD).Trim()
Write-Host "repo            : $repo"
Write-Host "branch          : $branch"

# -- 1. commit the applied changes (only the worker + owner route + tests) --------------
Push-Location $repo
git add -- cssl-edge/scripts/apocrypha-worker cssl-edge/pages/api/admin/apocrypha/jobs/index.ts cssl-edge/tests
git status --short -- cssl-edge | Select-Object -First 20
$staged = (git diff --cached --name-only)
if ($staged) {
    $msg = @'
feat(apocrypha-worker): context budget, exact tokenization, retrieval synthesis

Root cause: prompt.ts clamped context to 4096 and budgeted 1 byte = 1 token, so
an owner-chat prompt had ~2176 bytes total; admitted memory records were cut to
roughly 80 characters and the model confabulated over the gap.

- context window comes from config (server n_ctx clamps it), 3 bytes/token
- exact prompt token count from llama-server /tokenize, recompact only if over
- admitted memory raised to 37% of the compact system prompt, cap 28K
- retrieval synthesis: relevance ranking, cross-faculty duplicate folding with
  corroboration provenance, round-robin interleave, budgeted render
- owner retrieval query includes the last two user turns
- prompt cites [source, provenance id]; states when records lack the answer
- worker logs adapter_details and per-faculty record counts

Tests: apocrypha-retrieval-synthesis, chaos-worker-payload-contract,
apocrypha-worker{,-http,-retrieval,-recovery}, member-runtime; tsc clean.
Rollback: revert this commit; restart the worker.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_012uDJzNPmeowE7qyhFDWQ4S
'@
    $msgFile = Join-Path $env:TEMP 'apocrypha-commit-msg.txt'
    Set-Content -LiteralPath $msgFile -Value $msg -Encoding UTF8
    git commit -q -F $msgFile
    Write-Host "committed       : $(git log --oneline -1)"
} else { Write-Host 'committed       : nothing staged (already committed)' }

# -- 2. push this branch ----------------------------------------------------------------
git push -u origin $branch
if ($LASTEXITCODE -ne 0) { Write-Warning 'branch push failed - stopping before touching main'; Pop-Location; exit 1 }
$sha = (git rev-parse HEAD).Trim()
Write-Host "pushed          : $branch @ $($sha.Substring(0,12))"

# -- 3. merge into main and push (no force, abort on conflict) --------------------------
$main = (git ls-remote --heads origin main 2>$null)
$mainName = if ($main) { 'main' } else { 'master' }
git fetch origin $mainName
$work = Join-Path (Split-Path $repo -Parent) "_merge-$mainName-$([guid]::NewGuid().ToString('N').Substring(0,6))"
git worktree add -b "merge/apocrypha-fix-$(Get-Date -Format 'yyyyMMdd-HHmmss')" $work "origin/$mainName"
if ($LASTEXITCODE -ne 0) { Write-Warning "could not create a $mainName worktree - merge skipped"; Pop-Location; exit 1 }
Push-Location $work
git merge --no-edit $sha
if ($LASTEXITCODE -ne 0) {
    Write-Warning "MERGE CONFLICT against $mainName - nothing pushed. Resolve in $work, then: git push origin HEAD:$mainName"
    Pop-Location; Pop-Location; exit 1
}
git push origin "HEAD:$mainName"
if ($LASTEXITCODE -eq 0) { Write-Host "pushed          : $mainName updated" } else { Write-Warning "push to $mainName refused - resolve manually in $work" }
Pop-Location
git worktree remove $work --force
Pop-Location

# -- 4. deploy production ---------------------------------------------------------------
Push-Location (Split-Path $edge -Parent)
vercel --prod --yes
Pop-Location
Write-Host 'DONE'
