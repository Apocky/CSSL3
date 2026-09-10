# APPLY_APOCRYPHA_FIX4.ps1 - find the checkout that actually has node_modules, patch it, run worker from it, deploy to apocky-com, MCP config.
$ErrorActionPreference = 'Continue'
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$fixRoot = 'C:\Users\Apocky\source\repos\CSSLv3\_apocrypha-fix\cssl-edge'
$files = @('scripts\apocrypha-worker\prompt.ts','pages\api\admin\apocrypha\jobs\index.ts','tests\chaos-worker-payload-contract.test.ts','scripts\apocrypha-worker\worker.env.example','scripts\apocrypha-worker\profile.production.json')
function Backup($p) { if (Test-Path -LiteralPath $p) { Copy-Item -LiteralPath $p "$p.bak-$stamp" -Force } }

# all worker checkouts
$all = Get-ChildItem 'C:\Users\Apocky\Documents','C:\Users\Apocky\source' -Recurse -Depth 6 -Filter 'worker.ts' -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -like '*\scripts\apocrypha-worker\worker.ts' -and $_.FullName -notlike '*node_modules*' -and $_.FullName -notlike '*_apocrypha-fix*' } |
    ForEach-Object { (Resolve-Path (Join-Path $_.DirectoryName '..\..')).Path } | Sort-Object -Unique
Write-Host "checkouts       :"; $all | ForEach-Object { Write-Host "  $_  (tsx: $(Test-Path (Join-Path $_ 'node_modules\tsx')))" }
$runnable = $all | Where-Object { Test-Path (Join-Path $_ 'node_modules\tsx') }
if (-not $runnable) { Write-Warning 'no checkout has node_modules\tsx - installing into the live one'; $edge = $all | Select-Object -First 1; Push-Location $edge; npm install --no-audit --no-fund; Pop-Location; $runnable = @($edge) }
$edge = $runnable | Sort-Object { (Get-Item (Join-Path $_ 'scripts\apocrypha-worker\worker.ts')).LastWriteTime } -Descending | Select-Object -First 1
Write-Host "run from        : $edge"

# patch every checkout (idempotent, backups kept)
foreach ($root in $all) { foreach ($f in $files) { $dst = Join-Path $root $f; if (Test-Path $dst) { Backup $dst }; New-Item -ItemType Directory -Force -Path (Split-Path $dst) | Out-Null; Copy-Item (Join-Path $fixRoot $f) $dst -Force }; Write-Host "patched         : $root" }

# worker restart
Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -and $_.CommandLine -match 'apocrypha-worker[\\/]runner\.ts' } | ForEach-Object { Write-Host "worker          : stopping PID $($_.ProcessId)"; Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
Start-Sleep 2
$run = Join-Path $edge 'scripts\apocrypha-worker\run-production.ps1'
Write-Host 'probe           :'; & powershell -NoProfile -ExecutionPolicy Bypass -File $run -Mode probe; Write-Host "probe exit      : $LASTEXITCODE"
$log = Join-Path $env:LOCALAPPDATA 'Apocrypha\worker-stdout.log'; New-Item -ItemType Directory -Force -Path (Split-Path $log) | Out-Null
Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File',$run,'-Mode','run') -WindowStyle Hidden -RedirectStandardOutput $log -RedirectStandardError "$log.err"
Start-Sleep 8
Write-Host "worker          : relaunched; last log lines:"; Get-Content $log -Tail 5 -ErrorAction SilentlyContinue; Get-Content "$log.err" -Tail 5 -ErrorAction SilentlyContinue
try { Write-Host "health          : $((Invoke-WebRequest -UseBasicParsing http://127.0.0.1:19126/health -TimeoutSec 5).Content.Substring(0,400))" } catch { Write-Warning "health probe failed: $_" }

# deploy to the REAL project (apocky-com), not the accidental 'cssl-edge' project FIX3 created
Push-Location $edge
Remove-Item -Recurse -Force .vercel -ErrorAction SilentlyContinue
vercel link --yes --scope shawn-bakers-projects-cb1c9715 --project apocky-com
vercel --prod --yes
Pop-Location

# MCP config
$cfg = Join-Path $env:APPDATA 'Claude\claude_desktop_config.json'
if (Test-Path $cfg) {
    Backup $cfg
    $c = Get-Content $cfg -Raw | ConvertFrom-Json; $srv = $c.mcpServers
    foreach ($name in @($srv.PSObject.Properties.Name)) {
        if ($name -ceq 'filesystem' -and $srv.PSObject.Properties['Filesystem']) { $srv.PSObject.Properties.Remove($name); Write-Host "mcp             : removed duplicate 'filesystem'"; continue }
        $s = $srv.$name
        if ($s.args -and (($s.args -join ' ') -match 'server-filesystem')) { $s.args = @($s.args | ForEach-Object { $_ -replace '@modelcontextprotocol/server-filesystem(@[^\s]+)?', '@modelcontextprotocol/server-filesystem@latest' }); Write-Host "mcp             : '$name' -> server-filesystem@latest" }
    }
    ($c | ConvertTo-Json -Depth 20) | Set-Content $cfg -Encoding UTF8
    Write-Host 'mcp             : written - restart Claude desktop, then remove + re-add the repos folder'
} else { Write-Warning "no $cfg" }
Write-Host 'DONE'
