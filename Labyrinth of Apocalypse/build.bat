@echo off
REM § build.bat — pure-CSSL LoA.exe build pipeline (Windows)
REM ════════════════════════════════════════════════════════════════════════════
REM
REM § T11-LOA-PURE-CSSL (W-LOA-pure-cssl-engine)
REM
REM § PIPELINE
REM   1. Build cssl-rt + loa-host staticlibs via cargo (MSVC toolchain).
REM   2. Build csslc compiler binary.
REM   3. Compile main.cssl via csslc — auto-discovers + links cssl-rt + loa-host
REM      staticlibs · produces LoA.exe.
REM   4. Print READY banner.
REM
REM § ENV CONTROLS
REM   CSSL_LOA_PROFILE     — `release` (default) or `debug`. Affects cargo + path.
REM   CSSL_LOA_NO_CARGO=1  — skip cargo step (use pre-built staticlibs).
REM   CSSL_LOA_TOOLCHAIN   — cargo toolchain. Default: +stable-x86_64-pc-windows-msvc

setlocal enabledelayedexpansion

REM ─────────────────────────────────────────────────────────────────────────
REM § resolve repo root + paths
REM ─────────────────────────────────────────────────────────────────────────

set "LOA_DIR=%~dp0"
set "LOA_DIR=%LOA_DIR:~0,-1%"
pushd "%LOA_DIR%\.."
set "REPO_ROOT=%CD%"
popd
set "COMPILER_RS_DIR=%REPO_ROOT%\compiler-rs"

if not exist "%COMPILER_RS_DIR%" (
    echo ERROR: compiler-rs directory not found at %COMPILER_RS_DIR%
    exit /b 1
)

if "%CSSL_LOA_PROFILE%"=="" set "CSSL_LOA_PROFILE=release"
if "%CSSL_LOA_TOOLCHAIN%"=="" set "CSSL_LOA_TOOLCHAIN=+stable-x86_64-pc-windows-msvc"

set "CARGO_PROFILE_FLAG="
if "%CSSL_LOA_PROFILE%"=="release" set "CARGO_PROFILE_FLAG=--release"

set "CSSLC_EXE=%COMPILER_RS_DIR%\target\%CSSL_LOA_PROFILE%\csslc.exe"
set "OUTPUT_EXE=%LOA_DIR%\LoA.exe"

echo § build.bat · pure-CSSL LoA.exe pipeline
echo   repo-root       : %REPO_ROOT%
echo   compiler-rs     : %COMPILER_RS_DIR%
echo   profile         : %CSSL_LOA_PROFILE%
echo   toolchain       : %CSSL_LOA_TOOLCHAIN%
echo   csslc-exe       : %CSSLC_EXE%
echo   output-exe      : %OUTPUT_EXE%
echo.

REM ─────────────────────────────────────────────────────────────────────────
REM § Step 1 : build cssl-rt + loa-host staticlibs (+ csslc binary)
REM ─────────────────────────────────────────────────────────────────────────

if "%CSSL_LOA_NO_CARGO%"=="1" (
    echo [1/3] cargo build · SKIPPED ^(CSSL_LOA_NO_CARGO=1^)
) else (
    echo [1/3] cargo build · cssl-rt staticlib + loa-host staticlib + csslc

    pushd "%COMPILER_RS_DIR%"

    echo   ^> cargo build -p cssl-rt %CARGO_PROFILE_FLAG%
    cargo %CSSL_LOA_TOOLCHAIN% build -p cssl-rt %CARGO_PROFILE_FLAG%
    if errorlevel 1 (
        popd
        exit /b 1
    )

    echo   ^> cargo build -p loa-host --features runtime %CARGO_PROFILE_FLAG%
    cargo %CSSL_LOA_TOOLCHAIN% build -p loa-host --features runtime %CARGO_PROFILE_FLAG%
    if errorlevel 1 (
        popd
        exit /b 1
    )

    echo   ^> cargo build -p csslc %CARGO_PROFILE_FLAG%
    cargo %CSSL_LOA_TOOLCHAIN% build -p csslc %CARGO_PROFILE_FLAG%
    if errorlevel 1 (
        popd
        exit /b 1
    )

    popd
)

if not exist "%CSSLC_EXE%" (
    echo ERROR: csslc binary not found at %CSSLC_EXE%
    exit /b 1
)

REM ─────────────────────────────────────────────────────────────────────────
REM § Step 2 : compile main.cssl -^> LoA.exe via csslc
REM ─────────────────────────────────────────────────────────────────────────

echo.
echo [2/3] csslc compile · main.cssl -^> LoA.exe
echo   ^> "%CSSLC_EXE%" build "%LOA_DIR%\main.cssl" --emit=exe -o "%OUTPUT_EXE%"

if "%CSSL_RT_VERBOSE%"=="" set "CSSL_RT_VERBOSE=1"

"%CSSLC_EXE%" build "%LOA_DIR%\main.cssl" --emit=exe -o "%OUTPUT_EXE%"
if errorlevel 1 exit /b 1

if not exist "%OUTPUT_EXE%" (
    echo ERROR: csslc reported success but %OUTPUT_EXE% was not produced
    exit /b 1
)

REM ─────────────────────────────────────────────────────────────────────────
REM § Step 3 : ready
REM ─────────────────────────────────────────────────────────────────────────

echo.
echo [3/3] READY · LoA.exe = pure-CSSL navigatable engine
echo   output : %OUTPUT_EXE%
echo   source : %LOA_DIR%\main.cssl
echo   link   : cssl-rt staticlib + loa-host staticlib ^(auto-discovered^)
echo.
echo   next   : "%OUTPUT_EXE%"   ^(or double-click in Explorer^)
echo            the engine opens a borderless-fullscreen window at native
echo            resolution + captures input + serves MCP on localhost:3001
echo            Esc opens menu · F11 toggles fullscreen · Tab pauses
echo.

endlocal
