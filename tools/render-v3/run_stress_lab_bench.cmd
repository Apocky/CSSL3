@echo off
setlocal
chcp 65001 >nul
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0run_stress_lab_bench.ps1" %*
exit /b %ERRORLEVEL%
