# Sample game content

## Purpose / Non-goals

Owns shared demo map data and sample-game defaults consumed by client and server. The sample is data/configuration, not a dependency of engine crates.

## Public API

`minimium-game-sample` exports `SAMPLE_MAP`, `DEFAULT_SEED`, the sample `ResolvedPlanetConfig` preset loader, an immutable sample star/planet/moon catalogue, and `DEFAULT_COMBAT_CONFIG` for hitscan damage, range and fire cooldown.

## Dependencies

The `minimium-game-sample` package depends on procgen for reusable generation primitives, world and simulation contracts, environment catalogue contracts, and platform bounded preset reads. Engine packages do not depend on sample content; client and server applications compose it and pass combat values into the shared simulation rule.

## Data flow

Applications parse `SAMPLE_MAP` through `minimium-assets`. This crate composes the stable connected planet/dungeon scenario from reusable procgen primitives and resolves preset files to immutable planet definitions.

## Ownership and execution

Content is static repository data. Runtime applications own loaded instances.

## Invariants and errors

Keep the current demo world's identity and geometry behavior stable during the domain extraction.

## Commands

No commands are defined by this content directory.

## Change guide

Change sample map, planet presets, seed, scenario composition or sample combat balance here. Shared terrain/dungeon algorithms remain in procgen; server runtime policy remains in server runtime.

## Verification

Compile affected application packages. Runtime acceptance is deferred.
