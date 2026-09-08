[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [ValidateSet('Plan', 'Install', 'Uninstall')]
    [string]$Operation = 'Install',
    [string]$TaskName = 'Apocky-MetaHarness-MCP-Direct',
    [string]$LegacyTaskName = 'Apocky-MetaHarness-MCP',
    [string]$MetaHarnessRoot = 'C:\Users\Apocky\source\repos\MetaHarness',
    [string]$EnvFile = 'C:\Users\Apocky\Documents\Tarot\Chaos\New\chaos-tarot\.env.local',
    [switch]$RotateCredential,
    [switch]$DoNotStart
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$credentialName = 'APOCKY_METAHARNESS_MCP_TOKEN'
$expectedEndpoint = 'http://127.0.0.1:8765/mcp'
$expectedEntropy = 'Apocrypha.MetaHarness.observer-capability.v1'
$tokenPattern = '^[A-Za-z0-9_-]{43}$'

function Assert-TaskName {
    param([string]$Candidate)
    if ([string]::IsNullOrWhiteSpace($Candidate) -or $Candidate.Length -gt 120 -or
        $Candidate -match '[\\/\x00-\x1f]' -or $Candidate -ceq $LegacyTaskName) {
        throw 'The direct task name is invalid or aliases the disabled legacy task.'
    }
}

function Resolve-RegularFile {
    param([string]$Path, [string]$Label)
    if ([string]::IsNullOrWhiteSpace($Path) -or -not [IO.Path]::IsPathRooted($Path)) {
        throw "$Label must be an absolute path."
    }
    $resolved = (Resolve-Path -LiteralPath $Path).Path
    $item = Get-Item -LiteralPath $resolved -Force
    if (-not $item.PSIsContainer -and -not ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        return $resolved
    }
    throw "$Label must be a regular non-reparse file."
}

function Resolve-RegularDirectory {
    param([string]$Path, [string]$Label)
    if ([string]::IsNullOrWhiteSpace($Path) -or -not [IO.Path]::IsPathRooted($Path)) {
        throw "$Label must be an absolute path."
    }
    $resolved = (Resolve-Path -LiteralPath $Path).Path
    $item = Get-Item -LiteralPath $resolved -Force
    if ($item.PSIsContainer -and -not ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        return $resolved
    }
    throw "$Label must be a regular non-reparse directory."
}

function Read-DotEnv {
    param([string]$Path)
    $resolved = Resolve-RegularFile $Path 'Environment file'
    $item = Get-Item -LiteralPath $resolved -Force
    if ($item.Length -gt 1MB) { throw 'Environment file exceeds the bounded reader limit.' }
    $raw = [IO.File]::ReadAllText($resolved)
    $values = @{}
    foreach ($line in ($raw -split "`r?`n")) {
        if ($line -match '^\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)\s*$') {
            $name = $Matches[1]
            $value = $Matches[2].Trim()
            if (($value.StartsWith('"') -and $value.EndsWith('"')) -or
                ($value.StartsWith("'") -and $value.EndsWith("'"))) {
                $value = $value.Substring(1, $value.Length - 2)
            }
            if ($values.ContainsKey($name)) { throw "Environment file repeats $name." }
            $values[$name] = $value
        }
    }
    return [pscustomobject]@{ Path = $resolved; Raw = $raw; Values = $values }
}

function Set-DotEnvValue {
    param([string]$Path, [string]$Raw, [string]$Name, [string]$Value)
    if ($Value -notmatch $tokenPattern) { throw 'Refusing to persist a malformed bearer.' }
    $escaped = [Regex]::Escape($Name)
    $matches = [Regex]::Matches($Raw, "(?m)^\s*(?:export\s+)?$escaped\s*=.*$")
    if ($matches.Count -gt 1) { throw "Environment file repeats $Name." }
    $newline = if ($Raw.Contains("`r`n")) { "`r`n" } else { "`n" }
    $line = "$Name=$Value"
    if ($matches.Count -eq 1) {
        $updated = [Regex]::Replace($Raw, "(?m)^\s*(?:export\s+)?$escaped\s*=.*$", $line)
    }
    else {
        $separator = if ($Raw.Length -eq 0 -or $Raw.EndsWith("`n")) { '' } else { $newline }
        $updated = $Raw + $separator + $line + $newline
    }
    [IO.File]::WriteAllText($Path, $updated, [Text.UTF8Encoding]::new($false))
}

function New-ObserverToken {
    $bytes = New-Object byte[] 32
    $rng = [Security.Cryptography.RandomNumberGenerator]::Create()
    try { $rng.GetBytes($bytes) } finally { $rng.Dispose() }
    $token = [Convert]::ToBase64String($bytes).TrimEnd('=').Replace('+', '-').Replace('/', '_')
    [Array]::Clear($bytes, 0, $bytes.Length)
    if ($token -cnotmatch $tokenPattern) { throw 'Generated bearer failed its shape check.' }
    return $token
}

function Get-FileSha256 {
    param([string]$Path)
    $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($sha.ComputeHash($stream))).Replace('-', '').ToLowerInvariant()
    }
    finally {
        $sha.Dispose()
        $stream.Dispose()
    }
}

