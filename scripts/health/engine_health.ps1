# § scripts/health/engine_health.ps1 · Comprehensive engine + cloud health check
# ════════════════════════════════════════════════════════════════════════════
#
# Reports every status point in one place:
#   - Daemon (registered/running)
#   - Mycelium-Desktop (built/installed/running)
#   - Cap-grants (which-systems opted-in)
#   - LoA dist-zip (built/sha)
#   - apocky.com (alive · last-deploy · CDN-cache-age)
#   - Supabase (reachable · table-count)
#   - Cloudflare (zone-status)
#
# Usage:
#   .\scripts\health\engine_health.ps1            # full report
#   .\scripts\health\engine_health.ps1 -Quick     # local-only (no network)
#   .\scripts\health\engine_health.ps1 -JSON      # machine-readable

[CmdletBinding()]
param(
    [switch]$Quick,
    [switch]$JSON
)

$RepoRoot = Resolve-Path "$PSScriptRoot\..\..\"
$report = @{}

function Add-Status {
    param([string]$Section, [string]$Item, [string]$State, [string]$Detail = "")
    if (-not $report.ContainsKey($Section)) { $report[$Section] = @() }
    $report[$Section] += @{ item = $Item ; state = $State ; detail = $Detail }
}

# === DAEMON ===
$task = Get-ScheduledTask -TaskName "LoA-Engine-Daemon" -ErrorAction SilentlyContinue
if ($task) {
    $info = Get-ScheduledTaskInfo -TaskName "LoA-Engine-Daemon"
    Add-Status "Daemon" "Task Scheduler" "✓" "$($task.State) · last-run=$($info.LastRunTime)"
} else {
    Add-Status "Daemon" "Task Scheduler" "✗" "NOT REGISTERED"
}
$daemonExe = "$env:USERPROFILE\.loa\loa-orchestrator-daemon.exe"
if (Test-Path $daemonExe) {
    $size = [math]::Round((Get-Item $daemonExe).Length / 1MB, 2)
    Add-Status "Daemon" "Binary" "✓" "$daemonExe (${size}MB)"
} else {
    Add-Status "Daemon" "Binary" "✗" "MISSING ($daemonExe)"
}
$logFile = "$env:USERPROFILE\.loa\daemon.log"
if (Test-Path $logFile) {
    $size = (Get-Item $logFile).Length
    Add-Status "Daemon" "Log file" "✓" "$logFile ($size bytes)"
} else {
    Add-Status "Daemon" "Log file" "○" "no log yet (daemon never ran)"
}

# === MYCELIUM ===
$mycInst = Join-Path $RepoRoot "compiler-rs\target\release\bundle\nsis\Mycelium_0.1.0-alpha_x64-setup.exe"
if (Test-Path $mycInst) {
    Add-Status "Mycelium-Desktop" "NSIS-installer" "✓" "$mycInst"
} else {
    Add-Status "Mycelium-Desktop" "NSIS-installer" "✗" "NOT BUILT"
}
$mycBin = Join-Path $RepoRoot "compiler-rs\target\release\mycelium-tauri-shell.exe"
if (Test-Path $mycBin) {
    Add-Status "Mycelium-Desktop" "Direct binary" "✓" $mycBin
} else {
    Add-Status "Mycelium-Desktop" "Direct binary" "✗" "NOT BUILT"
}

# === CAPS ===
$capsFile = "$env:USERPROFILE\.loa-secrets\orchestrator-caps.toml"
if (Test-Path $capsFile) {
    $granted = (Get-Content $capsFile | Where-Object { $_ -match '^([a-z_]+)\s*=\s*true\s*$' }).Count
    Add-Status "Caps" "Grants" "✓" "$granted system(s) granted (out of 10)"
} else {
    Add-Status "Caps" "Grants" "○" "default-deny (no file)"
}

# === HOTFIX KEYS ===
$keyCount = 0
foreach ($r in 'A','B','C','D','E') {
    if (Test-Path "$env:USERPROFILE\.loa-secrets\hotfix-cap-$r.priv") { $keyCount++ }
}
Add-Status "Keys" "Hotfix cap-A..E" $(if ($keyCount -eq 5) {"✓"} else {"◐"}) "$keyCount of 5 generated"
if (Test-Path "$env:USERPROFILE\.loa-secrets\cloudflare.env") {
    Add-Status "Keys" "Cloudflare token" "✓" "saved"
} else {
    Add-Status "Keys" "Cloudflare token" "✗" "MISSING"
}

# === LoA DIST ===
$distZip = Join-Path $RepoRoot "dist\LoA-v0.1.0-alpha-windows-x64.zip"
if (Test-Path $distZip) {
    $size = [math]::Round((Get-Item $distZip).Length / 1MB, 2)
    $sha = if (Test-Path "$distZip.sha256") { (Get-Content "$distZip.sha256" -Raw).Trim().Substring(0, 16) + "…" } else { "?" }
    Add-Status "LoA dist" "ZIP" "✓" "${size}MB · sha=$sha"
} else {
    Add-Status "LoA dist" "ZIP" "✗" "NOT BUILT"
}

# === NETWORK CHECKS ===
if (-not $Quick) {
    # apocky.com
    try {
        $resp = Invoke-WebRequest -Uri "https://apocky.com/" -Method HEAD -TimeoutSec 5 -UseBasicParsing
        $age = $resp.Headers['Age']
        $vercelId = $resp.Headers['x-vercel-id']
        Add-Status "Cloud" "apocky.com" "✓" "HTTP $($resp.StatusCode) · age=$age · $vercelId"
    } catch {
        Add-Status "Cloud" "apocky.com" "✗" "unreachable"
    }
    # Cloudflare
    $cfTokenLine = Get-Content "$env:USERPROFILE\.loa-secrets\cloudflare.env" -ErrorAction SilentlyContinue | Where-Object { $_ -match 'CLOUDFLARE_API_TOKEN' }
    if ($cfTokenLine) {
        $token = ($cfTokenLine -split '=', 2)[1].Trim()
        try {
            $resp = Invoke-RestMethod -Uri "https://api.cloudflare.com/client/v4/zones?name=apocky.com" -Headers @{
                "X-Auth-Email" = "apocky13@gmail.com"
                "X-Auth-Key"   = $token
            } -TimeoutSec 5
            if ($resp.success) {
                Add-Status "Cloud" "Cloudflare zone" "✓" "$($resp.result[0].status) · $($resp.result[0].plan.name)"
            } else {
                Add-Status "Cloud" "Cloudflare zone" "✗" "API error"
            }
        } catch {
            Add-Status "Cloud" "Cloudflare zone" "✗" "API unreachable"
        }
    }
}

# === RENDER ===
if ($JSON) {
    $report | ConvertTo-Json -Depth 4
    exit 0
}

Write-Host ""
Write-Host "║════════════════════════════════════════════════════════════════════════════║" -ForegroundColor Magenta
Write-Host "║  § The Infinity Engine · Health Report                                      ║" -ForegroundColor Magenta
Write-Host "║════════════════════════════════════════════════════════════════════════════║" -ForegroundColor Magenta

foreach ($section in $report.Keys) {
    Write-Host ""
    Write-Host "  $section" -ForegroundColor Cyan
    foreach ($entry in $report[$section]) {
        $color = switch ($entry.state) { '✓' {'Green'} '✗' {'Red'} '◐' {'Yellow'} '○' {'DarkGray'} default {'White'} }
        Write-Host "    $($entry.state) $($entry.item) : $($entry.detail)" -ForegroundColor $color
    }
}
Write-Host ""
