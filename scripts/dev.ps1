param(
    [ValidateSet('setup', 'acpd', 'start', 'pair', 'android', 'android-test', 'test', 'probe', 'chat', 'mock')]
    [string]$Task = 'test',
    [string]$Prompt = 'Say hello without using tools or changing files.'
)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $repoRoot
try {
    switch ($Task) {
        'setup' { cargo fetch --locked }
        'acpd' { cargo build --locked --workspace --bins }
        'start' { cargo run --locked -p acpd -- --config examples/config.toml start }
        'pair' { cargo run --locked -p acpd -- --config examples/config.toml pair }
        'android' { & ./android/universal-acp/gradlew.bat -p android/universal-acp :app:assembleDebug }
        'android-test' { & ./android/universal-acp/gradlew.bat -p android/universal-acp :core:protocol:test :app:testDebugUnitTest }
        'test' {
            cargo fmt --all -- --check
            if ($LASTEXITCODE -ne 0) { throw 'Formatting failed' }
            cargo clippy --locked --workspace --all-targets -- -D warnings
            if ($LASTEXITCODE -ne 0) { throw 'Clippy failed' }
            cargo test --locked --workspace --all-targets
        }
        'probe' { cargo run --locked -p acpd -- --config examples/config.toml probe --workspace . }
        'chat' { cargo run --locked -p acpd -- --config examples/config.toml chat --workspace . --prompt $Prompt }
        'mock' {
            cargo build --locked --workspace --bins
            if ($LASTEXITCODE -ne 0) { throw 'Build failed' }
            $mockBinary = (Resolve-Path -LiteralPath 'target/debug/mock-acp-agent.exe').Path
            $mockRegistry = Join-Path $repoRoot '.local/mock-agents.json'
            New-Item -ItemType Directory -Path (Join-Path $repoRoot '.local') -Force | Out-Null
            @(@{ id = 'mock'; name = 'Mock ACP'; command = $mockBinary; args = @(); enabled = $true }) |
                ConvertTo-Json -Depth 4 -AsArray | Set-Content -LiteralPath $mockRegistry -Encoding utf8NoBOM
            cargo run --locked -p acpd -- --config examples/config.toml --registry $mockRegistry chat --agent mock --workspace . --prompt $Prompt
        }
    }
    if ($LASTEXITCODE -ne 0) { throw "$Task failed with exit code $LASTEXITCODE" }
} finally { Pop-Location }
