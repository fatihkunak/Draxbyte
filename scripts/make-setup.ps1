# Builds a single-file Draxbyte-Setup.exe (Windows IExpress, no extra tools) and copies it to the Desktop.
#   powershell -File scripts\make-setup.ps1 [-NoBuild] [-Out <dir>]
# The installed program is per-user (%LOCALAPPDATA%\Programs\Draxbyte); no admin rights needed.
param([switch]$NoBuild, [string]$Out = [Environment]::GetFolderPath('Desktop'))
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
if (-not $NoBuild) { cargo build --release; if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' } }

# Same self-signed certificate as scripts\release.ps1 (see there). Signing is skipped when it does not exist yet.
$subject = 'CN=Fatih Kunak (Draxbyte)'
$cert = Get-ChildItem Cert:\CurrentUser\My -CodeSigningCert -ErrorAction SilentlyContinue | Where-Object { $_.Subject -eq $subject -and $_.NotAfter -gt (Get-Date) } | Select-Object -First 1
function Sign($path) {
    if (-not $cert) { Write-Host "not signed (no certificate): $path"; return }
    $r = Set-AuthenticodeSignature -FilePath $path -Certificate $cert -HashAlgorithm SHA256 -TimestampServer 'http://timestamp.digicert.com'
    Write-Host "signed: $(Split-Path -Leaf $path) (self-signed: Windows reports it as untrusted, which is expected)"
}

$stage = Join-Path ([IO.Path]::GetTempPath()) ('draxbyte-setup-' + [guid]::NewGuid().ToString('n'))
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item (Join-Path $root 'target\release\draxbyte.exe') $stage
Sign (Join-Path $stage 'draxbyte.exe')
Copy-Item (Join-Path $PSScriptRoot 'setup\install-payload.ps1') (Join-Path $stage 'install.ps1')
Copy-Item (Join-Path $PSScriptRoot 'setup\uninstall.ps1') $stage

$target = Join-Path $stage 'Draxbyte-Setup.exe'
$sed = @"
[Version]
Class=IEXPRESS
SEDVersion=3
[Options]
PackagePurpose=InstallApp
ShowInstallProgramWindow=0
HideExtractAnimation=1
UseLongFileName=1
InsideCompressed=0
CAB_FixedSize=0
CAB_ResvCodeSigning=0
RebootMode=N
InstallPrompt=
DisplayLicense=
FinishMessage=
TargetName=$target
FriendlyName=Draxbyte Setup
AppLaunched=powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File install.ps1
PostInstallCmd=<None>
AdminQuietInstCmd=
UserQuietInstCmd=
SourceFiles=SourceFiles
[SourceFiles]
SourceFiles0=$stage\
[SourceFiles0]
draxbyte.exe=
install.ps1=
uninstall.ps1=
"@
$sedPath = Join-Path $stage 'setup.sed'
Set-Content $sedPath $sed -Encoding ascii
& "$env:WINDIR\System32\iexpress.exe" /N /Q $sedPath | Out-Null
# IExpress returns before it finishes writing; wait for the file.
for ($i = 0; $i -lt 60 -and -not (Test-Path $target); $i++) { Start-Sleep 1 }
if (-not (Test-Path $target)) { throw 'IExpress did not produce the setup file' }

Sign $target
New-Item -ItemType Directory -Force $Out | Out-Null
$final = Join-Path $Out 'Draxbyte-Setup.exe'
Copy-Item $target $final -Force
Remove-Item $stage -Recurse -Force
Write-Host "setup: $final ($([math]::Round((Get-Item $final).Length/1MB,1)) MB)"
