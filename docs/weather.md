# Earth weather

`start-earth.ps1` defaults to `weather simulated`. Weather makes no network requests unless live mode is enabled. The client reconstructs visual weather from the manifest seed, Earth position and synchronized UTC. Protocol and world saves are unchanged. Local cloud model revision **3** replaces the former top/bottom envelope renderer. This is a deterministic visual parameterization, not a numerical forecast or a water/heat-conserving simulation.

## Atmospheric input and physical field

`grid.rs`, `maps.rs`, `region.rs` and `simulated.rs` retain the regional atmospheric simulation and asynchronous global/local atlas publication. `weather resolution 64|128|256|512` chooses longitude divisions; latitude uses half as many. Default 256 means 32,768 atmospheric regions. It is independent of cloud instance budgets. The GPU atlas has 32 RGBA8 layers at 512 × 256 (16 MiB); its regional patch spans approximately 446 physical km. Regional interpolation and resolution are approximations of atmospheric conditions, so different regional resolutions can produce different weather parameters.

`systems.rs` organizes elongated frontal bands, wet areas, drying areas and convection. Their seeded locations and lifecycles follow the existing transport maps; wind does not recursively evaluate weather systems. Noise varies edges and local conditions. `climate.rs` includes directional water fetch, water/air temperature contrast, a downwind residence-time threshold, slope uplift and stable-air suppression. `column.rs` supplies layer coverage, base/top heights, condensate and wind. Coverage and optical condensate remain separate quantities.

`environment/weather/clouds` owns the physical field and has no renderer, camera or wgpu dependencies:

- `CloudId` includes seed, cube face, cell coordinates/level, formation population, candidate, altitude population and hierarchy path. Graphics settings and camera position are absent from identity.
- `coordinates` starts at cube-sphere level 5; cluster candidates at level 8 have deterministic subcell jitter. Binary lobe paths refine to level 12. No longitude seam or special polar population is needed.
- `CloudFieldSnapshot` is immutable for a seed, atmospheric revision, clock revision and UTC endpoint. It owns the atmospheric input and an index of transported root bounds. Descendant descriptors are generated only for intersecting physical queries and retained in a FIFO cache of at most 4096 cells. Queries deduplicate stable IDs.
- `CloudCluster` separates physical base/depth/width, convection, condensate mass, wind and shear. Cumulus, stratocumulus, stratus, nimbostratus, cirrus and storm labels are diagnostic classifications; shape parameters blend continuously.
- `CloudLobe` is an oriented ellipsoid clipped by the common cluster base plane. Horizontal semiaxes never exceed 25 physical km. Larger covers consist of many curved-world placements, not a continent-sized ellipsoid.
- `sampling` supports point density, ray optical depth, physical-region intersection and vertical columns. `moments` computes occupied area, occupied optical depth, heights, rain and storm potential from those columns.

All positions, transport reconstruction and dimensions use f64 physical Earth metres/UTC seconds on the CPU. Only the renderer converts with `planet_radius / 6371000`. Instance positions are uploaded relative to their publication origin; camera displacement from that origin is calculated in f64. Positive semiaxes interpolate logarithmically and orientations take the shortest quaternion path.

## Shape, mass and evolution

The canonical kernel uses two deterministic lobe-local 3D noise bands to erode the ellipsoid edges and modulate interior density. Rust and WGSL share the hash and constants; both CPU columns and GPU view extinction use six-point Gauss quadrature over the same base-clipped interval. The legacy polynomial volume still normalizes coarse mass, so erosion is an approximation rather than exactly mass-conserving microphysics. `method analytic` retains smooth view extinction for visual A/B, without altering the canonical physical field. Splitting a node distributes its extinction mass among descendants; changing size adjusts density instead of creating condensate. Occupied area is not inferred from alpha alone.

