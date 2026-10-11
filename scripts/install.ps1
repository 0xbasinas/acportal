<#
.SYNOPSIS
  Installs ACP Portal (the acpd host and the tray app) for the current user. No admin rights needed.

.DESCRIPTION
  Downloads acpd.exe and acpd-tray.exe from the project's GitHub release, checks them against the
  release's SHA256SUMS.txt, and installs them in %LOCALAPPDATA%\acpd\bin with a Start menu shortcut and
  that folder on your user PATH. Run it again to update. Nothing outside your user profile is changed.

  Quick install (read the script first if you like: it is short and lives in this repository):
    irm https://raw.githubusercontent.com/0xbasinas/acportal/main/scripts/install.ps1 | iex

  With options, save it and run it:
    irm https://raw.githubusercontent.com/0xbasinas/acportal/main/scripts/install.ps1 -OutFile install.ps1
    .\install.ps1 -Version v0.1.1 -Start

.PARAMETER Version
  Release tag to install, for example v0.1.1. Default: the latest release. Env: ACPD_VERSION.
.PARAMETER Dir
  Install folder. Default: %LOCALAPPDATA%\acpd\bin. Env: ACPD_INSTALL_DIR.
.PARAMETER Uninstall
  Remove the programs, the shortcut and the PATH entry. Your settings and paired devices are kept.
.PARAMETER NoPath
  Do not add the install folder to your user PATH.
.PARAMETER NoShortcut
  Do not create the Start menu shortcut.
.PARAMETER Start
  Start the tray app when the install finishes.
