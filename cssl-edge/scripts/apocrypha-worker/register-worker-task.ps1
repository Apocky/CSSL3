[CmdletBinding()]
param(
    [string]$TaskName = 'Apocrypha Outbound Worker',
    [string]$EnvFile = 'C:\Users\Apocky\Documents\Tarot\Chaos\New\chaos-tarot\.env.local'
)

$ErrorActionPreference = 'Stop'
$runner = Join-Path (Split-Path -Parent $MyInvocation.MyCommand.Path) 'run-production.ps1'
$escapedRunner = $runner.Replace("'", "''")
$escapedEnv = $EnvFile.Replace("'", "''")
$arguments = "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File '$escapedRunner' -EnvFile '$escapedEnv' -Mode run"
$action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument $arguments
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
$settings = New-ScheduledTaskSettingsSet -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1) -ExecutionTimeLimit (New-TimeSpan -Days 3650) -StartWhenAvailable
$principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -LogonType Interactive -RunLevel Limited

Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $trigger -Settings $settings -Principal $principal -Force | Out-Null
Start-ScheduledTask -TaskName $TaskName
Write-Output "Registered and started $TaskName"
