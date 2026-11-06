# Terrain presentation and climate

Implementation status: **STATIC CHECKS ONLY; RUNTIME / VISUAL ACCEPTANCE NOT RUN**.
The existing shadow receiver-plane correction is preserved. There is no claim
that the reported bands are eliminated until the diagnostics below are inspected.

The next correction aligns manual PCF depth comparisons with actual shadow texel
centers and carries the material warp Jacobian into mip selection and normals.
No extra material fetches were added. The offline recipes now use role-specific
periodic fields and stochastic elements, with metre-based relief normals. A BC4
8-bit index overflow was corrected, and both resolution variants now derive from
one source. Static PNG previews are emitted under `target/materials/sample-v1/previews`.

## Unified planet surface (2026-11-06)

Status: implementation and static validation; **visual/runtime acceptance pending**.

The overview and first-person camera now select the same terrain quadtree and
draw the same TerrainGpu material pipeline. The separate globe sphere, palette,
height-in-alpha map, detail overlay and associated GPU resources are removed.
Planet height, shoreline, normal sampling, material means and weather shading
therefore have one consumer path. Overview presentation interest is independent
of player collision interest. Opening the overview retains the floating origin,
avoiding a lossy round-trip of nearby vertices through planet-sized f32 positions.

Procgen derives retention and exposure from a fixed-radius height neighborhood.
TerrainSurfaceSample distinguishes displayed elevation, physical source elevation,
landform and mapped/procedural coverage. Sample content supplies a validated
SurfaceRecipe; its revision is independent of generator revisions and saves.
GPU noise perturbs habitat boundaries. Squared weights and excessive pioneer
takeover were removed. Unresolved masks converge toward their area fraction.
Two resolved materials use a bounded microheight transfer fading at pair changes.

Material and normal detail follow the pixel footprint instead of a fixed
128-256m cutoff. Existing 2K residency, mipmaps and triplanar arrays remain.
At most two detailed materials are sampled (up to 18 material texture reads).
Unresolved layers use their decoded means. Weather costs four reads for local
or global history, eight in their transition. These are source-derived bounds,
not measured performance improvements.

WeatherModel::surface_state evaluates climate and replayable cover from one
prepared site. A 65x33 planet atlas and a 9x9 local grid each store adjacent UTC
hours plus temperature/source elevation. The atlas is cached independently of
the camera; local coverage uses fixed angular lattice sites so overlapping
windows share samples. Longitude seams and pole samples are explicit.
Local tangent fog remains separate. Surface textures occupy 71,232 bytes.
Source elevation corrects coarse-grid temperature on mountains. The atlas is
deliberately coarse: small regional scenarios and snow lines need acceptance
against the local view. Analytic cover is the unavailable/stale-history fallback.

LOD uses viewport/FOV scale, 25% split/merge hysteresis and at most 256 leaves.
Terrain edge conformance moves fine boundary vertices onto the coarse neighbor's
actual line, including cube-face boundaries, and removes radial curtains.
Generated/collision geometry is unchanged. Conformed copies are cached by
source Arc and neighbor edge spacing. GPU uploads stage replacements, including
changed versions of active keys; publication swaps a complete selection atomically.
This is discrete LOD with hysteresis, not temporal vertex geomorphing.
CPU source geometry has 512 entries / a 128 MiB eviction threshold; a single
insertion can cross the threshold. Current conformed/prepared selections have
separate bounded residency. GPU uploads retain eight patches / 4 MiB per frame,
with one indivisible patch allowed to cross the byte limit.

Commands: terrain status reports camera, patch/triangle counts, surface memory,
last CPU preparation time, source-cache memory and history readiness.
Terrain debug habitat shows exposure in red and retention in green.
Terrain debug source shows mapped coverage in cyan and fallback in orange.
No WorldCover tiles were added; absent data still uses the explicit fallback.
Lakes formerly painted only on the overview no longer pretend to be surface
water. Earth now uses a separate sea-level sphere intersecting signed ground
height, as described below; full inland water geometry remains outside this change.

Acceptance locations: dry (23.5 N, 13 E), temperate (52 N, 19 E), alpine
(46.6 N, 10.8 E). Compare overview/FPP at identical seed/UTC, including January
and July, edge/face crossings, reconnect, teleport and rapid altitude changes.
Record frame-time percentiles and preparation/upload activity before assessing
performance. Compilation is not proof of appearance or shadow correctness.


## Separate ocean and shoreline correction (2026-11-06)

Inspection of the screenshot and code found a coupled failure: render vertices
below sea level were clamped to the water surface, mixed triangles were split
using a binary ocean flag, then that flag was resampled and interpolated again
by the terrain material shader. Clipping on the original triangle plane did not
establish a consistent ground/water intersection.

The presentation now retains Earth bathymetry and draws a separate sea-level
sphere. Sea and ground use the same canonical lattice, selected LOD and coarse
edge fractions. This matters: an independently tessellated sphere can otherwise
cover low coastal land because the two meshes have different chord sag.
Depth comparison defines the coast; terrain no longer paints blue from ocean
flags. Coastal nodes have twice the refinement priority within the same 256-leaf
budget. Source geography still sets the coastline; no random coast displacement
is added. Signed elevation controls a restrained damp band on exposed shore.

The ocean has radial normals, existing Sun/Moon illumination, broad Fresnel sky
reflection and atmospheric depth. It has no material texture reads, wave textures,
extra full-screen pass or reflection capture. Completely submerged ground skips
material and shadow preparation. One ocean vertex is 24 bytes, up to 512 triangles
per dense patch and 9 MiB for 256 patches (active and pending selections can overlap).
The same 8-patch / 4 MiB upload threshold covers both surfaces. All ocean buffers
participate in resource accounting, retirement and origin rebasing.
The terrain status command exposes ocean triangle counts.

Authoritative planet samples, collision, seed, protocol and saved world stay
unchanged. Ocean collision remains the existing sea surface. Start the existing
world normally; no ResetWorld or material regeneration is needed.

Static verification and final visual acceptance are separate. Pending acceptance:
- Ground and elevated views of a coast, including the location in the supplied screenshot.
- Fine/coarse sector borders and cube-face boundaries, at several zoom levels.
- Fly movement across render-origin shifts, teleport, reconnect and world transfer.
- Coast at noon, low Sun and night; terrain debug normals/depth/shadow.
- Ocean/land consistency in overview and FPP; no partial shoreline upload frames.
- Frame-time and memory comparison on the target GPU.

This is a tessellated surface ocean, without underwater rendering, tides, rivers
or elevated lakes. Coast detail remains limited by the geographic input and
mesh budget. Compilation cannot confirm the absence of visible artifacts.

