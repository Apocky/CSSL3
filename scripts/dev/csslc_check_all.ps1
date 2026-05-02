# § scripts/dev/csslc_check_all.ps1 · Run csslc check on every .csl file · pass/fail report
# ════════════════════════════════════════════════════════════════════════════
#
# Usage:
#   .\scripts\dev\csslc_check_all.ps1                # all .csl files
#   .\scripts\dev\csslc_check_all.ps1 -Verbose       # show error-text per-fail
#   .\scripts\dev\csslc_check_all.ps1 -Path "Labyrinth of Apocalypse"  # narrow

[CmdletBinding()]
param(
    [string]$Path = "",
    [switch]$Verbose
)

$RepoRoot = Resolve-Path "$PSScriptRoot\..\..\"
$searchRoots = if ($Path) { @($Path) } else {
    @(
        (Join-Path $RepoRoot "Labyrinth of Apocalypse\scenes"),
        (Join-Path $RepoRoot "Labyrinth of Apocalypse\systems"),
        (Join-Path $RepoRoot "stage1"),
        (Join-Path $RepoRoot "examples")
    )
}

# Build csslc once
Write-Host "=== building csslc (release) ===" -ForegroundColor Cyan
Push-Location (Join-Path $RepoRoot "compiler-rs")
& cargo build -p csslc --release 2>&1 | Select-Object -Last 2
$ec = $LASTEXITCODE
Pop-Location
if ($ec -ne 0) { Write-Host "✗ csslc build failed" -ForegroundColor Red ; exit 1 }
$csslcPath = Join-Path $RepoRoot "compiler-rs\target\release\csslc.exe"

$pass = @()
$fail = @()
$skip = @()

foreach ($root in $searchRoots) {
    if (-not (Test-Path $root)) { continue }
    Get-ChildItem -Recurse -Filter "*.csl" -Path $root | ForEach-Object {
        $file = $_.FullName
        $rel = $file.Substring($RepoRoot.Path.Length + 1)
        $out = & $csslcPath check $file 2>&1
        if ($LASTEXITCODE -eq 0) {
            $pass += $rel
            Write-Host "  ✓ $rel" -ForegroundColor Green
        } else {
            $fail += @{ path = $rel ; err = ($out | Out-String) }
            Write-Host "  ✗ $rel" -ForegroundColor Red
            if ($Verbose) {
                $out | Select-Object -Last 5 | ForEach-Object { Write-Host "    $_" -ForegroundColor DarkGray }
            }
        }
    }
}

Write-Host ""
Write-Host "════════ csslc check summary ════════" -ForegroundColor Cyan
Write-Host "  Pass  : $($pass.Count)" -ForegroundColor Green
Write-Host "  Fail  : $($fail.Count)" -ForegroundColor Red
Write-Host "  Total : $($pass.Count + $fail.Count)" -ForegroundColor White

if ($fail.Count -gt 0 -and -not $Verbose) {
    Write-Host ""
    Write-Host "  re-run with -Verbose to see error-text per-failing-file" -ForegroundColor Gray
}

if ($fail.Count -eq 0) { exit 0 } else { exit 1 }
