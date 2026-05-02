@echo off
:: § setup.bat · double-click launcher for The Infinity Engine setup
:: ════════════════════════════════════════════════════════════════════════════
:: Auto-elevates to Administrator (Task Scheduler registration needs it)
:: then runs scripts/setup/6_full_setup.ps1

setlocal

:: Check if already running as Admin
net session >nul 2>&1
if %errorlevel% neq 0 (
    echo Requesting Administrator elevation...
    powershell -Command "Start-Process cmd -ArgumentList '/c %~f0' -Verb RunAs"
    exit /b 0
)

cd /d "%~dp0"
echo ============================================================================
echo   The Infinity Engine - Full Setup
echo ============================================================================
echo.

powershell -ExecutionPolicy Bypass -NoProfile -File "%~dp0scripts\setup\6_full_setup.ps1" %*

echo.
echo ============================================================================
echo   Setup script finished. Press any key to close this window.
echo ============================================================================
pause >nul
