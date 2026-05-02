# § scripts/setup/1_install_daemon.ps1 · Register Windows-Task-Scheduler for loa-orchestrator-daemon
# ════════════════════════════════════════════════════════════════════════════
#
# § T11-W16 · The Infinity Engine · 24/7 local daemon registration
#
# Usage (Administrator PowerShell):
#   .\scripts\setup\1_install_daemon.ps1                # register (or update)
#   .\scripts\setup\1_install_daemon.ps1 -Action start  # start now
#   .\scripts\setup\1_install_daemon.ps1 -Action stop   # stop running
#   .\scripts\setup\1_install_daemon.ps1 -Action remove # unregister entirely
#   .\scripts\setup\1_install_daemon.ps1 -Action status # show current state

[CmdletBinding()]
param(
    [ValidateSet('register', 'start', 'stop', 'remove', 'status')]
    [string]$Action = 'register'
)

$TaskName = "LoA-Engine-Daemon"
$Description = "The Infinity Engine · 24/7 self-learning daemon · sovereign-revocable"
$DaemonPath = "C:\Users\Apocky\.loa\loa-orchestrator-daemon.exe"
$WorkDir = "C:\Users\Apocky\.loa"
$LogFile = "$WorkDir\daemon.log"

function Test-Admin {
    $current = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($current)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Show-Status {
    $task = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
    if ($null -eq $task) {
        Write-Host "✗ $TaskName : NOT REGISTERED" -ForegroundColor Yellow
        return
    }
    $info = Get-ScheduledTaskInfo -TaskName $TaskName
    Write-Host "✓ $TaskName : $($task.State)" -ForegroundColor Green
    Write-Host "  Last run     : $($info.LastRunTime)"
    Write-Host "  Last result  : $($info.LastTaskResult)"
    Write-Host "  Next run     : $($info.NextRunTime)"
    Write-Host "  Daemon path  : $DaemonPath"
    if (Test-Path $LogFile) {
        $size = (Get-Item $LogFile).Length
        Write-Host "  Log file     : $LogFile ($size bytes)"
    }
}

switch ($Action) {
    'status' {
        Show-Status
        exit 0
    }
    'start' {
        if (-not (Test-Admin)) { Write-Host "✗ requires Administrator PowerShell" -ForegroundColor Red ; exit 1 }
        Start-ScheduledTask -TaskName $TaskName
        Write-Host "✓ $TaskName started" -ForegroundColor Green
        Show-Status
        exit 0
    }
    'stop' {
        if (-not (Test-Admin)) { Write-Host "✗ requires Administrator PowerShell" -ForegroundColor Red ; exit 1 }
        Stop-ScheduledTask -TaskName $TaskName
        Write-Host "✓ $TaskName stopped" -ForegroundColor Green
        exit 0
    }
    'remove' {
        if (-not (Test-Admin)) { Write-Host "✗ requires Administrator PowerShell" -ForegroundColor Red ; exit 1 }
        Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false -ErrorAction SilentlyContinue
        Write-Host "✓ $TaskName unregistered" -ForegroundColor Green
        exit 0
    }
    'register' {
        if (-not (Test-Admin)) {
            Write-Host "✗ requires Administrator PowerShell" -ForegroundColor Red
            Write-Host "  Right-click PowerShell · 'Run as Administrator' · re-run this script" -ForegroundColor Yellow
            exit 1
        }
        if (-not (Test-Path $DaemonPath)) {
            Write-Host "✗ daemon binary not found at $DaemonPath" -ForegroundColor Red
            Write-Host "  Run: cd compiler-rs ; cargo build --release -p cssl-host-persistent-orchestrator --bin loa-orchestrator-daemon" -ForegroundColor Yellow
            Write-Host "  Then copy compiler-rs\target\release\loa-orchestrator-daemon.exe to $DaemonPath" -ForegroundColor Yellow
            exit 1
        }

        New-Item -ItemType Directory -Force -Path $WorkDir | Out-Null

        $Action_ = New-ScheduledTaskAction `
            -Execute $DaemonPath `
            -WorkingDirectory $WorkDir

        $Trigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME

        $Settings = New-ScheduledTaskSettingsSet `
            -AllowStartIfOnBatteries `
            -DontStopIfGoingOnBatteries `
            -RestartCount 999 `
            -RestartInterval (New-TimeSpan -Minutes 1) `
            -ExecutionTimeLimit (New-TimeSpan -Days 365)

        $Principal = New-ScheduledTaskPrincipal `
            -UserId $env:USERNAME `
            -LogonType Interactive `
            -RunLevel Limited

        Register-ScheduledTask `
            -TaskName $TaskName `
            -Description $Description `
            -Action $Action_ `
            -Trigger $Trigger `
            -Settings $Settings `
            -Principal $Principal `
            -Force | Out-Null

        Write-Host "✓ $TaskName registered" -ForegroundColor Green
        Write-Host "  starts at next login OR via -Action start" -ForegroundColor Gray
        Show-Status
        exit 0
    }
}
