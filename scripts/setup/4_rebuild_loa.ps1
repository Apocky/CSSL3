# § scripts/setup/4_rebuild_loa.ps1 · Rebuild LoA.exe + dist-zip + sha256
# ════════════════════════════════════════════════════════════════════════════
#
# Usage:
#   .\scripts\setup\4_rebuild_loa.ps1                # full rebuild + dist-build
#   .\scripts\setup\4_rebuild_loa.ps1 -SkipBuild     # skip cargo, just dist-build
#   .\scripts\setup\4_rebuild_loa.ps1 -Action verify # check artifacts only

[CmdletBinding()]
param(
    [ValidateSet('rebuild', 'verify')]
    [string]$Action = 'rebuild',
    [switch]$SkipBuild
)

$RepoRoot = Resolve-Path "$PSScriptRoot\..\..\"
$DistZip = Join-Path $RepoRoot "dist\LoA-v0.1.0-alpha-windows-x64.zip"
$DistSha = "$DistZip.sha256"

function Show-Status {
    if (Test-Path $DistZip) {
        $size = (Get-Item $DistZip).Length / 1MB
        Write-Host "✓ dist-zip : $DistZip ($([math]::Round($size, 2)) MB)" -ForegroundColor Green
    } else {
        Write-Host "✗ dist-zip : NOT BUILT" -ForegroundColor Yellow
    }
    if (Test-Path $DistSha) {
        $sha = (Get-Content $DistSha -Raw).Trim()
        Write-Host "  sha256 : $sha" -ForegroundColor Gray
    }
}

if ($Action -eq 'verify') {
    Show-Status
    exit 0
}

if (-not $SkipBuild) {
    Write-Host "=== rebuilding loa-host (--features runtime, release, msvc) ===" -ForegroundColor Cyan
    $env:RUSTUP_TOOLCHAIN = "stable-x86_64-pc-windows-msvc"
    Push-Location (Join-Path $RepoRoot "compiler-rs")
    & cargo build -p loa-host --features runtime --release
    $ec = $LASTEXITCODE
    Pop-Location
    if ($ec -ne 0) { Write-Host "✗ cargo build failed (exit $ec)" -ForegroundColor Red ; exit 1 }
    Write-Host "✓ loa-host runtime rebuilt" -ForegroundColor Green
}

Write-Host "=== running dist-build.sh ===" -ForegroundColor Cyan
Push-Location $RepoRoot
& bash dist-build.sh
$ec = $LASTEXITCODE
Pop-Location
if ($ec -ne 0) { Write-Host "✗ dist-build.sh failed (exit $ec)" -ForegroundColor Red ; exit 1 }

Show-Status
Write-Host "✓ rebuild complete" -ForegroundColor Green
