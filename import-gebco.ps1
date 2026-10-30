param(
    [Parameter(Mandatory = $true)][string]$GridPath,
    [string]$DataDir = (Join-Path $PSScriptRoot 'third_party')
)

$ErrorActionPreference = 'Stop'
$grid = (Resolve-Path -LiteralPath $GridPath).Path
if ([IO.Path]::GetFileName($grid) -ne 'GEBCO_2026.nc') {
    throw 'Extract and select the GEBCO_2026.nc ice-surface netCDF grid.'
}
if (-not (Get-Command gdal_translate -ErrorAction SilentlyContinue)) {
    throw 'gdal_translate is required to import GEBCO netCDF data.'
}
$data = [IO.Path]::GetFullPath($DataDir)
$package = Join-Path $data 'gebco-2026'
$staging = Join-Path $data 'gebco-2026.importing'
if ((Test-Path -LiteralPath $package) -or (Test-Path -LiteralPath $staging)) {
    throw "Destination already exists: $package or $staging"
}
New-Item -ItemType Directory -Path $staging -Force | Out-Null
$dataset = 'NETCDF:"' + $grid + '":elevation'
$hashes = [System.Collections.Generic.List[string]]::new()
for ($y = 0; $y -lt 2; $y++) {
    for ($x = 0; $x -lt 4; $x++) {
        $tile = Join-Path $staging "tile-$y-$x.bil"
        $sourceX = $x * 21600
        $sourceY = $y * 21600
        & gdal_translate -q -of EHdr -ot Int16 -srcwin $sourceX $sourceY 21600 21600 $dataset $tile
        if ($LASTEXITCODE -ne 0) { throw "GDAL failed on tile $y,$x; partial data remain in $staging" }
        if ((Get-Item -LiteralPath $tile).Length -ne 933120000) {
            throw "Tile $y,$x has the wrong byte size; partial data remain in $staging"
        }
        $header = [IO.Path]::ChangeExtension($tile, '.hdr')
        $headerText = Get-Content -LiteralPath $header -Raw
        if ($headerText -notmatch '(?im)^\s*BYTEORDER\s+I\s*$') {
            throw "Tile $y,$x is not little-endian Int16; partial data remain in $staging"
        }
        $hashes.Add((Get-FileHash -LiteralPath $tile -Algorithm SHA256).Hash.ToLowerInvariant())
    }
}
$sourceHash = (Get-FileHash -LiteralPath $grid -Algorithm SHA256).Hash.ToLowerInvariant()
$tileList = ($hashes | ForEach-Object { '"' + $_ + '"' }) -join ', '
$index = "(version: 1, edition: `"GEBCO_2026_ice_surface`", source_sha256: `"$sourceHash`", tile_sha256: [$tileList])`n"
[IO.File]::WriteAllText((Join-Path $staging 'index.ron'), $index, [Text.UTF8Encoding]::new($false))
Move-Item -LiteralPath $staging -Destination $package
Write-Output "Imported GEBCO 2026 ice-surface package to $package"
