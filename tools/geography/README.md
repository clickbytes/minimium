# Geography preparation tools

## Purpose / Non-goals

Contains the existing offline GEBCO package preparation tools. The scripts create input data packages; they are not runtime library code and were not executed during this architecture work.

## Public API

Keep the repository-root `import-gebco-lite.ps1` and `import-gebco.ps1` launchers as stable entry points. The implementations live beside this README. The lite importer calls `scripts/import_gebco_lite.py` through `uv`; the full importer requires GDAL's `gdal_translate`.

## Dependencies

The compact importer uses Python 3.12, `uv`, and rasterio. The full-resolution importer uses PowerShell and GDAL. These tools are independent of runtime Cargo crates.

## Data flow

The compact path reads the GEBCO 2026 overview as bounded row chunks and writes the indexed 1-arcminute package. The full path tiles the supplied netCDF into checked Int16 BIL files and writes the package index. Both keep their previous default output under repository `third_party/gebco-2026`.

## Ownership and execution

These are explicit offline user-invoked tools. They can take a long time and write large data packages; runtime applications only open already prepared packages through `minimium-asset-io`.

## Invariants and errors

The importers validate source identity, raster dimensions/type, tile sizes and checksums before publishing the completed package. Interrupted or invalid imports report errors and leave their staging data for inspection.

## Commands

Run `./import-gebco-lite.ps1` or `./import-gebco.ps1 -GridPath GEBCO_2026.nc` from the repository root when preparing the corresponding package.

## Change guide

Change import format and source handling here. Keep the root launchers and their default output paths stable. Runtime parsing and sampling belong to `minimium-assets`, `minimium-asset-io` and `minimium-procgen`.

## Verification

No importer was run during the architecture refactor. Use the commands above only when intentionally preparing geography data; static source checks do not verify remote data or installed GDAL/rasterio behavior.
