@echo off
:: § scripts/setup/status.bat · double-click status-check (no admin needed)

setlocal
cd /d "%~dp0..\..\"

echo ============================================================================
echo   The Infinity Engine - Status
echo ============================================================================

powershell -ExecutionPolicy Bypass -NoProfile -Command ^
    "& '%~dp01_install_daemon.ps1' -Action status; ^
     Write-Host ''; ^
     & '%~dp02_install_mycelium.ps1' -Action status; ^
     Write-Host ''; ^
     & '%~dp03_grant_caps.ps1'; ^
     Write-Host ''; ^
     & '%~dp04_rebuild_loa.ps1' -Action verify"

echo.
pause
