# § scripts/git/wave_status.ps1 · Survey W11..W16 commits across-branches
# ════════════════════════════════════════════════════════════════════════════
#
# Usage:
#   .\scripts\git\wave_status.ps1                 # all waves W11..W16
#   .\scripts\git\wave_status.ps1 -Wave W14       # one wave
#   .\scripts\git\wave_status.ps1 -Detailed       # per-commit detail

[CmdletBinding()]
param(
    [string]$Wave = "",
    [switch]$Detailed
)

$RepoRoot = Resolve-Path "$PSScriptRoot\..\..\"
Push-Location $RepoRoot

$waves = if ($Wave) { @($Wave) } else { 'W11', 'W12', 'W13', 'W14', 'W15', 'W16' }

foreach ($w in $waves) {
    $commits = & git log --all --oneline --grep "T11-$w-" 2>&1
    $count = ($commits | Measure-Object).Count
    Write-Host ""
    Write-Host "════════ $w ($count commits) ════════" -ForegroundColor Cyan
    if ($Detailed) {
        $commits | ForEach-Object { Write-Host "  $_" -ForegroundColor Gray }
    } else {
        # Group by sub-tag (T11-W14-A, T11-W14-G, etc.)
        $byTag = @{}
        foreach ($line in $commits) {
            if ($line -match 'T11-W\d+-([A-Z0-9-]+?)(?:[\s:][^A-Z]|\b)') {
                $tag = $matches[1]
                if (-not $byTag.ContainsKey($tag)) { $byTag[$tag] = 0 }
                $byTag[$tag]++
            }
        }
        $byTag.GetEnumerator() | Sort-Object Name | ForEach-Object {
            Write-Host "  $($_.Key) : $($_.Value) commit(s)" -ForegroundColor Gray
        }
    }
}

Write-Host ""
Write-Host "════════ branch state ════════" -ForegroundColor Cyan
$current = & git branch --show-current
Write-Host "  Current : $current"
$ahead = (& git rev-list "origin/$current..HEAD" 2>$null | Measure-Object).Count
$behind = (& git rev-list "HEAD..origin/$current" 2>$null | Measure-Object).Count
Write-Host "  Ahead   : $ahead commits (push needed)" -ForegroundColor $(if ($ahead -gt 0) {'Yellow'} else {'Green'})
Write-Host "  Behind  : $behind commits (pull needed)" -ForegroundColor $(if ($behind -gt 0) {'Yellow'} else {'Green'})

$dirty = (& git status --short | Measure-Object).Count
Write-Host "  Dirty   : $dirty files modified/untracked" -ForegroundColor $(if ($dirty -gt 0) {'Yellow'} else {'Green'})

Pop-Location
