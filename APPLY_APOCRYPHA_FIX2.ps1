# APPLY_APOCRYPHA_FIX.ps1 - one-shot host-side apply for the Apocrypha context-starvation fix.
# Run in PowerShell:  powershell -NoProfile -ExecutionPolicy Bypass -File C:\Users\Apocky\source\repos\CSSLv3\APPLY_APOCRYPHA_FIX.ps1
# Every file it edits gets a .bak-<timestamp> copy beside it. Rollback = restore .bak files, `git reset --hard HEAD~1` in the worker repo, restart worker.
[CmdletBinding()]
param(
    [string]$Patch = 'C:\Users\Apocky\source\repos\CSSLv3\0001-fix-apocrypha-worker-context-starvation.patch',
    [string]$EnvFile = 'C:\Users\Apocky\Documents\Tarot\Chaos\New\chaos-tarot\.env.local',
    [string]$RuntimeProfile = 'D:\Apocrypha\models\Qwen3.5-35B-A3B-Q4\runtime-profile.json',
    [int]$ContextTokens = 16384,
    [switch]$NoPush
)
$ErrorActionPreference = 'Stop'
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
function Backup($p) { if (Test-Path -LiteralPath $p) { Copy-Item -LiteralPath $p "$p.bak-$stamp" -Force; Write-Host "  backup -> $p.bak-$stamp" } }
function Sha256($p) { (Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash.ToLower() }

# -- 1. locate the worker checkout (the one run-production.ps1 lives in) ---------------
$candidates = @(
    'C:\Users\Apocky\Documents\Tarot\Chaos\New\chaos-tarot',
    'C:\Users\Apocky\source\repos\CSSLv3\cssl-edge'
) | Where-Object { Test-Path (Join-Path $_ 'scripts\apocrypha-worker\worker.ts') }
if (-not $candidates) {
    $candidates = Get-ChildItem 'C:\Users\Apocky' -Recurse -Depth 6 -Filter 'worker.ts' -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -like '*\scripts\apocrypha-worker\worker.ts' } |
        ForEach-Object { Resolve-Path (Join-Path $_.DirectoryName '..\..') } | ForEach-Object { $_.Path }
}
if (-not $candidates) { throw 'worker checkout not found (scripts\apocrypha-worker\worker.ts)' }
$edge = $candidates | Select-Object -First 1
$fixRoot = 'C:\Users\Apocky\source\repos\CSSLv3\_apocrypha-fix\cssl-edge'
$repo = $null
try { $repo = (git -C $edge rev-parse --show-toplevel 2>$null); if ($repo) { $repo = $repo.Trim() } } catch { $repo = $null }
Write-Host "worker checkout : $edge"
Write-Host "git repo        : $(if ($repo) { $repo } else { '(none - plain directory, files copied over)' })"

# -- 2. apply: copy the five fixed files over the checkout (works with or without git) ----
$files = @(
    'scripts\apocrypha-worker\prompt.ts',
    'pages\api\admin\apocrypha\jobs\index.ts',
    'tests\chaos-worker-payload-contract.test.ts',
    'scripts\apocrypha-worker\worker.env.example',
    'scripts\apocrypha-worker\profile.production.json'
)
foreach ($f in $files) {
    $src = Join-Path $fixRoot $f; $dst = Join-Path $edge $f
    if (-not (Test-Path $src)) { throw "missing fixed file: $src" }
    Backup $dst
    New-Item -ItemType Directory -Force -Path (Split-Path $dst) | Out-Null
    Copy-Item -LiteralPath $src -Destination $dst -Force
    Write-Host "  applied -> $dst"
}
if ($repo) { Push-Location $repo; git add -A; git commit -q -m "fix(apocrypha-worker): stop starving Qwen of memory and history (ctx clamp, byte budget, memory share, retrieval query, anti-confabulation)"; Write-Host "committed       : $(git log --oneline -1)"; Pop-Location }

# -- 3. worker env: real context window -----------------------------------------------
if (Test-Path -LiteralPath $EnvFile) {
    Backup $EnvFile
    $lines = Get-Content -LiteralPath $EnvFile
    if ($lines -match '^APOCRYPHA_QWEN_CONTEXT_TOKENS=') {
        $lines = $lines -replace '^APOCRYPHA_QWEN_CONTEXT_TOKENS=.*', "APOCRYPHA_QWEN_CONTEXT_TOKENS=$ContextTokens"
    } else { $lines += "APOCRYPHA_QWEN_CONTEXT_TOKENS=$ContextTokens" }
    Set-Content -LiteralPath $EnvFile -Value $lines -Encoding UTF8
    Write-Host "env             : APOCRYPHA_QWEN_CONTEXT_TOKENS=$ContextTokens in $EnvFile"
} else { Write-Warning "env file not found: $EnvFile - set APOCRYPHA_QWEN_CONTEXT_TOKENS=$ContextTokens yourself" }

