@echo off
setlocal

set "ROOT=%~dp0"
set "MODE=%~1"

if "%MODE%"=="" set "MODE=restart"

if /I "%MODE%"=="help" goto help
if /I "%MODE%"=="/?" goto help
if /I "%MODE%"=="open" goto open
if /I "%MODE%"=="start" goto start
if /I "%MODE%"=="restart" goto restart

echo Unknown mode: %MODE%
echo.
goto help

:restart
call :check_npm || exit /b 1
call :check_env
powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "$listeners = Get-NetTCPConnection -LocalPort 3000 -State Listen -ErrorAction SilentlyContinue | Select-Object -ExpandProperty OwningProcess -Unique; " ^
  "if ($listeners) { " ^
  "  Write-Host 'Port 3000 is already in use by:'; " ^
  "  foreach ($pidValue in $listeners) { " ^
  "    $proc = Get-Process -Id $pidValue -ErrorAction SilentlyContinue; " ^
  "    $name = if ($proc) { $proc.ProcessName } else { 'unknown' }; " ^
  "    Write-Host ('  PID {0}  {1}' -f $pidValue, $name); " ^
  "  } " ^
  "  $answer = Read-Host 'Stop these process IDs before restarting Next.js? [y/N]'; " ^
  "  if ($answer -notmatch '^[Yy]') { exit 20 }; " ^
  "  foreach ($pidValue in $listeners) { Stop-Process -Id $pidValue -Force -ErrorAction Stop }; " ^
  "}"
if errorlevel 20 (
  echo Restart cancelled. Existing server left running.
  exit /b 20
)
call :clear_next_cache
goto launch

:start
call :check_npm || exit /b 1
call :check_env
goto launch

:launch
start "CSSL Edge auth dev" powershell -NoExit -ExecutionPolicy Bypass -Command "Set-Location -LiteralPath '%ROOT%'; npm run dev"
goto open

:open
start "Open CSSL auth UI" powershell -NoExit -ExecutionPolicy Bypass -Command ^
  "Write-Host 'Waiting for http://localhost:3000/login ...'; " ^
  "for ($i = 0; $i -lt 60; $i++) { " ^
  "  try { Invoke-WebRequest -Uri 'http://localhost:3000/login' -UseBasicParsing -TimeoutSec 1 | Out-Null; Start-Process ('http://localhost:3000/login?fresh=' + [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()); exit 0 } " ^
  "  catch { Start-Sleep -Seconds 1 } " ^
  "}; " ^
  "Write-Host 'Timed out waiting for Next.js. Open http://localhost:3000/login after npm run dev finishes.'"
echo Auth dev launcher started.
exit /b 0

:check_npm
where npm >nul 2>nul
if errorlevel 1 (
  echo npm was not found on PATH.
  exit /b 1
)
exit /b 0

:check_env
powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "$envFile = Join-Path '%ROOT%' '.env.local'; " ^
  "if (!(Test-Path -LiteralPath $envFile)) { Write-Host 'WARN: .env.local not found.' -ForegroundColor Yellow; exit 0 }; " ^
  "$text = Get-Content -LiteralPath $envFile -Raw; " ^
  "$missing = @(); " ^
  "if ($text -notmatch '(?m)^\s*NEXT_PUBLIC_SUPABASE_URL\s*=\s*\S+') { $missing += 'NEXT_PUBLIC_SUPABASE_URL' }; " ^
  "if ($text -notmatch '(?m)^\s*NEXT_PUBLIC_SUPABASE_ANON_KEY\s*=\s*\S+') { $missing += 'NEXT_PUBLIC_SUPABASE_ANON_KEY' }; " ^
  "if ($missing.Count -gt 0) { Write-Host ('WARN: .env.local missing client auth vars: ' + ($missing -join ', ')) -ForegroundColor Yellow } " ^
  "else { Write-Host '.env.local client auth vars present.' -ForegroundColor Green }"
exit /b 0

:clear_next_cache
if exist "%ROOT%.next" (
  echo Clearing stale Next.js cache: %ROOT%.next
  rmdir /s /q "%ROOT%.next"
)
exit /b 0

:help
echo Usage:
echo   start-auth-dev.bat          Stops stale :3000 server after confirmation, clears .next, starts npm run dev, opens /login
echo   start-auth-dev.bat restart  Same as above
echo   start-auth-dev.bat start    Starts npm run dev and opens /login without stopping existing :3000 process
echo   start-auth-dev.bat open     Opens /login after waiting for server
echo.
echo File:
echo   %ROOT%start-auth-dev.bat
exit /b 0
