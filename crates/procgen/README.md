# minimium-procgen

## Purpose / Non-goals

Owns deterministic, addressable world generation, planet and dungeon geometry, and world materialization. It validates generator compatibility for manifests before materialization. Earth sampling uses the `PreparedGeography` port; this crate does not read files or own a global package. Worker scheduling, transport and rendering are outside this crate.

## Presentation surface

SurfaceRecipe supplies validated, presentation-only landform parameters.
TerrainSurfaceSample includes physical source elevation, retention, exposure and
coverage provenance. Landforms use the same fixed height neighborhood across
sectors/LODs/faces. Sampling does not change generated mesh, collision or generator
identity. Hosts select the recipe through PlanetTerrain::with_surface_recipe;
sample defaults are owned by games/sample.

## Public API

`terrain_surface::TerrainSurfaceSample` contains elevation, slope, water,
world-space normal and six ordered material fractions. Sampling uses a fixed
physical derivative footprint across sector/face/LOD boundaries. The optional
`PreparedGeography::land_cover` port supplies interpolated real-world fractions;
otherwise procedural fractions apply. This presentation path leaves `MapTriangle`,
generation digests, saved geometry and collision sampling unchanged.

Without WorldCover, presentation fractions distinguish arid latitude belts,
temperate ground and alpine rock using physical source elevation. They no longer
assign the same grass-dominated mixture to all low-elevation land.

See the re-exports in `src/lib.rs`, including sector materialization, planet sampling, dungeon generation, generator contract validation and addressable randomness. `PreparedGeography` is the host port for immutable geographic samples; `PlanetTerrain::with_geography` carries it into terrain and weather sampling. Sample world composition belongs to `minimium-game-sample`.

## Dependencies

Depends on `minimium-core` and `minimium-world` plus serialization, hashing and memory mapping libraries; world supplies manifests and neutral dungeon layout types. It has no dependency on sample-game content.

## Data flow

World and sector inputs produce generated maps, sector geometry and sampled terrain. Hosts open geography through `minimium-asset-io` and pass the resulting immutable object through generation and sampling calls. Missing Earth geography is reported at observer boundaries and rejected before Earth generation.

## Ownership and execution

Generation functions are called by client/server owners or their scheduled work. Hosts own prepared geography and attach it to terrain before Earth sampling. Results are installed by the caller after validating their ticket/context.

## Invariants and errors

Preserve seeds, address calculations, rounding, scales and algorithm order. Generation must remain reproducible for the same inputs.

Render patches and coarse blocks sample signed ground heights through
PlanetTerrain::ground_sample; Earth bathymetry remains below sea level.
PlanetTerrain::sea_position supplies canonical sea-level vertices for presentation.
The renderer owns the separate ocean mesh and its intersection with ground.
The earlier binary shoreline clipping is removed. PlanetTerrain::sample and
collision sector generation retain their original sea-clamped results and rounding;
no generator revision, save migration or world reset is required.

## Commands

None.

## Change guide

Change terrain or dungeon algorithms here. Keep file discovery and persistent data ownership outside this crate.

## Verification

Run `rtk cargo check -p minimium-procgen` and `rtk python scripts/check_architecture.py`. Runtime acceptance is deferred.

Opt-in Earth generator v5 applies PlanetDefinition::earth_elevation_mm to the
shared collision, presentation and traversal height. The v5 generator digest is
distinct; v3/v4 retain neutral relief and their original digest/sampling.
Source geography remains available for climate independently of shaped terrain.
Height derivatives and both planet views follow the same mapped field.

PLANET_RENDER_BLOCK_SUBDIVISIONS publishes the unchanged 16-cell presentation
lattice to renderer coast probes, projected-cell scoring and edge conformance.
Collision subdivisions remain independent; generation output is unchanged.