Verification for this shoreline correction passed: workspace/all-target compilation,
terminal-only compilation, composed WGSL and binding validation (including ocean),
architecture graph, formatting, diff whitespace and the 500-line limit (411 files).
Release client/server compiled under target/flight-check. The normal release path
could not replace minimium-server.exe while the user's server was running; that
session was left intact. After closing client/server, start-earth.ps1 rebuilds the
normal executables. No tests or game launch were performed.

## Ownership

Assets owns manifests, strict KTX2 parsing and fractional land-cover bytes;
asset-io owns files/HTTP/hash cache; procgen owns surface/normal/coverage queries;
environment owns climate/weather/history; client-runtime owns content readiness;
renderer owns typed terrain buffers, materials, cover-grid upload and lighting.
Sample recipes and scenario defaults live in games/sample. Collision, world map
triangles, save formats and generator fingerprints remain independent of materials.

## Offline base package and distribution

The performance path selects at most two detailed materials per fragment while
all six cover fractions contribute mean color. Detail fades with screen footprint;
see current bounds above. Means are decoded once, water skips material textures
and parallax remains disabled. 4096 squared sources remain valid; residency
starts at the 2048 mip (about 80 MiB for BC textures).

For a terminal-only executable, use `rtk cargo build -p minimium-client
--no-default-features`. This skips the material cooker, defaults to the console
and excludes GPU, window, clipboard and astronomy adapters from the Cargo graph.
The default desktop executable also supports `--console` without creating GPU
resources; use the separate feature configuration when dependency isolation matters.

Build prerequisites: Python 3 with numpy, plus the normal Rust toolchain.
`MINIMIUM_ASSET_PYTHON` can choose the cooker interpreter. The desktop client build
cooks six-layer albedo, normal and packed roughness/AO/height/reflectance arrays.
The high variant uses 4096 BC1 sRGB / BC5 / BC3; the fallback uses 2048 RGBA8,
with sRGB only for albedo. Both have complete mip chains. Normals are renormalized
and albedo mips are averaged in linear light. Source hashes cache the build output.
Generated KTX2 files are data assets, excluded from the hand-maintained line limit;
the cooker and sample recipe remain ordinary bounded source files.

Static package validation:

```
rtk cargo run -p minimium-asset-io --example validate_materials -- target/materials/sample-v1
```

After compiling the executable, package it without running the game:

```
rtk python tools/package_client.py target/release/minimium-client.exe target/client-package
```

The installed client looks beside its executable for `materials/sample-v1`.
Development builds can use their cooked build directory. No textures are generated
on startup. The packaged folder must be kept together for offline operation.

Mods use the same `manifest.ron` format and ordered roles, with different map
hashes/scales/blend width. Arrays have exactly six layers; dimensions are fixed to
the two supported variants. Each selected download and material allocation is
capped at 512 MiB. No mod code or shader is accepted.

Server startup example:

```
minimium-server run --material-package PATH --material-http-port 8787 --climate-scenario games/sample/climate/warming-drought.ron
```

The material endpoint uses the server's bind address and a separate port. The
game connection advertises the manifest hash and port, never sends texture data.
The client restricts downloads to the game peer's IP. Cache blobs live under
`LOCALAPPDATA/minimium/materials` (or `.minimium-local/minimium/materials`). Partial
downloads never replace a complete package. Failures require reconnecting with
`connect HOST:PORT`; verified blobs are reused. Base content is chosen only when
the offer has no required custom hash. The package is session-wide, including
world transfers; current prepare/ready still gates target collision/GPU sectors.

## Real coverage

Prepare WorldCover separately from height data:

```
rtk python tools/import_worldcover.py third_party/gebco-2026/worldcover.bin TILE1.tif TILE2.tif
```

The importer needs numpy/rasterio and supplied WGS84 ESA WorldCover 2021 v200
tiles. It aggregates class indicator fields to fractions at two angular minutes,
never nearest labels, and emits hashes plus source attribution. Untouched cells
are explicitly missing. The immutable geography adapter verifies and maps the
grid beside GEBCO. Data is interpolated across the antimeridian and poles clamp
to their edge rows. Missing data uses procedural coverage. The repository does
not include the global WorldCover raster or claim it has been imported here.

