# Builds the release exe and runs the install wizard from it (a developer shortcut for
# "build, then install"). End users get Draxbyte-Setup.exe (scripts\make-setup.ps1) instead.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try { cargo build --release } finally { Pop-Location }
& (Join-Path $root 'target\release\draxbyte.exe') --setup
