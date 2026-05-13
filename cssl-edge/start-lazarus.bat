@echo off
setlocal

set "ROOT=%~dp0"
set "MODE=%~1"

if "%MODE%"=="" set "MODE=both"

if /I "%MODE%"=="help" goto help
if /I "%MODE%"=="/?" goto help
if /I "%MODE%"=="both" goto both
if /I "%MODE%"=="server" goto server
if /I "%MODE%"=="runner" goto runner

echo Unknown mode: %MODE%
echo.
goto help

:both
call :check_npm || exit /b 1
start "Lazarus control plane" powershell -NoExit -ExecutionPolicy Bypass -Command "Set-Location -LiteralPath '%ROOT%'; npm run dev"
start "Lazarus runner" powershell -NoExit -ExecutionPolicy Bypass -Command "Set-Location -LiteralPath '%ROOT%'; Write-Host 'Waiting for Lazarus control plane @ http://localhost:3000 ...'; for ($i = 0; $i -lt 60; $i++) { try { Invoke-RestMethod -Uri 'http://localhost:3000/api/admin/lazarus/health' -TimeoutSec 1 | Out-Null; break } catch { Start-Sleep -Seconds 1 } }; npm run lazarus:runner"
echo Started Lazarus control plane and runner in separate windows.
exit /b 0

:server
call :check_npm || exit /b 1
pushd "%ROOT%"
npm run dev
set "CODE=%ERRORLEVEL%"
popd
exit /b %CODE%

:runner
call :check_npm || exit /b 1
pushd "%ROOT%"
npm run lazarus:runner
set "CODE=%ERRORLEVEL%"
popd
exit /b %CODE%

:check_npm
where npm >nul 2>nul
if errorlevel 1 (
  echo npm was not found on PATH.
  exit /b 1
)
exit /b 0

:help
echo Usage:
echo   start-lazarus.bat          Starts control plane + runner in separate windows
echo   start-lazarus.bat both     Same as above
echo   start-lazarus.bat server   Runs only npm run dev in this window
echo   start-lazarus.bat runner   Runs only npm run lazarus:runner in this window
echo.
echo File:
echo   %ROOT%start-lazarus.bat
exit /b 0
