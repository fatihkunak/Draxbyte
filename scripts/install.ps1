# Builds the release exe, installs it to %LOCALAPPDATA%\Programs\Draxbyte and
# creates Desktop + Start Menu shortcuts. Re-run after every update.
# Close Draxbyte first (a running exe cannot be overwritten).
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try { cargo build --release } finally { Pop-Location }

$dest = Join-Path $env:LOCALAPPDATA 'Programs\Draxbyte'
New-Item -ItemType Directory -Force $dest | Out-Null
$exe = Join-Path $dest 'draxbyte.exe'

# Prefer the signed copy from scripts\release.ps1 when it is at least as new as the fresh build.
$built = Join-Path $root 'target\release\draxbyte.exe'
$signed = Join-Path $root 'dist\draxbyte.exe'
if ((Test-Path $signed) -and (Get-Item $signed).LastWriteTime -ge (Get-Item $built).LastWriteTime) { $built = $signed }
Copy-Item $built $exe -Force

$shell = New-Object -ComObject WScript.Shell
$links = @(
    (Join-Path ([Environment]::GetFolderPath('Desktop')) 'Draxbyte.lnk'),
    (Join-Path ([Environment]::GetFolderPath('Programs')) 'Draxbyte.lnk')
)
foreach ($l in $links) {
    $s = $shell.CreateShortcut($l)
    $s.TargetPath = $exe
    $s.WorkingDirectory = $dest
    $s.IconLocation = "$exe,0"
    $s.Description = 'Draxbyte - disk scanner and cleaner'
    $s.Save()
    Write-Host "shortcut: $l"
}
Write-Host "installed: $exe"
