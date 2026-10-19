<#
Launches the redesign desktop app (desktop-redesign/) the same way scripts/dev-desktop.ps1
launches desktop/: build acpd, start OmniRoute and a host, print a pairing code, run the app.

  ./scripts/dev-desktop-redesign.ps1
  ./scripts/dev-desktop-redesign.ps1 -WorkspaceRoot C:/Work -Port 8770 -SkipOmniRoute -SkipBuild
#>
param(
    # Folder the host may open sessions in (and below). Defaults to this repository.
    [string]$WorkspaceRoot,
    [int]$Port = 8765,
    [switch]$SkipOmniRoute,
    [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
$forward = @{ App = 'desktop-redesign'; Port = $Port }
if ($PSBoundParameters.ContainsKey('WorkspaceRoot')) { $forward.WorkspaceRoot = $WorkspaceRoot }
if ($SkipOmniRoute) { $forward.SkipOmniRoute = $true }
if ($SkipBuild) { $forward.SkipBuild = $true }
& (Join-Path $PSScriptRoot 'dev-desktop.ps1') @forward
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
