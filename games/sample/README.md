# Sample game content

## Purpose / Non-goals

Owns shared demo map data and sample-game defaults consumed by client and server. The sample is data/configuration, not a dependency of engine crates.

`generate_worlds` derives a complete procedural `PlanetDefinition` from its seed before generating the map. Its manifest carries that definition so the client and server can materialize the same planet without inferring settings from the world instance ID.

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

`minimium-game-sample` exports `DEFAULT_SEED`, the sample `ResolvedPlanetConfig` preset loader, an immutable sample star/planet/moon catalogue, `DEFAULT_COMBAT_CONFIG` for hitscan damage, range and fire cooldown, and `SampleGameplayModule` for registering game-owned component schemas and systems with the host.
`PortalComponent` is the sample portal value and has a bounded, finite-value checked 40-byte encoding. `PORTAL_COMPONENT` and `PORTAL_SCHEMA` identify its registered replicated schema. The sample Startup system emits prepared portal entities only for `WorldLifecycle::New`; restored worlds retain their saved entity set. The PostSimulation interaction system decodes the replicated portal component, checks the player's distance, and emits `RequestTransfer`. The host owns context, sector readiness and placement validation. The client chooses a portal entity from its replica and sends `InteractEntity`; portal state is not carried through a separate runtime portal field.

The interaction system looks up that entity's portal component directly through
the read-only gameplay context; it does not copy or scan all world components
for each event.

When adding a sample gameplay feature, define its schema/system in this package
and register it through `SampleGameplayModule`. `apps/server/src/composition.rs`
installs that module and supplies the portal's initial entity template;
`apps/client/src/renderer/init.rs` and `apps/client/src/terminal_client.rs`
register the same component ID/schema with the replica session. The module uses
only `minimium-gameplay` contexts/effects; it cannot reach server runtime,
storage or transport APIs.

SURFACE_RECIPE supplies landform sampling radius and retention/exposure strengths
to the presentation sampler. Its recipe revision is separate from planet generator
revisions; these appearance changes do not require a world reset. The engine
depends on the public procgen value type, never on this sample package.

## Dependencies

The `minimium-game-sample` package depends on procgen for reusable generation primitives, world and simulation contracts, environment catalogue contracts, platform bounded preset reads and neutral gameplay registration contracts. Engine packages do not depend on sample content; client and server applications compose it and pass combat values into the shared simulation rule.

## Data flow

This crate composes the connected planet/dungeon scenario from reusable procgen primitives and resolves preset files to immutable planet definitions. Authored maps are selected explicitly by applications and are not injected into the sample world catalog.
`materialize_manifest_map` preserves typed `GenerationError` categories from
manifest validation and materialization; the application resolver retains that
cause in its session failure.
The paired-world producers also return `GenerationError`; server composition,
desktop and terminal preserve the code and cause until logging or UI display.

## Ownership and execution

Content is static repository data. Runtime applications own loaded instances.

## Invariants and errors

All generated sample worlds use generator revision 5. Planet mode selects the
procedural or Earth generation path explicitly. Other generator revisions are
rejected; they are not reinterpreted as revision 5.

Keep the current demo world's identity and geometry behavior stable during the domain extraction.

## Commands

No commands are defined by this content directory.

## Change guide

Change sample map, planet presets, seed, scenario composition or sample combat balance here. Shared terrain/dungeon algorithms remain in procgen; server runtime policy remains in server runtime.

## Verification

Source ownership is reviewed in E046-08; compilation belongs to E046-17.
Runtime acceptance requires a separate testing phase.

PeakReliefConfig is the RON adapter for the world-owned PeakRelief value.
An optional peak_relief block contains start_m, full_m, multiplier_permille and
exponent; it requires Earth mode at generator revision 5. Old resolved world
configurations without the current revision are rejected. The launcher writes a
temporary extending preset only when a Peak option is used.
The resolved curve is persisted as part of the immutable planet configuration.
The client regenerates an individual map from its manifest topology and recorded
generation seed. It does not infer world kind from instance IDs or reverse a
seed transform; the server's existing paired-world producer keeps its current
seed derivation and geometry.
