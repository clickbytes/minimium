# minimium-environment

## Purpose / Non-goals

Owns world clock algorithms and values, environment descriptors, renderer-independent sky observer contracts, the bounded solar-system definition catalogue and deterministic weather generation. It contains no FFI, GPU, camera or renderer implementation.

## Public API

`climate` defines validated global and circular regional `ClimateScenario`
offsets with smooth world-UTC transitions, `ClimateSample` and
`SurfaceCoverSample`. `WeatherModel` supports seasonal hemispheres and applies the
same scenario to cloud conditions and surface history. Surface cover replays 168
hourly samples for snow and six for wetness, using one prepared geographic site;
there is no reconnect-dependent accumulator or snapshot field.

Annual means use angular latitude to avoid overcooling temperate zones. Terrain
lapse rate/uplift default to the source elevation scale (`height_scale_permille` on
Earth), independently of planet radius and cloud render scale. Surface history
evaluates temperature/moisture without constructing cloud-layer conditions.
`WeatherModel::ground_elevation_m` exposes the physical source elevation through
the existing terrain port for presentation weather grids. The host supplies the
sea-level datum; this query has no camera, GPU, I/O or incremental state.

WeatherModel::surface_state returns replayed cover, current climate, physical
source elevation and effective climate elevation using one prepared climate site. Hosts cache spatial samples
and UTC endpoints; camera/texture/cache behavior stays outside this domain.

`time` exports clock values and deterministic clock operations; the crate root exports environment descriptors; `sky` exports observer and sky-model contracts; `universe` exports catalog identifiers, definitions and validation; `weather` exports climate/cloud models driven by a host-provided `TerrainSampler`.

## Dependencies

Depends on core values and public world manifest/definition types. It is independent of astronomy FFI and presentation adapters.

## Data flow

World manifest and observer inputs produce validated observations, catalog lookups and weather snapshots. Hosts supply monotonic/system samples and terrain queries.

## Ownership and execution

Clock values and weather models are deterministic; callers provide monotonic/system samples and a terrain adapter explicitly. The client owns astronomy calculators, GPU resources and worker lifetimes.

## Invariants and errors

Preserve coordinate conventions, catalogue bounds, model revisions and explicit invalid-observer errors.

## Commands

Owns time commands and pure UTC/time-scale argument parsing. Server runtime performs command dispatch and authorization checks.

## Change guide

Change clock, observer, catalogue and weather models here. Implement `TerrainSampler` at the host boundary; keep GPU and astronomy FFI in their adapters.

## Verification

Run `rtk cargo check -p minimium-environment` and `rtk python scripts/check_architecture.py`. Runtime acceptance is deferred.

ClimateScenario.relief_influence defaults to 0 (source Earth elevation); 1 uses
the additional peak exaggeration fully, and intermediate values blend the two.
It affects lapse rate, uplift and replayed surface cover together. TerrainSampler
may expose unexaggerated source height; the host does so for Earth. A monotone
inverse is the fallback for adapters without that port. SurfaceState carries
both source and effective climate elevations for renderer interpolation.
Cloud clearance and fog placement follow the actual displayed ground; astronomical
height anchors use the same relief mapping while retaining source Earth altitude.
This remains a heuristic climate model, not observed regional Earth weather.

Cosmetic ambient meteors (`docs/adr/0013-ambient-meteors.md`) are outside this
crate's responsibility. They add no weather, clock, observer or universe
vocabulary here; hosts gate meteor exposure with the existing observer/UTC/terrain
outputs, and the future tracked-object rendering adapter adds no domain state.
