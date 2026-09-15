# APPLY_APOCRYPHA_FIX7.ps1 - v3: retrieval synthesis + MetaHarness repair.
# Backups: every replaced file gets a .bak-<timestamp> beside it.
$ErrorActionPreference = 'Continue'
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$fixRoot = 'C:\Users\Apocky\source\repos\CSSLv3\_apocrypha-fix\cssl-edge'
$edge = 'C:\Users\Apocky\source\worktrees\apocky-contributor-node-prod-20260908\cssl-edge'
$EnvFile = 'C:\Users\Apocky\Documents\Tarot\Chaos\New\chaos-tarot\.env.local'
$files = @(
  'scripts\apocrypha-worker\prompt.ts','scripts\apocrypha-worker\qwen.ts','scripts\apocrypha-worker\worker.ts',
  'scripts\apocrypha-worker\retrieval.ts','scripts\apocrypha-worker\synthesis.ts',
  'pages\api\admin\apocrypha\jobs\index.ts',
  'tests\chaos-worker-payload-contract.test.ts','tests\apocrypha-worker.test.ts','tests\apocrypha-retrieval-synthesis.test.ts',
  'scripts\apocrypha-worker\worker.env.example','scripts\apocrypha-worker\profile.production.json')

# -- 1. apply to every checkout ---------------------------------------------------------
$all = Get-ChildItem 'C:\Users\Apocky\Documents','C:\Users\Apocky\source' -Recurse -Depth 6 -Filter 'worker.ts' -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -like '*\scripts\apocrypha-worker\worker.ts' -and $_.FullName -notlike '*node_modules*' -and $_.FullName -notlike '*_apocrypha-fix*' } |
    ForEach-Object { (Resolve-Path (Join-Path $_.DirectoryName '..\..')).Path } | Sort-Object -Unique
foreach ($root in $all) {
    foreach ($f in $files) {
        $dst = Join-Path $root $f
        if (Test-Path $dst) { Copy-Item $dst "$dst.bak-$stamp" -Force }
        New-Item -ItemType Directory -Force -Path (Split-Path $dst) | Out-Null
        Copy-Item (Join-Path $fixRoot $f) $dst -Force
    }
    Write-Host "patched         : $root"
}

# -- 2. env for the diagnostic ----------------------------------------------------------
$envMap = @{}
if (Test-Path $EnvFile) {
    foreach ($line in Get-Content -LiteralPath $EnvFile) {
        $t = $line.Trim(); if (-not $t -or $t.StartsWith('#') -or -not $t.Contains('=')) { continue }
        $pair = $t.Split('=', 2); $v = $pair[1].Trim()
        if (($v.StartsWith('"') -and $v.EndsWith('"')) -or ($v.StartsWith("'") -and $v.EndsWith("'"))) { $v = $v.Substring(1, $v.Length - 2) }
        $envMap[$pair[0].Trim()] = $v
    }
}

# -- 3. MetaHarness repair --------------------------------------------------------------
Write-Host "`n=== MetaHarness ==="
$cfgPath = $envMap['APOCRYPHA_MEMORY_FEDERATOR_CONFIG']
$bundlePath = $null; $endpoint = $null
if ($cfgPath -and (Test-Path -LiteralPath $cfgPath)) {
    try {
        $cfg = Get-Content -LiteralPath $cfgPath -Raw | ConvertFrom-Json
        $mh = $cfg.federation.metaharness
        if ($null -eq $mh) { Write-Warning "federator config has no federation.metaharness region: $cfgPath" }
        else {
            $endpoint = [string]$mh.endpoint
            $bundlePath = [string]$mh.capability.bundle_path
            Write-Host "region          : endpoint=$endpoint enabled=$($mh.enabled)"
            Write-Host "capability      : bundle=$bundlePath"
            if ($bundlePath -and (Test-Path -LiteralPath $bundlePath)) {
                $b = Get-Item -LiteralPath $bundlePath
                Write-Host "bundle file     : age $([int]((Get-Date) - $b.LastWriteTime).TotalMinutes) min (written $($b.LastWriteTime))"
                try {
                    $bj = Get-Content -LiteralPath $bundlePath -Raw | ConvertFrom-Json
                    foreach ($k in 'expires_at','expiry','not_after','expires_at_ms') {
                        if ($bj.PSObject.Properties[$k]) { Write-Host "bundle expiry   : $k = $($bj.$k)" }
                    }
                } catch { Write-Warning "bundle is not readable JSON: $_" }
            } else { Write-Warning "capability bundle MISSING at $bundlePath - the renewal task has not produced one" }
        }
    } catch { Write-Warning "federator config unreadable: $_" }
} else { Write-Warning "APOCRYPHA_MEMORY_FEDERATOR_CONFIG not set or missing (looked in $EnvFile)" }

