# Installs the Dereth Network companion for Decal: copies the plugin to a folder and lists it among
# Decal's plugins. Run it in an elevated PowerShell (Decal keeps its list in the machine's registry),
# with the game closed:
#
#   powershell -ExecutionPolicy Bypass -File install.ps1 -From <folder holding Dereth.Network.Companion.dll>
#
# -To names the folder to install to (C:\Games\Dereth\Companion unless given). Uninstall with
# uninstall.ps1.
param(
    [Parameter(Mandatory = $true)][string]$From,
    [string]$To = "C:\Games\Dereth\Companion"
)
$ErrorActionPreference = "Stop"

$Clsid = "{CAAB002B-7BA1-4EC2-A3C2-454604368082}"
$Assembly = "Dereth.Network.Companion.dll"
$Surrogate = "{71A69713-6593-47EC-0002-0000000DECA1}"

$dll = Join-Path $From $Assembly
if (-not (Test-Path $dll)) { throw "There is no $Assembly in $From." }

New-Item -ItemType Directory -Force -Path $To | Out-Null
Copy-Item $dll $To -Force
foreach ($extra in @("LICENSE", "settings.txt")) {
    $p = Join-Path $From $extra
    if (Test-Path $p) { Copy-Item $p $To -Force }
}

# Decal is a 32-bit program, so it reads the 32-bit view of the registry.
$hklm = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::LocalMachine, [Microsoft.Win32.RegistryView]::Registry32)
$decal = $hklm.OpenSubKey("SOFTWARE\Decal\Plugins", $true)
if ($null -eq $decal) { throw "Decal is not installed: there is no SOFTWARE\Decal\Plugins key." }
$key = $decal.CreateSubKey($Clsid)
$key.SetValue("", "Dereth Network")
$key.SetValue("Enabled", 1, [Microsoft.Win32.RegistryValueKind]::DWord)
$key.SetValue("Object", "Dereth.Network.Companion.Decal.CompanionPlugin")
$key.SetValue("Assembly", $Assembly)
$key.SetValue("Path", (Resolve-Path $To).Path)
$key.SetValue("Surrogate", $Surrogate)
$key.Close()
$decal.Close()
Write-Output "Installed in $To. Start the game through Decal; the Dereth Network button is on Decal's bar."