Cumulus parts share a flatter base and irregular upper lobes. Stable decks flatten and broaden; high cloud stretches along upper wind with a thinner vertical extent. Convection shifts mass upward into towers and downwind anvils, bounded by the atmospheric top. The large formation populations start every three hours, overlap three at a time and reconstruct no more than nine hours of history. Smaller parts use three overlapping populations at 20 minutes for convection, 90 minutes for stratiform development and 120 minutes for high cloud. Convective and stratiform populations mix by mass, so a changing convection parameter never changes an epoch divisor. Seeded phase offsets and smooth endpoint derivatives avoid a synchronized reset.

Each level-5 root follows a rigid spherical rotation reconstructed from the bounded RK2 transport maps. Descendant placement, short coastal displacement and shear deform its parts. This is a coherent patch approximation to differential advection, not exact fluid transport across every root boundary. Orographic roots stay anchored while their parts move with the flow; their index uses actual stationary bounds. Moving roots are indexed at their transported positions. The search bounds include part offsets and coastal displacement, rather than assuming birth-cell neighbors.

## Rendering and publication

`renderer/cloud_instances` owns representation selection, a bounded worker and GPU instances. A simple six-vertex screen enclosure is instanced for each ellipsoid. Each hit pixel analytically clips its chord against the cluster base, opaque depth, planet and camera position. Camera-inside-cloud visibility uses this same integral; there is no second cloud-interior fog pass and no multi-step 3D-noise march.

Four additive RGBA optical bins hold optical depth and optical-depth-weighted lit RGB. Low lighting uses two of them. An ellipsoid contributes the exact polynomial integral of each intersected distance bin. The composite processes bins front to back and attenuates the background with exponential transmittance. Draw order therefore does not control overlap opacity; illumination within a bin remains approximate. Depth-aware four-tap upsampling supports all four requested buffer scales and odd viewport dimensions.

LOD uses projected size in full-image pixels with 20% hysteresis. A 0.3 real-second transition divides optical contribution between the previous and new cuts, both reconstructed from the newly published physical endpoints. The previous cut comes from the published frame, not an unaccepted worker result. Half of the instance limit is reserved for the other transition cut; active and transitioning instances together obey the limit. Least-important refinements are collapsed before silhouettes are removed. If the root budget cannot cover a near region, that region contracts and the curved aggregate representation fills the remainder. Lowering the limit below the retained representation temporarily uses complete curved coverage until replacement instances arrive.

`cloud_mesh` now serves only distant curved aggregates. A complete coarse globe is retained while visible patches receive more subdivisions, so switching FPP/globe or looking elsewhere does not invalidate the physical field. The near/far representations share an optical-weight transition region. Aggregates come from eight deterministic area samples of canonical columns; coverage and occupied transmission are averaged separately. Their fixed 64 × 64 global and regional moment grids are intentionally coarse. Thin silhouettes, gaps, aggregate joins and transitions need visual tuning; aggregate sampling does not exactly preserve every fine projected feature.

The worker builds two physical endpoints on a 5/10/30/60/120/300-world-second cadence. Cadence/prefetch account for measured physical-field preparation and clock rate, not target FPS or graphics quality. At most one job/result is queued and one completed future interval waits for its UTC start. Four endpoint fields and two moment entries are retained for camera/quality rebuilds. Outdated seed, clock, serial, atmospheric revision and representation results cannot replace newer publication. Seed/date/reverse-clock invalidation clears incompatible data; ordinary view/quality changes retain the previous frame. At startup there is no completed frame yet. A slow worker freezes all effects at the last valid endpoint; very fast time skips states and can show lag.

`PresentedCloudFrame` publishes endpoints, GPU representation, atmospheric atlas and shadow/precipitation cache together. Local effects interpolate canonical lobes from these endpoints, independently of camera culling and graphics budgets. Rain requires low/middle optical mass and regional precipitation potential. Cirrus alone cannot rain. Lightning candidates must intersect developed convective cloud and include the host cluster identity in their event seed. Roof masks, snow, thunder delays and the existing local event-selection radius are retained.

