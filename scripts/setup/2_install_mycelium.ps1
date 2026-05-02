# § scripts/setup/2_install_mycelium.ps1 · Install Mycelium-Desktop NSIS bundle
# ════════════════════════════════════════════════════════════════════════════
#
# Usage:
#   .\scripts\setup\2_install_mycelium.ps1               # run installer
#   .\scripts\setup\2_install_mycelium.ps1 -Silent       # unattended (uses /S)
#   .\scripts\setup\2_install_mycelium.ps1 -Action launch # just launch the binary
#   .\scripts\setup\2_install_mycelium.ps1 -Action build  # rebuild from source

[CmdletBinding()]
param(
    [ValidateSet('install', 'launch', 'build', 'status')]
    [string]$Action = 'install',
    [switch]$Silent
)

$RepoRoot = Resolve-Path "$PSScriptRoot\..\..\"
$NsisInstaller = Join-Path $RepoRoot "compiler-rs\target\release\bundle\nsis\Mycelium_0.1.0-alpha_x64-setup.exe"
$DirectBinary = Join-Path $RepoRoot "compiler-rs\target\release\mycelium-tauri-shell.exe"
$ProjectDir = Join-Path $RepoRoot "compiler-rs\crates\cssl-host-mycelium-desktop"

function Show-Status {
    if (Test-Path $NsisInstaller) {
        $size = (Get-Item $NsisInstaller).Length / 1MB
        Write-Host "✓ NSIS installer : $NsisInstaller ($([math]::Round($size, 2)) MB)" -ForegroundColor Green
    } else {
        Write-Host "✗ NSIS installer : NOT BUILT (run -Action build)" -ForegroundColor Yellow
    }
    if (Test-Path $DirectBinary) {
        $size = (Get-Item $DirectBinary).Length / 1KB
        Write-Host "✓ Direct binary  : $DirectBinary ($([math]::Round($size, 1)) KB)" -ForegroundColor Green
    } else {
        Write-Host "✗ Direct binary  : NOT BUILT" -ForegroundColor Yellow
    }
}

switch ($Action) {
    'status' {
        Show-Status
        exit 0
    }
    'launch' {
        if (-not (Test-Path $DirectBinary)) {
            Write-Host "✗ binary not built · run -Action build first" -ForegroundColor Red
            exit 1
        }
        Write-Host "→ launching $DirectBinary" -ForegroundColor Cyan
        Start-Process $DirectBinary
        exit 0
    }
    'build' {
        Write-Host "=== Mycelium-Desktop : npm install ===" -ForegroundColor Cyan
        Push-Location (Join-Path $ProjectDir "frontend")
        & npm install
        if ($LASTEXITCODE -ne 0) { Pop-Location ; Write-Host "✗ npm install failed" -ForegroundColor Red ; exit 1 }
        Pop-Location

        Write-Host "=== Mycelium-Desktop : npm run build (frontend) ===" -ForegroundColor Cyan
        Push-Location (Join-Path $ProjectDir "frontend")
        & npm run build
        if ($LASTEXITCODE -ne 0) { Pop-Location ; Write-Host "✗ npm run build failed" -ForegroundColor Red ; exit 1 }
        Pop-Location

        Write-Host "=== Mycelium-Desktop : cargo tauri build (release) ===" -ForegroundColor Cyan
        $env:RUSTUP_TOOLCHAIN = "stable-x86_64-pc-windows-msvc"
        Push-Location $ProjectDir
        & cargo tauri build -f tauri-shell
        $ec = $LASTEXITCODE
        Pop-Location
        if ($ec -ne 0) { Write-Host "✗ cargo tauri build failed (exit $ec)" -ForegroundColor Red ; exit 1 }

        Write-Host "✓ Mycelium-Desktop built" -ForegroundColor Green
        Show-Status
        exit 0
    }
    'install' {
        if (-not (Test-Path $NsisInstaller)) {
            Write-Host "✗ NSIS installer not found · run -Action build first" -ForegroundColor Red
            exit 1
        }
        if ($Silent) {
            Write-Host "→ silent install $NsisInstaller /S" -ForegroundColor Cyan
            & $NsisInstaller /S
        } else {
            Write-Host "→ launching installer $NsisInstaller" -ForegroundColor Cyan
            Write-Host "  (close this window after installer completes)" -ForegroundColor Gray
            & $NsisInstaller
        }
        Write-Host "✓ Mycelium-Desktop installed (check Start menu)" -ForegroundColor Green
        exit 0
    }
}
