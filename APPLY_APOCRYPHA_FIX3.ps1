# APPLY_APOCRYPHA_FIX3.ps1 - finish: restart worker, probe, deploy, MCP config. Steps 1-5 already done by FIX2.
$ErrorActionPreference = 'Continue'
$edge = 'C:\Users\Apocky\Documents\deploy\apocky-795e8d8-live\cssl-edge'
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
function Backup($p) { if (Test-Path -LiteralPath $p) { Copy-Item -LiteralPath $p "$p.bak-$stamp" -Force } }

# -- 6. worker: kill any running instance, relaunch from the patched checkout -----------
$workers = Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -and $_.CommandLine -match 'apocrypha-worker[\\/]runner\.ts' }
foreach ($w in $workers) { Write-Host "worker          : stopping PID $($w.ProcessId)"; Stop-Process -Id $w.ProcessId -Force -ErrorAction SilentlyContinue }
Start-Sleep 2
$run = Join-Path $edge 'scripts\apocrypha-worker\run-production.ps1'
Write-Host 'probe           :'
& powershell -NoProfile -ExecutionPolicy Bypass -File $run -Mode probe
Write-Host "probe exit      : $LASTEXITCODE"
$log = Join-Path $env:LOCALAPPDATA 'Apocrypha\worker-stdout.log'
New-Item -ItemType Directory -Force -Path (Split-Path $log) | Out-Null
Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File',$run,'-Mode','run') -WindowStyle Hidden -RedirectStandardOutput $log -RedirectStandardError "$log.err"
Write-Host "worker          : relaunched hidden (log: $log)"
$task = Get-ScheduledTask -ErrorAction SilentlyContinue | Where-Object { $_.TaskName -match 'apocrypha' } | Select-Object -First 1
if ($task) { Write-Host "note            : scheduled task '$($task.TaskName)' is DISABLED; enable it (Enable-ScheduledTask) if you want auto-start at sign-in" }

# -- 7. deploy the owner-route change from the checkout ---------------------------------
Push-Location $edge
if (Get-Command vercel -ErrorAction SilentlyContinue) { Write-Host 'deploy          : vercel --prod'; vercel --prod --yes }
else { Write-Host 'deploy          : npx vercel --prod'; npx --yes vercel --prod --yes }
Pop-Location

# -- 8. Claude desktop Filesystem MCP ----------------------------------------------------
$cfg = Join-Path $env:APPDATA 'Claude\claude_desktop_config.json'
if (Test-Path $cfg) {
    Backup $cfg
    $c = Get-Content $cfg -Raw | ConvertFrom-Json
    $srv = $c.mcpServers
    foreach ($name in @($srv.PSObject.Properties.Name)) {
        if ($name -ceq 'filesystem' -and $srv.PSObject.Properties['Filesystem']) { $srv.PSObject.Properties.Remove($name); Write-Host "mcp             : removed duplicate 'filesystem'"; continue }
        $s = $srv.$name
        if ($s.args -and (($s.args -join ' ') -match 'server-filesystem')) {
            $s.args = @($s.args | ForEach-Object { $_ -replace '@modelcontextprotocol/server-filesystem(@[^\s]+)?', '@modelcontextprotocol/server-filesystem@latest' })
            Write-Host "mcp             : '$name' -> @modelcontextprotocol/server-filesystem@latest"
        }
    }
    ($c | ConvertTo-Json -Depth 20) | Set-Content $cfg -Encoding UTF8
    Write-Host 'mcp             : config written - restart the Claude desktop app, then remove + re-add the repos folder'
} else { Write-Warning "claude_desktop_config.json not found at $cfg" }
Write-Host 'DONE'