Lighting includes analytical self extinction and up to 0/4/8 preselected neighboring lobes, plus a spatial cache of other cloud layers, diffuse sky, Sun, Moon and local flashes. Dense bases darken through transmission. The eight-layer 64 × 64 RGBA8 cache contains opacity/rain and physical top heights for global/local areas at both endpoints. It is filtered and quantized, so distant shadows and rain curtains remain approximations. Ground fog remains a separate height-fog phenomenon.

## Cloud settings and costs

Settings last for the client session. Choosing a quality profile clears manual overrides. A later individual command changes only that parameter; `reset` restores sky-quality inheritance. `sky quality` still controls atmosphere/object-shadow quality; `weather clouds quality sky` inherits its low/standard selection.

| Parameter | Low | Standard | High |
| --- | ---: | ---: | ---: |
| Render width/height scale | 0.5 | 0.5 | 0.75 |
| Ellipsoid limit, including transitions | 2048 | 8192 | 16384 |
| Refinement threshold, full-image pixels | 48 | 24 | 12 |
| Distance bins | 2 | 4 | 4 |
| Local shadow neighbors | 0 | 4 | 8 |
| Curved aggregate patches | 128 | 256 | 512 |

Commands: `weather clouds status`, `quality sky|low|standard|high`, `scale 0.25|0.5|0.75|1`, `instances 512..32768`, `lod 8..128`, `lighting low|standard|high`, `method analytic|volumetric`, `reset`, and `debug off|bounds|ids|density|lod`. Every subcommand uses the `weather clouds` prefix. Debug views aid the deferred functional verification; they are not separate weather models.

Scale changes only dependent targets. LOD/limit changes rebuild representation while physical endpoint/moment caches are reused. Lighting/debug changes select calculations through uniforms. The adapter's buffer, texture-format and color-attachment limits constrain effective settings. RGBA16F is preferred; a nonblendable/unsupported float target falls back to RGBA8 optical accumulation with optical depth scaled by 1/32 and low effective settings. This fallback has coarser extinction quantization and saturates at depth 32 per bin. Status shows requested overrides, effective values, actual target format and adapter.

Maximum instance storage is 32768 × 176 bytes (5.5 MiB); curved vertex/index buffers reserve 6,216,000 bytes. The column GPU cache uses 131,072 bytes; the clear-air table uses 65,536 bytes. Four RGBA16F accumulation targets cost `ceil(width*scale) * ceil(height*scale) * 32` bytes (half as many with RGBA8). Low lighting still reserves four attachments. These are allocations, not FPS estimates. CPU caches are bounded, and status reports their estimate at publication, cluster/instance/aggregate counts, collapsed refinements, CPU build time, world-time lag and field revision. GPU timestamp queries cover cloud accumulation and composition when supported, using three asynchronous readbacks; unsupported or not-yet-ready measurements are labeled explicitly. There is no automatic quality controller.

## Orbital atmosphere and map readability

