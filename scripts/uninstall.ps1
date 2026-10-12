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
.PARAMETER Only
  What to remove: both (default), host (just acpd.exe) or tray (just acpd-tray.exe, its shortcut).
  Env: ACPD_ONLY. Cannot be combined with -Purge, which removes everything.

  With no options, and when run in a terminal, a small menu asks what to remove: everything, everything
  including your settings, only the host or only the tray.
.PARAMETER Dir
  Install folder to remove from. Default: <data folder>\acpd\bin. Env: ACPD_INSTALL_DIR.
#>
[CmdletBinding()]
param(
    [switch]$Purge,
    [switch]$Yes,
    [string]$Only = $env:ACPD_ONLY,
    [string]$Dir = $env:ACPD_INSTALL_DIR
)

$ErrorActionPreference = 'Stop'

# No options given: ask what to remove (a terminal only; otherwise the default is both, keeping settings).
if ($PSBoundParameters.Count -eq 0 -and -not $env:ACPD_ONLY -and -not $env:ACPD_INSTALL_DIR -and [Environment]::UserInteractive -and -not [Console]::IsInputRedirected) {
    Write-Host 'ACP Portal uninstall - what do you want to remove?'
    Write-Host '  1) Everything (host and tray); keep my settings and paired phones'
    Write-Host '  2) Everything, and DELETE my settings, paired phones and saved sessions'
    Write-Host '  3) Only the host (acpd)'
    Write-Host '  4) Only the tray'
    Write-Host '  q) Cancel'
    $choice = Read-Host 'Choose [1]'
    switch ($choice.Trim().ToLower()) {
        { $_ -in '', '1' } { }
        '2' { $Purge = [switch]$true }
        '3' { $Only = 'host' }
        '4' { $Only = 'tray' }
        default { Write-Host '  Cancelled. Nothing was removed.'; return }
    }
    Write-Host ''
}

$Only = if ($Only) { $Only.ToLower() } else { 'both' }
if ($Only -notin 'both', 'host', 'tray') { throw "-Only must be both, host or tray, got: $Only" }
if ($Purge -and $Only -ne 'both') { throw '-Purge removes everything, so it cannot be combined with -Only. Run it without -Only.' }
$WithHost = $Only -ne 'tray'
$WithTray = $Only -ne 'host'
$Label = switch ($Only) { 'host' { 'the acpd host' } 'tray' { 'the tray app' } default { 'ACP Portal (host and tray)' } }

# The same folders the host and the tray use: XDG_* overrides win, then the standard Windows places.
$configHome = if ($env:XDG_CONFIG_HOME) { $env:XDG_CONFIG_HOME } else { $env:APPDATA }
$dataHome   = if ($env:XDG_DATA_HOME)   { $env:XDG_DATA_HOME }   else { $env:LOCALAPPDATA }
$settingsDir = Join-Path $configHome 'acpd'
$dataDir     = Join-Path $dataHome 'acpd'
if (-not $Dir) { $Dir = Join-Path $dataDir 'bin' }
$Shortcut = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\ACP Portal.lnk'
$Programs = @()
if ($WithHost) { $Programs += 'acpd.exe' }
if ($WithTray) { $Programs += 'acpd-tray.exe' }

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

Write-Host "Uninstalling $Label"
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
if ($WithHost -and (Test-Path -LiteralPath $hostExe)) { try { & $hostExe daemon stop 2>$null | Out-Null } catch { } }
if ($WithTray) { Get-Process -Name 'acpd-tray' -ErrorAction SilentlyContinue | Stop-Process -Force }
if ($WithHost) {
    Get-Process -Name 'acpd' -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -and ($_.Path -eq $hostExe) } | Stop-Process -Force
}
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

if ($WithTray) {
    if (Test-Path -LiteralPath $Shortcut) { Remove-Item -LiteralPath $Shortcut -Force; Write-Step 'Removed the Start menu shortcut.' }
    Get-ChildItem -LiteralPath ([IO.Path]::GetTempPath()) -Directory -Filter 'acpd-pair-*' -ErrorAction SilentlyContinue |
        Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
}
# The folder stays on PATH for as long as the host is still there.
if (-not (Test-Path -LiteralPath $hostExe)) {
    if (Remove-FromUserPath $Dir) { Write-Step 'Removed the install folder from your user PATH.' }
}

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