function Assert-FederatorBootstrapContract {
    param([string]$Executable)
    $help = & $Executable 'bootstrap-metaharness-capability' '--help' 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0 -or $help -notmatch '(?s)bootstrap-metaharness-capability.*--output.*--owner-id.*--endpoint') {
        throw 'Configured memory executable lacks the required MetaHarness capability-bootstrap contract.'
    }
}

function Get-McpPayload {
    param([object]$Response)
    if ($null -eq $Response -or $null -eq $Response.result) {
        throw 'MetaHarness health returned no MCP result.'
    }
    if ($Response.result.PSObject.Properties.Name -contains 'structuredContent') {
        return $Response.result.structuredContent
    }
    $text = $Response.result.content[0].text
    if ([string]::IsNullOrWhiteSpace($text)) { throw 'MetaHarness health returned no structured payload.' }
    return $text | ConvertFrom-Json
}

function Test-ObserverHealth {
    param([string]$Token)
    $headers = @{
        Authorization = "Bearer $Token"
        Accept = 'application/json, text/event-stream'
        'MCP-Protocol-Version' = '2025-06-18'
    }
    $initialize = @{
        jsonrpc = '2.0'; id = 1; method = 'initialize'
        params = @{ protocolVersion = '2025-06-18'; capabilities = @{}; clientInfo = @{ name = 'apocrypha-resident-check'; version = '1' } }
    } | ConvertTo-Json -Depth 6 -Compress
    $call = @{
        jsonrpc = '2.0'; id = 2; method = 'tools/call'
        params = @{ name = 'health'; arguments = @{} }
    } | ConvertTo-Json -Depth 6 -Compress

    $null = Invoke-RestMethod -Uri $expectedEndpoint -Method Post -Headers $headers -ContentType 'application/json' -Body $initialize -TimeoutSec 5
    $response = Invoke-RestMethod -Uri $expectedEndpoint -Method Post -Headers $headers -ContentType 'application/json' -Body $call -TimeoutSec 10
    $payload = Get-McpPayload $response
    if ($payload.ok -ne $true -or $payload.authority -cne 'none' -or
        $payload.execution_authorized -ne $false -or
        $payload.transport.host -cne '127.0.0.1' -or
        [int]$payload.transport.port -ne 8765 -or $payload.transport.path -cne '/mcp') {
        throw 'MetaHarness authenticated health violated the closed observer contract.'
    }
}

function Test-UnauthenticatedDenial {
    $body = @{
        jsonrpc = '2.0'; id = 1; method = 'initialize'
        params = @{ protocolVersion = '2025-06-18'; capabilities = @{}; clientInfo = @{ name = 'apocrypha-denial-check'; version = '1' } }
    } | ConvertTo-Json -Depth 6 -Compress
    try {
        $null = Invoke-WebRequest -UseBasicParsing -Uri $expectedEndpoint -Method Post -Headers @{ Accept = 'application/json, text/event-stream' } -ContentType 'application/json' -Body $body -TimeoutSec 5
    }
    catch {
        $status = [int]$_.Exception.Response.StatusCode
        if ($status -eq 401) { return }
        throw 'MetaHarness unauthenticated probe failed with an unexpected status.'
    }
    throw 'MetaHarness admitted an unauthenticated observer request.'
}

