# § scripts/dev/test_all.ps1 · Run every test-suite across the repo
# ════════════════════════════════════════════════════════════════════════════
#
# Usage:
#   .\scripts\dev\test_all.ps1               # run all suites
#   .\scripts\dev\test_all.ps1 -Only cargo   # only cargo
#   .\scripts\dev\test_all.ps1 -Only npm     # only cssl-edge npm tests
#   .\scripts\dev\test_all.ps1 -Only csslc   # only csslc fixtures
#   .\scripts\dev\test_all.ps1 -SkipSlow     # skip integration-tests

[CmdletBinding()]
param(
    [ValidateSet('all', 'cargo', 'npm', 'csslc')]
    [string]$Only = 'all',
    [switch]$SkipSlow
)

$RepoRoot = Resolve-Path "$PSScriptRoot\..\..\"
$results = @{}
$failed = $false

function Run-Suite {
    param([string]$Name, [scriptblock]$Block)
    Write-Host ""
    Write-Host "════════ $Name ════════" -ForegroundColor Cyan
    $start = Get-Date
    & $Block
    $ec = $LASTEXITCODE
    $dur = ((Get-Date) - $start).TotalSeconds
    if ($ec -eq 0) {
        Write-Host "✓ $Name pass · $([math]::Round($dur,1))s" -ForegroundColor Green
        $results[$Name] = "✓"
    } else {
        Write-Host "✗ $Name fail (exit $ec) · $([math]::Round($dur,1))s" -ForegroundColor Red
        $results[$Name] = "✗"
        $script:failed = $true
    }
}

if ($Only -eq 'all' -or $Only -eq 'cargo') {
    Run-Suite "cargo test (loa-host · core)" {
        Push-Location (Join-Path $RepoRoot "compiler-rs")
        & cargo test -p loa-host --lib 2>&1 | Select-Object -Last 5
        Pop-Location
    }
    if (-not $SkipSlow) {
        Run-Suite "cargo test (workspace · slow)" {
            Push-Location (Join-Path $RepoRoot "compiler-rs")
            & cargo test --workspace --lib 2>&1 | Select-Object -Last 5
            Pop-Location
        }
    }
}

if ($Only -eq 'all' -or $Only -eq 'npm') {
    Run-Suite "npm test (cssl-edge)" {
        Push-Location (Join-Path $RepoRoot "cssl-edge")
        & npm test 2>&1 | Select-Object -Last 10
        Pop-Location
    }
    Run-Suite "tsc --noEmit (cssl-edge)" {
        Push-Location (Join-Path $RepoRoot "cssl-edge")
        & npx tsc --noEmit -p tsconfig.json 2>&1 | Select-Object -Last 10
        Pop-Location
    }
}

if ($Only -eq 'all' -or $Only -eq 'csslc') {
    Run-Suite "csslc fixtures" {
        Push-Location (Join-Path $RepoRoot "compiler-rs")
        & cargo test -p csslc 2>&1 | Select-Object -Last 5
        Pop-Location
    }
}

Write-Host ""
Write-Host "════════ SUMMARY ════════" -ForegroundColor Cyan
foreach ($k in $results.Keys) {
    $s = $results[$k]
    $color = if ($s -eq '✓') { 'Green' } else { 'Red' }
    Write-Host "  $s $k" -ForegroundColor $color
}

if ($failed) { exit 1 } else { exit 0 }
