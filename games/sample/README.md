# Sample game content

## Purpose / Non-goals

Owns shared demo map data and sample-game defaults consumed by client and server. The sample is data/configuration, not a dependency of engine crates.

## Public API

`materials/recipe.py` owns deterministic sand, soil, grass, litter, rock and snow
art recipes. The client build invokes `tools/bake_materials.py` (Python + numpy)
and caches complete KTX2 arrays in `target/materials/sample-v1`. Recipes do not
run at client startup. `climate/` contains neutral, warming/drought and cooling
scenarios with absolute UTC transition windows. See `docs/terrain-materials.md`
for preparation, packaging, overrides and outstanding visual acceptance.

`materials/fields.py` provides periodic quintic value noise, cellular distances
and wrapped stochastic elements. Roles use distinct grain/clod/fiber/leaf/crack
structures; pigment and roughness have independent variation. Normal strength
comes from relief in metres and tile size. The cooker evaluates each role once,
stages byte maps, derives the 2048 variant from the 4096 source, and emits static
albedo/detail PNG previews. BC4 index arithmetic uses float intermediates to avoid
8-bit overflow; BC1 fits endpoints along the largest component range. Previews
are asset review aids, not evidence of game rendering or measured performance.

`minimium-game-sample` exports `SAMPLE_MAP`, `DEFAULT_SEED`, the sample `ResolvedPlanetConfig` preset loader, an immutable sample star/planet/moon catalogue, and `DEFAULT_COMBAT_CONFIG` for hitscan damage, range and fire cooldown.

SURFACE_RECIPE supplies landform sampling radius and retention/exposure strengths
to the presentation sampler. Its recipe revision is separate from planet generator
revisions; these appearance changes do not require a world reset. The engine
depends on the public procgen value type, never on this sample package.

## Dependencies

The `minimium-game-sample` package depends on procgen for reusable generation primitives, world and simulation contracts, environment catalogue contracts, and platform bounded preset reads. Engine packages do not depend on sample content; client and server applications compose it and pass combat values into the shared simulation rule.

## Data flow

Applications parse `SAMPLE_MAP` through `minimium-assets`. This crate composes the stable connected planet/dungeon scenario from reusable procgen primitives and resolves preset files to immutable planet definitions.

## Ownership and execution

Content is static repository data. Runtime applications own loaded instances.

## Invariants and errors

Planet revisions are 3, and 4/5 for Earth (v5 permits peak-relief curves). Unsupported saved revisions report
their exact value and preserve the world; `start-earth.ps1 -ResetWorld` creates a
new world after moving the previous directory to a timestamped sibling backup.

Keep the current demo world's identity and geometry behavior stable during the domain extraction.

## Commands

No commands are defined by this content directory.

## Change guide

Change sample map, planet presets, seed, scenario composition or sample combat balance here. Shared terrain/dungeon algorithms remain in procgen; server runtime policy remains in server runtime.

## Verification

Compile affected application packages. Runtime acceptance is deferred.

PeakReliefConfig is the RON adapter for the world-owned PeakRelief value.
An optional peak_relief block contains start_m, full_m, multiplier_permille and
exponent; it requires Earth generator_revision 5. Missing fields in old resolved
world configurations retain the neutral mapping. Ordinary earth.ron remains v4;
the launcher writes a temporary extending preset only when a Peak option is used.
The resolved curve is persisted as part of the immutable planet configuration.
