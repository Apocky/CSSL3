[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [ValidateSet('Plan', 'Install', 'Uninstall')]
    [string]$Operation = 'Install',
    [string]$TaskName = 'Apocky-MetaHarness-MCP-Direct',
    [string]$RenewalTaskName = 'Apocky-MetaHarness-Capability-Renewal',
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
$capabilityTtlMs = 900000
$renewalIntervalMinutes = 5
$managedTaskPath = '\'

function Assert-TaskName {
    param([string]$Candidate)
    if ([string]::IsNullOrWhiteSpace($Candidate) -or $Candidate.Length -gt 120 -or
        $Candidate -match '[\\/\x00-\x1f]' -or
        [StringComparer]::OrdinalIgnoreCase.Equals($Candidate, $LegacyTaskName)) {
        throw 'A managed task name is invalid or aliases the disabled legacy task.'
    }
}

function Quote-TaskArgument {
    param([string]$Value, [string]$Label)
    if ([string]::IsNullOrWhiteSpace($Value) -or $Value -match '["\x00-\x1f]') {
        throw "$Label cannot be represented safely in a direct task action."
    }
    return '"' + $Value + '"'
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
    if ($LASTEXITCODE -ne 0 -or $help -notmatch '(?s)bootstrap-metaharness-capability.*--output.*--owner-id.*--endpoint.*--ttl-ms') {
        throw 'Configured memory executable lacks the required MetaHarness capability-bootstrap contract.'
    }
}

function Start-AndVerifyRenewalTask {
    param([string]$Name)
    $notBefore = (Get-Date).AddSeconds(-2)
    Start-ScheduledTask -TaskName $Name -TaskPath $managedTaskPath
    $deadline = [DateTimeOffset]::UtcNow.AddSeconds(30)
    do {
        Start-Sleep -Milliseconds 200
        $task = Get-ScheduledTask -TaskName $Name -TaskPath $managedTaskPath
        $info = Get-ScheduledTaskInfo -TaskName $Name -TaskPath $managedTaskPath
        $complete = $task.State -ne 'Running' -and $info.LastRunTime -ge $notBefore
    } while (-not $complete -and [DateTimeOffset]::UtcNow -lt $deadline)
    if (-not $complete -or [uint32]$info.LastTaskResult -ne 0) {
        throw 'The direct MetaHarness capability renewal task did not complete successfully.'
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
Assert-TaskName $RenewalTaskName
if ([StringComparer]::OrdinalIgnoreCase.Equals($TaskName, $RenewalTaskName)) {
    throw 'Observer and renewal task names must be distinct.'
}
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
$null = Resolve-RegularDirectory ([IO.Path]::GetDirectoryName($bundlePath)) 'MetaHarness capability directory'
if (Test-Path -LiteralPath $bundlePath) {
    $null = Resolve-RegularFile $bundlePath 'MetaHarness capability bundle'
}
$renewalWorkingDirectory = Resolve-RegularDirectory ([IO.Path]::GetDirectoryName($federatorExecutable)) 'Memory federator directory'
$quotedBundle = Quote-TaskArgument $bundlePath 'MetaHarness capability bundle path'
$quotedOwner = Quote-TaskArgument $ownerId 'MetaHarness capability owner'
$quotedEndpoint = Quote-TaskArgument $expectedEndpoint 'MetaHarness capability endpoint'
$renewalTaskArguments = "bootstrap-metaharness-capability --output $quotedBundle --owner-id $quotedOwner --endpoint $quotedEndpoint --ttl-ms $capabilityTtlMs"

if ($Operation -eq 'Uninstall') {
    $managed = @(
        [pscustomobject]@{ Name = $TaskName; Execute = $taskExecutable; Arguments = $taskArguments; WorkingDirectory = $root; Label = 'observer' },
        [pscustomobject]@{ Name = $RenewalTaskName; Execute = $federatorExecutable; Arguments = $renewalTaskArguments; WorkingDirectory = $renewalWorkingDirectory; Label = 'capability renewal' }
    )
    $validatedManaged = @()
    foreach ($expected in $managed) {
        $existingManaged = Get-ScheduledTask -TaskName $expected.Name -TaskPath $managedTaskPath -ErrorAction SilentlyContinue
        if ($null -eq $existingManaged) { continue }
        if (@($existingManaged.Actions).Count -ne 1) {
            throw "Refusing to remove a task that does not have exactly one canonical MetaHarness $($expected.Label) action."
        }
        $execute = [string]$existingManaged.Actions[0].Execute
        $arguments = [string]$existingManaged.Actions[0].Arguments
        $workingDirectory = [string]$existingManaged.Actions[0].WorkingDirectory
        if (-not [StringComparer]::OrdinalIgnoreCase.Equals($execute, $expected.Execute) -or
            -not [StringComparer]::OrdinalIgnoreCase.Equals($workingDirectory, $expected.WorkingDirectory) -or
            $arguments -cne $expected.Arguments) {
            throw "Refusing to remove a task whose action is not the canonical MetaHarness $($expected.Label) action."
        }
        $validatedManaged += [pscustomobject]@{
            Name = $expected.Name
            Label = $expected.Label
            Xml = Export-ScheduledTask -TaskName $expected.Name -TaskPath $managedTaskPath
            WasRunning = $existingManaged.State -eq 'Running'
        }
    }
    $attemptedManaged = @()
    try {
        foreach ($expected in $validatedManaged) {
            if ($PSCmdlet.ShouldProcess($expected.Name, "stop and unregister direct MetaHarness $($expected.Label) task")) {
                $attemptedManaged += $expected
                Stop-ScheduledTask -TaskName $expected.Name -TaskPath $managedTaskPath -ErrorAction SilentlyContinue
                Unregister-ScheduledTask -TaskName $expected.Name -TaskPath $managedTaskPath -Confirm:$false
            }
        }
    }
    catch {
        $removalFailure = $_
        $restoreFailures = @()
        foreach ($snapshot in $attemptedManaged) {
            try {
                Register-ScheduledTask -TaskName $snapshot.Name -TaskPath $managedTaskPath -Xml $snapshot.Xml -Force | Out-Null
                if ($snapshot.WasRunning) {
                    Start-ScheduledTask -TaskName $snapshot.Name -TaskPath $managedTaskPath
                }
            }
            catch {
                $restoreFailures += $snapshot.Name
            }
        }
        if ($restoreFailures.Count -gt 0) {
            throw "MetaHarness task removal failed and rollback could not restore every task: $($restoreFailures -join ', ')."
        }
        throw $removalFailure
    }
    [ordered]@{
        schema = 'apocrypha.metaharness-resident.result.v1'
        operation = 'uninstall'
        task_name = $TaskName
        renewal_task_name = $RenewalTaskName
        task_path = $managedTaskPath
        legacy_task_name = $LegacyTaskName
        legacy_untouched = $true
        credential_preserved = $true
        capability_bundle_preserved = $true
    } | ConvertTo-Json -Depth 5
    exit 0
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
    task_path = $managedTaskPath
    legacy_task_name = $LegacyTaskName
    legacy_untouched = $true
    action = [ordered]@{ execute = $taskExecutable; arguments = @('-m', 'meta_harness.mcp_server'); serialized_arguments = $taskArguments; working_directory = $root }
    trigger = @('user-logon')
    settings = [ordered]@{ restart_count = 999; restart_interval_seconds = 60; execution_time_limit_seconds = 0; multiple_instances = 'IgnoreNew'; hidden = $true }
    renewal = [ordered]@{
        task_name = $RenewalTaskName
        task_path = $managedTaskPath
        action = [ordered]@{
            execute = $federatorExecutable
            arguments = @('bootstrap-metaharness-capability', '--output', $bundlePath, '--owner-id', $ownerId, '--endpoint', $expectedEndpoint, '--ttl-ms', $capabilityTtlMs)
            serialized_arguments = $renewalTaskArguments
            working_directory = $renewalWorkingDirectory
        }
        trigger = @('user-logon', 'five-minute-repetition')
        ttl_ms = $capabilityTtlMs
        interval_seconds = $renewalIntervalMinutes * 60
        credential_included_in_task = $false
        output_contains_secret = $false
    }
    observer = [ordered]@{ endpoint = $expectedEndpoint; authority = 'none'; execution_authorized = $false }
    credential = [ordered]@{ environment_name = $credentialName; action = $credentialAction; included_in_task = $false; printed = $false }
    federation = [ordered]@{ executable = $federatorExecutable; config = $federatorConfigPath; capability_bundle = $bundlePath; endpoint = $expectedEndpoint }
    provenance = [ordered]@{
        virtual_environment_python = $venvPython
        base_python = $basePython
        editable_source = $installedSource
        observer_launcher_sha256 = Get-FileSha256 $taskExecutable
        observer_entry_point_sha256 = Get-FileSha256 $entryPoint
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

$legacy = Get-ScheduledTask -TaskName $LegacyTaskName -TaskPath $managedTaskPath -ErrorAction SilentlyContinue
if ($null -ne $legacy -and $legacy.State -ne 'Disabled') {
    throw 'The legacy PowerShell MetaHarness task must remain disabled; it was not changed.'
}
$existing = Get-ScheduledTask -TaskName $TaskName -TaskPath $managedTaskPath -ErrorAction SilentlyContinue
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
$existingRenewal = Get-ScheduledTask -TaskName $RenewalTaskName -TaskPath $managedTaskPath -ErrorAction SilentlyContinue
if ($null -ne $existingRenewal) {
    $renewalExecute = [string]$existingRenewal.Actions[0].Execute
    $renewalArguments = [string]$existingRenewal.Actions[0].Arguments
    $renewalWorking = [string]$existingRenewal.Actions[0].WorkingDirectory
    if (-not [StringComparer]::OrdinalIgnoreCase.Equals($renewalExecute, $federatorExecutable) -or
        -not [StringComparer]::OrdinalIgnoreCase.Equals($renewalWorking, $renewalWorkingDirectory) -or
        $renewalArguments -cne $renewalTaskArguments) {
        throw 'Refusing to reuse an existing task that is not the exact MetaHarness capability renewal action.'
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
        $bootstrapOutput = & $federatorExecutable 'bootstrap-metaharness-capability' '--output' $bundlePath '--owner-id' $ownerId '--endpoint' $expectedEndpoint '--ttl-ms' $capabilityTtlMs 2>&1 | Out-String
        $bootstrapExit = $LASTEXITCODE
        if ($bootstrapOutput.Contains($token)) { throw 'Federator bootstrap exposed the bearer in output.' }
        if ($bootstrapExit -ne 0) { throw 'Federator refused the MetaHarness capability bootstrap.' }
        $bootstrapReceipt = $bootstrapOutput | ConvertFrom-Json
        if ($bootstrapReceipt.status -cne 'OK' -or $bootstrapReceipt.code -cne 'FED_CAPABILITY_BOOTSTRAPPED') {
            throw 'Federator bootstrap did not return the exact accepted receipt.'
        }
    }

    $principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -LogonType Interactive -RunLevel Limited
    $renewalWasPresent = $null -ne $existingRenewal
    $renewalWasRunning = $renewalWasPresent -and $existingRenewal.State -eq 'Running'
    $renewalRollbackXml = if ($renewalWasPresent) {
        Export-ScheduledTask -TaskName $RenewalTaskName -TaskPath $managedTaskPath
    }
    else { $null }
    try {
        if ($renewalWasRunning) {
            Stop-ScheduledTask -TaskName $RenewalTaskName -TaskPath $managedTaskPath -ErrorAction SilentlyContinue
        }
        $renewalAction = New-ScheduledTaskAction -Execute $federatorExecutable -Argument $renewalTaskArguments -WorkingDirectory $renewalWorkingDirectory
        $renewalLogon = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
        $renewalRepeating = New-ScheduledTaskTrigger -Once -At (Get-Date).AddMinutes(1) -RepetitionInterval (New-TimeSpan -Minutes $renewalIntervalMinutes)
        $renewalSettings = New-ScheduledTaskSettingsSet -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1) -ExecutionTimeLimit (New-TimeSpan -Minutes 1) -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -MultipleInstances IgnoreNew -Hidden
        if ($PSCmdlet.ShouldProcess($RenewalTaskName, 'register or reconcile direct MetaHarness capability renewal task')) {
            Register-ScheduledTask -TaskName $RenewalTaskName -TaskPath $managedTaskPath -Action $renewalAction -Trigger @($renewalLogon, $renewalRepeating) -Settings $renewalSettings -Principal $principal -Force | Out-Null
        }
        if (-not $DoNotStart -and $PSCmdlet.ShouldProcess($RenewalTaskName, 'run and verify direct MetaHarness capability renewal task')) {
            Start-AndVerifyRenewalTask $RenewalTaskName
        }
    }
    catch {
        $renewalFailure = $_
        try {
            if ($renewalWasPresent) {
                Register-ScheduledTask -TaskName $RenewalTaskName -TaskPath $managedTaskPath -Xml $renewalRollbackXml -Force | Out-Null
                if ($renewalWasRunning) {
                    Start-ScheduledTask -TaskName $RenewalTaskName -TaskPath $managedTaskPath
                }
            }
            else {
                $partialRenewal = Get-ScheduledTask -TaskName $RenewalTaskName -TaskPath $managedTaskPath -ErrorAction SilentlyContinue
                if ($null -ne $partialRenewal) {
                    Unregister-ScheduledTask -TaskName $RenewalTaskName -TaskPath $managedTaskPath -Confirm:$false
                }
            }
        }
        catch {
            throw "MetaHarness renewal reconciliation failed and its prior task could not be restored: $($renewalFailure.Exception.Message)"
        }
        throw $renewalFailure
    }

    $observerWasPresent = $null -ne $existing
    $observerWasRunning = $observerWasPresent -and $existing.State -eq 'Running'
    $observerRollbackXml = if ($observerWasPresent) {
        Export-ScheduledTask -TaskName $TaskName -TaskPath $managedTaskPath
    }
    else { $null }
    try {
        if ($observerWasRunning) {
            Stop-ScheduledTask -TaskName $TaskName -TaskPath $managedTaskPath -ErrorAction SilentlyContinue
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
        if ($PSCmdlet.ShouldProcess($TaskName, 'register or reconcile direct persistent MetaHarness observer task')) {
            Register-ScheduledTask -TaskName $TaskName -TaskPath $managedTaskPath -Action $action -Trigger $logon -Settings $settings -Principal $principal -Force | Out-Null
        }

        if (-not $DoNotStart -and $PSCmdlet.ShouldProcess($TaskName, 'start and verify direct MetaHarness observer task')) {
            Start-ScheduledTask -TaskName $TaskName -TaskPath $managedTaskPath
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
    }
    catch {
        $observerFailure = $_
        try {
            if ($observerWasPresent) {
                Register-ScheduledTask -TaskName $TaskName -TaskPath $managedTaskPath -Xml $observerRollbackXml -Force | Out-Null
                if ($observerWasRunning) { Start-ScheduledTask -TaskName $TaskName -TaskPath $managedTaskPath }
            }
            else {
                $partialObserver = Get-ScheduledTask -TaskName $TaskName -TaskPath $managedTaskPath -ErrorAction SilentlyContinue
                if ($null -ne $partialObserver) {
                    Unregister-ScheduledTask -TaskName $TaskName -TaskPath $managedTaskPath -Confirm:$false
                }
            }
        }
        catch {
            throw "MetaHarness observer reconciliation failed and its prior task could not be restored: $($observerFailure.Exception.Message)"
        }
        throw $observerFailure
    }

    [ordered]@{
        schema = 'apocrypha.metaharness-resident.result.v1'
        operation = 'install'
        task_name = $TaskName
        renewal_task_name = $RenewalTaskName
        task_path = $managedTaskPath
        legacy_task_name = $LegacyTaskName
        legacy_untouched = $true
        direct_executable = $taskExecutable
        arguments = @('-m', 'meta_harness.mcp_server')
        started = -not [bool]$DoNotStart
        authenticated_health_verified = -not [bool]$DoNotStart
        renewal_verified = -not [bool]$DoNotStart
        capability_ttl_ms = $capabilityTtlMs
        renewal_interval_seconds = $renewalIntervalMinutes * 60
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
