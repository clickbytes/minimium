[CmdletBinding()]
param(
    [string]$DataDir = ([IO.Path]::Combine($env:LOCALAPPDATA, 'Minimium', 'earth')),
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
    [switch]$ResetWorld,
    [switch]$Windowed
)

$ErrorActionPreference = 'Stop'
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
if ($peakRequested -and $existingWorld -and -not $ResetWorld) {
    throw 'Peak settings define a new world. Use a new -DataDir or -ResetWorld (preserves a backup). To resume the saved world, omit the Peak parameters.'
}
$package = [IO.Path]::GetFullPath($PackageDir)
if (-not (Test-Path (Join-Path $package 'index.ron'))) {
    throw "GEBCO 2026 package is missing at $package. Run $PSScriptRoot/import-gebco-lite.ps1 -PackageDir $package for the compact 1-arcminute package, or import the full-resolution netCDF with import-gebco.ps1."
}
$projectRoot = $PSScriptRoot
& rtk cargo build --release -p minimium-server -p minimium-client --manifest-path (Join-Path $projectRoot 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
if ($ResetWorld -and (Test-Path -LiteralPath $DataDir)) {
    $worldDirectory = [IO.Path]::GetFullPath($DataDir).TrimEnd([IO.Path]::DirectorySeparatorChar)
    $worldParent = [IO.Path]::GetDirectoryName($worldDirectory)
    if (-not $worldParent -or $worldDirectory -eq [IO.Path]::GetPathRoot($worldDirectory).TrimEnd([IO.Path]::DirectorySeparatorChar)) {
        throw 'ResetWorld requires a specific world directory, not a filesystem root.'
    }
    # Match the server's OS file lock; keep it held while preserving the old world.
    $backupDirectory = "${worldDirectory}.backup-$([DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss'))-$([Guid]::NewGuid().ToString('N').Substring(0,8))"
    if ([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($backupDirectory)) -ne $worldParent) {
        throw 'World backup escaped its intended parent directory.'
    }
    $worldLock = [IO.FileStream]::new((Join-Path $worldDirectory 'server.lock'), [IO.FileMode]::OpenOrCreate,
        [IO.FileAccess]::ReadWrite, ([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
    $worldLockHeld = $false
    try {
        $worldLock.Lock(0, 1)
        $worldLockHeld = $true
        Move-Item -LiteralPath $worldDirectory -Destination $backupDirectory -ErrorAction Stop
    } finally {
        if ($worldLockHeld) { $worldLock.Unlock(0, 1) }
        $worldLock.Dispose()
    }
    Write-Host "Previous world preserved at $backupDirectory"
}
$target = if ($env:CARGO_TARGET_DIR) {
    if ([IO.Path]::IsPathRooted($env:CARGO_TARGET_DIR)) { $env:CARGO_TARGET_DIR }
    else { Join-Path (Get-Location) $env:CARGO_TARGET_DIR }
} else { Join-Path $projectRoot 'target' }
$client = Join-Path $target 'release/minimium-client.exe'
$profile = if ($ProfileDir) { $ProfileDir } else { "${DataDir}-profile" }
$arguments = @('--server-data-dir', $DataDir, '--profile-dir', $profile, '--require-earth')
if ($Windowed) { $arguments += '--windowed' }
$previousPackage = [Environment]::GetEnvironmentVariable('MINIMIUM_EARTH_PACKAGE', 'Process')
$previousConfig = [Environment]::GetEnvironmentVariable('MINIMIUM_LOCAL_PLANET_CONFIG', 'Process')
$previousSeed = [Environment]::GetEnvironmentVariable('MINIMIUM_LOCAL_SEED', 'Process')
$previousClimateRelief = [Environment]::GetEnvironmentVariable('MINIMIUM_LOCAL_CLIMATE_RELIEF_INFLUENCE', 'Process')
$temporaryPreset = $null
try {
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
    generator_revision: Some(5),
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
