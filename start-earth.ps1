param(
    [string]$DataDir = ([IO.Path]::Combine($env:LOCALAPPDATA, 'Minimium', 'earth')),
    [string]$PackageDir = (Join-Path $PSScriptRoot 'third_party/gebco-2026'),
    [string]$ProfileDir,
    [string]$Seed
)

$ErrorActionPreference = 'Stop'
$package = [IO.Path]::GetFullPath($PackageDir)
if (-not (Test-Path (Join-Path $package 'index.ron'))) {
    throw "GEBCO 2026 package is missing at $package. Run $PSScriptRoot/import-gebco-lite.ps1 -PackageDir $package for the compact 1-arcminute package, or import the full-resolution netCDF with import-gebco.ps1."
}
$projectRoot = $PSScriptRoot
& cargo build --release -p minimium-server -p minimium-client --manifest-path (Join-Path $projectRoot 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$target = if ($env:CARGO_TARGET_DIR) {
    if ([IO.Path]::IsPathRooted($env:CARGO_TARGET_DIR)) { $env:CARGO_TARGET_DIR }
    else { Join-Path (Get-Location) $env:CARGO_TARGET_DIR }
} else { Join-Path $projectRoot 'target' }
$client = Join-Path $target 'release/minimium-client.exe'
$profile = if ($ProfileDir) { $ProfileDir } else { "${DataDir}-profile" }
$arguments = @('--server-data-dir', $DataDir, '--profile-dir', $profile, '--require-earth')
$previousPackage = [Environment]::GetEnvironmentVariable('MINIMIUM_EARTH_PACKAGE', 'Process')
$previousConfig = [Environment]::GetEnvironmentVariable('MINIMIUM_LOCAL_PLANET_CONFIG', 'Process')
$previousSeed = [Environment]::GetEnvironmentVariable('MINIMIUM_LOCAL_SEED', 'Process')
try {
    [Environment]::SetEnvironmentVariable('MINIMIUM_EARTH_PACKAGE', (Resolve-Path -LiteralPath $package).Path, 'Process')
    [Environment]::SetEnvironmentVariable('MINIMIUM_LOCAL_PLANET_CONFIG', (Join-Path $projectRoot 'presets/planet/earth.ron'), 'Process')
    if ($Seed) {
        [Environment]::SetEnvironmentVariable('MINIMIUM_LOCAL_SEED', $Seed, 'Process')
    } else {
        [Environment]::SetEnvironmentVariable('MINIMIUM_LOCAL_SEED', $null, 'Process')
    }
    & $client @arguments
    $exitCode = $LASTEXITCODE
} finally {
    [Environment]::SetEnvironmentVariable('MINIMIUM_EARTH_PACKAGE', $previousPackage, 'Process')
    [Environment]::SetEnvironmentVariable('MINIMIUM_LOCAL_PLANET_CONFIG', $previousConfig, 'Process')
    [Environment]::SetEnvironmentVariable('MINIMIUM_LOCAL_SEED', $previousSeed, 'Process')
}
if ($exitCode -eq -1073741510) { exit 0 } # Ctrl+C on Windows
exit $exitCode