Assert-TaskName $TaskName
$root = Resolve-RegularDirectory $MetaHarnessRoot 'MetaHarness root'
if (-not (Test-Path -LiteralPath (Join-Path $root 'PRIME_DIRECTIVE.md') -PathType Leaf) -or
    -not (Test-Path -LiteralPath (Join-Path $root 'pyproject.toml') -PathType Leaf)) {
    throw 'MetaHarness authority or package marker is missing.'
}
$entryPoint = Resolve-RegularFile (Join-Path $root '.venv\Scripts\meta-harness-mcp.exe') 'MetaHarness MCP entry point'
$sourceRoot = Resolve-RegularDirectory (Join-Path $root 'src') 'MetaHarness source root'
$sitePackages = Resolve-RegularDirectory (Join-Path $root '.venv\Lib\site-packages') 'MetaHarness site-packages'
$editable = @(Get-ChildItem -LiteralPath $sitePackages -Filter '__editable__.meta_harness-*.pth' -File)
if ($editable.Count -ne 1) { throw 'MetaHarness editable package provenance is missing or ambiguous.' }
$installedSource = Resolve-RegularDirectory (([IO.File]::ReadAllText($editable[0].FullName)).Trim()) 'Installed MetaHarness source'
if (-not [StringComparer]::OrdinalIgnoreCase.Equals($installedSource, $sourceRoot)) {
    throw 'MetaHarness editable package does not resolve to the canonical repository source.'
}
if (@(Get-ChildItem -LiteralPath $sitePackages -Filter 'mcp-1.28.1.dist-info' -Directory).Count -ne 1) {
    throw 'The pinned MetaHarness MCP dependency is absent.'
}
$venvPython = Resolve-RegularFile (Join-Path $root '.venv\Scripts\python.exe') 'MetaHarness virtual-environment Python'
$venvConfig = Resolve-RegularFile (Join-Path $root '.venv\pyvenv.cfg') 'MetaHarness virtual-environment configuration'
$homeLines = @([IO.File]::ReadAllLines($venvConfig) | Where-Object { $_ -match '^\s*home\s*=\s*(.+?)\s*$' })
if ($homeLines.Count -ne 1 -or $homeLines[0] -notmatch '^\s*home\s*=\s*(.+?)\s*$') {
    throw 'MetaHarness virtual-environment base interpreter is missing or ambiguous.'
}
$basePython = Resolve-RegularFile (Join-Path $Matches[1] 'python.exe') 'MetaHarness base Python'
$taskExecutable = $venvPython
$taskArguments = '-m meta_harness.mcp_server'

if ($Operation -eq 'Uninstall') {
    $existing = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
    if ($null -ne $existing) {
        $execute = [string]$existing.Actions[0].Execute
        $arguments = [string]$existing.Actions[0].Arguments
        if (-not [StringComparer]::OrdinalIgnoreCase.Equals($execute, $taskExecutable) -or $arguments -cne $taskArguments) {
            throw 'Refusing to remove a task whose executable is not the canonical MetaHarness observer.'
        }
        if ($PSCmdlet.ShouldProcess($TaskName, 'stop and unregister direct MetaHarness observer task')) {
            Stop-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
            Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
        }
    }
    [ordered]@{
        schema = 'apocrypha.metaharness-resident.result.v1'
        operation = 'uninstall'
        task_name = $TaskName
        legacy_task_name = $LegacyTaskName
        legacy_untouched = $true
        credential_preserved = $true
    } | ConvertTo-Json -Depth 5
    exit 0
}

$dotenv = Read-DotEnv $EnvFile
$federatorExecutable = Resolve-RegularFile ([string]$dotenv.Values['APOCRYPHA_MEMORY_FEDERATOR_EXE']) 'Memory federator executable'
if ([IO.Path]::GetExtension($federatorExecutable) -cne '.exe') {
    throw 'Memory federator executable must be a native Windows executable.'
}
Assert-FederatorBootstrapContract $federatorExecutable
$federatorConfigPath = Resolve-RegularFile ([string]$dotenv.Values['APOCRYPHA_MEMORY_FEDERATOR_CONFIG']) 'Memory federator configuration'
$ownerId = [string]$dotenv.Values['APOCRYPHA_MEMORY_OWNER_ID']
if ($ownerId -cne 'apocky') { throw 'Memory owner identity must be exactly apocky.' }
$federation = [IO.File]::ReadAllText($federatorConfigPath) | ConvertFrom-Json
if ($null -eq $federation.metaharness -or $federation.metaharness.endpoint -cne $expectedEndpoint -or
    $federation.metaharness.capability.entropy_label -cne $expectedEntropy) {
    throw 'Federation configuration does not match the fixed MetaHarness observer boundary.'
}
$bundlePath = [string]$federation.metaharness.capability.bundle_path
if ([string]::IsNullOrWhiteSpace($bundlePath) -or -not [IO.Path]::IsPathRooted($bundlePath)) {
    throw 'MetaHarness capability bundle path must be absolute.'
}

