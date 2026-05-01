@echo off
:: dist-build-mycelium.cmd
:: Thin Windows-batch wrapper for cssl-edge\scripts\dist-build-mycelium.sh.
:: Tries Git-Bash first, falls back to WSL bash. Forwards all CLI args.
::
:: Usage :
::   cssl-edge\scripts\dist-build-mycelium.cmd                          (default checklist mode)
::   cssl-edge\scripts\dist-build-mycelium.cmd --apocky-action-pending  (explicit checklist)
::   cssl-edge\scripts\dist-build-mycelium.cmd --live                   (real build)

setlocal
set "SCRIPT_DIR=%~dp0"
set "BASH_SCRIPT=%SCRIPT_DIR%dist-build-mycelium.sh"

if not exist "%BASH_SCRIPT%" (
  echo [dist-build-mycelium.cmd] ERROR: missing %BASH_SCRIPT%
  exit /b 1
)

:: Prefer Git-Bash (sovereignty-friendly · no kernel hop).
where bash >nul 2>&1
if %errorlevel%==0 (
  bash "%BASH_SCRIPT%" %*
  exit /b %errorlevel%
)

:: Fallback: WSL.
where wsl >nul 2>&1
if %errorlevel%==0 (
  wsl bash "%BASH_SCRIPT%" %*
  exit /b %errorlevel%
)

echo [dist-build-mycelium.cmd] ERROR: neither Git-Bash nor WSL found on PATH.
echo   install Git for Windows : https://git-scm.com/download/win
echo   or enable WSL          : https://learn.microsoft.com/windows/wsl/install
exit /b 2
