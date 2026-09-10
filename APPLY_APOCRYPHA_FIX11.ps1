# APPLY_APOCRYPHA_FIX11.ps1 - remove the .bak files that FIX9 committed, push, redeploy, tidy worktrees.
$ErrorActionPreference = 'Continue'
$edge = 'C:\Users\Apocky\source\worktrees\apocky-contributor-node-prod-20260908\cssl-edge'
$repo = (git -C $edge rev-parse --show-toplevel).Trim()
$branch = (git -C $repo rev-parse --abbrev-ref HEAD).Trim()
Push-Location $repo

# -- 1. untrack and delete every backup file the apply scripts made ---------------------
$tracked = @(git ls-files -- '*.bak-2026*')
Write-Host "tracked backups : $($tracked.Count)"
if ($tracked.Count -gt 0) {
    git rm -q --cached -- $tracked
    foreach ($f in $tracked) { Remove-Item -LiteralPath (Join-Path $repo $f) -Force -ErrorAction SilentlyContinue }
}
# Keep them out for good.
$ignore = Join-Path $repo '.gitignore'
if (-not (Select-String -LiteralPath $ignore -Pattern '^\*\.bak-' -Quiet -ErrorAction SilentlyContinue)) {
    Add-Content -LiteralPath $ignore -Value "`n# apply-script backups`n*.bak-*"
    git add -- .gitignore
}
if (git diff --cached --name-only) {
    git commit -q -m @'
chore(apocrypha-worker): drop apply-script backup files from the tree

The .bak-<timestamp> copies were scratch from the host apply scripts and should
never have been committed. Removed from the index and ignored going forward.
The working code is unchanged.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_012uDJzNPmeowE7qyhFDWQ4S
'@
    Write-Host "committed       : $(git log --oneline -1)"
} else { Write-Host 'committed       : nothing to remove' }

# -- 2. push branch and main ------------------------------------------------------------
git push origin $branch
$sha = (git rev-parse HEAD).Trim()
git fetch origin main
git push origin "${sha}:main"
if ($LASTEXITCODE -eq 0) { Write-Host "pushed          : branch + main @ $($sha.Substring(0,12))" }
else { Write-Warning 'main push refused - main has moved; merge manually' }

# -- 3. untracked backups in the other checkouts ---------------------------------------
$roots = @('C:\Users\Apocky\Documents\deploy','C:\Users\Apocky\Documents\worktrees','C:\Users\Apocky\source\worktrees') | Where-Object { Test-Path $_ }
$loose = Get-ChildItem $roots -Recurse -File -Filter '*.bak-2026*' -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -like '*\cssl-edge\*' -and $_.FullName -notlike '*node_modules*' }
Write-Host "loose backups   : $($loose.Count) removed"
$loose | Remove-Item -Force -ErrorAction SilentlyContinue

# -- 4. tidy the stale merge worktree ---------------------------------------------------
foreach ($stale in (Get-ChildItem 'C:\Users\Apocky\source\worktrees' -Directory -Filter '_merge-*' -ErrorAction SilentlyContinue)) {
    git worktree remove $stale.FullName --force 2>$null
    if (Test-Path $stale.FullName) { Remove-Item $stale.FullName -Recurse -Force -ErrorAction SilentlyContinue }
    Write-Host "worktree        : removed $($stale.Name)"
}
git worktree prune
git branch --list 'merge/apocrypha*' | ForEach-Object { git branch -D $_.Trim() 2>$null }
Pop-Location

# -- 5. redeploy ------------------------------------------------------------------------
Push-Location (Split-Path $edge -Parent)
vercel --prod --yes
Pop-Location
Write-Host "`nfinal status:"
git -C $repo status --short | Select-Object -First 15
Write-Host 'DONE'