# -- 4. pinned runtime profile (hash-checked by config.ts) -----------------------------
if (Test-Path -LiteralPath $RuntimeProfile) {
    $json = Get-Content -LiteralPath $RuntimeProfile -Raw | ConvertFrom-Json
    $changed = $false
    foreach ($k in 'context_tokens','n_ctx','ctx_size','context_length','num_ctx') {
        if ($json.PSObject.Properties[$k]) { $json.$k = $ContextTokens; $changed = $true }
    }
    if ($changed) {
        Backup $RuntimeProfile
        $old = Sha256 $RuntimeProfile
        ($json | ConvertTo-Json -Depth 20) | Set-Content -LiteralPath $RuntimeProfile -Encoding UTF8 -NoNewline
        $new = Sha256 $RuntimeProfile
        foreach ($f in @("$edge\scripts\apocrypha-worker\config.ts", "$edge\scripts\apocrypha-worker\manifest.production.json",
                         "$edge\scripts\apocrypha-worker\profile.production.json", "$edge\scripts\apocrypha-worker\README.md")) {
            if (Test-Path $f) { Backup $f; (Get-Content $f -Raw) -replace $old, $new | Set-Content $f -Encoding UTF8 -NoNewline }
        }
        if ($repo) { Push-Location $repo; git add -A; git commit -q -m "chore(apocrypha-worker): repin Qwen runtime profile ctx=$ContextTokens ($($new.Substring(0,12)))"; Pop-Location }
        Write-Host "profile         : ctx=$ContextTokens, hash $($old.Substring(0,12)) -> $($new.Substring(0,12)) repinned + committed"
    } else { Write-Host "profile         : no context key in $RuntimeProfile - unchanged (hash pin intact)" }
} else { Write-Warning "runtime profile not found: $RuntimeProfile (fine if the worker runs without APOCRYPHA_QWEN_RUNTIME_PROFILE_PATH)" }

# -- 5. llama server on :19124 - restart with the bigger window if it was launched with a smaller one
$proc = Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -and $_.CommandLine -match '19124' -and $_.Name -notmatch '^node' } | Select-Object -First 1
if ($proc) {
    Write-Host "llama server    : PID $($proc.ProcessId)`n  $($proc.CommandLine)"
    if ($proc.CommandLine -match '(?<flag>\s(-c|--ctx-size|--ctx_size|-n_ctx)\s+)(?<n>\d+)') {
        $n = [int]$Matches.n
        if ($n -lt $ContextTokens) {
            $newCmd = $proc.CommandLine -replace "(\s(-c|--ctx-size|--ctx_size|-n_ctx)\s+)\d+", "`${1}$ContextTokens"
            Write-Host "  ctx $n -> $ContextTokens ; restarting"
            Stop-Process -Id $proc.ProcessId -Force; Start-Sleep 2
            Start-Process -FilePath 'cmd.exe' -ArgumentList "/c $newCmd" -WindowStyle Minimized
            Write-Host "  relaunched. ROLLBACK: rerun the original command line printed above."
        } else { Write-Host "  ctx already $n - ok" }
    } else { Write-Warning "  no explicit ctx flag on the server command line - check its config/model default is >= $ContextTokens" }
} else { Write-Warning "no process on :19124 found - start the Qwen server with a $ContextTokens-token context" }

# -- 6. restart the worker (scheduled task if registered, else foreground probe) -------
$task = Get-ScheduledTask -ErrorAction SilentlyContinue | Where-Object { $_.TaskName -match 'apocrypha' } | Select-Object -First 1
if ($task) { Stop-ScheduledTask -TaskName $task.TaskName -ErrorAction SilentlyContinue; Start-ScheduledTask -TaskName $task.TaskName; Write-Host "worker          : task '$($task.TaskName)' restarted" }
else { Write-Host "worker          : no scheduled task - run scripts/apocrypha-worker/run-production.ps1 -Mode run" }
Write-Host 'probe           :'
& powershell -NoProfile -ExecutionPolicy Bypass -File "$edge\scripts\apocrypha-worker\run-production.ps1" -Mode probe

# -- 7. deploy the owner-route change (git push if a repo, then vercel --prod from the checkout) ------------------------
if (-not $NoPush) {
    if ($repo) { Push-Location $repo; git push; Pop-Location }
    Push-Location $edge
    try {
        if (Get-Command vercel -ErrorAction SilentlyContinue) { Write-Host 'deploy          : vercel --prod'; vercel --prod --yes }
        else { Write-Host 'deploy          : npx vercel --prod'; npx --yes vercel --prod --yes }
    } finally { Pop-Location }
}

# -- 8. Claude desktop Filesystem MCP: repin to a schema-valid version, drop the duplicate
$cfg = Join-Path $env:APPDATA 'Claude\claude_desktop_config.json'
if (Test-Path $cfg) {
    Backup $cfg
    $c = Get-Content $cfg -Raw | ConvertFrom-Json
    $srv = $c.mcpServers
    foreach ($name in @($srv.PSObject.Properties.Name)) {
        if ($name -cmatch '^filesystem$' -and $srv.PSObject.Properties['Filesystem']) { $srv.PSObject.Properties.Remove($name); Write-Host "mcp             : removed duplicate '$name'"; continue }
        $s = $srv.$name
        if ($s.args -and ($s.args -join ' ') -match 'server-filesystem') {
            $s.args = @($s.args | ForEach-Object { $_ -replace '@modelcontextprotocol/server-filesystem(@[^\s]+)?', '@modelcontextprotocol/server-filesystem@latest' })
            Write-Host "mcp             : '$name' -> @modelcontextprotocol/server-filesystem@latest"
        }
    }
    ($c | ConvertTo-Json -Depth 20) | Set-Content $cfg -Encoding UTF8
    Write-Host "mcp             : config written - restart the Claude desktop app"
} else { Write-Warning "claude_desktop_config.json not found at $cfg" }

Write-Host "`nDONE. Remaining manual step: in the Claude desktop app remove and re-add the 'repos' folder (the VM share never mounted this session)."
Write-Host "Verify: ask Apocrypha the same question, then GET /api/admin/apocrypha/jobs/<id> -> usage.prompt_tokens should be in the thousands, not ~600."