# The observer and its capability-renewal task are the two live parts.
foreach ($name in 'Apocky-MetaHarness-Capability-Renewal','Apocky-MetaHarness-MCP-Direct') {
    $t = Get-ScheduledTask -TaskName $name -ErrorAction SilentlyContinue
    if (-not $t) { Write-Warning "task missing    : $name (run scripts\apocrypha-memory-gateway\metaharness-register-resident-task.ps1)"; continue }
    $info = Get-ScheduledTaskInfo -TaskName $name -TaskPath $t.TaskPath -ErrorAction SilentlyContinue
    Write-Host "task            : $name state=$($t.State) lastRun=$($info.LastRunTime) result=$($info.LastTaskResult)"
    if ($t.State -eq 'Disabled') { Enable-ScheduledTask -TaskName $name -TaskPath $t.TaskPath | Out-Null; Write-Host "  enabled" }
    # Renewal first: the observer is useless without a fresh capability.
    Start-ScheduledTask -TaskName $name -TaskPath $t.TaskPath -ErrorAction SilentlyContinue
    Write-Host "  started"
    Start-Sleep 6
    $info = Get-ScheduledTaskInfo -TaskName $name -TaskPath $t.TaskPath -ErrorAction SilentlyContinue
    Write-Host "  now: state=$((Get-ScheduledTask -TaskName $name -TaskPath $t.TaskPath).State) result=$($info.LastTaskResult)"
}
if ($endpoint) {
    try { $r = Invoke-WebRequest -UseBasicParsing $endpoint -TimeoutSec 5; Write-Host "endpoint        : HTTP $($r.StatusCode)" }
    catch { Write-Warning "endpoint $endpoint not answering: $($_.Exception.Message)" }
}

# -- 4. restart the worker --------------------------------------------------------------
Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -and $_.CommandLine -match 'apocrypha-worker[\\/]runner\.ts' } |
    ForEach-Object { Write-Host "worker          : stopping PID $($_.ProcessId)"; Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
Start-Sleep 2
$run = Join-Path $edge 'scripts\apocrypha-worker\run-production.ps1'
$log = Join-Path $env:LOCALAPPDATA 'Apocrypha\worker-stdout.log'
Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File',$run,'-Mode','run') -WindowStyle Hidden -RedirectStandardOutput $log -RedirectStandardError "$log.err"
Start-Sleep 12

# -- 5. live faculty states, with the detail that used to be hidden ----------------------
Write-Host "`n=== faculties ==="
try {
    $h = (Invoke-WebRequest -UseBasicParsing http://127.0.0.1:19126/ready -TimeoutSec 30).Content | ConvertFrom-Json
    $h.adapters | ConvertTo-Json -Depth 6
} catch { Write-Warning "worker /ready: $($_.Exception.Message)"; Get-Content $log -Tail 6 -ErrorAction SilentlyContinue }

# Ask the gateway directly - this prints MetaHarness's own error code.
$port = $envMap['APOCRYPHA_MEMORY_GATEWAY_PORT']; if (-not $port) { $port = '19127' }
$token = $envMap['APOCRYPHA_MEMORY_GATEWAY_TOKEN']
$tenant = ($envMap['APOCRYPHA_MEMORY_GATEWAY_ALLOWED_TENANTS'] -split ',')[0]
$principal = ($envMap['APOCRYPHA_MEMORY_GATEWAY_ALLOWED_PRINCIPALS'] -split ',')[0]
if ($token -and $tenant -and $principal) {
    $body = @{ operation='search'; read_only=$true; query='Trinity language stack NIL CSL CSSL'; limit=2
               tenant_id=$tenant.Trim(); principal_id=$principal.Trim(); capability='apocky_owner_chat'
               memory_manifest_hash='307a86ce2ec83a37ad30f86327195e47259167728cf32e4276af377f08988273' } | ConvertTo-Json
    foreach ($faculty in 'metaharness','mneme') {
        try {
            $r = Invoke-WebRequest -UseBasicParsing "http://127.0.0.1:$port/v1/memory/$faculty" -Method POST -TimeoutSec 30 `
                 -Headers @{ authorization = "Bearer $token"; 'content-type' = 'application/json' } -Body $body
            Write-Host "$faculty : HTTP $($r.StatusCode) $($r.Content.Substring(0,[Math]::Min(400,$r.Content.Length)))"
        } catch {
            $resp = $_.Exception.Response
            $text = ''
            if ($resp) { try { $text = (New-Object IO.StreamReader($resp.GetResponseStream())).ReadToEnd() } catch {} }
            Write-Warning "$faculty : $($_.Exception.Message) $($text.Substring(0,[Math]::Min(400,$text.Length)))"
        }
    }
} else { Write-Warning 'gateway token/tenant/principal not in the env file - skipped the direct faculty probe' }

Write-Host "`nDONE. Per-job evidence:  Get-Content `$env:LOCALAPPDATA\Apocrypha\worker-stdout.log -Wait | Select-String 'prompt_tokens|memory_read.partial'"
