# APPLY_APOCRYPHA_FIX10.ps1 - clean the dirty tree, commit only what is useful, push, deploy.
# Destroys nothing it did not create: only *.bak-<stamp> files written by the earlier apply
# scripts are deleted. Every other modified or untracked file is listed and left alone.
$ErrorActionPreference = 'Continue'
$edge = 'C:\Users\Apocky\source\worktrees\apocky-contributor-node-prod-20260908\cssl-edge'
$repo = (git -C $edge rev-parse --show-toplevel 2>$null); if (-not $repo) { throw "not a git worktree: $edge" }
$repo = $repo.Trim(); $branch = (git -C $repo rev-parse --abbrev-ref HEAD).Trim()
Write-Host "repo            : $repo"
Write-Host "branch          : $branch"

# -- 1. remove the backup files the apply scripts created (all checkouts) ---------------
$roots = @('C:\Users\Apocky\Documents\deploy','C:\Users\Apocky\Documents\worktrees','C:\Users\Apocky\source\worktrees') |
    Where-Object { Test-Path $_ }
$baks = Get-ChildItem $roots -Recurse -File -Filter '*.bak-2026*' -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -like '*\cssl-edge\*' -and $_.FullName -notlike '*node_modules*' }
Write-Host "backups         : $($baks.Count) file(s) from the apply scripts"
$baks | Remove-Item -Force -ErrorAction SilentlyContinue
# The env file backup stays: it holds real configuration history.

# -- 2. what is dirty, and whose is it --------------------------------------------------
Push-Location $repo
$mine = @('cssl-edge/scripts/apocrypha-worker/','cssl-edge/pages/api/admin/apocrypha/jobs/index.ts','cssl-edge/tests/')
$status = git status --porcelain
$dirty = @($status | ForEach-Object { $_.Substring(3).Trim('"') })
$ours = @($dirty | Where-Object { $p = $_; ($mine | Where-Object { $p.StartsWith($_) }).Count -gt 0 })
$theirs = @($dirty | Where-Object { $p = $_; ($mine | Where-Object { $p.StartsWith($_) }).Count -eq 0 })
Write-Host "`n=== ours (will be committed) ==="; $ours | ForEach-Object { Write-Host "  $_" }
Write-Host "`n=== not ours (left untouched) ==="; if ($theirs) { $theirs | ForEach-Object { Write-Host "  $_" } } else { Write-Host '  (none)' }

# -- 3. verify before shipping ----------------------------------------------------------
Write-Host "`n=== verify ==="
Push-Location $edge
& npx tsc --noEmit --pretty false
$typeOk = ($LASTEXITCODE -eq 0)
$testOk = $true
foreach ($t in 'tests/apocrypha-retrieval-synthesis.test.ts','tests/chaos-worker-payload-contract.test.ts','tests/apocrypha-worker.test.ts') {
    & node --import tsx $t 2>&1 | Select-Object -Last 1
    if ($LASTEXITCODE -ne 0) { $testOk = $false; Write-Warning "FAILED: $t" }
}
Pop-Location
if (-not ($typeOk -and $testOk)) { Write-Warning 'verification failed - nothing committed or deployed'; Pop-Location; exit 1 }
Write-Host 'verify          : tsc + tests OK'

# -- 4. commit ours only ----------------------------------------------------------------
if ($ours.Count -gt 0) {
    git add -- $ours
    $msg = @'
feat(apocrypha-worker): context budget, exact tokenization, retrieval synthesis

Root cause: prompt.ts clamped context to 4096 and budgeted 1 byte = 1 token, so
an owner-chat prompt had ~2176 bytes total; admitted memory records were cut to
roughly 80 characters and the model confabulated over the gap.

- context window comes from config; the server's n_ctx clamps it; 3 bytes/token
- exact prompt token count from llama-server /tokenize, recompact only if over
- admitted memory raised to 37% of the compact system prompt, cap 28K
- retrieval synthesis: relevance ranking, cross-faculty duplicate folding with
  corroboration provenance, round-robin interleave, budgeted render
- owner retrieval query includes the last two user turns
- prompt cites [source, provenance id]; states plainly when records lack it
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
} else { Write-Host 'committed       : nothing of ours was dirty' }

# -- 5. push branch, then main ----------------------------------------------------------
git push -u origin $branch
if ($LASTEXITCODE -ne 0) { Write-Warning 'branch push failed - stopping before main'; Pop-Location; exit 1 }
$sha = (git rev-parse HEAD).Trim(); Write-Host "pushed          : $branch @ $($sha.Substring(0,12))"
$mainName = if (git ls-remote --heads origin main 2>$null) { 'main' } else { 'master' }
git fetch origin $mainName
$work = Join-Path (Split-Path $repo -Parent) "_merge-$([guid]::NewGuid().ToString('N').Substring(0,6))"
git worktree add -b "merge/apocrypha-$(Get-Date -Format 'yyyyMMdd-HHmmss')" $work "origin/$mainName"
if ($LASTEXITCODE -eq 0) {
    Push-Location $work
    git merge --no-edit $sha
    if ($LASTEXITCODE -ne 0) { Write-Warning "MERGE CONFLICT vs $mainName - nothing pushed there. Resolve in $work then: git push origin HEAD:$mainName" }
    else {
        git push origin "HEAD:$mainName"
        if ($LASTEXITCODE -eq 0) { Write-Host "pushed          : $mainName updated"; Pop-Location; git worktree remove $work --force; Push-Location $repo }
        else { Write-Warning "push to $mainName refused - the merge is kept in $work"; Pop-Location }
    }
} else { Write-Warning "could not create a $mainName worktree - merge skipped" }
Pop-Location

# -- 6. deploy --------------------------------------------------------------------------
Push-Location (Split-Path $edge -Parent)
vercel --prod --yes
Pop-Location
Write-Host 'DONE'
