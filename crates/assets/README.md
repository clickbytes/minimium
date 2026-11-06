# minimium-assets

## Purpose / Non-goals

Parses and validates map, OBJ, sky catalog and authored world-definition data. Parsers consume supplied text and mesh-reader callbacks; this crate performs no filesystem access and does not generate worlds.

## Public API

`terrain_material` validates version 1 manifests, the six ordered roles and bounded
4096 BC / 2048 RGBA variants with SHA-256 file identities. `ktx2` validates the
supported six-layer 2D, full-mipmap subset before allocation. `land_cover` reads
two-minute fractional WorldCover fields with bilinear interpolation, longitude
wrapping and an explicit validity mask. These APIs consume bytes without I/O.

`parse_map`, `parse_map_with_mesh_loader`, `parse_world_definition`, and embedded sky-catalog accessors are exported from `src/lib.rs`.

## Dependencies

Uses world manifests and the neutral dungeon layout type from `minimium-world`. Generator compatibility is checked by the procedural host before materialization. Astronomy, renderer and procgen dependencies are forbidden.

## Data flow

Input text and caller-supplied mesh text are parsed into validated typed values; `minimium-asset-io` owns path resolution and bounded reads.

## Ownership and execution

Returned data is owned by the caller. Parsing is synchronous and does not start workers.

## Invariants and errors

Keep canonical map fingerprints and parser validation stable. Malformed input returns a typed or descriptive error; hosts validate generator compatibility before materialization.

## Commands

None.

## Change guide

Change data syntax and validation here. Keep file opening and persistent package access in the connected `asset-io` adapter.

## Verification

Run `rtk cargo check -p minimium-assets` and `rtk python scripts/check_architecture.py`. Runtime acceptance is deferred.
