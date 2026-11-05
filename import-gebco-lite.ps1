param(
    [string]$PackageDir = (Join-Path $PSScriptRoot 'third_party/gebco-2026')
)

$ErrorActionPreference = 'Stop'
& (Join-Path $PSScriptRoot 'tools/geography/import-gebco-lite.ps1') -PackageDir $PackageDir
exit $LASTEXITCODE
