param([switch]$Pair,[switch]$RequireAuth,[switch]$Terminal,[switch]$Media)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $repoRoot
try {
    New-Item -ItemType Directory -Path .local -Force | Out-Null
    $configFile = Join-Path $repoRoot '.local/mock-config.toml'
    if (-not $Pair) {
        cargo build --locked --workspace --bins
        if ($LASTEXITCODE -ne 0) { throw 'Host build failed' }
        $mockBinary = (Resolve-Path -LiteralPath 'target/debug/mock-acp-agent.exe').Path
        $mockArgs = @(if ($RequireAuth) { '--require-auth' }; if ($Terminal) { '--terminal-during-prompt' }; if ($Media) { '--prompt-media' })
        @(@{id='mock';name='Mock ACP';command=$mockBinary;args=$mockArgs;enabled=$true},@{id='goose';name='Goose';command='goose';enabled=$false},@{id='gemini';name='Gemini';command='gemini';enabled=$false}) |
            ConvertTo-Json -Depth 4 -AsArray | Set-Content -LiteralPath .local/mock-agents.json -Encoding utf8NoBOM
        $rootLiteral = ConvertTo-Json -InputObject $repoRoot -Compress
        @"
registry = 'mock-agents.json'
workspace_roots = [$rootLiteral]
state_directory = 'mock-state'
[server]
listen = '127.0.0.1:8767'
"@ | Set-Content -LiteralPath $configFile -Encoding utf8NoBOM
        & ./target/debug/acpd.exe --config $configFile start
    } else {
        if (-not (Test-Path -LiteralPath $configFile)) { throw 'Start scripts/mock-host.ps1 in another terminal first' }
        & ./target/debug/acpd.exe --config $configFile pair
    }
    if ($LASTEXITCODE -ne 0) { throw 'Mock host command failed' }
} finally { Pop-Location }
