param(
    [Parameter(Mandatory = $true)][string]$GridPath,
    [string]$DataDir = (Join-Path $PSScriptRoot 'third_party')
)

$ErrorActionPreference = 'Stop'
& (Join-Path $PSScriptRoot 'tools/geography/import-gebco.ps1') -GridPath $GridPath -DataDir $DataDir
exit $LASTEXITCODE
