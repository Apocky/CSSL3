# § scripts/setup/3_grant_caps.ps1 · Grant Σ-caps for-the-engine-systems-to-actually-DO-things
# ════════════════════════════════════════════════════════════════════════════
#
# By default ALL caps are DENY. The W16 wire-up calls 6 systems per-frame BUT
# they no-op without granted-caps. This script edits ~/.loa-secrets/orchestrator-caps.toml
# to grant per-system caps (or revoke them).
#
# Usage:
#   .\scripts\setup\3_grant_caps.ps1                  # show current state
#   .\scripts\setup\3_grant_caps.ps1 -GrantAll        # grant everything (¬ recommended)
#   .\scripts\setup\3_grant_caps.ps1 -Grant weapons   # grant ONE system
#   .\scripts\setup\3_grant_caps.ps1 -Revoke weapons  # revoke ONE system
#   .\scripts\setup\3_grant_caps.ps1 -RevokeAll       # back to default-deny
#
# Available systems: weapons · fps_feel · movement_aug · loot · mycelium_heartbeat ·
#                    content · self_author · playtest · kan_tick · network_egress

[CmdletBinding()]
param(
    [string]$Grant = '',
    [string]$Revoke = '',
    [switch]$GrantAll,
    [switch]$RevokeAll
)

$CapsFile = "$env:USERPROFILE\.loa-secrets\orchestrator-caps.toml"
$KnownCaps = @(
    'weapons', 'fps_feel', 'movement_aug', 'loot', 'mycelium_heartbeat',
    'content', 'self_author', 'playtest', 'kan_tick', 'network_egress'
)

function Read-Caps {
    if (-not (Test-Path $CapsFile)) {
        return @{} # default-deny everything
    }
    $caps = @{}
    Get-Content $CapsFile | ForEach-Object {
        $line = $_.Trim()
        if ($line -match '^([a-z_]+)\s*=\s*(true|false)\s*$') {
            $caps[$matches[1]] = ($matches[2] -eq 'true')
        }
    }
    return $caps
}

function Write-Caps {
    param($caps)
    $dir = Split-Path -Parent $CapsFile
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $lines = @(
        "# § ~/.loa-secrets/orchestrator-caps.toml · Σ-cap grants for-the-Infinity-Engine",
        "# Default-deny axiom : missing-key OR `false` = NOT-granted.",
        "# Edit via .\scripts\setup\3_grant_caps.ps1 -Grant <name> | -Revoke <name>",
        "# Last-edited : $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss zzz')",
        ""
    )
    foreach ($cap in $KnownCaps) {
        $val = if ($caps.ContainsKey($cap) -and $caps[$cap]) { 'true' } else { 'false' }
        $lines += "$cap = $val"
    }
    $lines | Out-File -FilePath $CapsFile -Encoding utf8 -Force
}

function Show-Caps {
    param($caps)
    Write-Host "=== Σ-cap status ($CapsFile) ===" -ForegroundColor Cyan
    foreach ($cap in $KnownCaps) {
        $granted = $caps.ContainsKey($cap) -and $caps[$cap]
        if ($granted) {
            Write-Host "  ✓ $cap" -ForegroundColor Green
        } else {
            Write-Host "  ⊘ $cap" -ForegroundColor DarkGray
        }
    }
}

$caps = Read-Caps

if ($GrantAll) {
    foreach ($c in $KnownCaps) { $caps[$c] = $true }
    Write-Caps $caps
    Write-Host "✓ ALL caps granted (¬ recommended for-most-users)" -ForegroundColor Yellow
    Show-Caps $caps
    exit 0
}
if ($RevokeAll) {
    $caps = @{}
    Write-Caps $caps
    Write-Host "✓ ALL caps revoked (default-deny restored)" -ForegroundColor Green
    Show-Caps $caps
    exit 0
}
if ($Grant -ne '') {
    if ($KnownCaps -notcontains $Grant) {
        Write-Host "✗ unknown cap '$Grant' · valid : $($KnownCaps -join ', ')" -ForegroundColor Red
        exit 1
    }
    $caps[$Grant] = $true
    Write-Caps $caps
    Write-Host "✓ granted : $Grant" -ForegroundColor Green
    Show-Caps $caps
    exit 0
}
if ($Revoke -ne '') {
    if ($KnownCaps -notcontains $Revoke) {
        Write-Host "✗ unknown cap '$Revoke' · valid : $($KnownCaps -join ', ')" -ForegroundColor Red
        exit 1
    }
    $caps[$Revoke] = $false
    Write-Caps $caps
    Write-Host "✓ revoked : $Revoke" -ForegroundColor Green
    Show-Caps $caps
    exit 0
}

Show-Caps $caps
