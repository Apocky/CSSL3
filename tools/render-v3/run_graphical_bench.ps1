param(
    [int]$Frames = 5000,
    [switch]$Quick,
    [switch]$NoBuild,
    [switch]$NoMixed,
    [switch]$SendInput,
    [int]$InputIntervalMs = 8,
    [ValidateSet("windowed", "borderless", "exclusive")]
    [string]$WindowMode = "windowed",
    [ValidateSet("shells", "printer", "material", "hair", "portal", "arena")]
    [string]$StressRoom = "shells",
    [int]$YawA = 0,
    [int]$YawB = 500,
    [switch]$AllowBudgetFail,
    [switch]$Open
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
$Exe = Join-Path $TargetDir "debug\loa-runtime.exe"
$Analyzer = Join-Path $ScriptDir "v3_present_bench.py"
$Stamp = Get-Date -Format "yyyyMMdd_HHmmss"
$Prefix = "v3_graphical_bench_$StressRoom`_$Stamp"

$YawALabel = "{0:D3}" -f $YawA
$YawBLabel = "{0:D3}" -f $YawB
$CaptureALog = Join-Path $TargetDir "$Prefix`_yaw$YawALabel`_capture.log"
$CaptureBLog = Join-Path $TargetDir "$Prefix`_yaw$YawBLabel`_capture.log"
$TimingLog = Join-Path $TargetDir "$Prefix`_timing.log"
$CaptureA = Join-Path $TargetDir "$Prefix`_yaw$YawALabel.ppm"
$CaptureB = Join-Path $TargetDir "$Prefix`_yaw$YawBLabel.ppm"
$DiffPpm = Join-Path $TargetDir "$Prefix`_yaw$YawALabel`_vs_yaw$YawBLabel`_diff.ppm"
$ReportJson = Join-Path $TargetDir "$Prefix`_report.json"
$SummaryCsl = Join-Path $TargetDir "$Prefix`_summary.csl"
$Dashboard = Join-Path $TargetDir "$Prefix`_dashboard.html"
$CaptureABmp = Join-Path $TargetDir "$Prefix`_yaw$YawALabel.bmp"
$CaptureBBmp = Join-Path $TargetDir "$Prefix`_yaw$YawBLabel.bmp"
$DiffBmp = Join-Path $TargetDir "$Prefix`_yaw$YawALabel`_vs_yaw$YawBLabel`_diff.bmp"

function Invoke-Step {
    param(
        [string]$Name,
        [scriptblock]$Body
    )
    Write-Host "== $Name"
    & $Body
}

function Invoke-LoaRun {
    param(
        [string]$LogPath,
        [int]$FramesToRun,
        [int]$YawMilli,
        [bool]$Mixed,
        [string]$CapturePath = ""
    )
    $old = @{
        LOA_RENDER_V3 = $env:LOA_RENDER_V3
        LOA_FRAME_PACE = $env:LOA_FRAME_PACE
        LOA_V3_PRESENT_TELEMETRY = $env:LOA_V3_PRESENT_TELEMETRY
        LOA_V3_MIXED_BENCH = $env:LOA_V3_MIXED_BENCH
        LOA_QUICK_QUIT = $env:LOA_QUICK_QUIT
        LOA_V3_OBSERVER_YAW_MILLI = $env:LOA_V3_OBSERVER_YAW_MILLI
        LOA_V3_PRESENT_CAPTURE = $env:LOA_V3_PRESENT_CAPTURE
        LOA_V3_PRESENT_CAPTURE_PATH = $env:LOA_V3_PRESENT_CAPTURE_PATH
        LOA_V3_STRESS_ROOM = $env:LOA_V3_STRESS_ROOM
        CSSL_LOA_WINDOW = $env:CSSL_LOA_WINDOW
    }
    try {
        $env:LOA_RENDER_V3 = "1"
        $env:LOA_FRAME_PACE = "poll"
        $env:LOA_V3_PRESENT_TELEMETRY = "1"
        $env:LOA_QUICK_QUIT = [string]$FramesToRun
        $env:LOA_V3_OBSERVER_YAW_MILLI = [string]$YawMilli
        $env:LOA_V3_STRESS_ROOM = $StressRoom
        $env:CSSL_LOA_WINDOW = $WindowMode
        if ($Mixed) {
            $env:LOA_V3_MIXED_BENCH = "1"
        } else {
            Remove-Item Env:\LOA_V3_MIXED_BENCH -ErrorAction SilentlyContinue
        }
        if ($CapturePath) {
            $env:LOA_V3_PRESENT_CAPTURE = "1"
            $env:LOA_V3_PRESENT_CAPTURE_PATH = $CapturePath
        } else {
            Remove-Item Env:\LOA_V3_PRESENT_CAPTURE -ErrorAction SilentlyContinue
            Remove-Item Env:\LOA_V3_PRESENT_CAPTURE_PATH -ErrorAction SilentlyContinue
        }
        $stdoutTmp = "$LogPath.stdout.tmp"
        $stderrTmp = "$LogPath.stderr.tmp"
        Remove-Item $stdoutTmp, $stderrTmp -ErrorAction SilentlyContinue
        try {
            $proc = Start-Process -FilePath $Exe `
                -WorkingDirectory $CompilerRoot `
                -NoNewWindow `
                -Wait `
                -PassThru `
                -RedirectStandardOutput $stdoutTmp `
                -RedirectStandardError $stderrTmp
            $stdout = if (Test-Path $stdoutTmp) { Get-Content -Raw $stdoutTmp } else { "" }
            $stderr = if (Test-Path $stderrTmp) { Get-Content -Raw $stderrTmp } else { "" }
            ($stdout + $stderr) | Set-Content -Path $LogPath -Encoding UTF8
            if ($proc.ExitCode -ne 0) {
                throw "loa-runtime failed with exit $($proc.ExitCode)"
            }
        } finally {
            Remove-Item $stdoutTmp, $stderrTmp -ErrorAction SilentlyContinue
        }
    } finally {
        foreach ($key in $old.Keys) {
            if ($null -eq $old[$key]) {
                Remove-Item "Env:\$key" -ErrorAction SilentlyContinue
            } else {
                Set-Item "Env:\$key" $old[$key]
            }
        }
    }
}

function Convert-Preview {
    param([string]$Ppm, [string]$Bmp)
    & python $Analyzer convert-image --image $Ppm --out-bmp $Bmp | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "convert-image failed for $Ppm"
    }
}

