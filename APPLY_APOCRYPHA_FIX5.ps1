# APPLY_APOCRYPHA_FIX5.ps1 - no probe (probe mode never exits). Relaunch worker, verify health, deploy to apocky-com, MCP config.
$ErrorActionPreference = 'Continue'
$edge = 'C:\Users\Apocky\source\worktrees\apocky-contributor-node-prod-20260908\cssl-edge'
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -and $_.CommandLine -match 'apocrypha-worker[\\/]runner\.ts' } | ForEach-Object { Write-Host "worker          : stopping PID $($_.ProcessId)"; Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
Start-Sleep 2
$run = Join-Path $edge 'scripts\apocrypha-worker\run-production.ps1'
$log = Join-Path $env:LOCALAPPDATA 'Apocrypha\worker-stdout.log'; New-Item -ItemType Directory -Force -Path (Split-Path $log) | Out-Null
Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File',$run,'-Mode','run') -WindowStyle Hidden -RedirectStandardOutput $log -RedirectStandardError "$log.err"
Write-Host "worker          : launched (run mode, hidden) from $edge"
Start-Sleep 10
Get-Content $log -Tail 4 -ErrorAction SilentlyContinue
try { $h = Invoke-WebRequest -UseBasicParsing http://127.0.0.1:19126/health -TimeoutSec 5; Write-Host "health          : $($h.Content.Substring(0,[Math]::Min(500,$h.Content.Length)))" } catch { Write-Warning "health probe failed: $_" }

Push-Location $edge
Remove-Item -Recurse -Force .vercel -ErrorAction SilentlyContinue
vercel link --yes --scope shawn-bakers-projects-cb1c9715 --project apocky-com
vercel --prod --yes
Pop-Location

$cfg = Join-Path $env:APPDATA 'Claude\claude_desktop_config.json'
if (Test-Path $cfg) {
    Copy-Item $cfg "$cfg.bak-$stamp" -Force
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
