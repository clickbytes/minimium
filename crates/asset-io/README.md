# minimium-asset-io

## Purpose / Non-goals

Owns bounded file access for authored maps, referenced OBJ meshes and world definitions, plus opened prepared GEBCO geography. It does not parse authored formats or generate worlds.

## Public API

`load_map` opens a map and supplies bounded mesh text to assets. `load_world_definition` reads a bounded source file and delegates validation to assets. `open_earth_package` validates and returns an immutable, shareable package that implements procgen's `PreparedGeography` port.

## Dependencies

Depends on `minimium-assets` parsers, `minimium-world` map types and the narrow `minimium-procgen::PreparedGeography` interface. It does not depend on application runtimes.

## Data flow

Paths and file bytes enter the adapter; validated `WorldMap` or `WorldDefinition` values leave it. Mesh paths are relative to the map directory and canonicalized before opening.

## Ownership and execution

The calling application selects paths and owns loaded definitions. Reads are synchronous and bounded by parser limits.

## Invariants and errors

The adapter enforces map-directory containment for mesh files and caps reads at format limits. File errors retain `io::Error`; parse errors become `InvalidData`.

## Commands

None.

## Change guide

Add file or package access here. Keep format validation in assets and generation in procgen.

## Verification

Run `rtk cargo check -p minimium-asset-io -p minimium-client -p minimium-server` and architecture/file-length checks. Runtime acceptance is deferred.
