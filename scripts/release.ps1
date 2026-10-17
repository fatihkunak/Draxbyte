# Builds the release exe, signs it, and writes dist\ (exe + SHA256SUMS.txt).
#   powershell -File scripts\release.ps1            # build + sign + checksums
#   powershell -File scripts\release.ps1 -Publish   # ...and create the GitHub release (needs `gh`)
#
# Signing uses a SELF-SIGNED code-signing certificate ("CN=Fatih Kunak (Draxbyte)") that lives in
# the current user's certificate store; it is created on first use. The public certificate is
# exported to assets\draxbyte-codesign.cer. A self-signed signature proves the file was not
# changed after signing; it does NOT make Windows SmartScreen trust the publisher.
param([switch]$Publish)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"(.+)"').Matches[0].Groups[1].Value
$tag = "v$version"
$subject = 'CN=Fatih Kunak (Draxbyte)'

cargo build --release
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

$dist = Join-Path $root 'dist'
New-Item -ItemType Directory -Force $dist | Out-Null
$exe = Join-Path $dist 'draxbyte.exe'
Copy-Item (Join-Path $root 'target\release\draxbyte.exe') $exe -Force

$cert = Get-ChildItem Cert:\CurrentUser\My -CodeSigningCert | Where-Object { $_.Subject -eq $subject -and $_.NotAfter -gt (Get-Date) } | Select-Object -First 1
if (-not $cert) {
    Write-Host "creating self-signed code-signing certificate: $subject"
    $cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject $subject -KeyAlgorithm RSA -KeyLength 3072 `
        -HashAlgorithm SHA256 -KeyUsage DigitalSignature -NotAfter (Get-Date).AddYears(5) -CertStoreLocation Cert:\CurrentUser\My
}
Export-Certificate -Cert $cert -FilePath (Join-Path $root 'assets\draxbyte-codesign.cer') -Force | Out-Null

$sig = Set-AuthenticodeSignature -FilePath $exe -Certificate $cert -HashAlgorithm SHA256 -TimestampServer 'http://timestamp.digicert.com'
if (-not $sig.SignerCertificate) { throw "signing failed: $($sig.StatusMessage)" }
$check = Get-AuthenticodeSignature $exe
Write-Host "signature: $($check.Status) | signer: $($check.SignerCertificate.Subject) | timestamp: $($check.TimeStamperCertificate.Subject)"

$hash = (Get-FileHash $exe -Algorithm SHA256).Hash.ToLower()
"$hash  draxbyte.exe" | Set-Content (Join-Path $dist 'SHA256SUMS.txt') -Encoding ascii
Write-Host "sha256: $hash"

if ($Publish) {
    $notes = Join-Path $dist 'RELEASE_NOTES.md'
    if (-not (Test-Path $notes)) { throw "write $notes first" }
    gh release create $tag $exe (Join-Path $dist 'SHA256SUMS.txt') (Join-Path $root 'assets\draxbyte-codesign.cer') `
        --title "Draxbyte $version" --notes-file $notes
}
