# minimium-environment

## Purpose / Non-goals

Owns world clock algorithms and values, environment descriptors, renderer-independent sky observer contracts, the bounded solar-system definition catalogue and deterministic weather generation. It contains no FFI, GPU, camera or renderer implementation.

## Public API

`Cloud` publishes physical support dimensions and their generation quantum in
metres, density inputs, position and UTC evolution. The existing inward support
quantization remains part of physical generation: changing it would change cloud
clearance, visibility and density. It is independent of client quality settings.
Clouds carry no debug mode, tessellation levels or grid operation; those belong to
renderer `cloud_presentation`.

`climate` defines validated global and circular regional `ClimateScenario`
offsets with smooth world-UTC transitions, `ClimateSample` and
`SurfaceCoverSample`. `WeatherModel` supports seasonal hemispheres and applies the
same scenario to cloud conditions and surface history. Surface cover replays 168
hourly samples for snow and six for wetness, using one prepared geographic site;
there is no reconnect-dependent accumulator or snapshot field.

`WeatherModel::surface_state` includes liquid dew in the published wetness.
It replays six hours of deposition and drying at the same site for regional
and planet cover. `WeatherModel::ground_fog_endpoints` samples two UTC times
at one site and returns surface extinction (m⁻¹), scale height, displayed
ground height (both optical metres) and regional haze extinction. Liquid
condensation uses a calibrated moisture-to-relative-humidity estimate and
the dew-point spread. A seasonal local-solar cooling curve has a two-hour
thermal lag and a weather-dependent cloudiness approximation. Four opposing
terrain samples estimate valley pooling across a two-kilometre physical
radius. Ocean columns start at the sea surface above any bathymetry.
The atmosphere receives instantaneous air condensation independently of rain
and retained surface dew. The model adds no wind field or persisted water mass.

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

`WeatherModel` keeps its manifest, radius, seasonal profile, derived scales and
cloud cache private. Construction fixes these source values together; external
consumers read only `manifest()` and `radius_m()`. Changing worlds constructs a
new model. The used `set_scenario` operation validates first and clears cached
clouds only when the accepted scenario changes; invalid or identical scenarios
leave the current cache intact. There are no independent source-field setters.

## Invariants and errors

Preserve coordinate conventions, catalogue bounds, model revisions and explicit invalid-observer errors.

The catalogue owns `SKY_SUN_ID` (1) and `SKY_MOON_ID` (2), reserved for the
astronomy adapter's built-in observations. `UniverseCatalog::register` rejects
either with `CatalogError::ReservedBodyId` before changing the catalogue. Other
body IDs, including the sample's 1001/1002/1003, retain their current meaning.

`TerrainSampler` has an associated error type. Terrain, source-height and bound
failures propagate through climate, cover and cloud queries; environment does not
depend on procgen to name that error. `Ok(None)` means an optional source/bound
port is unsupported and permits the existing analytical fallback, whereas `Err`
aborts preparation. A failed cloud query does not return a partial snapshot.

## Commands

Owns time commands, pure UTC/time-scale argument parsing and shared clock operation descriptors in `console::clock_entry`. Clients bind the session sample and administrative hosts bind the owner snapshot. Server runtime performs command dispatch and authorization checks.

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

`SimulationBoundary` binds a validated tick to a calendar epoch. Multiplying its tick by 50 yields elapsed game milliseconds. Calendar changes preserve that elapsed duration. `CloudSnapshot` carries a half-open population interval. The roster includes clouds whose lifetimes overlap the interval. Age controls their visibility at the frame UTC. The desktop host intersects that interval with staggered morph windows and retains surviving regional clouds during local fallback.

The console status projection reads one ClockSample. MeasuredRatePpm represents observed progress independently of the validated configuration range. ClockWrite encodes field writes and UTC or solar HHMM methods. The CPU solar validator uses finite Earth-fixed coordinates in metres and rejects observers within one micrometre of the polar axis.