`weather_gpu/atmosphere.rs` builds a camera-independent optical-depth table once with 64 integration samples per texel. `atmosphere_common.wgsl` samples it for direct solar/lunar attenuation and integrates a 100 km spherical atmosphere with Rayleigh, Mie and ozone extinction. Nonlinear altitude/horizon coordinates allocate resolution near the surface and terminator. The limb and globe surface use 8 view samples in standard quality or 4 in low; cloud surfaces use 4 or 2. The table uses physical metres regardless of the game planet radius. Multiple scattering is only an inexpensive diffuse approximation. A weak 87 km airglow layer contributes natural night emission; it is not an aurora or a measured spectral atmosphere. This follows the separation of transmittance and view scattering used by [Bruneton](https://ebruneton.github.io/precomputed_atmospheric_scattering/) and [Hillaire](https://github.com/sebh/UnrealEngineSkyAtmosphere), without adopting a volumetric cloud marcher.

The default `weather globe readable` presentation caps the final cloud stack opacity at 0.80, proportionally rescales its premultiplied RGB, limits cloud-shadow darkening of land and adds a small explicitly artistic night fill to clouds, land and atmospheric scattering. It preserves the map under dense clouds and makes the night hemisphere readable. `weather globe natural` removes those presentation adjustments: a dense bank may obscure land and unlit clouds may be almost invisible. Natural airglow and moonlight are deliberately weak; daylight exposure does not make the entire night atmosphere bright. Orbital exposure uses the view's configured exposure instead of the player's local solar altitude. The command does not change weather density, wind, precipitation, FPP, protocol or saves; it lasts for the client session. Bare `weather globe` reports the active presentation.

## Lakes, coasts and fog

The shipped `assets/weather/lakes.bin` contains 3368 polygons from [Natural Earth lakes and regional supplements](https://www.naturalearthdata.com/downloads/10m-physical-vectors/10m-lakes/), including island rings. These data are [public domain](https://www.naturalearthdata.com/about/terms-of-use/). `lakes.json` records source URLs, archive checksums, output checksum and binary format. `scripts/import_weather_lakes.py` explicitly downloads and regenerates the asset; the game does not download it. Lakes are also colored on the globe from these same polygons.

The climate uses a separate approximate water temperature with a reduced daily cycle and delayed seasonal response. Shore probes at 5 and 20 km estimate the direction of nearby water and upwind water exposure. A weaker nighttime reverse breeze and daytime onshore flow are reduced by strong background wind. Cool water stabilizes low convection; inland convergence and terrain uplift can increase it. Cold air crossing warmer, unfrozen water increases humidity, cloud development and precipitation downwind. These are parameterizations of [lake breezes](https://www.weather.gov/bgm/WeatherInActionLakeShadowBreeze) and [lake-effect precipitation](https://www.weather.gov/safety/winter-lake-effect-snow?lv=true), not a fluid solver.

Cloud detail reconstructs up to thirty minutes of local coastal displacement from the solar cycle. This is a bounded residence-time approximation using current shore geometry, not long-term transport of water vapour across a resolved coastline. Stable humid flow up a slope can sustain a partly terrain-anchored low-cloud cap while air flows through it; regional drying reduces condensate supply. Neither effect conserves atmospheric water/heat or resolves mountain-wave dynamics. Cloud motion uses nonsingular world-space flow at the poles.

Fog has distinct radiation, advection and evaporation contributions. Radiation fog favors moist, calm nights and valleys; cooling follows seasonal solar elevation instead of a fixed morning clock. Wetness reconstructed from precipitation modulates it. Advection fog can form when moist air moves over colder water; evaporation fog favors warmer water beneath colder air. These remain approximations of [NWS fog processes](https://www.weather.gov/lmk/fog_tutorial).

Ground fog uses one analytic exponential-height integral in `weather_fog.wgsl`, with a 90–220 physical metre scale height, local fog potential and world-anchored spatial variation. It samples opaque scene depth, including the sky limit; it does not march cloud volumes. The same pass approximates distant precipitation with one spatial cache sample and an analytic path length between ground and a rain-bearing layer's base. Extinction grows with rate and distance, and color follows sky/ambient luminance. This does not resolve individual shafts, integrate varying terrain along the ray or fully order fog/rain against transparent cloud surfaces. It is not a measured visibility profile, resolved valley volume or simulation of cold-air drainage.

Natural Earth is generalized **1:10 million cartography**, not a 10 m raster. It omits many small ponds, rivers and shoreline details; the regional weather grid cannot recover missing source geometry. Water exposure uses directional probes, not an exact continuous fetch integral or resolved lake-effect snow bands. Lake polygons currently affect weather and globe color; they do not generate physical inland water surfaces in terrain geometry. Those limitations must not be described as a complete hydrological simulation.

`lakes.bin` and the retained legacy `cloud_noise.rgba` are generated data assets, exempt from the source line limit only as data. Their hand-maintained import/generation scripts remain below 500 lines. `scripts/build_cloud_noise.py` regenerates the legacy periodic texture without network access; geometric clouds do not load it.

## Precipitation, wet surfaces and storms

Instanced precipitation occupies 3D space near the observer and is occluded by scene depth. The emitter uses local precipitation from occupied low/middle cloud columns at the published physical endpoints; the regional precipitation potential alone cannot emit drops under a resolved clear gap. High cirrus alone cannot host surface rain. Conditions are treated as uniform across the approximately 45 m emitter. Rate changes particle count; wind changes slant and motion. Below freezing, particles become slower snowflakes. Radiance from sky, sun, moon and lightning controls visibility, with no artificial minimum night brightness. Distance fading reduces the visible edge of the local emitter. A cached 8 × 8 sky-cover grid suppresses particles beneath roofs; its 5.6 m cells are an approximation for narrow shelters. The globe's precipitation tint reads the filtered cloud-hosted cache from the same publication.

Ground moisture is reconstructed from current precipitation and preceding one- and three-hour samples. It darkens exposed horizontal surfaces after rain and gives a visual snow tint below freezing. It does not deform terrain or model snow mass, runoff, puddles or melting.

Nearby storm cells derive brief lightning candidates from seed, host cluster, cell and world time. Eligibility now also requires cloud-hosted rain, convection and a developed low-cloud tower; steady stratiform rain alone does not trigger thunder. The selected bolt and cloud flash feed both views. Windows can play generated thunder after a distance-based delay when audio playback is available. The existing selection is a nearby-cell effect, not a globe-wide list of simultaneous strikes. Eligibility depends on the published field, so different regional resolutions or worker lag can affect agreement between clients. The effects change neither damage nor movement physics.

## Commands and optional forecast

Other client modes default to local `partly` weather. Use `weather preset clear|partly|overcast|rain`, `weather simulated`, and `weather status`. `weather live on` is available only on Earth, at 1× world time, within five minutes of real UTC. Leaving those conditions switches back to simulation with a console message. `weather live off` also returns to simulation.

`weather resolution` reports the current requested grid; supplying 64, 128, 256 or 512 schedules reconstruction at that resolution. The corresponding region counts are 2,048, 8,192, 32,768 and 131,072. This preserves the source/mode and never enables networking. Presets intentionally hold coverage and atmospheric state fixed, although detail continues to move; use simulated weather to observe evolving regional conditions. The resolution setting is not saved across client restarts.

Live data comes from [MET Norway Locationforecast 2.0](https://api.met.no/weatherapi/locationforecast/2.0/documentation), provided by the Norwegian Meteorological Institute under [CC BY 4.0](https://api.met.no/doc/License). Cloud fractions, wind, humidity, temperature and next-hour precipitation from the first forecast time step locally correct both snapshots. Fog is estimated where visibility data is unavailable. It is a point forecast, not a global live-weather dataset.

Point forecasts correct atmospheric input and local precipitation/particle winds. Multi-hour formation trajectories reconstruct procedural flow; historical forecast winds are not stored or replayed.

When enabled, the client sends the **in-game observer's** latitude and longitude rounded to 0.01° to `api.met.no`. MET Norway receives the request's IP address and application User-Agent. Requests refresh at most hourly for a stationary location, retry after ten minutes, and expire after three hours. Location changes can request again after ten minutes. Live mode requires internet; it is disabled by default.

## Verification status

Implementation verification uses formatting, workspace compilation, Clippy with warnings denied, exact shader-source composition through Naga, CPU/WGSL layout and stage-visibility checks, architecture boundaries, the 500-line limit, diff whitespace checks and workspace release compilation. The shader validator is `rtk cargo run --manifest-path scripts/wgsl_validator/Cargo.toml`; it creates no GPU device and does not run the game.

Unit/integration/E2E tests and runtime verification remain deferred by `AGENTS.md` until the user explicitly starts final project testing. Static readiness does not confirm appearance or performance.

That phase must cover each morphology; intersections and draw-order invariance; camera entry/exit, near-plane and opaque geometry clipping; fronts/clearings/water/mountains; births, population boundaries and wind shear; identical UTC in FPP/globe; cloud-hosted rain and clear gaps; slow/fast zoom, LOD hysteresis and strict budgets; pause/reverse/forward UTC and stale results; poles/date line/cube boundaries/teleport; Sun/Moon/night/terminator/lightning; all cloud profiles and buffer scales at 1080p and higher; repeated resize/settings and bounded cache memory. Compare canonical precipitation and total allocated mass across quality changes, and check that image, shadow and effects identify the same published revision. Report measured GPU time and adapter; no FPS is inferred from the design.


### P1/P2 visual-first hybrid (review pending)

Near clouds retain their physical transported lobes, but `weather clouds method volumetric` (default) integrates eroded 3D density along each view chord and lights detail normals. `weather clouds method analytic` is a smooth view-only fallback. The globe's curved 2.5D patches use optical opacity per fragment from the same global/regional 64x64 cloud-column cache that drives weather shadows and precipitation. Layer height/lighting and transitions remain approximations.

This first hybrid is not yet one fullscreen sample per view ray. Lobe overdraw and CPU moment cost remain for later tuning. No new temporal history is introduced. P0 profiling is deferred; GPU, runtime and appearance results have not been verified. P3 teleport consistency, P4 a stateful climate solver and P5 planet atmosphere parameters remain separate implementation stages.


## P4 planetary circulation field (visual-first)

`minimium-weather` is a pure Rust three-level global circulation model. It uses 48×24 equal-angle horizontal cells and approximately 0.8/4.5/10.5 km levels, semi-Lagrangian advection, pressure-gradient acceleration, Coriolis rotation, solar forcing, terrestrial ocean/land heat response, saturation/condensation and precipitation fallout. It reconstructs two 6-hour-overlapping physical cohorts from the **same** seed and world UTC; finite warm-up and 10-minute steps bound cost for fast time scales. Cohort interpolation avoids abrupt time-window resets. This is **not** mass/energy-conserving numerical weather prediction. Polar and coastal flows are deliberately coarse.

The global result influences all 32 weather-atlas channels through the existing regional/terrain refinement pathway: temperature, humidity, horizontal winds, cloud coverage, condensate and rain potential. Existing canonical cloud IDs, optical sampling, precipitation hosting and lightning conditions remain the source of visible effects. The solver does **not** yet replace lobe transport with a full solved wind field, so wind motion is only approximate.

The new solver is independent of FPP/globe camera position and renders through the same published field. Its Earth surface uses the loaded GEBCO package when available. No live network weather fetch or new wire/save format is introduced.

Code changes are committed, not visually verified; the user will measure performance and approve appearance. P0 profiling remains deferred.


### Server weather owner

The headless server now runs a bounded `WeatherAuthority` worker. At most one outstanding global Earth reconstruction is enqueued; outdated frames are rejected by world instance, seed, clock revision and UTC interval. It is driven by the server's authoritative clock, not the client's FPS. The admin `status` command reports the last published weather epoch. Client and server use the same `minimium-weather` reconstruction code and GEBCO surface, so no full planet atlas is transferred in each replication tick.

**Limitations:** Server weather is currently read-only to gameplay. It is not persisted as an independent state snapshot and no weather checksums travel in protocol v13. Floating-point cross-platform bit equality and live forecast client corrections are not guaranteed. This first-stage authority supports a single Earth instance and must be generalized to independent per-planet worlds before multiple Earth simulations can coexist. It does not imply weather gameplay parity is already verified.


## P5 planet atmosphere ownership

`minimium-weather::AtmosphereProfile` owns radius, gravity, rotation, obliquity, orbital period, insolation, atmospheric pressure/temperature, optics and condensable. `GasComposition` validates gas fractions; optical coefficients remain separately calibrated. The same Earth profile now supplies CPU transmittance LUTs and the four additional 16-byte WGSL uniform vectors used by near-cloud and globe atmospheric sampling. This removes a second copy of the Earth optical constants.

Earth is the **only active runtime atmospheric profile**. Future planet presets require world/profile network persistence, non-water microphysics and GPU LUT resource rebuilding. The star/sky artistic horizon model is still Earth-specific. No claim of full planetary universality, visual acceptance or performance validation is made.
