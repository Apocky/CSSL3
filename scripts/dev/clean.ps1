# § scripts/dev/clean.ps1 · Wipe build artifacts for fresh-rebuild
# ════════════════════════════════════════════════════════════════════════════
#
# Usage:
#   .\scripts\dev\clean.ps1            # interactive · confirms each
#   .\scripts\dev\clean.ps1 -All       # wipe everything
#   .\scripts\dev\clean.ps1 -Targets cargo,npm   # comma-list
#
# Targets:
#   cargo    : compiler-rs/target/
#   npm      : cssl-edge/.next/ + cssl-edge/node_modules/.cache/
#   tauri    : compiler-rs/crates/cssl-host-mycelium-desktop/frontend/dist/ + node_modules/
#   dist     : repo-root/dist/
#   logs     : ~/.loa/daemon.log

[CmdletBinding()]
param(
    [string[]]$Targets = @(),
    [switch]$All,
    [switch]$Yes
)

$RepoRoot = Resolve-Path "$PSScriptRoot\..\..\"
$validTargets = @('cargo', 'npm', 'tauri', 'dist', 'logs')

if ($All) { $Targets = $validTargets }
if ($Targets.Count -eq 0) {
    Write-Host "No targets specified. Use -All or -Targets cargo,npm,..." -ForegroundColor Yellow
    Write-Host "Valid : $($validTargets -join ', ')" -ForegroundColor Gray
    exit 1
}

function Remove-Path {
    param([string]$Path, [string]$Label)
    if (-not (Test-Path $Path)) {
        Write-Host "  ⊘ $Label : already-clean ($Path)" -ForegroundColor DarkGray
        return
    }
    if (-not $Yes) {
        $r = Read-Host "  remove $Label ($Path) ? (y/N)"
        if ($r -notmatch '^[yY]') { Write-Host "  ⊘ skipped" -ForegroundColor Yellow ; return }
    }
    Remove-Item -Recurse -Force -Path $Path -ErrorAction SilentlyContinue
    Write-Host "  ✓ removed $Label" -ForegroundColor Green
}

foreach ($t in $Targets) {
    Write-Host ""
    Write-Host "=== clean : $t ===" -ForegroundColor Cyan
    switch ($t) {
        'cargo' {
            Remove-Path (Join-Path $RepoRoot "compiler-rs\target") "compiler-rs/target"
        }
        'npm' {
            Remove-Path (Join-Path $RepoRoot "cssl-edge\.next") "cssl-edge/.next"
            Remove-Path (Join-Path $RepoRoot "cssl-edge\node_modules\.cache") "cssl-edge/node_modules/.cache"
        }
        'tauri' {
            Remove-Path (Join-Path $RepoRoot "compiler-rs\crates\cssl-host-mycelium-desktop\frontend\dist") "tauri/frontend/dist"
            Remove-Path (Join-Path $RepoRoot "compiler-rs\crates\cssl-host-mycelium-desktop\frontend\node_modules") "tauri/frontend/node_modules"
        }
        'dist' {
            Remove-Path (Join-Path $RepoRoot "dist") "dist/"
        }
        'logs' {
            Remove-Path "$env:USERPROFILE\.loa\daemon.log" "~/.loa/daemon.log"
        }
        default {
            Write-Host "  ✗ unknown target : $t" -ForegroundColor Red
        }
    }
}
Write-Host ""
Write-Host "✓ clean complete" -ForegroundColor Green
