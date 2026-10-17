# Runs inside Draxbyte-Setup.exe (built by scripts\make-setup.ps1). Per-user install, no admin needed.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName PresentationFramework
function Say($msg, $icon = 'Information') {
    [void][System.Windows.MessageBox]::Show($msg, 'Draxbyte', 'OK', $icon)
}
try {
    $src = Split-Path -Parent $MyInvocation.MyCommand.Path
    $dest = Join-Path $env:LOCALAPPDATA 'Programs\Draxbyte'
    New-Item -ItemType Directory -Force $dest | Out-Null
    $exe = Join-Path $dest 'draxbyte.exe'
    # An in-app update starts this setup and then closes Draxbyte; give the old process a few seconds to exit.
    $copied = $false
    for ($i = 0; $i -lt 15 -and -not $copied; $i++) {
        try { Copy-Item (Join-Path $src 'draxbyte.exe') $exe -Force; $copied = $true }
        catch { Start-Sleep -Seconds 1 }
    }
    if (-not $copied) {
        Say "Draxbyte acik gorunuyor. Kapatip kurulumu tekrar calistir.`n`nDraxbyte seems to be running. Close it and run the setup again." 'Warning'
        exit 1
    }
    Copy-Item (Join-Path $src 'uninstall.ps1') (Join-Path $dest 'uninstall.ps1') -Force

    $shell = New-Object -ComObject WScript.Shell
    foreach ($l in @(
        (Join-Path ([Environment]::GetFolderPath('Desktop')) 'Draxbyte.lnk'),
        (Join-Path ([Environment]::GetFolderPath('Programs')) 'Draxbyte.lnk'))) {
        $s = $shell.CreateShortcut($l)
        $s.TargetPath = $exe
        $s.WorkingDirectory = $dest
        $s.IconLocation = "$exe,0"
        $s.Description = 'Draxbyte - disk scanner and cleaner'
        $s.Save()
    }

    # "Apps & features" entry (current user only).
    $key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Draxbyte'
    New-Item -Path $key -Force | Out-Null
    $ver = (Get-Item $exe).VersionInfo.ProductVersion
    Set-ItemProperty $key DisplayName 'Draxbyte'
    Set-ItemProperty $key DisplayVersion $ver
    Set-ItemProperty $key Publisher 'Fatih Kunak'
    Set-ItemProperty $key DisplayIcon $exe
    Set-ItemProperty $key InstallLocation $dest
    Set-ItemProperty $key UninstallString "powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File `"$dest\uninstall.ps1`""
    Set-ItemProperty $key NoModify 1 -Type DWord
    Set-ItemProperty $key NoRepair 1 -Type DWord

    Say "Draxbyte kuruldu. Masaustundeki kisayolla acabilirsin.`n`nDraxbyte installed. Open it from the Desktop shortcut."
    Start-Process $exe
} catch {
    Say "Kurulum basarisiz / Setup failed:`n$($_.Exception.Message)" 'Error'
    exit 1
}
