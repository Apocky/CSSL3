[CmdletBinding()]
param(
    [string]$EnvFile = 'C:\Users\Apocky\Documents\Tarot\Chaos\New\chaos-tarot\.env.local',
    [ValidateSet('run', 'once', 'probe', 'recover-only')]
    [string]$Mode = 'run'
)

$ErrorActionPreference = 'Stop'
$workerRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$edgeRoot = Resolve-Path (Join-Path $workerRoot '..\..')

if (Test-Path -LiteralPath $EnvFile) {
    foreach ($line in Get-Content -LiteralPath $EnvFile) {
        $trimmed = $line.Trim()
        if (-not $trimmed -or $trimmed.StartsWith('#') -or -not $trimmed.Contains('=')) { continue }
        $pair = $trimmed.Split('=', 2)
        $name = $pair[0].Trim()
        $value = $pair[1].Trim()
        if (($value.StartsWith('"') -and $value.EndsWith('"')) -or ($value.StartsWith("'") -and $value.EndsWith("'"))) {
            $value = $value.Substring(1, $value.Length - 2)
        }
        if ($name -match '^[A-Za-z_][A-Za-z0-9_]*$' -and -not [Environment]::GetEnvironmentVariable($name, 'Process')) {
            [Environment]::SetEnvironmentVariable($name, $value, 'Process')
        }
    }
}

$arguments = @('--import', 'tsx', 'scripts/apocrypha-worker/runner.ts')
switch ($Mode) {
    'once' { $arguments += '--once' }
    'probe' { $arguments += '--probe' }
    'recover-only' { $arguments += '--recover-only' }
}

Push-Location $edgeRoot
try {
    & node @arguments
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
