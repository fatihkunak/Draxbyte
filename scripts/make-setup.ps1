# Builds Draxbyte-Setup.exe and copies it to the Desktop (or -Out).
#   powershell -File scripts\make-setup.ps1 [-NoBuild] [-Out <dir>]
#
# The setup file IS the program: Draxbyte-Setup.exe is a signed copy of draxbyte.exe. A file whose
# name contains "setup" starts the install wizard (same theme, logo and languages as the app);
# the wizard copies the program, creates shortcuts and registers it in "Apps & features".
# The installed program starts the uninstall wizard with `draxbyte.exe --uninstall`.
param([switch]$NoBuild, [string]$Out = [Environment]::GetFolderPath('Desktop'))
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
if (-not $NoBuild) { cargo build --release; if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' } }

# Same self-signed certificate as scripts\release.ps1 (see there). Signing is skipped when it does not exist yet.
$subject = 'CN=Fatih Kunak (Draxbyte)'
$cert = Get-ChildItem Cert:\CurrentUser\My -CodeSigningCert -ErrorAction SilentlyContinue | Where-Object { $_.Subject -eq $subject -and $_.NotAfter -gt (Get-Date) } | Select-Object -First 1

New-Item -ItemType Directory -Force $Out | Out-Null
$final = Join-Path $Out 'Draxbyte-Setup.exe'
Copy-Item (Join-Path $root 'target\release\draxbyte.exe') $final -Force

if ($cert) {
    # The timestamp server is sometimes unreachable; a signature without a timestamp is not accepted here.
    $ok = $false
    for ($i = 1; $i -le 4 -and -not $ok; $i++) {
        $r = Set-AuthenticodeSignature -FilePath $final -Certificate $cert -HashAlgorithm SHA256 -TimestampServer 'http://timestamp.digicert.com'
        $ok = [bool](Get-AuthenticodeSignature $final).TimeStamperCertificate
        if (-not $ok) { Write-Host "no timestamp (attempt $i), retrying"; Start-Sleep 3 }
    }
    if (-not $ok) { throw 'could not obtain a timestamped signature' }
    Write-Host "signed: Draxbyte-Setup.exe (self-signed: Windows reports it as untrusted, which is expected)"
} else {
    Write-Host 'not signed (no certificate)'
}
Write-Host "setup: $final ($([math]::Round((Get-Item $final).Length/1MB,1)) MB)"
