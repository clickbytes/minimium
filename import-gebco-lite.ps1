param(
    [string]$PackageDir = (Join-Path $PSScriptRoot 'third_party/gebco-2026')
)

$ErrorActionPreference = 'Stop'
if (-not (Get-Command uv -ErrorAction SilentlyContinue)) {
    throw 'uv is required to fetch the compact GEBCO package (https://docs.astral.sh/uv/getting-started/installation/).'
}
& uv run --python 3.12 --with rasterio python (Join-Path $PSScriptRoot 'scripts/import_gebco_lite.py') --package-dir $PackageDir
exit $LASTEXITCODE