function HtmlPath {
    param([string]$Path)
    return ([System.Uri]::new((Resolve-Path $Path).Path)).AbsoluteUri
}

if (-not $NoBuild) {
    Invoke-Step "build.loa-runtime" {
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
}

if (-not (Test-Path $Exe)) {
    throw "loa-runtime.exe not found: $Exe"
}

Invoke-Step "capture.yaw$YawALabel" {
    Invoke-LoaRun -LogPath $CaptureALog -FramesToRun 3 -YawMilli $YawA -Mixed:$false -CapturePath $CaptureA
}

Invoke-Step "capture.yaw$YawBLabel" {
    Invoke-LoaRun -LogPath $CaptureBLog -FramesToRun 3 -YawMilli $YawB -Mixed:$false -CapturePath $CaptureB
}

if ($SendInput) {
    Invoke-Step "timing.with.input.diagnostic" {
        $inputArgs = @(
            "run",
            "--frames", [string]$Frames,
            "--window-mode", $WindowMode,
            "--stress-room", $StressRoom,
            "--send-input",
            "--input-interval-ms", [string]$InputIntervalMs,
            "--log", $TimingLog
        )
        if (-not $NoMixed) {
            $inputArgs += "--mixed-bench"
        }
        & python $Analyzer @inputArgs
        if ($LASTEXITCODE -ne 0) {
            throw "timing input run failed with exit $LASTEXITCODE"
        }
    }
} else {
    Invoke-Step "timing.windowed.$Frames" {
        Invoke-LoaRun -LogPath $TimingLog -FramesToRun $Frames -YawMilli $YawA -Mixed:(!$NoMixed)
    }
}

Invoke-Step "analyze.report" {
    $analyzeArgs = @(
        "analyze",
        "--log", $TimingLog,
        "--image", $CaptureA,
        "--compare-image", $CaptureB,
        "--compare-label", "yaw$YawBLabel",
        "--diff-image", $DiffPpm,
        "--out-json", $ReportJson,
        "--print-csl",
        "--accepted-culling-proof"
    )
    if ($StressRoom -ne "shells") {
        $analyzeArgs += "--rich-scene"
    }
    $analysis = & python $Analyzer @analyzeArgs 2>&1
    $exit = $LASTEXITCODE
    $analysis | Tee-Object -FilePath $SummaryCsl
    if ($exit -ne 0) {
        throw "benchmark analysis failed with exit $exit"
    }
}

$Report = Get-Content -Raw $ReportJson | ConvertFrom-Json
$BenchmarkStatus = [string]$Report.verdict.status

Invoke-Step "preview.bmp" {
    Convert-Preview -Ppm $CaptureA -Bmp $CaptureABmp
    Convert-Preview -Ppm $CaptureB -Bmp $CaptureBBmp
    Convert-Preview -Ppm $DiffPpm -Bmp $DiffBmp
}

$summaryHtml = [System.Net.WebUtility]::HtmlEncode((Get-Content -Raw $SummaryCsl))
$html = @"
<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <title>LoA V3 Graphical Benchmark $Stamp</title>
  <style>
    body { font-family: Segoe UI, Arial, sans-serif; margin: 24px; background: #111; color: #eee; }
    main { max-width: 1400px; margin: 0 auto; }
    h1, h2 { font-weight: 600; }
    pre { white-space: pre-wrap; background: #1d1d1d; padding: 16px; border: 1px solid #333; overflow-x: auto; }
    .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(320px, 1fr)); gap: 16px; }
    figure { margin: 0; background: #1a1a1a; border: 1px solid #333; padding: 12px; }
    img { width: 100%; height: auto; image-rendering: auto; display: block; background: #000; }
    figcaption { margin-top: 8px; color: #bbb; font-size: 14px; }
    a { color: #9cc9ff; }
  </style>
</head>
<body>
<main>
  <h1>LoA V3 Graphical Benchmark $StressRoom · $Stamp</h1>
  <p>Report: <a href="$(HtmlPath $ReportJson)">$ReportJson</a></p>
  <p>Summary: <a href="$(HtmlPath $SummaryCsl)">$SummaryCsl</a></p>
  <h2>Verdict</h2>
  <pre>$summaryHtml</pre>
  <h2>Images</h2>
  <div class="grid">
    <figure><img src="$(HtmlPath $CaptureABmp)" alt="Yaw $YawALabel capture"><figcaption>yaw $YawALabel</figcaption></figure>
    <figure><img src="$(HtmlPath $CaptureBBmp)" alt="Yaw $YawBLabel capture"><figcaption>yaw $YawBLabel</figcaption></figure>
    <figure><img src="$(HtmlPath $DiffBmp)" alt="Amplified yaw diff"><figcaption>amplified yaw diff</figcaption></figure>
  </div>
</main>
</body>
</html>
"@
$html | Set-Content -Path $Dashboard -Encoding UTF8

Write-Host "== render.v3.graphical.bench.done"
Write-Host "  status: $BenchmarkStatus"
Write-Host "  stress_room: $StressRoom"
Write-Host "  report: $ReportJson"
Write-Host "  summary: $SummaryCsl"
Write-Host "  dashboard: $Dashboard"
Write-Host "  images: $CaptureABmp ; $CaptureBBmp ; $DiffBmp"
Write-Host "  rerun: tools\render-v3\run_graphical_bench.cmd -Quick"
Write-Host "  full:  tools\render-v3\run_graphical_bench.cmd -Frames 5000 -Open"

if ($Open) {
    Invoke-Item $Dashboard
}

if ($BenchmarkStatus -eq "fail_budget" -and -not $AllowBudgetFail) {
    Write-Error "render.v3 benchmark failed budget gate; dashboard/report were still generated."
    exit 2
}
