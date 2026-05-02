# § scripts/setup/5_deploy_apocky_com.ps1 · Vercel-deploy + alias + CF-purge
# ════════════════════════════════════════════════════════════════════════════
#
# Usage:
#   .\scripts\setup\5_deploy_apocky_com.ps1               # full pipeline
#   .\scripts\setup\5_deploy_apocky_com.ps1 -SkipBuild    # skip TS-check
#   .\scripts\setup\5_deploy_apocky_com.ps1 -SkipPurge    # skip CF cache-purge
#   .\scripts\setup\5_deploy_apocky_com.ps1 -Action status # show last-deploy state

[CmdletBinding()]
param(
    [ValidateSet('deploy', 'status', 'purge')]
    [string]$Action = 'deploy',
    [switch]$SkipBuild,
    [switch]$SkipPurge
)

$RepoRoot = Resolve-Path "$PSScriptRoot\..\..\"
$EdgeDir = Join-Path $RepoRoot "cssl-edge"
$CfEnvFile = "$env:USERPROFILE\.loa-secrets\cloudflare.env"
$ZoneId = "812707f4d46b6eb62070577e7da280f9"

function Get-CfToken {
    if (-not (Test-Path $CfEnvFile)) {
        Write-Host "✗ Cloudflare env-file missing : $CfEnvFile" -ForegroundColor Red
        return $null
    }
    $line = (Get-Content $CfEnvFile | Where-Object { $_ -match 'CLOUDFLARE_API_TOKEN' })
    if (-not $line) { return $null }
    return ($line -split '=', 2)[1].Trim()
}

function Invoke-CfPurge {
    $token = Get-CfToken
    if (-not $token) { return }
    Write-Host "=== Cloudflare cache-purge apocky.com ===" -ForegroundColor Cyan
    $headers = @{
        "X-Auth-Email" = "apocky13@gmail.com"
        "X-Auth-Key"   = $token
        "Content-Type" = "application/json"
    }
    $body = '{"purge_everything":true}'
    try {
        $resp = Invoke-RestMethod -Uri "https://api.cloudflare.com/client/v4/zones/$ZoneId/purge_cache" -Method POST -Headers $headers -Body $body
        if ($resp.success) {
            Write-Host "✓ Cloudflare cache purged · zone $ZoneId" -ForegroundColor Green
        } else {
            Write-Host "✗ Cloudflare purge failed : $($resp.errors | ConvertTo-Json -Compress)" -ForegroundColor Red
        }
    } catch {
        Write-Host "✗ Cloudflare API error : $_" -ForegroundColor Red
    }
}

if ($Action -eq 'status') {
    Push-Location $EdgeDir
    & vercel ls 2>&1 | Select-Object -First 10
    Pop-Location
    exit 0
}

if ($Action -eq 'purge') {
    Invoke-CfPurge
    exit 0
}

# === DEPLOY pipeline ===

if (-not $SkipBuild) {
    Write-Host "=== TS-check + npm-build (cssl-edge) ===" -ForegroundColor Cyan
    Push-Location $EdgeDir
    & npx tsc --noEmit -p tsconfig.json
    if ($LASTEXITCODE -ne 0) {
        Pop-Location
        Write-Host "✗ TS-check failed · fix errors before deploy" -ForegroundColor Red
        exit 1
    }
    Pop-Location
    Write-Host "✓ TS-check clean" -ForegroundColor Green
}

Write-Host "=== Vercel deploy --prod ===" -ForegroundColor Cyan
Push-Location $EdgeDir
$deployOut = & vercel deploy --prod --yes 2>&1
Pop-Location

# Parse the production URL out of the output
$prodUrl = ($deployOut | Where-Object { $_ -match 'Production:\s+(https://\S+)' }) -replace '.*Production:\s+(https://\S+).*', '$1'
if (-not $prodUrl) {
    Write-Host "✗ deploy failed · output below :" -ForegroundColor Red
    $deployOut | Select-Object -Last 15 | ForEach-Object { Write-Host "  $_" }
    exit 1
}
Write-Host "✓ deployed : $prodUrl" -ForegroundColor Green

# Alias to apex apocky.com
$shortUrl = $prodUrl -replace 'https://', '' -replace '/$', ''
Write-Host "=== Vercel alias apocky.com ===" -ForegroundColor Cyan
Push-Location $EdgeDir
& vercel alias set $shortUrl apocky.com 2>&1 | Select-Object -Last 3
Pop-Location

# Cloudflare cache-purge (so apocky.com hits the new dpl_id immediately)
if (-not $SkipPurge) {
    Invoke-CfPurge
}

Write-Host "✓ deploy + alias + purge complete · apocky.com → $prodUrl" -ForegroundColor Green
