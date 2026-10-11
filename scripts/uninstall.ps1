<#
.SYNOPSIS
  Removes ACP Portal (the acpd host and the tray app) from this Windows account.

.DESCRIPTION
  Stops the host and the tray, deletes the programs, the Start menu shortcut and the PATH entry that
  install.ps1 added, and any leftover pairing page. By default your settings and paired devices are
  KEPT, so installing again picks up where you left off. Add -Purge to delete those too.

  One line:
    irm https://raw.githubusercontent.com/0xbasinas/acportal/main/scripts/uninstall.ps1 | iex

  With options, save it and run it:
    irm https://raw.githubusercontent.com/0xbasinas/acportal/main/scripts/uninstall.ps1 -OutFile uninstall.ps1
    .\uninstall.ps1 -Purge

  Not removed (they belong to other programs): the phone app, and Tailscale Serve. To stop publishing
  the host over Tailscale run:  tailscale serve reset   (this clears ALL Serve settings).

.PARAMETER Purge
  Also delete your settings (config.toml, agents.json, launcher.json) and the data folder (paired
  devices, saved sessions, folders and agents added from the phone). Paired phones must pair again.
.PARAMETER Yes
  Do not ask for confirmation before -Purge deletes anything.
.PARAMETER Dir
  Install folder to remove from. Default: <data folder>\acpd\bin. Env: ACPD_INSTALL_DIR.
#>
[CmdletBinding()]
param(
    [switch]$Purge,
    [switch]$Yes,
    [string]$Dir = $env:ACPD_INSTALL_DIR
)

$ErrorActionPreference = 'Stop'

# The same folders the host and the tray use: XDG_* overrides win, then the standard Windows places.
$configHome = if ($env:XDG_CONFIG_HOME) { $env:XDG_CONFIG_HOME } else { $env:APPDATA }
$dataHome   = if ($env:XDG_DATA_HOME)   { $env:XDG_DATA_HOME }   else { $env:LOCALAPPDATA }
$settingsDir = Join-Path $configHome 'acpd'
$dataDir     = Join-Path $dataHome 'acpd'
if (-not $Dir) { $Dir = Join-Path $dataDir 'bin' }
$Shortcut = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\ACP Portal.lnk'
$Programs = @('acpd.exe', 'acpd-tray.exe')

function Write-Step([string]$Text) { Write-Host "  $Text" }

# The user PATH is edited in the registry so its type and %VARIABLES% are kept; it is only written
# when the entry is really there.
function Get-UserPathRaw {
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $false)
    try { $key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) } finally { $key.Close() }
}
function Remove-FromUserPath([string]$Folder) {
    $all = @((Get-UserPathRaw) -split ';')
    $keep = @($all | Where-Object { $_ -and ($_.TrimEnd('\') -ne $Folder.TrimEnd('\')) })
    if ($keep.Count -eq @($all | Where-Object { $_ }).Count) { return $false }
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
    try { $key.SetValue('Path', ($keep -join ';'), [Microsoft.Win32.RegistryValueKind]::ExpandString) } finally { $key.Close() }
    try {
        Add-Type -Namespace AcpdUninstall -Name Native -MemberDefinition '[System.Runtime.InteropServices.DllImport("user32.dll", SetLastError=true, CharSet=System.Runtime.InteropServices.CharSet.Auto)] public static extern System.IntPtr SendMessageTimeout(System.IntPtr hWnd, uint Msg, System.UIntPtr wParam, string lParam, uint flags, uint timeout, out System.UIntPtr result);'
        $result = [UIntPtr]::Zero
        [void][AcpdUninstall.Native]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero, 'Environment', 2, 2000, [ref]$result)
    } catch { }
    return $true
}

Write-Host 'Uninstalling ACP Portal'
if ($Purge) {
    Write-Host ''
    Write-Host '  -Purge will also delete:'
    Write-Host "    $settingsDir   (settings, agent list)"
    Write-Host "    $dataDir   (paired devices, saved sessions, folders and agents added from the phone)"
    if (-not $Yes) {
        if (-not [Environment]::UserInteractive -or [Console]::IsInputRedirected) {
            throw 'Refusing to purge without confirmation. Run it from a terminal, or add -Yes.'
        }
        $answer = Read-Host '  Type DELETE to continue'
        if ($answer -ne 'DELETE') { Write-Host '  Cancelled. Nothing was removed.'; return }
    }
    Write-Host ''
}

# Stop the host and the tray first: Windows cannot delete a running program.
$hostExe = Join-Path $Dir 'acpd.exe'
if (Test-Path -LiteralPath $hostExe) { try { & $hostExe daemon stop 2>$null | Out-Null } catch { } }
Get-Process -Name 'acpd-tray' -ErrorAction SilentlyContinue | Stop-Process -Force
Get-Process -Name 'acpd' -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -and ($_.Path -eq $hostExe) } | Stop-Process -Force
Start-Sleep -Milliseconds 300

$removed = 0
foreach ($name in $Programs) {
    $path = Join-Path $Dir $name
    if (Test-Path -LiteralPath $path) {
        try { Remove-Item -LiteralPath $path -Force; $removed++ }
        catch { throw "Could not delete $path. Close anything using it and run this again. ($($_.Exception.Message))" }
    }
}
if ($removed) { Write-Step "Removed $removed program file(s) from $Dir" } else { Write-Step "No programs found in $Dir" }
if ((Test-Path -LiteralPath $Dir) -and -not (Get-ChildItem -LiteralPath $Dir -Force)) { Remove-Item -LiteralPath $Dir -Force }

if (Test-Path -LiteralPath $Shortcut) { Remove-Item -LiteralPath $Shortcut -Force; Write-Step 'Removed the Start menu shortcut.' }
if (Remove-FromUserPath $Dir) { Write-Step 'Removed the install folder from your user PATH.' }
Get-ChildItem -LiteralPath ([IO.Path]::GetTempPath()) -Directory -Filter 'acpd-pair-*' -ErrorAction SilentlyContinue |
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue

if ($Purge) {
    foreach ($folder in @($settingsDir, $dataDir)) {
        if (Test-Path -LiteralPath $folder) { Remove-Item -LiteralPath $folder -Recurse -Force; Write-Step "Deleted $folder" }
    }
    Write-Host ''
    Write-Host 'Everything was removed. Paired phones must pair again after a reinstall.'
} else {
    Write-Host ''
    Write-Host 'Your settings and paired devices were kept:'
    Write-Host "  $settingsDir"
    Write-Host "  $(Join-Path $dataDir 'state')"
    Write-Host 'Delete them too with:  .\uninstall.ps1 -Purge'
}
Write-Host 'To stop publishing the host over Tailscale (this clears ALL Serve settings): tailscale serve reset'
