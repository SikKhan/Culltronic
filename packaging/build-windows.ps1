param([switch]$NoBuild)
$ErrorActionPreference = 'Stop'
Push-Location (Split-Path -Parent $PSScriptRoot)
$previousFlags = $env:RUSTFLAGS
try {
    # Explicit flags also cover callers whose RUSTFLAGS override .cargo/config.toml.
    $env:RUSTFLAGS = '-D warnings -C target-feature=+crt-static'
    if (-not $NoBuild) {
        cargo build --release --locked --target x86_64-pc-windows-msvc -p culltronic-ui
        if ($LASTEXITCODE -ne 0) { throw 'Windows release build failed' }
    }
    python packaging/package-windows.py
    if ($LASTEXITCODE -ne 0) { throw 'Windows packaging failed' }
} finally {
    $env:RUSTFLAGS = $previousFlags
    Pop-Location
}
