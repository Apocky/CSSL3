# § scripts/dev/validate_renderer_transport.ps1
# ════════════════════════════════════════════════════════════════════════════
# Comprehensive product-path validation for the W-0..W-6 renderer transport
# foundation. This is intentionally stricter than a smoke script: it validates
# specs, CSSL modules, MIR lowering, object emission, exe link/run, and Rust
# unit/integration tests across runtime + compiler layers.
#
# Usage:
#   .\scripts\dev\validate_renderer_transport.ps1
#   .\scripts\dev\validate_renderer_transport.ps1 -KeepArtifacts
#   .\scripts\dev\validate_renderer_transport.ps1 -Workspace   # slower, includes workspace cargo test

[CmdletBinding()]
param(
    [switch]$KeepArtifacts,
    [switch]$Workspace
)

$ErrorActionPreference = "Stop"
$RepoRoot = Resolve-Path "$PSScriptRoot\..\.."
$CompilerRoot = Join-Path $RepoRoot "compiler-rs"
$Csslc = Join-Path $CompilerRoot "target\debug\csslc.exe"
$TempRoot = Join-Path $env:TEMP "cssl_renderer_transport_validation"

if (-not (Test-Path $TempRoot)) { New-Item -ItemType Directory -Path $TempRoot | Out-Null }

$script:Pass = 0
$script:Fail = 0
$script:Steps = @()

function Invoke-Step {
    param(
        [Parameter(Mandatory=$true)][string]$Name,
        [Parameter(Mandatory=$true)][scriptblock]$Body
    )
    Write-Host "== $Name ==" -ForegroundColor Cyan
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    try {
        & $Body
        $sw.Stop()
        $script:Pass++
        $script:Steps += [pscustomobject]@{ Name=$Name; Result="PASS"; Ms=$sw.ElapsedMilliseconds }
        Write-Host "  ✓ $Name ($($sw.ElapsedMilliseconds) ms)" -ForegroundColor Green
    } catch {
        $sw.Stop()
        $script:Fail++
        $script:Steps += [pscustomobject]@{ Name=$Name; Result="FAIL"; Ms=$sw.ElapsedMilliseconds }
        Write-Host "  ✗ $Name ($($sw.ElapsedMilliseconds) ms)" -ForegroundColor Red
        Write-Host "  $_" -ForegroundColor Red
        throw
    }
}

function Invoke-CheckedCommand {
    param(
        [Parameter(Mandatory=$true)][string]$Exe,
        [Parameter(Mandatory=$true)][string[]]$Args,
        [string]$WorkingDirectory = $RepoRoot
    )
    Push-Location $WorkingDirectory
    try {
        & $Exe @Args
        if ($LASTEXITCODE -ne 0) {
            throw "Command failed ($LASTEXITCODE): $Exe $($Args -join ' ')"
        }
    } finally {
        Pop-Location
    }
}

function Assert-FileContains {
    param(
        [Parameter(Mandatory=$true)][string]$Path,
        [Parameter(Mandatory=$true)][string]$Pattern
    )
    $full = Join-Path $RepoRoot $Path
    if (-not (Test-Path $full)) { throw "Missing file: $Path" }
    $hit = Select-String -Path $full -Pattern $Pattern -SimpleMatch -Quiet
    if (-not $hit) { throw "Pattern '$Pattern' not found in $Path" }
}

function Invoke-Csslc {
    param([Parameter(Mandatory=$true)][string[]]$Args)
    Invoke-CheckedCommand -Exe $Csslc -Args $Args -WorkingDirectory $CompilerRoot
}

