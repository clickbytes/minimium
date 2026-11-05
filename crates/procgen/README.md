# minimium-procgen

## Purpose / Non-goals

Owns deterministic, addressable world generation, planet and dungeon geometry, and world materialization. It validates generator compatibility for manifests before materialization. Earth sampling uses the `PreparedGeography` port; this crate does not read files or own a global package. Worker scheduling, transport and rendering are outside this crate.

## Public API

See the re-exports in `src/lib.rs`, including sector materialization, planet sampling, dungeon generation, generator contract validation and addressable randomness. `PreparedGeography` is the host port for immutable geographic samples; `PlanetTerrain::with_geography` carries it into terrain and weather sampling. Sample world composition belongs to `minimium-game-sample`.

## Dependencies

Depends on `minimium-core` and `minimium-world` plus serialization, hashing and memory mapping libraries; world supplies manifests and neutral dungeon layout types. It has no dependency on sample-game content.

## Data flow

World and sector inputs produce generated maps, sector geometry and sampled terrain. Hosts open geography through `minimium-asset-io` and pass the resulting immutable object through generation and sampling calls. Missing Earth geography is reported at observer boundaries and rejected before Earth generation.

## Ownership and execution

Generation functions are called by client/server owners or their scheduled work. Hosts own prepared geography and attach it to terrain before Earth sampling. Results are installed by the caller after validating their ticket/context.

## Invariants and errors

Preserve seeds, address calculations, rounding, scales and algorithm order. Generation must remain reproducible for the same inputs.

## Commands

None.

## Change guide

Change terrain or dungeon algorithms here. Keep file discovery and persistent data ownership outside this crate.

## Verification

Run `rtk cargo check -p minimium-procgen` and `rtk python scripts/check_architecture.py`. Runtime acceptance is deferred.