$fileToken = if ($dotenv.Values.ContainsKey($credentialName)) { [string]$dotenv.Values[$credentialName] } else { '' }
$userToken = [Environment]::GetEnvironmentVariable($credentialName, [EnvironmentVariableTarget]::User)
$fileValid = $fileToken -cmatch $tokenPattern
$userValid = $userToken -cmatch $tokenPattern
if ($fileToken -and -not $fileValid) { throw 'Environment-file MetaHarness bearer is malformed.' }
if ($userToken -and -not $userValid) { throw 'User-scoped MetaHarness bearer is malformed.' }
if ($fileValid -and $userValid -and $fileToken -cne $userToken -and -not $RotateCredential) {
    throw 'MetaHarness bearer stores disagree; use -RotateCredential for an explicit synchronized rotation.'
}
$credentialAction = if ($RotateCredential) { 'rotate' }
elseif ($fileValid -and $userValid) { 'reuse' }
elseif ($fileValid) { 'restore-user-store' }
elseif ($userValid) { 'mirror-private-env-file' }
else { 'create' }

$plan = [ordered]@{
    schema = 'apocrypha.metaharness-resident.plan.v1'
    operation = $Operation.ToLowerInvariant()
    task_name = $TaskName
    legacy_task_name = $LegacyTaskName
    legacy_untouched = $true
    action = [ordered]@{ execute = $taskExecutable; arguments = @('-m', 'meta_harness.mcp_server'); serialized_arguments = $taskArguments; working_directory = $root }
    trigger = @('user-logon')
    settings = [ordered]@{ restart_count = 999; restart_interval_seconds = 60; execution_time_limit_seconds = 0; multiple_instances = 'IgnoreNew'; hidden = $true }
    observer = [ordered]@{ endpoint = $expectedEndpoint; authority = 'none'; execution_authorized = $false }
    credential = [ordered]@{ environment_name = $credentialName; action = $credentialAction; included_in_task = $false; printed = $false }
    federation = [ordered]@{ executable = $federatorExecutable; config = $federatorConfigPath; capability_bundle = $bundlePath; endpoint = $expectedEndpoint }
    provenance = [ordered]@{
        virtual_environment_python = $venvPython
        base_python = $basePython
        editable_source = $installedSource
        observer_executable_sha256 = Get-FileSha256 $entryPoint
        federator_executable_sha256 = Get-FileSha256 $federatorExecutable
    }
}
if ($Operation -eq 'Plan') {
    $plan | ConvertTo-Json -Depth 7
    exit 0
}
if ($WhatIfPreference) {
    $plan.operation = 'what-if'
    $plan | ConvertTo-Json -Depth 7
    exit 0
}

$legacy = Get-ScheduledTask -TaskName $LegacyTaskName -ErrorAction SilentlyContinue
if ($null -ne $legacy -and $legacy.State -ne 'Disabled') {
    throw 'The legacy PowerShell MetaHarness task must remain disabled; it was not changed.'
}
$existing = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
if ($null -ne $existing) {
    $existingExecute = [string]$existing.Actions[0].Execute
    $existingWorkingDirectory = [string]$existing.Actions[0].WorkingDirectory
    $existingArguments = [string]$existing.Actions[0].Arguments
    if (-not [StringComparer]::OrdinalIgnoreCase.Equals($existingExecute, $taskExecutable) -or
        -not [StringComparer]::OrdinalIgnoreCase.Equals($existingWorkingDirectory, $root) -or
        $existingArguments -cne $taskArguments) {
        throw 'Refusing to reuse an existing task that is not the exact direct MetaHarness action.'
    }
}
$listeners = @(Get-NetTCPConnection -LocalPort 8765 -State Listen -ErrorAction SilentlyContinue)
if ($listeners.Count -gt 0 -and ($null -eq $existing -or $existing.State -ne 'Running')) {
    throw 'Port 8765 is owned by a process outside the direct MetaHarness task.'
}

