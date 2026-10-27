# Takes the Dereth Network companion out of Decal's plugins, and deletes its folder when -To names it.
# Run it in an elevated PowerShell with the game closed:
#
#   powershell -ExecutionPolicy Bypass -File uninstall.ps1 [-To C:\Games\Dereth\Companion]
param([string]$To = "")
$ErrorActionPreference = "Stop"

$Clsid = "{CAAB002B-7BA1-4EC2-A3C2-454604368082}"
$hklm = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::LocalMachine, [Microsoft.Win32.RegistryView]::Registry32)
$decal = $hklm.OpenSubKey("SOFTWARE\Decal\Plugins", $true)
if ($null -ne $decal) {
    if ($null -ne $decal.OpenSubKey($Clsid)) { $decal.DeleteSubKeyTree($Clsid) }
    $decal.Close()
}
if ($To -ne "" -and (Test-Path (Join-Path $To "Dereth.Network.Companion.dll"))) {
    Remove-Item -Recurse -Force $To
}
Write-Output "Removed from Decal."
