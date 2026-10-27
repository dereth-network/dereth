# Takes Dereth's plugin host out of Decal's network filters and plugins, and deletes its folder when
# -To names it. The plugins and their saved settings in %APPDATA%\Dereth\decal are kept. Run it in an
# elevated PowerShell with the game closed:
#
#   powershell -ExecutionPolicy Bypass -File uninstall.ps1 [-To C:\Games\Dereth\Decal]
param([string]$To = "")
$ErrorActionPreference = "Stop"

$hklm = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::LocalMachine, [Microsoft.Win32.RegistryView]::Registry32)
foreach ($entry in @(@("NetworkFilters", "{2213A2F7-7461-4A02-A928-96219359B84A}"), @("Plugins", "{5BD45CFF-7060-41D5-84A5-2FB2890336FA}"))) {
    $decal = $hklm.OpenSubKey("SOFTWARE\Decal\$($entry[0])", $true)
    if ($null -ne $decal) {
        if ($null -ne $decal.OpenSubKey($entry[1])) { $decal.DeleteSubKeyTree($entry[1]) }
        $decal.Close()
    }
}
if ($To -ne "" -and (Test-Path (Join-Path $To "Dereth.Decal.dll"))) {
    Remove-Item -Recurse -Force $To
}
Write-Output "Removed from Decal."
