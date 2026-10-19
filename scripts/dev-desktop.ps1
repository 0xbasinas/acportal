<#
One command for desktop development on Windows:
  1. builds acpd,
  2. starts OmniRoute in its own window (your profile's omniroute-serve, else `omniroute serve --no-open`),
  3. starts a development acpd host with no default agents in its own window,
  4. installs the desktop app's packages (and the Electron download npm's ignore-scripts skips),
  5. opens a terminal in the repository that prints a pairing code,
  6. runs the desktop app here.

The host has its own config in .local/dev-host. It has no agent registry, so the only agents are the
ones you add from the apps (Settings > Agents > Add agent); they are kept in .local/dev-host/state.

  ./scripts/dev-desktop.ps1
  ./scripts/dev-desktop.ps1 -WorkspaceRoot C:/Work -Port 8770 -SkipOmniRoute

The redesign app is the same command with -App desktop-redesign, or ./scripts/dev-desktop-redesign.ps1.
#>
param(
    # Folder the host may open sessions in (and below). Defaults to this repository.
    [string]$WorkspaceRoot,
    # Which app folder to install and run: desktop, or desktop-redesign.
    [ValidateSet('desktop', 'desktop-redesign')]
    [string]$App = 'desktop',
    [int]$Port = 8765,
    [switch]$SkipOmniRoute,
    [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$shell = (Get-Process -Id $PID).Path
$acpd = Join-Path $repoRoot 'target/debug/acpd.exe'
$hostDir = Join-Path $repoRoot '.local/dev-host'
$config = Join-Path $hostDir 'config.toml'
if (-not $WorkspaceRoot) { $WorkspaceRoot = $repoRoot }
$WorkspaceRoot = (Resolve-Path -LiteralPath $WorkspaceRoot).Path
function Step($text) { Write-Host "==> $text" -ForegroundColor Cyan }
# Opens a new window in the repository. It loads your PowerShell profile, so its functions work there.
function Open-Window($title, $command) {
    Start-Process -FilePath $shell -WorkingDirectory $repoRoot -ArgumentList @('-NoExit', '-Command', "`$Host.UI.RawUI.WindowTitle = '$title'; $command")
}
function Test-Listening { [bool](Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue) }

if (Test-Listening) {
    $owner = (Get-NetTCPConnection -State Listen -LocalPort $Port)[0].OwningProcess
    throw "Port $Port is already in use (process $owner). Stop that host or pass -Port."
}

if (-not $SkipBuild) {
    Step 'Building acpd'
    Push-Location -LiteralPath $repoRoot
    try {
        cargo build --locked -p acpd
        if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
    } finally { Pop-Location }
}

if (-not $SkipOmniRoute) {
    Step 'Starting OmniRoute in a new window'
    Open-Window 'OmniRoute' 'if (Get-Command omniroute-serve -ErrorAction SilentlyContinue) { omniroute-serve } else { omniroute serve --no-open }'
}

Step "Starting the host on http://127.0.0.1:$Port (no default agents) in a new window"
New-Item -ItemType Directory -Path (Join-Path $hostDir 'state') -Force | Out-Null
# Rewritten every run so -WorkspaceRoot and -Port take effect. "agents.json" does not exist in
# .local/dev-host on purpose: a missing registry means no agents except the ones added from an app.
$root = $WorkspaceRoot -replace '\\', '/'
@"
# Written by scripts/dev-desktop.ps1. Paths are relative to this file.
registry = "agents.json"
workspace_roots = ["$root"]
state_directory = "state"

[server]
listen = "127.0.0.1:$Port"
token_lifetime_seconds = 2592000
"@ | Set-Content -LiteralPath $config -Encoding utf8NoBOM
if (Test-Path -LiteralPath (Join-Path $hostDir 'agents.json')) {
    Write-Warning '.local/dev-host/agents.json exists and will be loaded; delete it for a host with no default agents.'
}
Open-Window 'acpd host' "& '$acpd' --config '$config' start"

$appDir = Join-Path $repoRoot $App
if (-not (Test-Path -LiteralPath (Join-Path $appDir 'package.json'))) { throw "No package.json in $appDir" }

Step "Installing $App packages"
Push-Location -LiteralPath $appDir
try {
    # npm.cmd, not npm: "npm" can resolve to npm.ps1, which a restricted execution policy blocks.
    # Package downloads go through Socket Firewall when sfw.exe is installed.
    $sfw = Get-Command sfw.exe -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($sfw) { & $sfw.Source npm.cmd install } else { npm.cmd install }
    if ($LASTEXITCODE -ne 0) { throw 'npm install failed' }
    # With npm's ignore-scripts on, Electron's own install step (which downloads the Electron
    # program) never runs and electron-vite fails with "Electron uninstall".
    if (-not (Test-Path -LiteralPath 'node_modules/electron/path.txt')) {
        Step 'Downloading the Electron program'
        node node_modules/electron/install.js
        if ($LASTEXITCODE -ne 0) { throw 'Electron download failed' }
    }

    # The code is valid for 60 seconds, so it is printed just before the app opens.
    Step 'Opening the pairing terminal'
    $deadline = (Get-Date).AddSeconds(30)
    while (-not (Test-Listening)) {
        if ((Get-Date) -gt $deadline) { throw "The host did not start listening on port $Port; see the 'acpd host' window." }
        Start-Sleep -Milliseconds 500
    }
    $pair = "& '$acpd' --config '$config' pair"
    # Single quotes only: Start-Process does not pass embedded double quotes through reliably.
    $shown = $pair -replace "'", "''"
    Open-Window 'acpd pair' "$pair; Write-Host ''; Write-Host 'In the app: address http://127.0.0.1:$Port and the code above. New code: run the line below.' -ForegroundColor Cyan; Write-Host '$shown'"

    Step "Running the $App app (Ctrl+C to stop; close the other windows yourself)"
    npm.cmd run dev
} finally { Pop-Location }
