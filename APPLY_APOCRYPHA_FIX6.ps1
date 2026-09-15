# APPLY_APOCRYPHA_FIX6.ps1 - v2: exact token budget + provenance-citing prompt. Copies files into every checkout, restarts the worker. No redeploy needed.
$ErrorActionPreference = 'Continue'
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$fixRoot = 'C:\Users\Apocky\source\repos\CSSLv3\_apocrypha-fix\cssl-edge'
$edge = 'C:\Users\Apocky\source\worktrees\apocky-contributor-node-prod-20260908\cssl-edge'
$files = @('scripts\apocrypha-worker\prompt.ts','scripts\apocrypha-worker\qwen.ts','scripts\apocrypha-worker\worker.ts','pages\api\admin\apocrypha\jobs\index.ts','tests\chaos-worker-payload-contract.test.ts','tests\apocrypha-worker.test.ts','scripts\apocrypha-worker\worker.env.example','scripts\apocrypha-worker\profile.production.json')
$all = Get-ChildItem 'C:\Users\Apocky\Documents','C:\Users\Apocky\source' -Recurse -Depth 6 -Filter 'worker.ts' -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -like '*\scripts\apocrypha-worker\worker.ts' -and $_.FullName -notlike '*node_modules*' -and $_.FullName -notlike '*_apocrypha-fix*' } |
    ForEach-Object { (Resolve-Path (Join-Path $_.DirectoryName '..\..')).Path } | Sort-Object -Unique
foreach ($root in $all) { foreach ($f in $files) { $dst = Join-Path $root $f; if (Test-Path $dst) { Copy-Item $dst "$dst.bak-$stamp" -Force }; New-Item -ItemType Directory -Force -Path (Split-Path $dst) | Out-Null; Copy-Item (Join-Path $fixRoot $f) $dst -Force }; Write-Host "patched         : $root" }
Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -and $_.CommandLine -match 'apocrypha-worker[\\/]runner\.ts' } | ForEach-Object { Write-Host "worker          : stopping PID $($_.ProcessId)"; Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
Start-Sleep 2
$run = Join-Path $edge 'scripts\apocrypha-worker\run-production.ps1'
$log = Join-Path $env:LOCALAPPDATA 'Apocrypha\worker-stdout.log'
Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File',$run,'-Mode','run') -WindowStyle Hidden -RedirectStandardOutput $log -RedirectStandardError "$log.err"
Start-Sleep 10
Get-Content $log -Tail 3 -ErrorAction SilentlyContinue
try { $h = Invoke-WebRequest -UseBasicParsing http://127.0.0.1:19126/ready -TimeoutSec 15; Write-Host "ready           : $($h.Content.Substring(0,[Math]::Min(300,$h.Content.Length)))" } catch { Write-Warning "ready probe: $_" }
$task = Get-ScheduledTask -TaskName 'Apocky-Apocrypha-Edge-Resident' -ErrorAction SilentlyContinue
if ($task) { Enable-ScheduledTask -TaskName $task.TaskName | Out-Null; Write-Host 'task            : Apocky-Apocrypha-Edge-Resident enabled (auto-start at sign-in)' }
Write-Host 'DONE - watch prompt_tokens per job:  Get-Content $env:LOCALAPPDATA\Apocrypha\worker-stdout.log -Wait | Select-String prompt_tokens'
