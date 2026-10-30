param(
    [string]$DataDir = ([IO.Path]::Combine($env:LOCALAPPDATA, 'Minimium', 'procedural')),
    [string]$Seed
)

$ErrorActionPreference = 'Stop'
$projectRoot = $PSScriptRoot
$arguments = @('run', '--data-dir', $DataDir, '--planet-config', (Join-Path $projectRoot 'presets/planet/procedural.ron'))
if ($Seed) { $arguments += @('--seed', $Seed) }
& cargo build --release -p minimium-server --manifest-path (Join-Path $projectRoot 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$target = if ($env:CARGO_TARGET_DIR) {
    if ([IO.Path]::IsPathRooted($env:CARGO_TARGET_DIR)) { $env:CARGO_TARGET_DIR }
    else { Join-Path (Get-Location) $env:CARGO_TARGET_DIR }
} else { Join-Path $projectRoot 'target' }
& (Join-Path $target 'release/minimium-server.exe') @arguments
if ($LASTEXITCODE -eq -1073741510) { exit 0 } # Ctrl+C on Windows
exit $LASTEXITCODE
