param(
    [string]$Rooms = "printer,material,hair,portal,arena",
    [int]$Frames = 1500,
    [switch]$Quick,
    [switch]$NoBuild,
    [switch]$Open,
    [switch]$AllowBudgetFail
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new()
if ($Quick) {
    $Frames = 1500
}

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Resolve-Path (Join-Path $ScriptDir "..\..")
$CompilerRoot = Join-Path $RepoRoot "compiler-rs"
$TargetDir = Join-Path $CompilerRoot "target"
$Graphical = Join-Path $ScriptDir "run_graphical_bench.ps1"
$Stamp = Get-Date -Format "yyyyMMdd_HHmmss"
$MenuHtml = Join-Path $TargetDir "v3_stress_lab_$Stamp`_menu.html"
$SuiteJson = Join-Path $TargetDir "v3_stress_lab_$Stamp`_suite.json"

$KnownRooms = @("shells", "printer", "material", "hair", "portal", "arena")
$RoomList = $Rooms.Split(",") | ForEach-Object { $_.Trim().ToLowerInvariant() } | Where-Object { $_ }
foreach ($room in $RoomList) {
    if ($KnownRooms -notcontains $room) {
        throw "unknown stress room '$room' ; valid=$($KnownRooms -join ',')"
    }
}

function HtmlEncode {
    param([string]$Text)
    return [System.Net.WebUtility]::HtmlEncode($Text)
}

function HtmlPath {
    param([string]$Path)
    if (-not $Path -or -not (Test-Path $Path)) {
        return "#"
    }
    return ([System.Uri]::new((Resolve-Path $Path).Path)).AbsoluteUri
}

function JsonValue {
    param($Obj, [string]$Path)
    $cur = $Obj
    foreach ($part in $Path.Split(".")) {
        if ($null -eq $cur) { return $null }
        $cur = $cur.$part
    }
    return $cur
}

if (-not $NoBuild) {
    Write-Host "== build.loa-runtime"
    Push-Location $CompilerRoot
    try {
        cargo +stable-x86_64-pc-windows-msvc build -p loa-host --features runtime --bin loa-runtime
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build failed with exit $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }
}

$Results = @()
$AnyBudgetFail = $false
foreach ($room in $RoomList) {
    Write-Host "== stress.room.$room"
    $childArgs = @(
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", $Graphical,
        "-Frames", [string]$Frames,
        "-NoBuild",
        "-StressRoom", $room,
        "-AllowBudgetFail"
    )
    $childOut = & powershell @childArgs 2>&1
    $exit = $LASTEXITCODE
    $childOut | ForEach-Object { Write-Host $_ }

    $reportPath = $null
    $summaryPath = $null
    $dashboardPath = $null
    foreach ($line in $childOut) {
        $s = [string]$line
        if ($s -match "^\s*report:\s*(.+)$") { $reportPath = $Matches[1].Trim() }
        if ($s -match "^\s*summary:\s*(.+)$") { $summaryPath = $Matches[1].Trim() }
        if ($s -match "^\s*dashboard:\s*(.+)$") { $dashboardPath = $Matches[1].Trim() }
    }

    $report = $null
    $status = "missing_report"
    $wallP99 = $null
    $wallP999 = $null
    $gpuP99 = $null
    $cpuP99 = $null
    if ($reportPath -and (Test-Path $reportPath)) {
        $report = Get-Content -Raw $reportPath | ConvertFrom-Json
        $status = [string](JsonValue $report "verdict.status")
        $wallP99 = JsonValue $report "telemetry.summary.wall_since_prev_us.p99"
        $wallP999 = JsonValue $report "telemetry.summary.wall_since_prev_us.p99_9"
        $gpuP99 = JsonValue $report "telemetry.summary.gpu_dispatch_us.p99"
        $cpuP99 = JsonValue $report "telemetry.summary.cpu_total_us.p99"
    }
    if ($status -eq "fail_budget" -or $exit -ne 0) {
        $AnyBudgetFail = $true
    }
    $Results += [pscustomobject]@{
        room = $room
        exit_code = $exit
        status = $status
        wall_p99_us = $wallP99
        wall_p99_9_us = $wallP999
        gpu_p99_us = $gpuP99
        cpu_p99_us = $cpuP99
        report = $reportPath
        summary = $summaryPath
        dashboard = $dashboardPath
    }
}

$Results | ConvertTo-Json -Depth 6 | Set-Content -Path $SuiteJson -Encoding UTF8

$rows = foreach ($r in $Results) {
    $statusClass = if ($r.status -eq "fail_budget" -or $r.exit_code -ne 0) { "bad" } else { "ok" }
    @"
    <tr class="$statusClass">
      <td>$(HtmlEncode $r.room)</td>
      <td>$(HtmlEncode $r.status)</td>
      <td>$($r.wall_p99_us)</td>
      <td>$($r.wall_p99_9_us)</td>
      <td>$($r.gpu_p99_us)</td>
      <td>$($r.cpu_p99_us)</td>
      <td><a href="$(HtmlPath $r.dashboard)">dashboard</a></td>
      <td><a href="$(HtmlPath $r.report)">json</a></td>
    </tr>
"@
}
$roomCards = foreach ($r in $Results) {
    @"
    <article>
      <h2>$(HtmlEncode $r.room)</h2>
      <p class="status">$(HtmlEncode $r.status)</p>
      <p>wall p99: $($r.wall_p99_us) us<br>wall p99.9: $($r.wall_p99_9_us) us</p>
      <p><a href="$(HtmlPath $r.dashboard)">open room dashboard</a></p>
    </article>
"@
}

$html = @"
<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <title>LoA V3 Stress Lab $Stamp</title>
  <style>
    body { font-family: Segoe UI, Arial, sans-serif; margin: 24px; background: #101114; color: #eee; }
    main { max-width: 1400px; margin: 0 auto; }
    h1, h2 { font-weight: 600; }
    .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(240px, 1fr)); gap: 14px; margin: 18px 0; }
    article { border: 1px solid #333842; background: #171a20; padding: 14px; }
    table { width: 100%; border-collapse: collapse; background: #151820; }
    th, td { border: 1px solid #303541; padding: 8px; text-align: left; }
    th { background: #202532; }
    tr.ok td:first-child { border-left: 4px solid #4ba66a; }
    tr.bad td:first-child { border-left: 4px solid #d95b5b; }
    a { color: #9cc9ff; }
    .note { color: #c9c9c9; }
  </style>
</head>
<body>
<main>
  <h1>LoA V3 Stress Lab $Stamp</h1>
  <p class="note">Frames per room: $Frames. This menu links per-room graphical dashboards. Budget pass is not production readiness; reports still carry evidence gaps until real input, active scene scale, and adapter telemetry are closed.</p>
  <p>Suite JSON: <a href="$(HtmlPath $SuiteJson)">$SuiteJson</a></p>
  <section class="grid">
$($roomCards -join "`n")
  </section>
  <table>
    <thead>
      <tr><th>Room</th><th>Status</th><th>wall p99 us</th><th>wall p99.9 us</th><th>gpu p99 us</th><th>cpu p99 us</th><th>Dashboard</th><th>Report</th></tr>
    </thead>
    <tbody>
$($rows -join "`n")
    </tbody>
  </table>
</main>
</body>
</html>
"@
$html | Set-Content -Path $MenuHtml -Encoding UTF8

Write-Host "== render.v3.stress.lab.done"
Write-Host "  suite: $SuiteJson"
Write-Host "  menu: $MenuHtml"
Write-Host "  rerun.quick: tools\render-v3\run_stress_lab_bench.cmd -Quick -Open"
Write-Host "  rerun.full:  tools\render-v3\run_stress_lab_bench.cmd -Frames 5000 -Open"

if ($Open) {
    Invoke-Item $MenuHtml
}

if ($AnyBudgetFail -and -not $AllowBudgetFail) {
    Write-Error "one or more stress-lab rooms failed budget; menu/report artifacts were generated."
    exit 2
}