Attribution: © ESA WorldCover project 2021 / Contains modified Copernicus Sentinel
data (2021) processed by ESA WorldCover consortium. CC BY 4.0. Dataset:
[ESA WorldCover 2021 v200](https://doi.org/10.5281/zenodo.7254221);
[access and licensing](https://esa-worldcover.org/en/data-access).
Texture container: [KTX2 specification](https://registry.khronos.org/KTX/specs/2.0/ktxspec.v2.html).

## Climate and cover

Scenarios use absolute UTC seconds, smooth start/end interpolation, global
temperature/moisture offsets and up to 32 circular regions with smooth influence.
`--climate-scenario` initializes `world.climate.ron`; restart restores that file.
Cover is not stored in snapshots. The seasonal weather model drives both clouds
and cover; opposite hemispheres have opposite seasonal phases.

Snow replays seven days at hourly steps; wetness replays six hours.
Both cameras consume the planet atlas and anchored local grid described above.
Weather and cover remain off-frame, with shared scenario/UTC inputs. Physical
source height controls temperature lapse rate. Material detail never changes
collision or the terrain silhouette.

## Recovering an unsupported saved world

If startup reports an unsupported generator revision, do not edit the revision
number inside saved files: that could change geometry under an old snapshot.
`start-earth.ps1 -ResetWorld` now explicitly backs up the entire world directory
to a timestamped sibling and starts a fresh supported Earth world. The client
profile and GEBCO package remain separate. An active server lock prevents reset.

## Deferred acceptance

No unit/integration/E2E or game/load runs were started. At the user-directed final
phase, inspect all of the following; compilation is not visual evidence:

- Six-material transitions across sectors, LOD, distance and planet faces.
- Pattern stability during teleport and render-origin changes.
- Bands under bright/dark light: `terrain debug albedo`, `normals`, `depth`,
  `shadow`; restore with `terrain debug off`. Check overlap, depth and self-shadow.
- Wet reflections, hemispheric seasons, cold mountains, drought and scenario ramps.
- Same cover after reconnect and arbitrary time jumps at the same location/UTC.
- Packaged offline startup, custom HTTP download, cached reconnect, bad checksums,
  interrupted transfers, required-package failure/retry and graphics capability fallback.
- Headless server/terminal/loadtest never download materials or initialize GPU resources.

All visual and runtime items remain pending. The global WorldCover import also
requires the external source tiles and is separate from code completion.

## Research: procedural materials and local terrain mosaics (2026-11-05)

### Conclusion for this engine

There are three independent scales: climate and broad land cover, local habitat
patches, and centimetre/millimetre material structure. Increasing texture resolution
cannot supply missing habitat patches. Conversely, adding noise to coverage cannot
repair shadow artifacts or make a weak grass material photorealistic. The practical
recommendation is cooked PBR maps plus inexpensive continuous coverage fields.
Recommendations below are engineering judgments for this repository, rather than
performance or visual claims made by the cited papers.

The new local layer evaluates three independent, seeded 3D value-noise fields with
2048m, 32m and 4m lattice spacing. These are approximate variation scales, not fixed
patch diameters. They modulate habitat suitability, loose sand/soil and vegetation
clumping. A smooth pioneer mask permits grass on loose substrate when moisture,
temperature and slope support it, including coarse source cells with zero grass.
Severe drought/cold can suppress it. This is a visual heuristic; a lighter patch
does not establish groundwater, a spring, a river or a physically simulated oasis.

### Why visiting regions need not create seams

Use `coverage = F(world_seed, planet_position, base_cover, climate)`. The static
patch field excludes visit time, sector identifiers and RNG traversal state.
Computing it on demand already provides lazy generation: an unvisited location
needs no saved patch data. Reconnecting or approaching from the other side samples
the same function. Climate-dependent appearance can still change with world UTC.

The current implementation samples Cartesian planet coordinates, avoiding UV
longitude seams, poles and cube-face coordinate changes. Large and local position
components remain separate until the lattice cell and fractional offset are known.
Quintic interpolation smooths the lattice transitions. Render-origin changes do
not enter the function. This guarantees consistent field inputs, not identical
images between different GPUs or differently approximated LOD surfaces.

If later baked into sector masks, sample a common global lattice including a
border halo; never normalize each sector independently. Neighboring masks need the
same scale, seed, recipe version and boundary samples. Persistent player changes
would be separate deltas on top. No such mask cache or persistent ecology is added
here. The existing top-two material approximation and different LOD geometry can
still affect transitions; continuity of the noise alone does not prove seam-free
final shading.

### Classical procedural techniques

The research survey by Lagae et al. distinguishes noise construction, spectral
properties, surface mapping and filtering. This is a useful framework: selecting
a noise family is only one part of a texture system.
[Lagae et al., 2010](https://graphics.cs.kuleuven.be/publications/LLCDDELPZ10STARPNF/).

| Technique | Useful role here | Limitation / decision |
| --- | --- | --- |
| Value noise, several weighted scales | Habitat masks, soil pigment, large color variation | Smooth blobs alone have no recognizable material structure. Currently used. |
| Gradient / improved Perlin noise | Alternative smooth fields and derivatives | Not automatically more realistic than value noise; replacing the basis alone is low priority. |
| Cellular / nearest-feature distances | Granules, clods, pores, sparse cracks | Unmasked cell borders readily resemble paving. Use disrupted features and varied scales. |
| Domain warping | Break straight boundaries and regularity | Preserve derivatives for filtering; excessive warp stretches features. |
| Scattered shaped elements | Leaves, twigs, short blades, pebbles | Shapes need density, orientation and size distributions; avoid identical stamps. |
| Gabor noise | Controlled directional frequency structure | More useful for offline authored patterns than a wholesale runtime replacement. |
| Wavelet noise | Controlled frequency content and filtering | Relevant when aliasing limits detail; not a substitute for material art direction. |

Improved Perlin noise uses gradient noise and quintic interpolation. Our quintic
interpolated random scalar lattice is **value noise**, not an implementation of
Perlin's gradient algorithm. Both can provide coherent masks.
[Perlin reference implementation](https://mrl.cs.nyu.edu/~perlin/noise/).
Cellular distances provide different structures from smooth lattice noise.
[Worley, 1996](https://cedric.cnam.fr/~cubaud/PROCEDURAL/worley.pdf).
Sparse Gabor convolution specifically allows frequency/orientation control.
[Lagae et al., 2009](https://doi.org/10.1145/1531326.1531360).
Wavelet noise addresses the tradeoff between retained detail and aliasing.
[Cook and DeRose, 2005](https://graphics.pixar.com/people/derose/publications/WaveletNoise/paper.pdf).

Texture bombing scatters small motifs rather than repeating one image rigidly;
features crossing a cell require neighboring-cell consideration. For this engine,
scattering leaves/stones during the offline cook is preferable to paying for those
neighbors every terrain pixel.
[Glanville, GPU Gems chapter 20](https://developer.nvidia.com/gpugems/gpugems/part-iii-materials/chapter-20-texture-bombing).

### Synthesis from examples and learned materials

Image Quilting creates texture images from overlapping example patches. Our
recommendation for PBR is to keep patch selection and transforms synchronized
across all channels: quilting RGB alone does not establish matching normals or
roughness. Source photography also needs lighting separation.
[Efros and Freeman, 2001](https://graphics.cs.cmu.edu/people/efros/research/quilting.html).

Heitz and Neyret synthesize stochastic textures from examples with a
histogram-preserving blending operator, addressing contrast loss from ordinary
blending. This is a stronger anti-repetition technique than our current smooth
coordinate warp. It should be evaluated on one material after profiling, rather
than enabled for every triplanar sample at once.
[Heitz and Neyret, 2018](https://eheitzresearch.wordpress.com/722-2/).

DiffMat/MATch optimize parameters of differentiable procedural material graphs
against reference appearance. MatFuse generates material reflectance maps with
diffusion and conditioning. These are possible **offline authoring** tools; no
PyTorch/model inference is proposed for the running client.
[DiffMat implementation](https://github.com/mit-gfx/diffmat),
[MATch](https://research.adobe.com/publication/match/),
[MatFuse, CVPR 2024](https://openaccess.thecvf.com/content/CVPR2024/papers/Vecchio_MatFuse_Controllable_Material_Generation_with_Diffusion_Models_CVPR_2024_paper.pdf).

MatSynth provides thousands of tileable 4K PBR materials with provenance metadata.
It is useful as a reference and potential source pool; inspect individual material
licenses, physical size and channel conventions before importing anything. No
external material dataset or AI-generated production pack was installed here.
[MatSynth, CVPR 2024](https://openaccess.thecvf.com/content/CVPR2024/papers/Vecchio_MatSynth_A_Modern_PBR_Materials_Dataset_CVPR_2024_paper.pdf).

### What makes the six materials convincing

Base color, surface orientation and roughness describe different aspects of light
interaction. Work in linear light when averaging color; do not copy the same noise
unchanged into RGB, height and roughness. Some correlation is physical, but dark
pigment does not necessarily imply a deep hole. Dielectric ground needs plausible
reflectance, without painting directional highlights into albedo.
[Filament material and lighting reference](https://google.github.io/filament/main/filament.html).

| Role | Suggested visual structure | Scope limit |
| --- | --- | --- |
| Sand | Small mineral grains, subtle pigment differences, occasional larger grains | Ripples only where intentional; not the same waves on every material. |
| Soil | Disconnected clods, pores, mixed grain sizes, occasional stones | Avoid a continuous Voronoi paving network. |
| Grass ground | Uneven clumps, dry/live color variation, exposed substrate | Flat maps do not provide blade silhouettes. |
| Litter | Overlapping differently sized leaves, twigs, soil gaps | Avoid a uniform orange noise field. |
| Rock | Sparse irregular fissures, weathering, chips and mineral variation | Texture relief cannot create boulder silhouettes. |
| Snow | Low-contrast settled surface with restrained granular detail | Current lighting does not model full snow subsurface scattering. |

The current recipes implement initial versions of these structures. Their physical
normal amplitude follows relief in metres divided by the material tile size;
changing pixel resolution therefore does not arbitrarily strengthen the bumps.
Source crops, complete tiles and repeated tiles should all be reviewed. A contact
sheet is an asset inspection aid, not proof of realism under in-game lighting.

### Filtering, tiling and distance

Every cooked channel must wrap consistently, including elements crossing the tile
edge. Color mips average decoded linear values; normal mips average and normalize
vectors. The latter loses unresolved normal variance: LEAN mapping stores moments
to preserve the broader specular response of small bumps. A cheaper future option
is offline roughness adjustment from normal variance, evaluated against the
current BRDF before adoption.
[Olano and Baker, 2010](https://userpages.cs.umbc.edu/olano/papers/lean/).

Warped coordinates need transformed screen-space derivatives for correct mip
selection. The shader now applies the warp Jacobian to explicit gradients, and
the local patch fields fade unresolved scales using pixel footprint. This is
approximate filtering, not an analytic band-limited solution. Strong thresholding
would reintroduce high frequencies, so patch transitions remain soft.
[WGSL textureSampleGrad](https://www.w3.org/TR/WGSL/#texturesamplegrad).

Near, intermediate and distant rendering should retain the same broad habitat
field while dropping material microstructure. Existing 128–256m material fading
uses mean albedo in the distance; the habitat fields remain independently sampled.
Anisotropic filtering and more elaborate stochastic sampling remain candidates
after frame-time measurements, not assumptions about free quality.

### Storage and GPU budget

KTX2 is a container for texture data and mip levels; it does not by itself improve
art quality or compress all content. The actual format and encoder matter.
[KTX2 specification](https://registry.khronos.org/KTX/specs/2.0/ktxspec.v2.html).
BC1 stores an eight-byte 4×4 block; BC3, BC5 and BC7 use sixteen-byte blocks.
[Microsoft block compression reference](https://learn.microsoft.com/en-us/windows/win32/direct3d11/texture-block-compression-in-direct3d-11).

Our calculations for six layers and complete mip chains, excluding small headers:

| Resident maps | 2048² | 4096² |
| --- | ---: | ---: |
| BC1 albedo + BC5 normal + BC3 packed | about 80 MiB | about 320 MiB |
| Proposed BC7 albedo + BC5 + BC3 | about 96 MiB | about 384 MiB |
| Three RGBA8 maps | about 384 MiB | about 1536 MiB; outside budget |

BC7 is a possible albedo improvement, but the current cooker is a simple BC1
range-fit encoder. Adopting a mature offline encoder should precede inventing a
complex handwritten replacement. BC7 is not implemented in the current manifest
or loader; the figures are a design comparison.

Logical read bounds from the current shader are separate from measured cost:

| Path | Material reads per fragment |
| --- | ---: |
| One projection, one material, all three maps | 3 |
| Three projections, two detailed materials plus uniform cover means | at most 18 |
| Normal-free transition plus uniform mean colors | at most 12 |
| Beyond 256m, uniform cover means | 0 |
| Hypothetical three-example stochastic near path | up to 54 before extra lookup tables |

Shadow PCF and the climate grid are additional reads. The new patch layer adds 24
integer lattice hashes and interpolation arithmetic, without material texture
reads. Hardware filtering, branch coherence, cache behavior and screen coverage
make these counts unsuitable as FPS predictions. Measure GPU milliseconds on the
actual adapter before expanding sampling.

### Implementation status and next decisions

Implemented: texel-center shadow comparisons; derivative-aware warp; semantic
offline recipes; physical normal amplitude; corrected BC4 indexing; linear color
and normalized normal mips; shared source for both resolutions; asset previews;
seeded local coverage mosaics with filtered detail. Collision, generator revisions
and save formats are unchanged. Existing worlds need no reset for these fixes.

Recommended next: compare the six recipes with reference ground photographs and
licensed PBR examples under fixed lighting; tune patch distributions in humid,
semi-arid and desert locations; measure frame time; then consider stronger offline
compression and roughness filtering. Only introduce more runtime stochastic reads
where a visible repeat problem justifies them. Vegetation geometry, hydrology,
virtual texturing and runtime learned generation are separate future work.

Acceptance remains deferred under repository policy. In the final phase, inspect
the same location across reconnect, origin changes, sector/face boundaries and
LOD; inspect patch blends and any top-two selection boundaries; compare albedo,
normal and shadow diagnostics; measure frame times with stable camera/weather.
Compilation and static WGSL validation cannot establish photorealism, seam-free
rendering or removal of the screenshot artifacts.

Verification of the local-mosaic addition: workspace/all-target compilation,
static WGSL validation, formatting, architecture and file-length checks passed.
Release relinking was attempted but Windows denied replacement of both running
client/server executables. Thus the earlier release build covers the asset and
shadow fixes; the new mosaic still requires relinking after those executables are
closed. No processes were stopped and no game/test/benchmark run was initiated.

## Flicker and dry-habitat follow-up

The reported angular dark marks in a still image are consistent with shadow
self-occlusion or geometry/depth artifacts, but a still image cannot identify the
cause. The scene/shadow paths use the same precise triangle positions; the active
render traversal selects quadtree leaves, with no evident parent/child duplicate
draw in the inspected code. This does not prove absence of geometry artifacts.

Confirmed discontinuities were corrected: swapping the second/third material no
longer swaps a fully weighted texture; all six mean colors contribute and only
two layers add gated detail. The gate is zero when a selected weight meets the
strongest omitted weight, including three-way ties. This adds up to six reads of
the final 1×1 mip, without adding triplanar layers or GPU allocations. It can
soften detail where multiple materials are equally strong; GPU impact is not
measured. The older top-two-only read budgets above have been updated accordingly.

Triplanar planes fade to zero before culling, warp lattice fractions retain local
precision, and nine shadow samples use continuous tent weights. These remove
specific discontinuities; screenshot artifact removal still needs visual evidence.

Desert ground should not be uniformly sand or uniformly dotted with lush green
grass. USGS describes varied desert substrates and sparse vegetation; NPS reports
that precipitation and water-retaining soils affect desert grass cover.
[USGS desert features](https://pubs.usgs.gov/gip/deserts/features/),
[NPS desert grass study](https://home.nps.gov/articles/ncpn_grasslandstudy.htm).

Our inexpensive approximation uses the fixed broad field as a soil-retention
proxy, existing humidity and surface wetness as supply indicators, and finer
fields for clumps. Dry habitat raises the clump threshold and reduces vigor;
extreme dryness, cold, heat and steep slopes suppress colonization. Dry grass is
less green. Thresholds are visual tuning parameters, not values calibrated from
those ecological studies. Actual groundwater, runoff, soil classes and plant
species are absent. This does not claim to locate real Earth oases or valleys.

Latest verification: WGSL validation, workspace/all-target compilation, release
client build, formatting, architecture and file-length checks passed. The release
executable was successfully relinked on this attempt; the earlier file-lock note
is historical. Game, FPS and visual verification remain pending. No world reset
is required.

## Full-path stabilization follow-up

Inspection found radial LOD curtains in the same triangle stream as ground.
They were shaded with sampled ground normals and included in shadow casters.
They now use face normals and are filtered from the caster stream by detecting
radial edges in precise coordinates (3mm lateral tolerance). Crack concealment
remains visible; authoritative generation, saves and collision are not modified.
This removes an artificial caster class, but does not establish that every mark
in the screenshot originated from it. Rare real vertical radial cliffs would
also match this geometric classifier; the current height-field terrain does not
author such walls as independent features.

The earlier six additional mean-albedo reads have been eliminated. Supported BC1
and RGBA8 final mip colors are decoded and converted from sRGB once after package
validation. Six linear colors are stored in the terrain uniform (240 bytes, with
Rust size/offset assertions and static WGSL layout validation). Mod packages use
their own colors, without hard-coded role colors in normal rendering. Material
fetch bounds are again 18 nearby, 12 in the distance transition and zero beyond
256m, plus two cover-grid reads and the separate shadow filter. These remain
source bounds, not measured GPU timings.

History cover now contains two 9x9 array layers for neighboring UTC hours. The
shader interpolates them in time using the absolute world clock, removing the
old hourly step. If the next worker result is delayed, the shared endpoint is
held while stale influence fades over one world hour. This adds 648 bytes of
cover texture storage and one cover lookup; replay work for two endpoints occurs
on the worker. It does not introduce a frame-integrated simulation or saved cover.
Grid recenters and geometry LOD changes still require visual review.

New diagnostics: `terrain debug base_color` shows actual color without lighting;
`terrain debug coverage` shows coverage in a fixed role palette. Existing
`albedo` deliberately keeps uniform grey with lighting. `shadow` isolates shadow
visibility, and `off` restores normal shading. All 128 seed bits now influence
the two shader seed words; the earlier implementation used only the lower 64.

Verification: release client/server build, workspace/all-target compilation,
terminal-only compilation, WGSL/ABI validation, both cooked package variants,
formatting, architecture, file lengths and diff whitespace checks passed.
Runtime visual/FPS acceptance remains deferred. This section supersedes historical
read budgets and single-hour grid descriptions elsewhere in this document.

## Confirmed shadow-channel artifact follow-up

The user's subsequent `terrain debug shadow` image contains the same angular
bands. This establishes a shadow-visibility artifact rather than a material-map
pattern. It does not alone distinguish comparison precision from caster geometry.
Earlier texel alignment, filter continuity and curtain changes did not resolve
the artifact in the user's running build.

The inspected receiver-plane calculation differentiated projected UV/depth values
in f32 and divided by their screen-space determinant. Nearly equal depth values
can lose precision on small nearby triangles. The revised implementation derives
the depth gradient analytically from the orthographic shadow matrix's orthogonal
scaled rows and the actual triangle normal. Terrain now carries a flat geometric
normal computed from the same rounded origin-relative positions used in both
raster passes. Smooth height-field normals remain lighting inputs. Light-tangent
planes have an explicit singularity guard. This is a targeted numerical correction,
not a claim of visual acceptance. The nine-tap filter and bias settings are retained.

Terrain vertices grow from 84 to 96 bytes; Rust size/offset assertions accompany
the WGSL layout. World/protocol/save formats do not change. Coverage fields also
receive a smooth contrast remap before filtering, dry sand/soil redistribution,
and normalized squared weights to avoid a uniformly mixed ground color. Cold or
very dry sites remain unsuitable for grass; no new noise samples or texture reads
are introduced. Runtime appearance and frame-time effects remain pending.

Current verification: workspace compilation and WGSL validation passed. Release
compilation produced the client artifact, but Windows denied replacement of the
running client/server executables. Close them before using `start-earth.ps1` to
relink and launch the correction; no process was terminated by the agent.

## Visual refinement using the existing wgpu path

This iteration keeps the six material assets and geometry. The terrain sampler
uses 4x anisotropy with linear filters; normal strength additionally fades by
texel footprint, rather than distance alone. A screen-space normal-variance term
broadens specular response where small normals could sparkle. The half-vector
normalization also handles opposing view/light vectors without dividing by zero.

Ambient diffuse light now responds to the normal's sky-facing hemisphere. A weak
broad reflection uses the existing zenith/horizon sky colors, surface roughness
and Fresnel response, with an approximate ground-facing contribution. This can
give wet ground a more coherent sky response, but is not a measured environment
convolution, scene reflection, global illumination or ambient-occlusion solution.
The existing material AO remains the only local occlusion input in this term.

Bare soil/sand receive restrained warm/cool pigment variation using the already
computed fields; snow receives much less broad contrast. No new texture assets,
noise octaves, mesh objects, render targets or render passes are introduced.
Logical texture read bounds remain unchanged; anisotropy can increase hardware
filter work and normal-variance/lighting arithmetic adds cost. Visual balance and
GPU milliseconds still need acceptance on the target adapter.

Verification for this refinement: latest release client executable successfully
rebuilt, including the preceding analytic shadow-plane correction; workspace and
terminal-only compilation, WGSL validation, formatting, architecture, file-length
and whitespace checks passed. No game, benchmark or final test phase was run.

## Unified atmosphere implementation

Implementation status: code and static checks; visual/runtime acceptance is pending.
This section supersedes the earlier orbit-only atmosphere descriptions.

The actual render camera now supplies optical altitude, local up and the inverse
scene projection. Earth optical length scale follows height_scale_permille,
independently of the configured planet radius; Earthlike uses game metres.
The optical sea-level radius includes the world's sea-level offset. CPU f64
preparation preserves small heights before upload; stable radial differences
avoid subtracting Earth-sized f32 distances. Below-sea-level depressions retain air.

Render-only astronomy follows camera latitude/longitude. Its reference position
is constructed to produce the desired physical altitude within the existing
bounded observer API, capped at 80 km; the atmospheric model retains the uncapped
camera. Authoritative player observers, geometry, saves and protocol are unchanged.

Shared frame bindings expose the 256-byte atmosphere uniform, optical-depth LUT,
sky scattering/transmission, aerial scattering/transmission, isotropic-fill LUT
and regional fog grid. Sampled textures and storage staging are distinct, with
explicit copies between compute passes and before scene rendering. There are
no simultaneous sampled/storage aliases or scene read/render feedback loops.

Current resources and update rules:

| Resource | Capacity | Standard work | Low work |
| --- | --- | --- | --- |
| Optical-depth table | 128x64 RGBA16F | CPU rebuild for optical-radius changes | same |
| Isotropic fill | 32x32 RGBA16F | cached GPU rebuild for optical changes | same |
| Sky scattering + RGB transmission | two 192x108 RGBA16F | 192x108, 24 ray steps | 128x64 subregion, 12 steps |
| Aerial scattering + RGB transmission | two 32x32x32 RGBA16F | 32x32x32, 16 steps | 16x16x32 subregion, 8 steps |
| Regional fog endpoints | 9x9x2 RGBA16F | worker update | same |

Low quality retains maximum allocations but reduces active work and remaps lookup
coordinates to the updated subregions. View/light/weather changes invalidate view
LUTs. An unchanged paused view reuses them. The isotropic-fill table is a bounded
approximation, not a full production multiple-scattering solver. No temporal
reprojection or history rejection is introduced.

Single-scattering integration includes Rayleigh, aerosols, ozone, directional
solar illumination, lunar illumination and the reference planet's shadow. Samples
concentrate around the lowest point of each ray segment so thin fog is not skipped
when viewed from above. Detailed terrain does not cast volumetric light shafts.

The HDR scene is rendered before a shared RGB composite:
surface_color * RGB_transmission + scattered_radiance.
The scene is copied to an explicit sampled snapshot, and clear depth is handled
as sky. The old globe-specific view-path fog and celestial attenuation paths are
disabled in the physical route. Surface solar/lunar irradiance is evaluated at
the receiving point; camera altitude does not relight every surface as if it
were at the camera. Apparent celestial positions retain their display refraction;
ray integration uses straight segments and geometric planet shadowing.

Clouds receive the camera-to-cloud path before weighted OIT or sorted-alpha
blending, over the already fogged opaque background. Their opacity is retained.
OIT remains approximate for many overlapping layers. HDR exposure/tone mapping
occurs once in the final output; UI is composed afterward.

Weather requests follow the render view. The worker supplies local extinction,
100 m fog scale height, physical ground elevation and aerosol haze, with two UTC
hour endpoints. Fog potential is an explicit visual heuristic from model humidity,
temperature and surface wetness, not a measured relative-humidity/dew-point model.
Parameters interpolate in UTC, fade at regional boundaries and expire smoothly.
A clock/world reset discards old weather. There is no saved or frame-integrated fog.
The ground grid is broad: it does not resolve individual small valleys or air
moving around obstacles.

HDR/compute/RGBA16F-storage capability gates the physical route. The analytic
fallback fades sky scattering by optical height and restores celestial contrast
aloft. It does not provide the full physical aerial perspective. The fallback
does not create storage-output textures or compute pipelines.

Logical maximum LUT/staging/fog/uniform storage is approximately 1.71 MiB, plus a
scene snapshot of about 15.8 MiB at 1080p or 63.3 MiB at 4K. Optional timestamp
buffers add 512 bytes. Driver allocation overhead and resize overlap are excluded.

Commands:
- atmosphere status: active route, quality, physical optical altitude and memory.
- atmosphere debug off|transmission|scattering|optical_depth|distance|fog.
- terrain diagnostics bypass atmosphere composition; cloud overlays are suppressed
  during diagnostic views. Selecting one diagnostic family disables the other.

On adapters supporting timestamp queries, status reports median/p95 over up to
120 asynchronous samples. The measured scope is sky LUT + aerial LUT + RGB
composite, excluding the scene-copy operation, profile rebuild and additional
atmospheric lighting work in other passes. Readbacks are mapped after submission
and polled without waits. These values are not a complete GPU-frame benchmark.

Remaining acceptance, only in the user-started final verification phase:
- Ground-to-space ascent, camera separation from player and below-sea-level sites.
- Day, night, terminator and both twilight directions at different longitudes.
- Thin horizon, planet occultation and agreement between globe and local terrain.
- Near/far geometry, depth precision, cloud overlap and wet/foggy versus clear air.
- Render-origin changes, resize, quality changes, pauses, time jumps and reconnect.
- GPU frame-time delta and peak memory on the target adapter at 1080p and 4K.

Compilation, Naga WGSL/ABI validation, dependency checks, formatting and the source
line limit are implementation checks only. No game or final test phase was run.

Final static verification for this implementation passed: release client/server
build, workspace/all-target compilation, terminal-only compilation, all composed
WGSL shaders including the three compute variants, architecture, formatting,
399-file length check and diff whitespace. Runtime GPU resource creation, visual
acceptance and measured frame-time budgets remain unverified under the deferred
final verification policy. Existing worlds require no reset.


## Flight stutter and altitude command (2026-11-05)

Code inspection found synchronous render-mesh generation on every 20 m refresh,
unbounded dense-planet patch selection, and destruction of all resident sector
buffers whenever the 256 m render origin changed. These are confirmed code
paths, not measured GPU/CPU timings.

Render selection now refines the highest projected cell error while retaining
complete parent coverage, capped at 256 leaves. A 512-entry CPU cache retains
meshes; latest-request workers prepare typed vertices, with cancellation between
patches. Each interested world has its own worker so portal preparation cannot
overwrite the active world's request. Collision indices are cached for unchanged
sector interests. Planet collision sectors also materialize on a bounded worker
(up to 32 queued/running/completed jobs); the owner lane installs typed ticket
results. Until collision readiness, walking waits and fly retains its existing
noclip behavior. Authored nonplanet startup remains synchronous. This change does
not claim to remove all possible frame stalls.

The GPU keeps active geometry while uploading its replacement. Eight new patches
and a 4 MiB threshold apply across the entire frame, including repeated fixed-step
calls. An indivisible patch may cross that threshold. Superseded partial uploads
are pruned, bounding active/pending residency to their union. Floating-origin
hysteresis is 4 km; a compute pass adjusts resident position fields instead of
regenerating/uploading all meshes. CPU prepared data invalidates for a new origin;
planet-space texture anchors remain unchanged. Adapters without the compute path
retain rebuilds. A conservative near plane based on resident geometry/nearby
remote players improves depth precision while flying. Curtains retain smooth
ground normals, while geometric normals remain available for shadow projection.

The broad material/retention field now spans 2048 m (ground 32 m, tufts 4 m), so
distant terrain keeps broad color variation after microdetail fades. Unresolved
noise octaves skip their hash evaluation and converge to their mean. This adds
no flora, fauna, geometry displacement or new geographic source data. Coastline
resolution and cloud voxel shapes still require separate visual acceptance.

`fly on` followed by `set_altitude 10` requests 10 physical km above local
collision terrain at the same horizontal location; oceans use the water surface.
The server validates fly, current membership, transfer state, numeric bounds and
world-coordinate bounds, applies Earth relief compression, preserves movement
settings and resets velocity. Fractional kilometres work; 0..10000 is the command
range. Protocol v16 adds contextual WorldInput subtype 8 and AltitudeResult 0x91.
Both executables must be updated; worlds/saves do not need regeneration.

Acceptance is still pending: compare frame-time spikes during long fast flights,
origin crossings, high-altitude views, coastline/snow seams, teleport/loading,
portal prepare/commit/cancel and repeated altitude commands. Compilation and
static shader validation are not proof of visual quality or an FPS improvement.

Verification of this flight/altitude change: workspace/all-target and terminal-only
compilation, Naga validation of all assembled WGSL variants (including rebasing),
architecture dependency graph, formatting, whitespace and the 500-line check
passed (406 source files). Both release executables compiled successfully under
target/flight-check. The normal target/release executables were locked against
replacement by Windows; close the running client/server before the normal launcher
rebuild. No tests or game execution were performed; visual/performance acceptance
remains pending.

Bootstrap regression fix (2026-11-06): Renderer::new previously still required
its first ensure_scene call and GPU upload to finish synchronously. A pending
planet job therefore aborted startup with initial world sectors could not be
installed. Bootstrap now accepts pending collision/presentation, hides terrain
and disables walking until the normal frame readiness path completes. Earth
materialization also waits for prepared session geography before dispatch. No
world reset is required. Runtime acceptance remains deferred.

## Selective peak exaggeration and independent climate (2026-11-06)

Earth generation v5 optionally applies a bounded monotone height curve before
linear world compression. Source height `h` is measured in metres above the
configured sea datum. With threshold `a`, full-effect height `b`, maximum
multiplier `M` and exponent `p`, the mapping is:

```text
t = clamp((h - a) / (b - a), 0, 1)
w = (t * t * (3 - 2 * t)) ^ p
displayed height = linear height scale * h * (1 + (M - 1) * w)
```

Below `a`, height is unchanged; above `b`, the multiplier remains bounded by `M`.
The mapping is continuous with continuous first derivative at both thresholds.
Higher exponents concentrate the effect nearer the tallest peaks. It uses a
power of a smooth ramp, rather than an unbounded exponential. The existing
mountain-height cap still applies. Sea level and bathymetry remain unchanged.
Ground, collision, traversal, globe/FPP terrain and astronomical height anchors
consume the same world-owned mapping. There is no per-fragment curve evaluation.

The launcher exposes these new-world settings:

| Parameter | Default when peak settings are requested | Meaning |
| --- | --- | --- |
| `-PeakMultiplier` | 1.5 | Maximum gain, from 1 to 3. |
| `-PeakExponent` | 3 | Integer power from 1 to 8; larger values select higher peaks. |
| `-PeakStartMeters` | 1000 | Source height where the extra gain begins. |
| `-PeakFullMeters` | 9000 | Source height reaching the maximum gain. |

For these defaults and the Earth preset's linear scale of 0.1:

| Source height | Gain | Displayed height |
| --- | --- | --- |
| 1000 m | 1.0000 | 100.0 m |
| 3000 m | 1.0019 | 300.6 m |
| 5000 m | 1.0625 | 531.2 m |
| 7000 m | 1.3003 | 910.2 m |
| 9000 m | 1.5000 | 1350.0 m |

Plain launcher invocation preserves existing settings, and still creates a v4
world without exaggeration if no world exists. Explicit peak options create v5
worlds. Use a separate directory to retain the previous world:

```powershell
.\start-earth.ps1 -DataDir "$env:LOCALAPPDATA\Minimium\earth-peaks" -PeakMultiplier 1.5 -PeakExponent 3 -ClimateReliefInfluence 0
```

Resume that world with the same `-DataDir` and omit peak options. The immutable
resolved curve is saved in `world.planet.ron`; changing it requires a new directory
or explicit `-ResetWorld`, which preserves the previous directory as a backup.
Temporary launcher presets are removed after use. Revisions 3/4 retain the
original height arithmetic and default to a neutral curve.

`-ClimateReliefInfluence` is a separate finite value from 0 to 1. Zero (the
default) uses original source Earth elevations for lapse rate, terrain uplift,
cloud conditions and replayed snow/wetness. One uses exaggerated ground height
after undoing linear world compression; intermediate values blend the two.
Cloud clearance and fog placement always follow displayed geometry. The base
climate remains a latitude/season/moisture approximation, not a measured Earth
climatology. This parameter controls added peak exaggeration, not the planet
radius or the linear compression itself.

The climate value can be changed on an existing world at restart, without reset:

```powershell
.\start-earth.ps1 -ClimateReliefInfluence 0
```

Server startup `--climate-relief-influence` validates and atomically stores it in
`world.climate.ron`, retaining other scenario settings. Omission retains the saved
value; old scenario files default to zero. A launcher requesting a change rejects
attachment to a running server rather than silently ignoring that request. Close
the previous session first. No world directory was reset during implementation.

Terrain vertices now contain both source and exaggerated heights (112 bytes).
The shader blends them using the same climate value; local and global history
grids carry effective climate elevation for lapse-rate correction. Shader snow
previously took the maximum of replayed snow and an instantaneous cold/humidity
mask. That second path could create snow without modeled precipitation history;
it has been removed. Snow now comes from replayed weather or mapped permanent
snow/ice, with a temperature-dependent melt mask. This is a code-level finding,
not a measurement of how much of the planet will be snowy after the change.

Protocol v17 transports all four curve parameters and the climate influence.
Both client and server must be rebuilt. Snapshot format remains v9: generation
v5 planet records append the four curve fields; earlier records remain unchanged.
`terrain status` reports the saved curve and the renderer's climate influence.

Implementation verification: release client/server compilation under
`target/flight-check`, workspace/all-target and terminal-only compilation, Naga
validation of composed WGSL, architecture dependencies, PowerShell syntax,
formatting, whitespace and the 500-line limit. No tests or game runs were made.
Final visual/runtime acceptance remains pending: inspect source-height thresholds,
coasts and LOD continuity, compare influence 0/0.5/1 at the same UTC and location,
check cloud clearance/fog, and verify saved settings and identical cover after
restart. Compilation alone does not establish realistic snow coverage.

Startup diagnostics follow-up (2026-11-06): a reported local-server exit code 1
was displayed without stderr, hiding the actionable reason. Bootstrap now drains
and mirrors stderr with an 8 KiB tail, reporting the final server error in the
client connection failure. The screenshot alone does not establish the server
loading failure's cause. Existing saves are untouched; runtime reproduction and
visual acceptance remain pending.

Startup failure identified (2026-11-06): the reported 'cannot parse float from
empty string' came from forwarding an empty optional climate-relief environment
entry as an explicit CLI number. The launcher now removes omitted entries; the
bootstrap filters empty optional values consistently for attachment and spawning,
including planet config and seed. Explicit zero is retained. Server parse errors
now identify the offending option. No save repair or world reset is required.
Validation covers release compilation, desktop/terminal compilation, PowerShell
syntax, formatting, architecture and source limits; runtime acceptance is pending.

## Overview coastline detail regression (2026-11-06)

The previous globe sampled land/water into a dense texture independently of its
mesh. The shared globe/FPP path instead intersects signed ground with the ocean.
That made coastline fidelity depend on mesh sampling. Inspection found the same
256-leaf budget as FPP, no overview frustum rejection, and coast classification
using only the patch centre and four corners. Small islands and inlets could
therefore be skipped even when available in the underlying GEBCO source.

Overview now uses conservative patch/frustum intersections with a 20% guard band
and at most 512 complete leaves. A bounded 2048-entry cache probes the 17x17
render-block source lattice, stopping as soon as both land and ocean are found.
World/manifest changes clear the derived cache. Land, coast and ocean priorities
are 2, 8 and 1 respectively: the existing 8-pixel cell target becomes 4 pixels
inland and 1 pixel on coasts, subject to available budget and mesh resolution.
FPP retains its 256-leaf cap and existing cell targets. Both cameras still use
the same geography, ground/ocean geometry, materials and climate.

The CPU source mesh cache now permits 1024 entries under its existing 128 MiB
eviction threshold, allowing previous and next overview selections to coexist.
Ocean vertices are at most 18 MiB for 512 fully populated leaves, before overlap
with the previous selection. Preparation remains off the frame lane; GPU staging
still admits eight patches and a 4 MiB threshold per frame. An increased geometry
budget is a bounded cost increase, not a measured claim of unchanged frame time.

The 16-cell render-block constant is shared by generation, probes, selection and
edge conformance. No shader, collision, save, protocol or geographic asset change
is required, and worlds need no reset. Small features below raster/mesh sampling,
including islands between all probes, remain limited by source resolution and
the available LOD budget. Inland water bodies still need their own representation.

Implementation verification: workspace/all-target and terminal compilation,
release client/server build, architecture dependency checks, formatting and the
500-line source limit. No tests or game runs were performed. Final acceptance is
pending for Denmark's islands, Norwegian fjords, the Baltic archipelagos, cube-face
and LOD seams, aspect-ratio/zoom changes, rapid globe rotation, FPP transitions,
peak memory and measured frame times during preparation and upload.


## Terrain shadow coverage and night readability (2026-11-06)

Source inspection identified an explicit overview shadow disable, a 128 m
player-centred receiver region, a hard-coded 514 m projection, and night ambient
far above lunar irradiance. Physical lunar lighting also used an apparent
direction while shadow projection used the geometric direction.

The shared shadow region now follows the render view, retains near-ground contact
detail, and fits the reference-shell footprint during flight/overview. Resolution
remains 1024x1024 for one dominant source. F64 world-space texel anchoring and
quantized extents reduce projection movement; the prior geometric-plane PCF
correction is retained. The previous overview frustum rejection is superseded
by coarse off-screen caster retention within the unchanged 512-leaf total budget.
FPP keeps 256 leaves and a coarse raised-horizon caster halo. No extra geometry
worker, texture, readback or pass is introduced.

Sun/Moon selection follows the viewed ground. Physical lunar light and shadows
share a direction. Reduced daytime fill, weak phase-dependent night fill and a
bounded landform cavity approximation improve the direct/indirect balance.
A deterministic view exposure curve spans 0..15 stops between solar altitudes
-6..-18 degrees, constrained by the brightest sampled viewed location. It is
not temporal automatic exposure or photometric validation. Lunar irradiance
remains unchanged. HDR radiance is exposed before scene/sky/aerial RGBA16F writes;
LUT readers and final output account for that once, avoiding low-light underflow
or double exposure. Authored celestial sprite colors and diagnostics retain
display calibration. Analytic fallback has no new night adaptation.

Acceptance remains deferred: compare terrain debug shadow and normals at low Sun,
high flight and globe zoom; inspect full/crescent Moon and moonless darkness,
night/day transitions, ocean/coast shadow edges, origin rebasing, static-camera
stability, LOD/caster changes and frame-time/memory measurements. One map has
limited distant/contact resolution; only one source casts, coarse off-screen
ridges can lose detail, cavity shading is not full sky occlusion, and clouds do
not cast shadows. Compilation is not evidence of final image quality.

Implementation checks: workspace/all-target and terminal-only compilation,
composed WGSL validation, architecture dependency checks, formatting and changed
file lengths. No tests or game runs. Repository-wide size audit also found an
existing unrelated 572-line server runtime/admission.rs; this change does not
modify that file or claim the whole repository meets the size limit.

Release client/server compilation also passed in target/flight-check. The user's
running target/release client/server were left running; start-earth.ps1 rebuilds
the normal release path on the next launch after those processes exit.
