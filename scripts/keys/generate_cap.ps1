# § scripts/keys/generate_cap.ps1 · Generate Ed25519 cap-keypair
# ════════════════════════════════════════════════════════════════════════════
#
# Usage:
#   .\scripts\keys\generate_cap.ps1 -Role A     # cap-A (loa.binary)
#   .\scripts\keys\generate_cap.ps1 -Role D -Force   # overwrite existing
#   .\scripts\keys\generate_cap.ps1 -Role custom -Name myrole
#
# Cap roles (per specs/26b_HOTFIX_KEYS.csl):
#   A : loa.binary                              (rarest · cold-storage)
#   B : cssl.bundle                             (language-runtime)
#   C : kan.weights + balance.config            (model + balance)
#   D : security.patch                          (incident-response)
#   E : recipe + nemesis + storylet + render    (cosmetic content)

[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)]
    [string]$Role,
    [string]$Name = "",
    [switch]$Force,
    [switch]$Rotate
)

$SecretsDir = "$env:USERPROFILE\.loa-secrets"
$keyName = if ($Name) { "hotfix-$Name" } else { "hotfix-cap-$Role" }
$privPath = Join-Path $SecretsDir "$keyName.priv"
$pubPath = Join-Path $SecretsDir "$keyName.pub"
$pubHexPath = "$pubPath.hex"

if ((Test-Path $privPath) -and -not $Force -and -not $Rotate) {
    Write-Host "✗ key already exists at $privPath" -ForegroundColor Red
    Write-Host "  use -Force to overwrite OR -Rotate to backup-then-replace" -ForegroundColor Yellow
    exit 1
}

if ($Rotate -and (Test-Path $privPath)) {
    $ts = Get-Date -Format 'yyyyMMdd-HHmmss'
    $backupPriv = Join-Path $SecretsDir "$keyName.OLD.$ts.priv"
    $backupPub = Join-Path $SecretsDir "$keyName.OLD.$ts.pub"
    Move-Item $privPath $backupPriv
    Move-Item $pubPath $backupPub -ErrorAction SilentlyContinue
    Move-Item $pubHexPath "$backupPub.hex" -ErrorAction SilentlyContinue
    Write-Host "→ rotated · old key backed up to $backupPriv" -ForegroundColor Yellow
}

New-Item -ItemType Directory -Force -Path $SecretsDir | Out-Null

# Use openssl for Ed25519 (available via mingw on Apocky's setup)
$openssl = (Get-Command openssl -ErrorAction SilentlyContinue).Source
if (-not $openssl) { $openssl = "C:\Program Files\Git\mingw64\bin\openssl.exe" }
if (-not (Test-Path $openssl)) {
    Write-Host "✗ openssl not found · need git-bash openssl OR mingw openssl" -ForegroundColor Red
    exit 1
}

Write-Host "=== generating Ed25519 keypair $keyName ===" -ForegroundColor Cyan
& $openssl genpkey -algorithm Ed25519 -out $privPath
if ($LASTEXITCODE -ne 0) { Write-Host "✗ openssl genpkey failed" -ForegroundColor Red ; exit 1 }
& $openssl pkey -in $privPath -pubout -out $pubPath
if ($LASTEXITCODE -ne 0) { Write-Host "✗ openssl pubout failed" -ForegroundColor Red ; exit 1 }

# Extract raw 32-byte hex pubkey
$pubText = & $openssl pkey -in $pubPath -pubin -text -noout 2>&1
$rawHex = ($pubText | Out-String) -split "`n" | Where-Object { $_ -match '^\s+[0-9a-fA-F:]+\s*$' } | ForEach-Object { ($_ -replace '[\s:]', '') } | Out-String
$rawHex = $rawHex.Trim() -replace '\s', ''
# Take last 64 chars (32 bytes raw pubkey · before that is structural-prefix)
if ($rawHex.Length -ge 64) { $rawHex = $rawHex.Substring($rawHex.Length - 64) }
$rawHex | Out-File -FilePath $pubHexPath -Encoding ascii -NoNewline

# chmod 600 equivalent on Windows (ACL · current-user-only)
icacls $privPath /inheritance:r /grant:r "$($env:USERNAME):F" 2>&1 | Out-Null

Write-Host ""
Write-Host "✓ generated cap-key '$keyName'" -ForegroundColor Green
Write-Host "  priv : $privPath (current-user-only ACL)" -ForegroundColor Gray
Write-Host "  pub  : $pubPath" -ForegroundColor Gray
Write-Host "  hex  : $pubHexPath ($rawHex)" -ForegroundColor Gray
Write-Host ""
Write-Host "→ next step : update compiler-rs/crates/cssl-host-config/keys/hotfix-pubkeys.toml" -ForegroundColor Yellow
Write-Host "  (replace the cap_$($Role.ToLower()) pubkey_hex with $rawHex)" -ForegroundColor Yellow
