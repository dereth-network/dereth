# Installs Dereth's plugin host for Decal: copies it to a folder and lists its two halves among
# Decal's network filters and plugins. Run it in an elevated PowerShell (Decal keeps its lists in the
# machine's registry), with the game closed:
#
#   powershell -ExecutionPolicy Bypass -File install.ps1 -From <folder holding Dereth.Decal.dll>
#
# The folder holds Dereth.Decal.dll and dereth_decal_host.dll, and may hold a plugins folder (the
# plugins that come with it). -To names the folder to install to (C:\Games\Dereth\Decal unless
# given). Uninstall with uninstall.ps1.
param(
    [Parameter(Mandatory = $true)][string]$From,
    [string]$To = "C:\Games\Dereth\Decal"
)
$ErrorActionPreference = "Stop"

$Filter = "{2213A2F7-7461-4A02-A928-96219359B84A}"
$Plugin = "{5BD45CFF-7060-41D5-84A5-2FB2890336FA}"
$Assembly = "Dereth.Decal.dll"
$Library = "dereth_decal_host.dll"
$Surrogate = "{71A69713-6593-47EC-0002-0000000DECA1}"

foreach ($f in @($Assembly, $Library)) {
    if (-not (Test-Path (Join-Path $From $f))) { throw "There is no $f in $From." }
}

New-Item -ItemType Directory -Force -Path $To | Out-Null
foreach ($f in @($Assembly, $Library, "LICENSE")) {
    $p = Join-Path $From $f
    if (Test-Path $p) { Copy-Item $p $To -Force }
}
$bundled = Join-Path $From "plugins"
if (Test-Path $bundled) { Copy-Item $bundled $To -Recurse -Force }
$path = (Resolve-Path $To).Path

# Decal is a 32-bit program, so it reads the 32-bit view of the registry.
$hklm = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::LocalMachine, [Microsoft.Win32.RegistryView]::Registry32)
function Register($list, $clsid, $class) {
    $decal = $hklm.OpenSubKey("SOFTWARE\Decal\$list", $true)
    if ($null -eq $decal) { throw "Decal is not installed: there is no SOFTWARE\Decal\$list key." }
    $key = $decal.CreateSubKey($clsid)
    $key.SetValue("", "Dereth Plugins")
    $key.SetValue("Enabled", 1, [Microsoft.Win32.RegistryValueKind]::DWord)
    $key.SetValue("Object", $class)
    $key.SetValue("Assembly", $Assembly)
    $key.SetValue("Path", $path)
    $key.SetValue("Surrogate", $Surrogate)
    $key.Close()
    $decal.Close()
}
Register "NetworkFilters" $Filter "Dereth.Decal.Plugin.DerethFilter"
Register "Plugins" $Plugin "Dereth.Decal.Plugin.DerethPlugin"
Write-Output "Installed in $To. Put plugins in $env:APPDATA\Dereth\decal\plugins and start the game through Decal."
