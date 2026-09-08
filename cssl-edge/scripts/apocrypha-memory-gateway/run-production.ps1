[CmdletBinding()]
param(
    [string]$EnvFile = 'C:\Users\Apocky\Documents\Tarot\Chaos\New\chaos-tarot\.env.local'
)

$ErrorActionPreference = 'Stop'
$gatewayRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$edgeRoot = Resolve-Path (Join-Path $gatewayRoot '..\..')

if (-not (Test-Path -LiteralPath $EnvFile)) {
    throw "Apocrypha environment file not found: $EnvFile"
}

foreach ($line in Get-Content -LiteralPath $EnvFile) {
    $trimmed = $line.Trim()
    if (-not $trimmed -or $trimmed.StartsWith('#') -or -not $trimmed.Contains('=')) { continue }
    $pair = $trimmed.Split('=', 2)
    $name = $pair[0].Trim()
    $value = $pair[1].Trim()
    if (($value.StartsWith('"') -and $value.EndsWith('"')) -or ($value.StartsWith("'") -and $value.EndsWith("'"))) {
        $value = $value.Substring(1, $value.Length - 2)
    }
    if ($name -match '^[A-Za-z_][A-Za-z0-9_]*$') {
        [Environment]::SetEnvironmentVariable($name, $value, 'Process')
    }
}

Push-Location $edgeRoot
try {
    & node --import tsx scripts/apocrypha-memory-gateway/server.ts
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
