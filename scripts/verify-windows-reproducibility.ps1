# Run through GitHub Actions. This compares independent build outputs on one runner.
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
if (-not $IsWindows -or $env:GITHUB_ACTIONS -ne 'true') {
    throw 'This supported-build check must run on a Windows GitHub Actions runner.'
}
$repoPath = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$buildRoot = Join-Path $env:RUNNER_TEMP ('acpd-repro-' + [guid]::NewGuid().ToString('N'))
$evidencePath = Join-Path $repoPath 'reproducibility-evidence.txt'
$previousFlags = $env:RUSTFLAGS
$previousTarget = $env:CARGO_TARGET_DIR
$previousIncremental = $env:CARGO_INCREMENTAL
try {
    New-Item -ItemType Directory -Path $buildRoot | Out-Null
    $env:CARGO_INCREMENTAL = '0'
    # MSVC /Brepro removes wall-clock PE timestamps. Normalize build directory paths.
    $hashes = @()
    Push-Location $repoPath
    try {
        foreach ($buildName in @('first', 'second')) {
            $env:CARGO_TARGET_DIR = Join-Path $buildRoot $buildName
            $env:RUSTFLAGS = "--remap-path-prefix=$repoPath=/acportal --remap-path-prefix=$env:CARGO_TARGET_DIR=/build -C link-arg=/Brepro"
            & cargo build --locked --release -p acpd --bin acpd
            if ($LASTEXITCODE -ne 0) { throw "Release build $buildName failed." }
            $binaryPath = Join-Path $env:CARGO_TARGET_DIR 'release/acpd.exe'
            $hashes += (Get-FileHash -LiteralPath $binaryPath -Algorithm SHA256).Hash
            & $binaryPath --version
            if ($LASTEXITCODE -ne 0) { throw "Release binary $buildName failed its version smoke check." }
        }
        $toolchain = & rustc --version --verbose
        if ($LASTEXITCODE -ne 0) { throw 'Cannot read Rust toolchain evidence.' }
        @(
            "Commit: $env:GITHUB_SHA"
            "Runner image: $env:ImageOS $env:ImageVersion"
            $toolchain
            "First SHA256: $($hashes[0])"
            "Second SHA256: $($hashes[1])"
            'Scope: two fresh target directories, same source checkout and runner/toolchain.'
            'Excludes cross-runner/toolchain reproducibility, PDBs and signed artifacts.'
        ) | Set-Content -LiteralPath $evidencePath
        if ($hashes[0] -ne $hashes[1]) { throw 'Independent release executable hashes differ.' }
    } finally {
        Pop-Location
    }
} finally {
    $env:RUSTFLAGS = $previousFlags
    $env:CARGO_TARGET_DIR = $previousTarget
    $env:CARGO_INCREMENTAL = $previousIncremental
    # Keep outputs on the ephemeral runner for diagnosis; never delete user resources.
}