#>
[CmdletBinding()]
param(
    [string]$Version = $env:ACPD_VERSION,
    [string]$Dir = $env:ACPD_INSTALL_DIR,
    [switch]$Uninstall,
    [switch]$NoPath,
    [switch]$NoShortcut,
    [switch]$Start
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'   # the progress bar makes downloads much slower in Windows PowerShell
try { [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12 } catch { }

$Repo = '0xbasinas/acportal'
$Files = @(
    @{ Asset = 'acpd-windows-x86_64.exe';      Name = 'acpd.exe' },
    @{ Asset = 'acpd-tray-windows-x86_64.exe'; Name = 'acpd-tray.exe' }
)
if (-not $Dir) {
    # The same place the host and the tray use: XDG_DATA_HOME if set, else the local app data folder.
    $dataHome = if ($env:XDG_DATA_HOME) { $env:XDG_DATA_HOME } else { $env:LOCALAPPDATA }
    $Dir = Join-Path $dataHome 'acpd\bin'
}
$Shortcut = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\ACP Portal.lnk'

function Write-Step([string]$Text) { Write-Host "  $Text" }

# The user PATH is edited in the registry so that its type (expandable) and any %VARIABLES% in it are
# kept, and it is only written when it really changes.
function Get-UserPathRaw {
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $false)
    try { $key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) } finally { $key.Close() }
}
function Set-UserPathRaw([string]$Value) {
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
    try { $key.SetValue('Path', $Value, [Microsoft.Win32.RegistryValueKind]::ExpandString) } finally { $key.Close() }
    # Tell running programs (Explorer, new terminals) that the environment changed.
    try {
        Add-Type -Namespace AcpdInstall -Name Native -MemberDefinition '[System.Runtime.InteropServices.DllImport("user32.dll", SetLastError=true, CharSet=System.Runtime.InteropServices.CharSet.Auto)] public static extern System.IntPtr SendMessageTimeout(System.IntPtr hWnd, uint Msg, System.UIntPtr wParam, string lParam, uint flags, uint timeout, out System.UIntPtr result);'
        $result = [UIntPtr]::Zero
        [void][AcpdInstall.Native]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero, 'Environment', 2, 2000, [ref]$result)
    } catch { }
}
function Test-InUserPath([string]$Folder) {
    [bool](@((Get-UserPathRaw) -split ';') | Where-Object { $_ -and ($_.TrimEnd('\') -eq $Folder.TrimEnd('\')) })
}
function Add-ToUserPath([string]$Folder) {
    if (Test-InUserPath $Folder) { return $false }
    $current = (Get-UserPathRaw).TrimEnd(';')
    Set-UserPathRaw ($(if ($current) { "$current;$Folder" } else { $Folder }))
    return $true
}
function Remove-FromUserPath([string]$Folder) {
    if (-not (Test-InUserPath $Folder)) { return $false }
    $parts = @((Get-UserPathRaw) -split ';' | Where-Object { $_ -and ($_.TrimEnd('\') -ne $Folder.TrimEnd('\')) })
    Set-UserPathRaw ($parts -join ';')
    return $true
}

function Stop-Running {
    # A running program cannot be replaced or deleted on Windows.
    Get-Process -Name 'acpd-tray' -ErrorAction SilentlyContinue | Stop-Process -Force
    $host_ = Join-Path $Dir 'acpd.exe'
    if (Test-Path $host_) {
        try { & $host_ daemon stop 2>$null | Out-Null } catch { }
        Get-Process -Name 'acpd' -ErrorAction SilentlyContinue |
            Where-Object { $_.Path -eq $host_ } | Stop-Process -Force
    }
}

if ($Uninstall) {
    Write-Host "Uninstalling ACP Portal from $Dir"
    Stop-Running
    foreach ($file in $Files) { Remove-Item -LiteralPath (Join-Path $Dir $file.Name) -Force -ErrorAction SilentlyContinue }
    Remove-Item -LiteralPath $Shortcut -Force -ErrorAction SilentlyContinue
    $pathChanged = Remove-FromUserPath $Dir
    if ((Test-Path $Dir) -and -not (Get-ChildItem -LiteralPath $Dir -Force)) { Remove-Item -LiteralPath $Dir -Force }
    Write-Step ('Removed the programs and the Start menu shortcut' + $(if ($pathChanged) { ' and the PATH entry.' } else { '.' }))
    Write-Step "Your settings ($env:APPDATA\acpd) and paired devices ($env:LOCALAPPDATA\acpd\state) were kept."
    return
}

if ($Version -and $Version -notmatch '^v\d+(\.\d+)*$') { throw "Version must look like v0.1.1, got: $Version" }
$base = if ($Version) { "https://github.com/$Repo/releases/download/$Version" } else { "https://github.com/$Repo/releases/latest/download" }
Write-Host ("Installing ACP Portal ({0}) into {1}" -f ($(if ($Version) { $Version } else { 'latest release' })), $Dir)

$work = Join-Path ([IO.Path]::GetTempPath()) ("acpd-install-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work | Out-Null
try {
    Write-Step 'Downloading checksums...'
    $sumsText = (Invoke-WebRequest -UseBasicParsing -Uri "$base/SHA256SUMS.txt").Content
    if ($sumsText -is [byte[]]) { $sumsText = [Text.Encoding]::UTF8.GetString($sumsText) }
    $expected = @{}
    foreach ($line in ($sumsText -split "`n")) {
        if ($line -match '^\s*([0-9a-fA-F]{64})\s+\*?(\S+)\s*$') { $expected[$Matches[2]] = $Matches[1].ToLower() }
    }

    foreach ($file in $Files) {
        $asset = $file.Asset
        if (-not $expected.ContainsKey($asset)) { throw "SHA256SUMS.txt has no checksum for $asset." }
        Write-Step "Downloading $asset..."
        $target = Join-Path $work $file.Name
        Invoke-WebRequest -UseBasicParsing -Uri "$base/$asset" -OutFile $target
        $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $target).Hash.ToLower()
        if ($actual -ne $expected[$asset]) { throw "$asset did not match its checksum, so nothing was installed." }
    }
    Write-Step 'Checksums match.'

    Stop-Running
    New-Item -ItemType Directory -Path $Dir -Force | Out-Null
    foreach ($file in $Files) {
        $final = Join-Path $Dir $file.Name
        try { Move-Item -LiteralPath (Join-Path $work $file.Name) -Destination $final -Force }
        catch { throw "Could not replace $final. Close anything using it (acpd daemon stop) and run this again. ($($_.Exception.Message))" }
        Unblock-File -LiteralPath $final -ErrorAction SilentlyContinue
    }
    Write-Step "Installed acpd.exe and acpd-tray.exe in $Dir"

    if (-not $NoPath) {
        if (Add-ToUserPath $Dir) { Write-Step 'Added the folder to your user PATH (open a new terminal to use acpd).' }
    }
    if (-not $NoShortcut) {
        $shell = New-Object -ComObject WScript.Shell
        $link = $shell.CreateShortcut($Shortcut)
        $link.TargetPath = Join-Path $Dir 'acpd-tray.exe'
        $link.WorkingDirectory = $Dir
        $link.Description = 'ACP Portal tray'
        $link.Save()
        Write-Step 'Added a Start menu shortcut: ACP Portal.'
    }
}
finally {
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host ''
Write-Host 'Done. Next:'
Write-Host '  1. Open ACP Portal from the Start menu (or run acpd-tray).'
Write-Host '  2. In its menu: Set up with Tailscale, Start host, Pair a phone.'
Write-Host '  Or use the command line: acpd setup --address https://YOUR-HOST ; acpd start --bg ; acpd pair'
if ($Start) { Start-Process -FilePath (Join-Path $Dir 'acpd-tray.exe') }
