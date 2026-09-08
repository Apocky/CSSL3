[CmdletBinding()]
param(
    [string]$TaskName = 'Apocrypha Outbound Worker',
    [string]$EnvFile = 'C:\Users\Apocky\Documents\Tarot\Chaos\New\chaos-tarot\.env.local'
)

$ErrorActionPreference = 'Stop'
$workerRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$edgeRoot = (Resolve-Path (Join-Path $workerRoot '..\..')).Path
$node = (Get-Command node -ErrorAction Stop).Source
if ($edgeRoot.Contains('"') -or $EnvFile.Contains('"') -or $node.Contains('"')) { throw 'Task paths cannot contain double quotes' }
$arguments = "--env-file=`"$EnvFile`" --import tsx scripts/apocrypha-worker/runner.ts"
$action = New-ScheduledTaskAction -Execute $node -Argument $arguments -WorkingDirectory $edgeRoot
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
$settings = New-ScheduledTaskSettingsSet -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1) -ExecutionTimeLimit (New-TimeSpan -Days 3650) -StartWhenAvailable
$principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -LogonType Interactive -RunLevel Limited

Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $trigger -Settings $settings -Principal $principal -Force | Out-Null
Start-ScheduledTask -TaskName $TaskName
Write-Output "Registered and started $TaskName"
