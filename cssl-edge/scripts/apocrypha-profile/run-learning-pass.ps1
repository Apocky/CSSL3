# One autonomous learning pass. Registered as a scheduled task; safe to run by hand.
#
# Two steps, in order, because the second depends on the first:
#   1. index any transcripts written since the last run -- new conversations are not learnable
#      until they are in the index
#   2. distil forward from the last FINISHED pass's watermark
#
# Runs against the resident engine on 19128, which serves both lanes from one port. This pass takes
# a slot, never VRAM, so it cannot evict live chat -- but it is still real contention, which is why
# it is scheduled for the small hours rather than run continuously.

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$Repo = 'C:\Users\Apocky\source\repos'
# WAS 'CSSLv3-wt-apocrypha-desktop\cssl-edge', a worktree deleted in the
# consolidation onto worklane-prod. This script then lived in worklane-prod and
# pointed at the tree it had been moved out of: step 1 ran, indexed 413 files,
# and step 2 died on Push-Location with the task still reporting a generic 1.
# The fifth stale `Documents`/old-worktree path found on 2026-09-17.
$Edge = Join-Path $Repo 'CSSLv3-wt-worklane-prod\cssl-edge'
$LogDir = 'C:\Apocrypha\profile'
$Log = Join-Path $LogDir ('learn-{0}.log' -f (Get-Date -Format 'yyyyMMdd'))

if (-not (Test-Path $LogDir)) { New-Item -ItemType Directory -Force -Path $LogDir | Out-Null }

function Write-Log([string]$Message) {
    $line = '[{0}] {1}' -f (Get-Date -Format 'HH:mm:ss'), $Message
    Add-Content -Path $Log -Value $line
    Write-Output $line
}

Write-Log '=== learning pass start ==='

# The engine must actually be up. Distilling against a dead engine would burn the whole window on
# failed batches and then record a finished pass, advancing the watermark over chunks it never read.
try {
    $probe = Invoke-WebRequest -Uri 'http://127.0.0.1:19128/health' -TimeoutSec 10 -UseBasicParsing
    if ($probe.StatusCode -ne 200) { throw "engine health HTTP $($probe.StatusCode)" }
} catch {
    Write-Log "ABORT: engine unreachable on 19128 -- $($_.Exception.Message)"
    exit 0
}

Write-Log 'step 1: indexing new transcripts'
foreach ($root in @("$env:USERPROFILE\.claude\projects", "$env:USERPROFILE\.codex")) {
    if (-not (Test-Path $root)) { Write-Log "  skip (absent): $root"; continue }
    try {
        $out = & python (Join-Path $Repo 'anamnesis\anamnesis.py') index-transcripts $root --days 7 2>&1 | Out-String
        $parsed = $out | ConvertFrom-Json
        Write-Log ("  {0}: +{1} files, {2} messages, {3} unrecognized" -f `
            (Split-Path $root -Leaf), $parsed.files_indexed, $parsed.messages_new, $parsed.schema_unrecognized.Count)
        if ($parsed.schema_unrecognized.Count -gt 0) {
            # A file whose every line is an unknown envelope means the upstream format moved. Left
            # unreported it becomes a silent, permanent hole in the corpus.
            Write-Log "  WARN unrecognized schema: $($parsed.schema_unrecognized[0])"
        }
    } catch {
        Write-Log "  ERROR indexing ${root}: $($_.Exception.Message)"
    }
}

Write-Log 'step 2: distilling forward from the last finished watermark'
Push-Location $Edge
try {
    & node --import tsx 'scripts/apocrypha-profile/distil.ts' 2>&1 | ForEach-Object { Write-Log "  $_" }
    Write-Log "distil exit code: $LASTEXITCODE"
} catch {
    Write-Log "ERROR distilling: $($_.Exception.Message)"
} finally {
    Pop-Location
}

Write-Log '=== learning pass done ==='
