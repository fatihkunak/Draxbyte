# Builds the release exe, installs it to %LOCALAPPDATA%\Programs\Draxbyte and
# creates Desktop + Start Menu shortcuts. Re-run after every update.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try { cargo build --release } finally { Pop-Location }

$dest = Join-Path $env:LOCALAPPDATA 'Programs\Draxbyte'
New-Item -ItemType Directory -Force $dest | Out-Null
$exe = Join-Path $dest 'draxbyte.exe'
Copy-Item (Join-Path $root 'target\release\draxbyte.exe') $exe -Force

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
    $s.Description = 'Draxbyte - disk tarama ve temizleme'
    $s.Save()
    Write-Host "kisayol: $l"
}
Write-Host "kuruldu: $exe"
