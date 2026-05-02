# § scripts/git/sync.ps1 · Pull + push current branch · auto-handles upstream-not-set
# ════════════════════════════════════════════════════════════════════════════
#
# Wraps `git pull` + `git push` so you don't need to remember the long branch-name
# OR set upstream tracking manually.
#
# Usage:
#   .\scripts\git\sync.ps1                # pull + push
#   .\scripts\git\sync.ps1 -PullOnly       # pull only
#   .\scripts\git\sync.ps1 -PushOnly       # push only
#   .\scripts\git\sync.ps1 -SetUpstream    # one-time : link branch to origin/<branch>

[CmdletBinding()]
param(
    [switch]$PullOnly,
    [switch]$PushOnly,
    [switch]$SetUpstream
)

$RepoRoot = Resolve-Path "$PSScriptRoot\..\..\"
Push-Location $RepoRoot

$branch = (& git branch --show-current).Trim()
if (-not $branch) {
    Write-Host "✗ no current branch (detached HEAD?)" -ForegroundColor Red
    Pop-Location ; exit 1
}
Write-Host "Current branch : $branch" -ForegroundColor Cyan

# Check upstream
$upstream = (& git rev-parse --abbrev-ref --symbolic-full-name "@{u}" 2>$null)
$hasUpstream = ($LASTEXITCODE -eq 0 -and $upstream)

if ($SetUpstream -or -not $hasUpstream) {
    Write-Host "→ setting upstream to origin/$branch" -ForegroundColor Yellow
    & git fetch origin $branch 2>&1 | Select-Object -Last 3
    & git branch --set-upstream-to="origin/$branch" $branch
    if ($LASTEXITCODE -ne 0) {
        Write-Host "✗ upstream set failed · try : git push -u origin $branch" -ForegroundColor Red
        Pop-Location ; exit 1
    }
    Write-Host "✓ upstream set : origin/$branch" -ForegroundColor Green
}

if (-not $PushOnly) {
    Write-Host "=== git pull ===" -ForegroundColor Cyan
    & git pull origin $branch 2>&1 | Select-Object -Last 5
}

if (-not $PullOnly) {
    Write-Host "=== git push ===" -ForegroundColor Cyan
    & git push origin $branch 2>&1 | Select-Object -Last 5
}

Pop-Location
