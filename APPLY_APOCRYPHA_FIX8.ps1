# APPLY_APOCRYPHA_FIX8.ps1 - MetaHarness root cause, read-only. Prints the federator's own verdict.
$ErrorActionPreference = 'Continue'
$EnvFile = 'C:\Users\Apocky\Documents\Tarot\Chaos\New\chaos-tarot\.env.local'
$envMap = @{}
foreach ($line in Get-Content -LiteralPath $EnvFile) {
    $t = $line.Trim(); if (-not $t -or $t.StartsWith('#') -or -not $t.Contains('=')) { continue }
    $p = $t.Split('=', 2); $v = $p[1].Trim()
    if (($v.StartsWith('"') -and $v.EndsWith('"')) -or ($v.StartsWith("'") -and $v.EndsWith("'"))) { $v = $v.Substring(1, $v.Length - 2) }
    $envMap[$p[0].Trim()] = $v
}
$cfgPath = $envMap['APOCRYPHA_MEMORY_FEDERATOR_CONFIG']
$exe     = $envMap['APOCRYPHA_MEMORY_FEDERATOR_EXE']
$owner   = $envMap['APOCRYPHA_MEMORY_OWNER_ID']
$part    = $envMap['APOCRYPHA_MEMORY_PRIVACY_PARTITION']
function Sha256Text([string]$text) {
    $sha = [Security.Cryptography.SHA256]::Create()
    -join ($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($text)) | ForEach-Object { $_.ToString('x2') })
}

# -- 1. what the federator config actually declares (top level, not under .federation) --
Write-Host "=== federator config ==="
Write-Host "path            : $cfgPath"
$cfg = Get-Content -LiteralPath $cfgPath -Raw | ConvertFrom-Json
Write-Host "top-level keys  : $(($cfg.PSObject.Properties.Name) -join ', ')"
foreach ($region in 'metaharness','three_mneme','mneme') {
    $node = $cfg.$region
    if ($null -eq $node) { Write-Host "region $region : ABSENT" }
    else { Write-Host "region $region :"; ($node | ConvertTo-Json -Depth 8) }
}
if ($cfg.PSObject.Properties['regions']) { Write-Host 'regions node    :'; ($cfg.regions | ConvertTo-Json -Depth 6) }

# -- 2. the federator's own answer for each region -------------------------------------
Write-Host "`n=== federator observe ==="
foreach ($region in 'metaharness','three_mneme') {
    $req = @{
        schema_version = 'apocrypha.memory.remote-observe-request.v1'
        request_id = "diag-$region-$([guid]::NewGuid().ToString('N').Substring(0,8))"
        query = 'Trinity language stack NIL CSL CSSL'
        regions = @($region)
        limit = 2
        deadline_ms = 10000
        expected_owner_sha256 = Sha256Text $owner
        expected_privacy_partition_sha256 = Sha256Text $part
    } | ConvertTo-Json -Compress -Depth 6
    $out = Join-Path $env:TEMP "fed-$region.out"; $err = Join-Path $env:TEMP "fed-$region.err"
    $in  = Join-Path $env:TEMP "fed-$region.in"
    Set-Content -LiteralPath $in -Value $req -Encoding UTF8 -NoNewline
    $p = Start-Process -FilePath $exe -ArgumentList @('observe','--config',"`"$cfgPath`"") -NoNewWindow -PassThru `
         -RedirectStandardInput $in -RedirectStandardOutput $out -RedirectStandardError $err
    $null = $p.WaitForExit(30000)
    Write-Host "--- $region (exit $($p.ExitCode))"
    $stdout = (Get-Content -LiteralPath $out -Raw -ErrorAction SilentlyContinue)
    $stderr = (Get-Content -LiteralPath $err -Raw -ErrorAction SilentlyContinue)
    if ($stdout) { Write-Host $stdout.Substring(0, [Math]::Min(1200, $stdout.Length)) }
    if ($stderr) { Write-Warning $stderr.Substring(0, [Math]::Min(600, $stderr.Length)) }
}

# -- 3. gateway with the correct owner scope -------------------------------------------
Write-Host "`n=== gateway ==="
$port = $envMap['APOCRYPHA_MEMORY_GATEWAY_PORT']; if (-not $port) { $port = '19127' }
$token = $envMap['APOCRYPHA_MEMORY_GATEWAY_TOKEN']
$scope = ($envMap['APOCRYPHA_MEMORY_GATEWAY_OWNER_SCOPES'] -split ',')[0]
if ($token -and $scope -and $scope.Contains(':')) {
    $parts = $scope.Split(':')
    $body = @{ operation='search'; read_only=$true; query='Trinity language stack NIL CSL CSSL'; limit=2
               tenant_id=$parts[0]; principal_id=$parts[1]; capability='apocky_owner_chat'
               memory_manifest_hash=$envMap['APOCRYPHA_MEMORY_MANIFEST_HASH'] } | ConvertTo-Json
    foreach ($faculty in 'metaharness','mneme','graphify') {
        try {
            $r = Invoke-WebRequest -UseBasicParsing "http://127.0.0.1:$port/v1/memory/$faculty" -Method POST -TimeoutSec 40 `
                 -Headers @{ authorization = "Bearer $token"; 'content-type' = 'application/json' } -Body $body
            Write-Host "$faculty : HTTP $($r.StatusCode) $($r.Content.Substring(0,[Math]::Min(300,$r.Content.Length)))"
        } catch {
            $text = ''
            if ($_.Exception.Response) { try { $text = (New-Object IO.StreamReader($_.Exception.Response.GetResponseStream())).ReadToEnd() } catch {} }
            Write-Warning "$faculty : $($_.Exception.Message) $text"
        }
    }
} else { Write-Warning 'APOCRYPHA_MEMORY_GATEWAY_OWNER_SCOPES or TOKEN missing from the env file' }

# -- 4. what the worker saw on its last turns ------------------------------------------
Write-Host "`n=== worker log ==="
Get-Content (Join-Path $env:LOCALAPPDATA 'Apocrypha\worker-stdout.log') -Tail 400 -ErrorAction SilentlyContinue |
    Select-String 'memory_read.partial|prompt_tokens|context_clamped' | Select-Object -Last 6