try {
    Invoke-Step "spec cross-checks" {
        Assert-FileContains "specs/16_TRANSPORT_TIERS.csl" "P1  TIER-LADDER"
        Assert-FileContains "specs/16_TRANSPORT_TIERS.csl" "P8  ITERATIVE-SLICE"
        Assert-FileContains "specs/16_TRANSPORT_TIERS.csl" "1440p fullscreen"
        Assert-FileContains "specs/16_TRANSPORT_TIERS.csl" "64 B × 1_000_000"
        Assert-FileContains "specs/24_HOST_FFI.csl" "W-1-GPU-TRANSPORT-ABI-APPENDIX"
        Assert-FileContains "specs/24_HOST_FFI.csl" "__cssl_gpu_cmd_buf_draw_indirect"
    }

    Invoke-Step "build cssl-rt release rlib for linker" {
        Invoke-CheckedCommand -Exe "cargo" -Args @("build", "-p", "cssl-rt", "--release") -WorkingDirectory $CompilerRoot
    }

    Invoke-Step "build csslc debug binary" {
        Invoke-CheckedCommand -Exe "cargo" -Args @("build", "-p", "csslc") -WorkingDirectory $CompilerRoot
        if (-not (Test-Path $Csslc)) { throw "csslc binary missing: $Csslc" }
    }

    Invoke-Step "runtime host_gpu W-1 tests" {
        Invoke-CheckedCommand -Exe "cargo" -Args @("test", "-p", "cssl-rt", "host_gpu", "--lib") -WorkingDirectory $CompilerRoot
        Invoke-CheckedCommand -Exe "cargo" -Args @("test", "-p", "cssl-rt", "w1_transport", "--lib") -WorkingDirectory $CompilerRoot
    }

    Invoke-Step "compiler MIR + cgen W-1 tests" {
        Invoke-CheckedCommand -Exe "cargo" -Args @("test", "-p", "cssl-mir", "lower_gpu", "--lib") -WorkingDirectory $CompilerRoot
        Invoke-CheckedCommand -Exe "cargo" -Args @("test", "-p", "cssl-cgen-cpu-cranelift", "cgen_gpu", "--lib") -WorkingDirectory $CompilerRoot
    }

    Invoke-Step "csslc renderer transport integration tests" {
        Invoke-CheckedCommand -Exe "cargo" -Args @("test", "-p", "csslc", "--test", "t11_w20_renderer_transport") -WorkingDirectory $CompilerRoot
    }

    $csslFiles = @(
        "..\engine\conventions.cssl",
        "..\stdlib\gpu_transport.cssl",
        "..\stdlib\vec_mut.cssl",
        "..\engine\frame_arena.cssl",
        "..\engine\mesh.cssl",
        "..\engine\instance.cssl",
        "..\engine\profiler.cssl",
        "..\engine\frame_graph.cssl",
        "..\examples\hello_million_entities.cssl",
        "..\examples\gpu_transport_abi_smoke.cssl",
        "..\examples\gpu_transport_abi_exhaustive.cssl",
        "..\examples\million_entities_transport_smoke.cssl"
    )

    Invoke-Step "csslc check all renderer transport CSSL files" {
        foreach ($f in $csslFiles) { Invoke-Csslc @("check", $f) }
    }

    Invoke-Step "MIR emission for product-path examples" {
        Invoke-Csslc @("emit-mlir", "..\examples\gpu_transport_abi_exhaustive.cssl")
        Invoke-Csslc @("emit-mlir", "..\examples\million_entities_transport_smoke.cssl")
        Invoke-Csslc @("emit-mlir", "..\examples\hello_million_entities.cssl")
    }

    Invoke-Step "object builds for renderer transport examples" {
        Invoke-Csslc @("build", "..\examples\gpu_transport_abi_smoke.cssl", "--emit=object", "-o", (Join-Path $TempRoot "gpu_transport_abi_smoke.obj"))
        Invoke-Csslc @("build", "..\examples\gpu_transport_abi_exhaustive.cssl", "--emit=object", "-o", (Join-Path $TempRoot "gpu_transport_abi_exhaustive.obj"))
        Invoke-Csslc @("build", "..\examples\million_entities_transport_smoke.cssl", "--emit=object", "-o", (Join-Path $TempRoot "million_entities_transport_smoke.obj"))
        Invoke-Csslc @("build", "..\examples\hello_million_entities.cssl", "--emit=object", "-o", (Join-Path $TempRoot "hello_million_entities.obj"))
    }

    Invoke-Step "exe link + run direct W-1 ABI smoke" {
        $exe = Join-Path $TempRoot "gpu_transport_abi_exhaustive.exe"
        if (Test-Path $exe) { Remove-Item $exe -Force }
        Invoke-Csslc @("build", "..\examples\gpu_transport_abi_exhaustive.cssl", "--emit=exe", "-o", $exe)
        & $exe
        if ($LASTEXITCODE -ne 0) { throw "gpu_transport_abi_exhaustive.exe exit=$LASTEXITCODE" }
    }

    Invoke-Step "exe link + run 1M transport data-path" {
        $exe = Join-Path $TempRoot "million_entities_transport_smoke.exe"
        if (Test-Path $exe) { Remove-Item $exe -Force }
        Invoke-Csslc @("build", "..\examples\million_entities_transport_smoke.cssl", "--emit=exe", "-o", $exe)
        & $exe
        if ($LASTEXITCODE -ne 0) { throw "million_entities_transport_smoke.exe exit=$LASTEXITCODE" }
    }

    if ($Workspace) {
        Invoke-Step "workspace cargo tests (serial)" {
            Invoke-CheckedCommand -Exe "cargo" -Args @("test", "--workspace", "--", "--test-threads=1") -WorkingDirectory $CompilerRoot
        }
    }

    Invoke-Step "diff whitespace hygiene for renderer tracked edits" {
        $tracked = @(
            "specs/24_HOST_FFI.csl",
            "compiler-rs/crates/cssl-rt/src/host_gpu.rs",
            "compiler-rs/crates/cssl-mir/src/body_lower.rs",
            "compiler-rs/crates/cssl-cgen-cpu-cranelift/src/cgen_gpu.rs",
            "compiler-rs/crates/cssl-cgen-cpu-cranelift/src/object.rs"
        )
        Invoke-CheckedCommand -Exe "git" -Args (@("diff", "--check", "--") + $tracked) -WorkingDirectory $RepoRoot
    }

    Write-Host ""
    Write-Host "════════ renderer transport validation summary ════════" -ForegroundColor Cyan
    $script:Steps | Format-Table -AutoSize | Out-String | Write-Host
    Write-Host "Pass: $script:Pass" -ForegroundColor Green
    Write-Host "Fail: $script:Fail" -ForegroundColor Red
    Write-Host "Artifacts: $TempRoot" -ForegroundColor Gray

    if (-not $KeepArtifacts) {
        Remove-Item $TempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
    exit 0
} catch {
    Write-Host ""
    Write-Host "════════ renderer transport validation FAILED ════════" -ForegroundColor Red
    $script:Steps | Format-Table -AutoSize | Out-String | Write-Host
    Write-Host $_ -ForegroundColor Red
    Write-Host "Artifacts retained: $TempRoot" -ForegroundColor Yellow
    exit 1
}
