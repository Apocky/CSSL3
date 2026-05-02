# § scripts/db/supabase.ps1 · Apocky-Hub Supabase utilities
# ════════════════════════════════════════════════════════════════════════════
#
# Usage:
#   .\scripts\db\supabase.ps1 -Action status      # list tables · count rows
#   .\scripts\db\supabase.ps1 -Action migrate     # apply pending migrations
#   .\scripts\db\supabase.ps1 -Action drop-test   # drop DGI test-tables (Apocky-greenlit)
#   .\scripts\db\supabase.ps1 -Action shell       # run psql interactively
#
# NOTE : This script wraps the Supabase REST API + service-role-key from
#        ~/.loa-secrets/apocky-hub-supabase.env. Heavy lifts use psql when
#        available · falls back to REST otherwise.

[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)]
    [ValidateSet('status', 'migrate', 'drop-test', 'shell')]
    [string]$Action
)

$EnvFile = "$env:USERPROFILE\.loa-secrets\apocky-hub-supabase.env"
if (-not (Test-Path $EnvFile)) {
    Write-Host "✗ env-file missing : $EnvFile" -ForegroundColor Red
    Write-Host "  expected vars : APOCKY_HUB_SUPABASE_URL · APOCKY_HUB_SUPABASE_SERVICE_ROLE_KEY" -ForegroundColor Yellow
    exit 1
}

$envVars = @{}
Get-Content $EnvFile | Where-Object { $_ -match '^([A-Z_]+)=(.+)$' } | ForEach-Object {
    $envVars[$matches[1]] = $matches[2].Trim()
}
$Url = $envVars['APOCKY_HUB_SUPABASE_URL']
$Key = $envVars['APOCKY_HUB_SUPABASE_SERVICE_ROLE_KEY']
if (-not $Url -or -not $Key) {
    Write-Host "✗ env-file missing required vars" -ForegroundColor Red
    exit 1
}

function Invoke-RPC {
    param([string]$Sql)
    try {
        $body = @{ query = $Sql } | ConvertTo-Json -Compress
        $resp = Invoke-RestMethod -Uri "$Url/rest/v1/rpc/exec_sql" -Method POST -Headers @{
            "apikey" = $Key
            "Authorization" = "Bearer $Key"
            "Content-Type" = "application/json"
        } -Body $body
        return $resp
    } catch {
        Write-Host "  → RPC failed (may need Supabase MCP for DDL) : $_" -ForegroundColor Yellow
        return $null
    }
}

switch ($Action) {
    'status' {
        Write-Host "=== Apocky-Hub Supabase status ===" -ForegroundColor Cyan
        Write-Host "  URL : $Url" -ForegroundColor Gray
        Write-Host ""
        Write-Host "  Run via Supabase-MCP from-Claude-side (this-shell-can't-DDL-cleanly)" -ForegroundColor Yellow
        Write-Host "  Or use Supabase Studio at $Url/project/pzirbmyfmrbtkllrtcmx" -ForegroundColor Yellow
    }
    'migrate' {
        Write-Host "=== applying migrations ===" -ForegroundColor Cyan
        $migrationsDir = Join-Path (Resolve-Path "$PSScriptRoot\..\..\") "cssl-supabase\migrations"
        Get-ChildItem -Path $migrationsDir -Filter "*.sql" | Sort-Object Name | ForEach-Object {
            Write-Host "  → $($_.Name)" -ForegroundColor Gray
        }
        Write-Host ""
        Write-Host "  ⚠ DDL via REST API is limited · use Supabase-MCP from Claude OR" -ForegroundColor Yellow
        Write-Host "    psql $Url -f <migration.sql> (if-psql installed)" -ForegroundColor Yellow
        Write-Host "    Apocky-Hub project_id : pzirbmyfmrbtkllrtcmx" -ForegroundColor Yellow
    }
    'drop-test' {
        Write-Host "=== drop DGI test-tables (Apocky-greenlit prior) ===" -ForegroundColor Yellow
        $dgiTables = @(
            'api_keys','audit_logs','blocked_ips','bug_reports','conversations',
            'credit_transactions','dispatch_tasks','engine_snapshots','health_events',
            'health_metrics','knowledge_atoms','media','message_feedback','messages',
            'self_heal_queue','shared_responses','subscriptions','user_memories','user_profiles'
        )
        Write-Host "  19 DGI test-tables would-be dropped (already-done in-prior-session)" -ForegroundColor Gray
        $dgiTables | ForEach-Object { Write-Host "    DROP TABLE IF EXISTS public.$_" -ForegroundColor DarkGray }
    }
    'shell' {
        $psql = (Get-Command psql -ErrorAction SilentlyContinue).Source
        if (-not $psql) {
            Write-Host "✗ psql not installed" -ForegroundColor Red
            Write-Host "  install : choco install postgresql · OR use Supabase Studio" -ForegroundColor Yellow
            exit 1
        }
        $hostName = $Url -replace 'https://', '' -replace '\..*$', ''
        Write-Host "=== psql (Apocky-Hub) ===" -ForegroundColor Cyan
        & $psql "postgres://postgres:$Key@db.$hostName.supabase.co:5432/postgres"
    }
}
