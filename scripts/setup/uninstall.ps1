# Removes the program, shortcuts and the "Apps & features" entry. Settings/data in ~\.draxbyte are kept.
$dest = Join-Path $env:LOCALAPPDATA 'Programs\Draxbyte'
foreach ($l in @(
    (Join-Path ([Environment]::GetFolderPath('Desktop')) 'Draxbyte.lnk'),
    (Join-Path ([Environment]::GetFolderPath('Programs')) 'Draxbyte.lnk'))) {
    Remove-Item $l -Force -ErrorAction SilentlyContinue
}
Remove-Item 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Draxbyte' -Recurse -Force -ErrorAction SilentlyContinue
# The running script lives in $dest, so delete it from a detached shell after we exit.
Start-Process powershell.exe -WindowStyle Hidden -ArgumentList '-NoProfile', '-Command', "Start-Sleep 2; Remove-Item -LiteralPath '$dest' -Recurse -Force -ErrorAction SilentlyContinue"