$token = if ($RotateCredential -or (-not $fileValid -and -not $userValid)) { New-ObserverToken }
elseif ($fileValid) { $fileToken }
else { $userToken }

try {
    if ($PSCmdlet.ShouldProcess($credentialName, 'synchronize private MetaHarness bearer stores')) {
        if (-not $fileValid -or $fileToken -cne $token) {
            Set-DotEnvValue $dotenv.Path $dotenv.Raw $credentialName $token
        }
        [Environment]::SetEnvironmentVariable($credentialName, $token, [EnvironmentVariableTarget]::User)
        [Environment]::SetEnvironmentVariable($credentialName, $token, [EnvironmentVariableTarget]::Process)
    }

    if ($PSCmdlet.ShouldProcess($bundlePath, 'seal synchronized MetaHarness DPAPI capability')) {
        $bootstrapOutput = & $federatorExecutable 'bootstrap-metaharness-capability' '--output' $bundlePath '--owner-id' $ownerId '--endpoint' $expectedEndpoint 2>&1 | Out-String
        $bootstrapExit = $LASTEXITCODE
        if ($bootstrapOutput.Contains($token)) { throw 'Federator bootstrap exposed the bearer in output.' }
        if ($bootstrapExit -ne 0) { throw 'Federator refused the MetaHarness capability bootstrap.' }
        $bootstrapReceipt = $bootstrapOutput | ConvertFrom-Json
        if ($bootstrapReceipt.status -cne 'OK' -or $bootstrapReceipt.code -cne 'FED_CAPABILITY_BOOTSTRAPPED') {
            throw 'Federator bootstrap did not return the exact accepted receipt.'
        }
    }

    if ($null -ne $existing -and $existing.State -eq 'Running') {
        Stop-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
        $closeDeadline = [DateTimeOffset]::UtcNow.AddSeconds(15)
        do {
            Start-Sleep -Milliseconds 250
            $stillListening = @(Get-NetTCPConnection -LocalPort 8765 -State Listen -ErrorAction SilentlyContinue).Count -gt 0
        } while ($stillListening -and [DateTimeOffset]::UtcNow -lt $closeDeadline)
        if ($stillListening) { throw 'Existing direct MetaHarness task did not release port 8765.' }
    }

    $action = New-ScheduledTaskAction -Execute $taskExecutable -Argument $taskArguments -WorkingDirectory $root
    $logon = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
    $settings = New-ScheduledTaskSettingsSet -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1) -ExecutionTimeLimit ([TimeSpan]::Zero) -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -MultipleInstances IgnoreNew -Hidden
    $principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -LogonType Interactive -RunLevel Limited
    if ($PSCmdlet.ShouldProcess($TaskName, 'register or reconcile direct persistent MetaHarness observer task')) {
        Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $logon -Settings $settings -Principal $principal -Force | Out-Null
    }
    if (-not $DoNotStart -and $PSCmdlet.ShouldProcess($TaskName, 'start and verify direct MetaHarness observer task')) {
        Start-ScheduledTask -TaskName $TaskName
        $deadline = [DateTimeOffset]::UtcNow.AddSeconds(40)
        $verified = $false
        do {
            Start-Sleep -Milliseconds 500
            try {
                Test-UnauthenticatedDenial
                Test-ObserverHealth $token
                $verified = $true
            }
            catch {
                if ([DateTimeOffset]::UtcNow -ge $deadline) { throw }
            }
        } while (-not $verified)
    }

    [ordered]@{
        schema = 'apocrypha.metaharness-resident.result.v1'
        operation = 'install'
        task_name = $TaskName
        legacy_task_name = $LegacyTaskName
        legacy_untouched = $true
        direct_executable = $taskExecutable
        arguments = @('-m', 'meta_harness.mcp_server')
        started = -not [bool]$DoNotStart
        authenticated_health_verified = -not [bool]$DoNotStart
        authority = 'none'
        execution_authorized = $false
        credential_printed = $false
    } | ConvertTo-Json -Depth 6
}
finally {
    if ($null -ne $token) {
        [Environment]::SetEnvironmentVariable($credentialName, $null, [EnvironmentVariableTarget]::Process)
        $token = $null
    }
}
