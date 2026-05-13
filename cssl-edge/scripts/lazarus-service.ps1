param(
  [ValidateSet('install', 'uninstall', 'start', 'stop', 'restart', 'status', 'smoke')]
  [string] $Action = 'status',
  [string] $ServiceName = 'ApockyLazarusRunner',
  [string] $ControlUrl = 'https://www.apocky.com',
  [string] $RunnerId = 'apocky-windows-runner',
  [string] $RunnerLabel = 'Apocky Lazarus Windows runner',
  [string] $NssmPath = ''
)

$ErrorActionPreference = 'Stop'

$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$ProgramDataRoot = Join-Path $env:ProgramData 'Apocky\Lazarus'
$LogDir = Join-Path $ProgramDataRoot 'logs'
$StdoutLog = Join-Path $LogDir 'runner.out.log'
$StderrLog = Join-Path $LogDir 'runner.err.log'

function Write-Info([string] $Message) {
  Write-Host "[lazarus-service] $Message"
}

function Test-IsAdmin {
  $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
  $principal = [Security.Principal.WindowsPrincipal]::new($identity)
  return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Get-NssmCommand {
  if ($NssmPath.Trim()) {
    if (-not (Test-Path $NssmPath)) {
      throw "NSSM path not found: $NssmPath"
    }
    return (Resolve-Path $NssmPath).Path
  }

  $cmd = Get-Command nssm.exe -ErrorAction SilentlyContinue
  if (-not $cmd) { $cmd = Get-Command nssm -ErrorAction SilentlyContinue }
  if (-not $cmd) {
    throw 'NSSM was not found on PATH. Install NSSM from https://nssm.cc/download or pass -NssmPath C:\path\to\nssm.exe.'
  }
  return $cmd.Source
}

function Assert-Npm() {
  $npm = Get-Command npm.cmd -ErrorAction SilentlyContinue
  if (-not $npm) { $npm = Get-Command npm -ErrorAction SilentlyContinue }
  if (-not $npm) { throw 'npm was not found on PATH.' }
}

function Get-ServiceOrNull([string] $Name) {
  return Get-Service -Name $Name -ErrorAction SilentlyContinue
}

function Invoke-Nssm([string[]] $Args) {
  $nssm = Get-NssmCommand
  & $nssm @Args
  if ($LASTEXITCODE -ne 0) {
    throw "nssm failed: $($Args -join ' ')"
  }
}

function Test-LocalRunnerToken {
  if ($env:LAZARUS_RUNNER_TOKEN -and $env:LAZARUS_RUNNER_TOKEN.Trim()) { return $true }
  $envFile = Join-Path $Root '.env.local'
  if (-not (Test-Path $envFile)) { return $false }
  return [bool](Select-String -Path $envFile -Pattern '^\s*LAZARUS_RUNNER_TOKEN\s*=\s*.+' -Quiet)
}

function Ensure-LogDir() {
  New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
}

function Install-Service() {
  if (-not (Test-IsAdmin)) {
    throw 'Install/uninstall must run from an elevated PowerShell session.'
  }
  Assert-Npm
  Ensure-LogDir

  if (Get-ServiceOrNull $ServiceName) {
    throw "Service already exists: $ServiceName. Use -Action uninstall first or choose another -ServiceName."
  }

  if (-not (Test-LocalRunnerToken)) {
    Write-Info 'WARNING: LAZARUS_RUNNER_TOKEN was not found in this process or .env.local. The service will fail runner auth until the token is configured.'
  }

  $cmd = $env:ComSpec
  if (-not $cmd) { $cmd = 'C:\Windows\System32\cmd.exe' }

  Invoke-Nssm @('install', $ServiceName, $cmd)
  Invoke-Nssm @('set', $ServiceName, 'AppDirectory', $Root)
  Invoke-Nssm @('set', $ServiceName, 'AppParameters', '/d /s /c "npm run lazarus:runner"')
  Invoke-Nssm @('set', $ServiceName, 'DisplayName', 'Apocky Lazarus Runner')
  Invoke-Nssm @('set', $ServiceName, 'Description', 'Runs the Lazarus task runner against apocky.com in stub-safe mode by default.')
  Invoke-Nssm @('set', $ServiceName, 'Start', 'SERVICE_AUTO_START')
  Invoke-Nssm @('set', $ServiceName, 'AppStdout', $StdoutLog)
  Invoke-Nssm @('set', $ServiceName, 'AppStderr', $StderrLog)
  Invoke-Nssm @('set', $ServiceName, 'AppRotateFiles', '1')
  Invoke-Nssm @('set', $ServiceName, 'AppRotateOnline', '1')
  Invoke-Nssm @('set', $ServiceName, 'AppRotateSeconds', '86400')
  Invoke-Nssm @('set', $ServiceName, 'AppRotateBytes', '10485760')
  Invoke-Nssm @('set', $ServiceName, 'AppRestartDelay', '5000')
  Invoke-Nssm @(
    'set',
    $ServiceName,
    'AppEnvironmentExtra',
    "LAZARUS_CONTROL_URL=$ControlUrl",
    'LAZARUS_ENABLE_MODEL_CALLS=0',
    "LAZARUS_RUNNER_ID=$RunnerId",
    "LAZARUS_RUNNER_LABEL=$RunnerLabel"
  )

  Write-Info "Installed $ServiceName. Logs: $LogDir"
  Write-Info 'Start with: npm run lazarus:service:start'
}

function Uninstall-Service() {
  if (-not (Test-IsAdmin)) {
    throw 'Install/uninstall must run from an elevated PowerShell session.'
  }
  if (-not (Get-ServiceOrNull $ServiceName)) {
    Write-Info "Service does not exist: $ServiceName"
    return
  }
  try { Stop-Service -Name $ServiceName -ErrorAction SilentlyContinue } catch { }
  Invoke-Nssm @('remove', $ServiceName, 'confirm')
  Write-Info "Removed $ServiceName. Logs remain at: $LogDir"
}

function Show-Status() {
  $service = Get-ServiceOrNull $ServiceName
  if (-not $service) {
    Write-Info "Service not installed: $ServiceName"
    Write-Info 'Install with: npm run lazarus:service:install'
    return
  }
  Write-Info "$ServiceName status: $($service.Status)"
  Write-Info "Logs: $LogDir"
}

function Run-Smoke() {
  Assert-Npm
  if (-not (Test-LocalRunnerToken)) {
    throw 'LAZARUS_RUNNER_TOKEN was not found in this process or .env.local. Configure it before production smoke.'
  }

  $previous = @{}
  foreach ($key in @('LAZARUS_CONTROL_URL', 'LAZARUS_ENABLE_MODEL_CALLS', 'LAZARUS_ONCE', 'LAZARUS_RUNNER_ID', 'LAZARUS_RUNNER_LABEL')) {
    $previous[$key] = [Environment]::GetEnvironmentVariable($key, 'Process')
  }

  try {
    $env:LAZARUS_CONTROL_URL = $ControlUrl
    $env:LAZARUS_ENABLE_MODEL_CALLS = '0'
    $env:LAZARUS_ONCE = '1'
    $env:LAZARUS_RUNNER_ID = "$RunnerId-smoke"
    $env:LAZARUS_RUNNER_LABEL = "$RunnerLabel smoke"
    Push-Location $Root
    try {
      npm run lazarus:runner
      if ($LASTEXITCODE -ne 0) { throw "lazarus smoke failed with exit code $LASTEXITCODE" }
    } finally {
      Pop-Location
    }
  } finally {
    foreach ($key in $previous.Keys) {
      if ($null -eq $previous[$key]) { Remove-Item "Env:$key" -ErrorAction SilentlyContinue }
      else { [Environment]::SetEnvironmentVariable($key, [string] $previous[$key], 'Process') }
    }
  }
}

switch ($Action) {
  'install' { Install-Service }
  'uninstall' { Uninstall-Service }
  'start' {
    if (-not (Get-ServiceOrNull $ServiceName)) { throw "Service not installed: $ServiceName" }
    Start-Service -Name $ServiceName
    Show-Status
  }
  'stop' {
    if (-not (Get-ServiceOrNull $ServiceName)) { throw "Service not installed: $ServiceName" }
    Stop-Service -Name $ServiceName
    Show-Status
  }
  'restart' {
    if (-not (Get-ServiceOrNull $ServiceName)) { throw "Service not installed: $ServiceName" }
    Restart-Service -Name $ServiceName
    Show-Status
  }
  'status' { Show-Status }
  'smoke' { Run-Smoke }
}