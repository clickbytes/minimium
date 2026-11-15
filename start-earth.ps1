[CmdletBinding()]
param(
    [string]$DataDir = ([IO.Path]::Combine($env:LOCALAPPDATA, 'Minimium', 'earth-v6')),
    [string]$PackageDir = (Join-Path $PSScriptRoot 'third_party/gebco-2026'),
    [string]$ProfileDir,
    [string]$Seed,
    [ValidateScript({ -not [double]::IsNaN($_) -and -not [double]::IsInfinity($_) -and $_ -ge 1 -and $_ -le 3 })]
    [double]$PeakMultiplier = 1.5,
    [ValidateRange(1, 8)]
    [int]$PeakExponent = 3,
    [ValidateRange(0, 11999)]
    [int]$PeakStartMeters = 1000,
    [ValidateRange(1, 12000)]
    [int]$PeakFullMeters = 9000,
    [ValidateScript({ -not [double]::IsNaN($_) -and -not [double]::IsInfinity($_) -and $_ -ge 0 -and $_ -le 1 })]
    [double]$ClimateReliefInfluence = 0,
    [Alias('ResetWorld')]
    [switch]$RebuildWorld,
    [switch]$Windowed,
    [switch]$DevStdin
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'scripts/earth-world.ps1')
$DataDir = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($DataDir)
if ($DataDir -ne [IO.Path]::GetPathRoot($DataDir)) { $DataDir = $DataDir.TrimEnd('\', '/') }
$profile = if ($ProfileDir) {
    $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($ProfileDir)
} else { "${DataDir}-profile" }
function Set-LauncherEnvironmentVariable {
    param([string]$Name, [AllowNull()][AllowEmptyString()][string]$Value)
    # Explicitly remove omitted options instead of relying on null-to-string binding.
    if ([string]::IsNullOrEmpty($Value)) {
        Remove-Item -LiteralPath "Env:$Name" -ErrorAction SilentlyContinue
    } else {
        [Environment]::SetEnvironmentVariable($Name, $Value, 'Process')
    }
}

$peakRequested = @('PeakMultiplier', 'PeakExponent', 'PeakStartMeters', 'PeakFullMeters') |
    Where-Object { $PSBoundParameters.ContainsKey($_) }
if ($PeakStartMeters -ge $PeakFullMeters) {
    throw 'PeakStartMeters must be lower than PeakFullMeters (source metres above sea level).'
}
$existingWorld = @('world.planet.ron', 'world.snapshot', 'world.seed') |
    Where-Object { Test-Path -LiteralPath (Join-Path $DataDir $_) }
if ($peakRequested -and $existingWorld -and -not $RebuildWorld) {
    throw 'Peak settings define a new world. Use a new -DataDir or -RebuildWorld (preserves world and profile backups). To resume the saved world, omit the Peak parameters.'
}
$package = [IO.Path]::GetFullPath($PackageDir)
if (-not (Test-Path (Join-Path $package 'index.ron'))) {
    throw "GEBCO 2026 package is missing at $package. Run $PSScriptRoot/import-gebco-lite.ps1 -PackageDir $package for the compact 1-arcminute package, or import the full-resolution netCDF with import-gebco.ps1."
}
$projectRoot = $PSScriptRoot
$target = if ($env:CARGO_TARGET_DIR) {
    if ([IO.Path]::IsPathRooted($env:CARGO_TARGET_DIR)) { $env:CARGO_TARGET_DIR }
    else { Join-Path (Get-Location) $env:CARGO_TARGET_DIR }
} else { Join-Path $projectRoot 'target' }
$client = [IO.Path]::GetFullPath((Join-Path $target 'release/minimium-client.exe'))
$activeClient = Get-Process minimium-client -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -eq $client }
if ($activeClient) {
    throw "Close the running Minimium client (PID $($activeClient.Id -join ', ')) before rebuilding $client."
}
$assetPython = if ($env:MINIMIUM_ASSET_PYTHON) { $env:MINIMIUM_ASSET_PYTHON } else { 'python' }
$materialManifest = & rtk proxy $assetPython (Join-Path $projectRoot 'games/sample/materials/cook.py') (Join-Path $projectRoot 'target/materials/sample-v1')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$materialManifest = (Resolve-Path -LiteralPath $materialManifest).Path
& rtk cargo build --release -p minimium-server -p minimium-client --manifest-path (Join-Path $projectRoot 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
if ($RebuildWorld) {
    Backup-EarthWorldForRebuild -WorldDirectory $DataDir -ProfileDirectory $profile
}
Write-Host "Earth world: $DataDir"
Write-Host "Earth player profile: $profile"
$arguments = @('--server-data-dir', $DataDir, '--profile-dir', $profile, '--require-earth', '--dev')
if ($Windowed) { $arguments += '--windowed' }
if ($DevStdin) { $arguments += '--dev-stdin' }
$previousPackage = [Environment]::GetEnvironmentVariable('MINIMIUM_EARTH_PACKAGE', 'Process')
$previousConfig = [Environment]::GetEnvironmentVariable('MINIMIUM_LOCAL_PLANET_CONFIG', 'Process')
$previousSeed = [Environment]::GetEnvironmentVariable('MINIMIUM_LOCAL_SEED', 'Process')
$previousClimateRelief = [Environment]::GetEnvironmentVariable('MINIMIUM_LOCAL_CLIMATE_RELIEF_INFLUENCE', 'Process')
$previousMaterialManifest = [Environment]::GetEnvironmentVariable('MINIMIUM_MATERIAL_MANIFEST', 'Process')
$temporaryPreset = $null
try {
    Set-LauncherEnvironmentVariable 'MINIMIUM_MATERIAL_MANIFEST' $materialManifest
    # Ordinary resumes use the immutable saved definition, including its peak curve.
    $planetPreset = if (Test-Path -LiteralPath (Join-Path $DataDir 'world.planet.ron')) {
        $null
    } else {
        Join-Path $projectRoot 'presets/planet/earth.ron'
    }
    if ($peakRequested) {
        $parentPreset = '"' + (Join-Path $projectRoot 'presets/planet/earth.ron').Replace('\', '\\').Replace('"', '\"') + '"'
        $multiplierPermille = [int][Math]::Round($PeakMultiplier * 1000, [MidpointRounding]::AwayFromZero)
        $temporaryPreset = Join-Path ([IO.Path]::GetTempPath()) ("minimium-earth-peaks-{0}.ron" -f [Guid]::NewGuid().ToString('N'))
        $presetText = @"
(
    extends: Some($parentPreset),
    generator_revision: Some(6),
    peak_relief: Some((
        start_m: $PeakStartMeters,
        full_m: $PeakFullMeters,
        multiplier_permille: $multiplierPermille,
        exponent: $PeakExponent,
    )),
)
"@
        [IO.File]::WriteAllText($temporaryPreset, $presetText, [Text.UTF8Encoding]::new($false))
        $planetPreset = $temporaryPreset
    }
    Set-LauncherEnvironmentVariable 'MINIMIUM_EARTH_PACKAGE' (Resolve-Path -LiteralPath $package).Path
    Set-LauncherEnvironmentVariable 'MINIMIUM_LOCAL_PLANET_CONFIG' $planetPreset
    $climateValue = if ($PSBoundParameters.ContainsKey('ClimateReliefInfluence')) {
        $ClimateReliefInfluence.ToString([Globalization.CultureInfo]::InvariantCulture)
    } else { $null }
    Set-LauncherEnvironmentVariable 'MINIMIUM_LOCAL_CLIMATE_RELIEF_INFLUENCE' $climateValue
    if ($Seed) {
        Set-LauncherEnvironmentVariable 'MINIMIUM_LOCAL_SEED' $Seed
    } else {
        Set-LauncherEnvironmentVariable 'MINIMIUM_LOCAL_SEED' $null
    }
    & $client @arguments
    $exitCode = $LASTEXITCODE
} finally {
    Set-LauncherEnvironmentVariable 'MINIMIUM_MATERIAL_MANIFEST' $previousMaterialManifest
    Set-LauncherEnvironmentVariable 'MINIMIUM_EARTH_PACKAGE' $previousPackage
    Set-LauncherEnvironmentVariable 'MINIMIUM_LOCAL_PLANET_CONFIG' $previousConfig
    Set-LauncherEnvironmentVariable 'MINIMIUM_LOCAL_SEED' $previousSeed
    Set-LauncherEnvironmentVariable 'MINIMIUM_LOCAL_CLIMATE_RELIEF_INFLUENCE' $previousClimateRelief
    if ($temporaryPreset -and (Test-Path -LiteralPath $temporaryPreset)) {
        Remove-Item -LiteralPath $temporaryPreset
    }
}
if ($exitCode -eq -1073741510) { exit 0 } # Ctrl+C on Windows
exit $exitCode
