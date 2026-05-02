# § scripts/setup/6_full_setup.ps1 · ONE-CLICK full Infinity-Engine setup
# ════════════════════════════════════════════════════════════════════════════
#
# Runs all setup-scripts in canonical order with prompts at gating-points.
# Run from elevated (Administrator) PowerShell for daemon-registration to-work.
#
# Usage:
#   .\scripts\setup\6_full_setup.ps1                    # interactive
#   .\scripts\setup\6_full_setup.ps1 -Yes               # auto-confirm (non-interactive)
#   .\scripts\setup\6_full_setup.ps1 -SkipMyceliumBuild # skip Tauri-build (long)
#   .\scripts\setup\6_full_setup.ps1 -SkipDeploy        # skip Vercel deploy

[CmdletBinding()]
param(
    [switch]$Yes,
    [switch]$SkipLoaRebuild,
    [switch]$SkipMyceliumBuild,
    [switch]$SkipDaemonRegister,
    [switch]$SkipMyceliumInstall,
    [switch]$SkipCapsGrant,
    [switch]$SkipDeploy
)

$ErrorActionPreference = 'Continue'
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

function Confirm-Step {
    param([string]$Prompt)
    if ($Yes) { return $true }
    $r = Read-Host "$Prompt (y/N)"
    return ($r -match '^[yY]')
}

function Run-Step {
    param(
        [string]$Title,
        [string]$Script,
        [string[]]$Args = @(),
        [switch]$Skip
    )
    Write-Host ""
    Write-Host "════════════════════════════════════════════════════════════════════════════" -ForegroundColor Cyan
    Write-Host "  $Title" -ForegroundColor Cyan
    Write-Host "════════════════════════════════════════════════════════════════════════════" -ForegroundColor Cyan
    if ($Skip) {
        Write-Host "→ skipped (flag-set)" -ForegroundColor Yellow
        return
    }
    if (-not (Confirm-Step "→ run this step?")) {
        Write-Host "→ user-skipped" -ForegroundColor Yellow
        return
    }
    & "$ScriptDir\$Script" @Args
    if ($LASTEXITCODE -ne 0) {
        Write-Host "✗ step failed (exit $LASTEXITCODE) · stopping" -ForegroundColor Red
        exit $LASTEXITCODE
    }
    Write-Host "✓ step complete" -ForegroundColor Green
}

Write-Host ""
Write-Host "║════════════════════════════════════════════════════════════════════════════║" -ForegroundColor Magenta
Write-Host "║  § The Infinity Engine · Full Setup · 6-step pipeline                       ║" -ForegroundColor Magenta
Write-Host "║════════════════════════════════════════════════════════════════════════════║" -ForegroundColor Magenta
Write-Host ""
Write-Host "  Steps :" -ForegroundColor White
Write-Host "    1. Rebuild LoA.exe (cargo + dist-build)" -ForegroundColor Gray
Write-Host "    2. Build Mycelium-Desktop NSIS-installer (Tauri)" -ForegroundColor Gray
Write-Host "    3. Install Mycelium-Desktop" -ForegroundColor Gray
Write-Host "    4. Register loa-orchestrator-daemon Task Scheduler" -ForegroundColor Gray
Write-Host "    5. Grant Σ-caps (interactive · default-deny)" -ForegroundColor Gray
Write-Host "    6. Deploy apocky.com (Vercel + CF-purge)" -ForegroundColor Gray
Write-Host ""

Run-Step -Title "STEP 1/6 · Rebuild LoA.exe + dist-zip" -Script "4_rebuild_loa.ps1" -Skip:$SkipLoaRebuild
Run-Step -Title "STEP 2/6 · Build Mycelium-Desktop NSIS-installer" -Script "2_install_mycelium.ps1" -Args @('-Action', 'build') -Skip:$SkipMyceliumBuild
Run-Step -Title "STEP 3/6 · Install Mycelium-Desktop (NSIS)" -Script "2_install_mycelium.ps1" -Args @('-Action', 'install', '-Silent') -Skip:$SkipMyceliumInstall
Run-Step -Title "STEP 4/6 · Register loa-orchestrator-daemon (Task Scheduler · ADMIN-required)" -Script "1_install_daemon.ps1" -Args @('-Action', 'register') -Skip:$SkipDaemonRegister
Run-Step -Title "STEP 5/6 · Grant Σ-caps (default-deny stays unless you opt-in)" -Script "3_grant_caps.ps1" -Skip:$SkipCapsGrant
Run-Step -Title "STEP 6/6 · Deploy apocky.com (Vercel + Cloudflare-purge)" -Script "5_deploy_apocky_com.ps1" -Skip:$SkipDeploy

Write-Host ""
Write-Host "║════════════════════════════════════════════════════════════════════════════║" -ForegroundColor Green
Write-Host "║  ✓ Full setup complete · The Infinity Engine is armed                       ║" -ForegroundColor Green
Write-Host "║════════════════════════════════════════════════════════════════════════════║" -ForegroundColor Green
Write-Host ""
Write-Host "  Quick-status :" -ForegroundColor White
Write-Host "    .\scripts\setup\1_install_daemon.ps1 -Action status   # daemon" -ForegroundColor Gray
Write-Host "    .\scripts\setup\2_install_mycelium.ps1 -Action status # Mycelium" -ForegroundColor Gray
Write-Host "    .\scripts\setup\3_grant_caps.ps1                      # caps" -ForegroundColor Gray
Write-Host "    .\scripts\setup\4_rebuild_loa.ps1 -Action verify      # dist-zip" -ForegroundColor Gray
Write-Host "    .\scripts\setup\5_deploy_apocky_com.ps1 -Action status # last-deploy" -ForegroundColor Gray
Write-Host ""
