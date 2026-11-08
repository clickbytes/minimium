# minimium-renderer

## Purpose / Non-goals

Owns presentation contracts, reusable rendering primitives and pass encoding: camera/frame values, geometry conversion, visibility and LOD selection, visual lighting, quality profiles, WGSL sources, GPU resources and pipelines. The client still owns window lifecycle, frame scheduling, scene visibility decisions, shadow caster selection and some GPU resources. This crate does not own sessions, simulation state, or a window.

## Public API

Atmosphere presentation is shared from ground to orbit. `atmosphere_view` prepares
camera/scale values in f64 and uploads a 256-byte checked uniform; `atmosphere_resources`
owns sampled and staged LUTs, and `AtmosphereGpu` computes and composes them in
linear HDR. Frame group zero shares optics with terrain, authored objects, clouds
and celestial pipelines. Globe shading no longer applies its own view-path fog.
The RGB composition uses an explicit scene snapshot, never simultaneous sampling
and rendering to the same texture. HDR resources require compute and RGBA16F
storage support; the analytic fallback still fades with optical altitude.

Standard view LUTs are 192x108 sky scattering/transmission and 32x32x32 aerial
scattering/transmission; low quality updates 128x64 and 16x16x32 subregions with
fewer integration steps while retaining the bounded maximum allocations. The
128x64 optical-depth LUT is regenerated only for a new optical radius. A 32x32
isotropic-fill approximation is cached; it is not a full Hillaire multiple-scattering
solver. View/light/weather changes invalidate the view LUTs; unchanged paused
views reuse them. Separate storage staging textures avoid read/write aliasing.

Regional fog uses two 9x9 layers with extinction, scale height, source ground
height and aerosol haze. It is a weather heuristic, not dew-point microphysics.
Ray integration concentrates samples near the lowest part of each segment and
clips the reference planet. Optical heights, sea level and origin-relative geometry
are distinct; below-sea-level depressions retain air. Surface and atmospheric radiance receive exposure before half-float storage;
the final output applies tone mapping once. Clouds receive the camera-to-cloud path before OIT,
after the opaque scene has received its own path.

`atmosphere_debug::AtmosphereDebug` is CPU-only. `AtmosphereGpu::status` reports
logical allocations and optional timestamp-query median/p95 over up to 120 samples.
Readbacks are asynchronous and polled without waiting. Timings cover sky/aerial
compute and RGB composition, excluding scene-copy, profile rebuild and atmospheric
lighting work inside other scene passes; they are not a whole-frame benchmark.
LUTs, staging, fog and uniforms use approximately 1.71 MiB in the physical path;
the scene snapshot adds about 15.8 MiB at 1080p or 63.3 MiB at 4K.
Runtime image/performance acceptance is pending.

TerrainVertex (112 bytes) and TerrainGpu form the shared planet surface path
for overview and FPP. Both cameras use TerrainView, planet_visibility and the
same shader, material means and climate history. The separate globe texture,
palette, detail sphere and resources have been retired.

Static landform/coverage comes from procgen. Continuous planet-space GPU masks
use retention, climate, slope and seeded noise to perturb substrate boundaries.
All six roles contribute mean color; two dominant roles add filtered detail and
microheight mixing with continuous pair-selection gates. Material/normal detail
uses screen footprint, not a fixed distance cutoff. Residency remains 2K with
mipmaps, 4x anisotropy and at most 18 material texture reads per pixel.
TerrainDebug includes habitat and source diagnostics in addition to color,
normal, depth, shadow and coverage views.

SurfaceAtlas prepares camera-independent 65x33 global history; CoverGrid prepares
anchored 9x9 local coverage and a separate tangent fog grid. Both terrain history
textures have four layers: cover/climate at two UTC-hour endpoints.
TerrainCoverGpu owns their 71,232 bytes of GPU storage and upload lifetime;
no history calculation occurs on the rendering thread. Local/global blending
uses the same domain model and corrects temperature for source elevation.
Small regions, snow lines and coarse/fine agreement still need visual acceptance.

EdgeConformance moves fine edges onto coarse neighbors and removes radial
curtains from the dense presentation path. Its cached copies are independent
of authoritative geometry. Workers prepare matching surface/shadow vertices.
The prepared_upload module stages new buffers until a complete selection is
ready, including replacements for active keys. Floating-origin rebasing covers
active and staged geometry.

- `RenderViewInput` and `RenderViewFrame` carry camera matrices, viewport and world-space presentation metadata. `RenderViewFrame::from_camera` rejects non-finite values and clamps empty viewports to one pixel.
- `RenderFrame` carries the prepared view, camera/environment uniforms, adapted `ClientFrameState`, halo count and UI vertices for one frame. It owns CPU values only; `upload` copies uniforms and UI vertices into caller-owned buffers; the client returns the reusable allocation to its console after submission.
- `quality::{SkyQuality, SkyQualityProfile, SkyQualityState}` defines renderer quality choices and shadow parameters.
- `geometry::Vertex`, its layout, and `sector_geometry`, `sector_geometry_relative` and `cube` convert world geometry to render data.
- `scene_encoder::{MainScenePass, EnvironmentDrawFlags, encode_main_scene_pass}` encodes the scene pass from renderer GPU handles and app-prepared visibility decisions. `frame_encoder::encode_frame` orders shadow, scene, atmosphere, cloud, UI and output passes; `encode_final_passes` is also available independently.
- `surface_frame::{acquire, SurfaceAcquire, SurfaceFrame}` wraps surface image acquisition, recoverable suboptimal configuration and queue-submit/present. The app still decides when to schedule a frame and handles window events.
- `CloudGpu::encode`, `AtmosphereGpu::encode` and `OverviewGpuBuffers` own cloud/atmosphere pass encoding and overview vertex-buffer lifecycle. `WorldGeometryGpu`, `ShadowGpu`, `DynamicVertexBuffer`, `StaticVertexBuffer` and `UiGpuResources` own extracted GPU resource groups and upload behavior.
- `shadow_uniform::prepare`, `ShadowSettings`, `shadow_target`, `shadow_geometry` and `ShadowGpu` prepare shadow data and own shared shadow resources.
- `AtmosphereUniform`, `AtmosphereGpu`, `atmosphere::{Optics, EARTH}`, `atmosphere_table` and `atmosphere_pipeline` define clear-air optics and its LUT/GPU resources.
- `EnvironmentPipelines`, `gpu_frame`, `gpu_bindings`, `CelestialHaloGpu`, `MoonGpu`, `MoonSphereGpu`, `MilkyWayGpu` and `StarGpu` provide environment presentation resources and pipelines.
- `OutputEncoder` owns the linear scene target and final sRGB encode pipeline. Its dimensions are plain pixels, with no window dependency.
- `planet_visibility::render_patches_for_view` returns visible terrain patches and LOD levels for the client presentation worker, using a `TerrainView`, collision cache and optional prepared geography. The old `TerrainView::legacy` constructor and duplicate patch-generation wrappers have been removed.
- `profiles::LightingProfile`, `lighting` and `moon_lighting` provide visual lighting values and calculations. `shader_source` composes WGSL fragments; `shaders::SHADOW_WGSL` exposes the standalone shadow pass.

## Dependencies

The default `gpu` feature gates rendering modules and GPU/astronomy dependencies.
With default features disabled, camera metadata and quality/debug selections are
CPU-only contracts usable by the terminal command frontend.

Uses `glam` for camera and geometry calculations; `bytemuck` and `half` for GPU layouts and optical LUT values; `wgpu` for rendering resources; `pollster` for bounded GPU error-scope resolution; `minimium-assets` for the embedded star catalogue; `minimium-procgen` and `minimium-world` for manifests, geography and generated patches; and public astronomy/environment observations for visual lighting. It has no window, client-runtime, session, protocol, storage or transport dependency.

## Data flow

The client runtime publishes an immutable, GPU-independent `ClientView`; the app adapts its world, position, health and readiness values into `RenderFrame::client` alongside camera, environment and console data. The client prepares an immutable `PreparedShadowFrame` with context, world revision, caster keys, uniform and CPU map vertices. GPU submission validates that identity, uploads the complete map-caster batch, then writes the matching uniform and encodes the shadow pass only when publication is ready. Renderer encoders consume these values and their owned handles for shadow, scene, cloud, atmosphere, UI and output passes. The client schedules frames and handles window events; renderer `surface_frame` wraps image acquisition and submission/presentation.

## Ownership and execution

Frames, visibility and prepared shadow results are value data owned by the caller. Quality profiles are immutable values selected by the application. Geometry conversion and visibility selection are synchronous deterministic transformations without I/O. GPU resources remain split across the crate and client: renderer owns extracted pipelines, pass encoders, surface-frame operations, overview buffers, world geometry, shadow, UI, cloud and atmosphere resources, while the application owns the surface handle, remaining buffers and update/scheduling state. CPU shadow caster geometry is prepared outside `GpuResources`; GPU cache metadata is committed only after the bounded upload succeeds. The client retains the world-clock star epoch and supplies immutable instances; render encoders consume explicit references and flags. `UiGpuResources` keeps its pipeline, bind group and buffer private; callers can upload through `RenderFrame` and use narrow accessors to encode the UI pass.

## Invariants and errors

Shadow PCF loads nine integer texels and corrects receiver depth at the actual
texel centers. The previous correction at unsnapped UVs left a periodic depth
error inside each nearest-filtered texel. Material sampling propagates the warp
Jacobian into explicit texture gradients and tangent normals, without additional
texture reads. These corrections still require visual acceptance in the game.

Nine shadow taps now have continuous tent weights rather than equal weights that
jump when the kernel moves by one texel. Triplanar projection culling also fades
to zero before skipping a plane. UV warp separates high/local coordinates before
evaluating lattice fractions, preserving local precision on large planets.

Shadow PCF taps adjust comparison depth analytically from the geometric triangle
plane and the orthographic light-projection rows. Terrain carries a separate
flat geometric normal derived from the exact origin-relative f32 vertex positions;
the smooth sampled normal remains for lighting. The 112-byte vertex includes 12
extra bytes for that plane normal. Differencing projected screen depths caused
precision loss on small nearby triangles; it is no longer used for correction.
This still needs visual confirmation against the reported shadow diagnostic.

Terrain repeat sampling uses 4x anisotropic filtering (linear min/mag/mip).
Normal-map contribution also fades as its texel footprint becomes unresolved,
and screen-space normal variance broadens specular highlights. Ambient terrain
lighting uses a hemisphere factor and a weak broad reflection from the existing
sky palette, including a ground-facing fallback. This is an inexpensive analytic
environment approximation, not captured scene reflections or visibility-aware IBL.
Macro pigment variation is subtle on snow, stronger on bare ground, with restrained
warm/cool mineral tint on sand/soil. Existing textures and render passes are reused;
anisotropy can increase physical filtering work despite unchanged shader read counts.

Invalid matrices, origins, eyes, or negative/non-finite planet radii return `None`. Zero viewport dimensions normalize to one. Coarse planet traversal caps visited sectors at 512; dense planet subdivision retains its existing horizon and subdivision bounds.

## Commands

This crate exports no commands.

## Change guide

Change frame metadata, camera preparation, geometry conversion, quality profiles, visible patch selection, visual lighting, atmosphere/star preparation, extracted GPU resource owners, pass encoding, surface-frame operations and WGSL composition here. Window startup/scheduling, visibility/caster selection and remaining client-coupled GPU resources stay in the application while extraction continues.

## Verification

Run `rtk cargo check -p minimium-renderer -p minimium-client` and `rtk python scripts/check_architecture.py`. Runtime visual acceptance is deferred.


Flight presentation uses a complete, priority-refined quadtree capped at 256
leaves; overview uses up to 512 leaves within a guarded camera frustum. The highest projected cell error is refined first, with no truncation of
the selected coverage. A 1024-entry / 128 MiB eviction-threshold CPU source mesh cache and worker-prepared
vertices avoid regeneration on each 20 m refresh. The application owns worker
threads; renderer preparation owns no threads, transport or filesystem effects.
GPU uploads are limited across the frame to eight patches and a 4 MiB threshold
(one indivisible patch can cross the byte threshold). Active plus pending GPU
geometry is bounded by their union; obsolete partial uploads are discarded.
The renderer publishes one complete sector-key set with a stamp containing world,
content, geometry-publication and floating-origin revisions. Receiver hints,
shadow caster selection and map upload validate that same stamp; pending LOD
uploads leave the prior publication active until replacement buffers are ready.
Rebase advances the origin generation and invalidates dependent shadow data until
remote casters and the map are ready for the new publication.

On compute-capable rendering, floating-origin shifts update the position fields
of resident vertex/storage buffers. Anchored material coordinates, normals and
world-space bounds stay unchanged. Unsupported adapters retain buffer rebuilds.
Dense presentation boundaries conform to coarser neighbors; radial curtains
are removed before surface/shadow preparation. The client moves the near plane conservatively away from 5 cm
when flying clear of resident geometry, improving distant depth precision.
Visual seam, depth stability and frame-time acceptance remain pending.

## Sea-level ocean (2026-11-06)

Earth ocean is a separate opaque sphere mesh, approximated with the selected
ground lattice. Both shells share LOD selection and canonical edge fractions;
FPP coastal leaves receive twice the refinement priority within its 256-leaf cap.
Overview prioritizes coasts 8x and inland terrain 2x within a 512-leaf cap.
Ground retains signed bathymetry, and depth intersection defines the coast.
Binary water flags no longer blend water into terrain materials. A narrow damp
band uses signed shore elevation. Procedural worlds retain their prior dry surface.

Ocean vertices contain only position and radial normal (24 bytes). The shader
uses existing lighting/atmosphere/shadow resources, a broad Fresnel sky reflection
and no material texture reads. No new texture asset, wave simulation, scene
capture or transparent-water pass is added. Fully submerged ground triangles
are excluded before material/shadow preparation; coastal triangles remain.
Sea and ground buffers stage/publish together and rebase with strides 6/28 floats.
At most 512 ocean triangles per dense leaf add up to 9 MiB of vertex storage
per complete 256-leaf FPP selection or 18 MiB for 512 overview leaves, before
active/pending overlap. Existing frame
upload limits include these bytes; extra sea draws are counted in telemetry.

The sphere is tessellated, not an analytic infinitely smooth surface. Small
coastal features remain limited by geography resolution and available mesh LOD.
This is surface water only: no underwater rendering, tides, rivers or elevated
lakes. Visual coast/seam/flicker and measured performance acceptance are pending.

Terrain surface attributes now carry source and exaggerated elevation separately.
A uniform climate-relief influence blends them consistently with the environment
model, including fallback and local/global history temperature corrections.
History stores effective climate elevation. No curve evaluation occurs per pixel.
Snow uses replayed precipitation or mapped permanent ice; cold/humidity alone no
longer generates an instant blanket of snow. Visual snow-line acceptance is pending.

Overview shoreline detail (2026-11-06): TerrainView::overview carries side planes
with a 20% guard band and uses the same terrain/ocean generation as FPP. Selection
prioritizes conservative patch bounds inside that frustum for refinement. A coarse
off-screen halo is retained for terrain shadows within the same leaf budget. A
2048-entry coast-probe cache classifies source height on the complete 17x17 block
lattice, stopping early when land and water are both found. Coast, inland and
ocean priorities aim for 1, 4 and 8 pixel cells respectively, subject to the leaf
budget and finest supported lattice; these are targets, not guaranteed error bounds.
FPP keeps its existing 8/4 pixel targets and 256-leaf maximum.

The probe is derived presentation data, invalidated with the source mesh cache
on world/manifest changes. No source data, save, generator behavior or shader
changes are introduced. CPU sampling stays on the host worker; the 8-patch/4 MiB
per-frame GPU upload thresholds remain unchanged. More overview geometry can
cost additional preparation time, memory and draws; performance and shoreline
acceptance are pending. Islands between all probes and features below GEBCO or
mesh resolution can still disappear. There is no separate painted globe mask.

## View-dependent terrain shadows and night lighting (2026-11-06)

ShadowRegion fits one directional map to a 5x5 camera/reference-shell footprint,
including the planet limb when rays miss. Near resident ground it retains a
128 m contact region; flight and planetary overview expand/retarget that region.
Projection width/depth follow its settings instead of fixed 514/768 m dimensions.
Extent quantization and shrink hysteresis limit rescaling; absolute f64 light-space
texel snapping survives floating-origin changes. PCF still uses nine tent-weighted
taps and the exact geometric receiver plane. Standard remains one 1024-square
depth target; Low still disables shadows. No additional shadow texture/pass is added.

The quadtree retains coarse off-screen and raised-horizon casters without refining
them, under the existing 256/512-leaf budgets. This can trade some visible detail
for caster coverage. Client selection uses complete active buffers, never stale
overlapping LODs. Distant shadows are limited by the map texel size and coarse
caster geometry; this is not cascaded/full-resolution planet shadow coverage.
Refinement replaces a parent only when all four child patches survive culling;
otherwise the parent remains. This preserves complete globe coverage near the
frustum and horizon boundaries.

Physical surface lighting and its lunar shadow now use the same geometric
camera-to-Moon direction. Source choice considers the viewed region and actual
lunar phase, with the existing source hysteresis. Only the dominant source casts.
Day ambient is reduced; night fill is weak airglow plus phase/horizon-dependent
moon scatter. Reflections use local illumination rather than the observer's sky
palette across the whole globe. Cached landform concavity reduces indirect light
by at most 35%; this is a cavity approximation, not measured horizon AO or GI.

Physical HDR exposure varies continuously with the brightest sampled solar
altitude in the view: no night gain above -6 degrees and at most 15 stops by
-18 degrees. This is deterministic view metering, not temporal eye adaptation.
Moon irradiance is unchanged. Scene and sky/aerial LUT radiance are pre-exposed
before RGBA16F storage to preserve low light precision, with matching cache keys.
Clouds decode LUT radiance before lighting; final composition applies exposure
exactly once. Terrain/atmosphere diagnostics and existing display-calibrated
celestial sprites/markers retain their own display scale. Analytic fallback
keeps its prior exposure policy. No histogram, readback, new asset or GPU target.

Static compilation and WGSL validation are separate from visual acceptance.
Still pending: low-Sun ridges/valleys, full/crescent Moon and moonless nights,
day/night terminator in overview, altitude/zoom transitions, origin shifts,
PCF acne/contact loss, halo LOD seams and measured CPU/GPU frame times.
Clouds remain receivers only; there is no cloud shadow map.

## Atmospheric bolids

milky_way_gpu::BolidEvolution caches observer-independent emission history from
v19 event endpoints, duration, kind and seed. Six material families cover smooth
ablation, erosion, a dominant breakup, cascades, intermittent layered ablation
and terminal fragmentation. Strength/damage, exposed area, velocity and remaining
mass drive emission. Wire duration sets initial velocity, not a fade deadline.
Twelve solid-body slots and eight grain packets bound fragmentation and erosion.
Children inherit momentum and can fail again; seeded transverse breakup
directions span both axes while the recoil preserves total momentum; unresolved grains remain in mass
accounting. This is a visual entry model, not plasma chemistry or impact physics.
Cooling gas contracts after a flare; there is no inward material implosion.

Histories contain at most 1501 snapshots at 40 ms spacing, each advanced by four
10 ms steps. EvolutionDiagnostics reports mass accounting, fracture/grain counts,
emission episodes and a still-luminous integration-cap hit. Natural extinction
normally ends integration earlier. Physics uses physical metres, converting
positions, velocities, widths and wind to the world's Earth relief scale.

The GPU event frame holds at most eight events in 14608 bytes, below the portable
16 KiB uniform limit. Twenty emitters have compact cores, gas sheaths, temperature
tint and short wakes. Twenty-four adaptive train nodes retain flares and gaps.
Fresh emission, cooling gas and rare persistent emission have separate decay.
Persistent emission favors higher atmospheric deposition and loses contrast in
daylight. Deterministic height-dependent wind deforms the train; expansion lowers
line brightness. Its slower emission reservoir is calibrated separately from
fresh plasma so expansion does not prematurely erase the persistent branch.
Physical bounds cover the corrugated train and expanded dust. The shader adds
the minimum angular/pixel support, preventing distant filtered halos from being
culled by a smaller physical sphere.
Dust derives from deposited matter, expands, dilutes and fades;
it scatters daylight rather than glowing on a black night sky. HDR and fallback
use solar irradiance at the dust position, including the elevated horizon and
light/view atmospheric transmission, instead of a fixed observer-daylight tint. The AMS fireball
FAQ distinguishes these luminous trains from non-luminous particulate trails:
https://www.amsmeteors.org/fireballs/faqf/

The strongest event at the camera supplies one omnidirectional point light to
terrain, objects, ocean and clouds. Its position is the power-weighted fragment
centroid. Inverse-square strength uses physical distance through scale-squared
conversion and planet occlusion. Sky emission, lighting and shadows share one
prepared frame. Strong night flashes can use the existing bounded perspective
shadow map; only shadow coverage is bounded, not light direction. Clouds receive
the point light but do not cast terrain shadows. Full volumetric plasma, spectral
line synthesis and delayed sound are outside this reduced model.

Emission and dust use a finite-distance layer after atmospheric composition,
with scene-depth and planet occlusion. A second cloud-face draw accumulates only
foreground opacity and foreground radiance. Resolve preserves those clouds while
attenuating the bolid and obscuring its background with dust. Overlapping sources
use radiance-weighted distance and the existing weighted-cloud approximation,
not exact transparent-fragment sorting. Four textures use 28 bytes/pixel with a
64 MiB cap; only their resolution degrades above that cap. Memory is accounted
in client GPU diagnostics. No active bolid means no extra bolid/cloud-mask draws.

Verification to date: client/server compilation, all 23 WGSL variants (including
cloud-mask binding visibility), architecture and the 500-line check passed.
start-earth.ps1 launched release in an isolated target directory and diagnostic
world/profile. Manual night captures showed concurrent compact heads, long
trails and terrain/cloud illumination. An above-atmosphere view at set_altitude
500 showed the streak in front of the planet. Seeded diagnostics included all
six families, multiple intermittent episodes, cascades and consistent mass sums
without a still-luminous cap hit. This is not a controlled performance benchmark.

Two nearby clients, physical HDR and forced fallback, received the same event
and produced identical histories and almost identical projected tracks. Shared
day/night captures showed the same flight and a night flash lighting both scenes.
Their baseline atmosphere appearance differs; whole-scene visual parity is not
claimed. A daylight replay of dust seed 279 at ages 5 and 30 seconds showed a
fading widened trace after the head vanished. A second client transferred through
the Earth portal to the dungeon; subsequent Earth emission did not appear there.

A release replay of persistent Cascade seed 327825 at 30, 60 and 85 seconds
showed a gradually dimming, widening greenish train after the head vanished;
at 92 seconds the trace had disappeared. This checks the revised reservoir
calibration and final fade for that event, not all seeds.

Remaining visual acceptance: full cloud coverage, foreground/background cloud
ordering from orbit, twilight dust, persistent trains across other trajectories, moonlit cases and
clear visual distinction of secondary fragment wakes. Manual-path framing now
uses a bounded endpoint search on the server. Release captures at the minimum
30-degree FOV kept the flight in view from the ground, 70 km and 500 km altitude.
The orbital flash exposed striped terrain/ocean intersections even with shadows
disabled. Removing the 10 m ceiling from the clearance-based near plane removed
the broad stripes in release captures at 500 km; the plane still uses one percent
of the nearest resident bounds distance, capped further by nearby players.
Fine shoreline tessellation/precision irregularities remain visible.
The two-axis breakup and expanded culling support compile and run in release;
screenshots alone do not yet prove distinct wakes.
Static checks and the
observed cases do not establish completion of these remaining scenarios.

A controlled moonlit Burst replay (seed 590085, kind 2) selected the bolid
shadow source during its flash and restored the Moon afterward, with 29 casters
and complete coverage. Comparing standard and disabled shadows exposed receiver
striping in the point projection. Perspective receiver-plane depth gradients now
correct individual PCF texel comparisons. Release captures removed the regular
receiver stripes, with residual fine artifacts at grazing angles. The point-map
cone now fits the receiver sphere while its depth interval retains the caster
sphere; this avoids spending map texels outside the visible shadow region.
Release capture target/bolid-point-receiver-cone.png confirms the narrower cone
runs with the flash, but fine grazing-angle receiver artifacts remain unresolved.
Before/after captures are
in target/bolid-point-shadow-{standard,disabled}.png and
target/bolid-point-gradient-standard.png.

Floating-origin terrain rebasing now updates each triangle and its geometric
normal in one compute invocation. The normal therefore describes the rounded
positions actually rasterized after a shift, rather than the pre-shift plane.
Smooth lighting normals and high/low material coordinates remain unchanged.
WGSL validation covers the rebase entry point as well as world, terrain and ocean.
Release runtime exercised origin generations 2, 3 and 4 during an altitude
excursion (80 km observed) and return to 152 m physical elevation. Shadow caster
coverage remained complete without GPU validation errors. This removes a source
of receiver-plane disagreement; its contribution to the remaining grazing-angle
artifacts has not yet been isolated in runtime captures.

A bright Burst capture after that round trip still shows fine receiver artifacts
(target/bolid-after-origin-roundtrip.png), so stale rebased normals were not the
sole cause. The point-shadow grazing-angle investigation remains open.

A bounded subpixel receiver-depth tolerance did not visibly remove the remaining
artifacts and was discarded (target/bolid-subpixel-bias-diagnostic.png). Dense
planet edge conformance already removes radial curtains, so their shading is not
a sufficient explanation. Terrain shadow debug now reports bolid visibility when
source 3 is active, rather than showing the default directional-light value.

A release replay at 52 N, 20 E reproduced fine receiver patterns on lowland
terrain (target/bolid-point-lowland.png), so they are not specific to the Bergen
mesh. The updated shadow debug was exercised with source=bolid. Manual requests
now always choose bright class 2 on the server; a release spawn confirmed it.

Fixed-age local replay now permits matched shadow comparisons. The class-2 Burst
at 1.2 seconds reproduced receiver patterns with standard shadows and a smooth
receiver without them (target/bolid-frozen-shadow-{standard,low}.png). A vertex-projected light-clip varying was also tried in release and did not
remove these patterns (target/bolid-vertex-projection-diagnostic.png); that
experimental change was discarded. The fixed-age diagnostic remains available
for further matched comparisons.

Screen-derivative receiver normals were tried in release and discarded: the
pattern persisted and added noise (target/bolid-raster-normal-diagnostic.png).
Point shadows now use a minimum half-texel receiver separation, capped by the
existing maximum metre bias, to bound PCF comparisons across adjacent facets.
The release build and WGSL validation pass; target/bolid-point-texel-bias.png
still shows residual patterns, so final shadow acceptance remains open.

Direct screen-space depth derivatives, both from fragment-projected positions
and an interpolated vertex light-clip varying, were evaluated in release. They
did not eliminate receiver artifacts and introduced fine noise; both experiments
were discarded. Captures: target/bolid-screen-gradient.png and
target/bolid-screen-vertex-gradient.png. Incoming bolid irradiance now also integrates the finite clear-air path to each
receiver using eight fixed samples concentrated at the lowest altitude. It shares
optical scale, Rayleigh/aerosol coefficients and ozone profile with the atmosphere;
HDR and fallback use the same calculation without requiring a LUT. Planet
occlusion remains separate. This accounts for extinction rather than adding
volumetric light scattering or cloud-cast terrain shadows. Runtime acceptance
of the new attenuation is pending.

Dominant bolid selection and the point-shadow activation threshold now use
received luminance with planet occlusion and the same eight-sample incoming
clear-air extinction as the shader. The original source power/color remain
unattenuated in the GPU upload, so each surface computes its own path exactly
once. Atmosphere upload precedes shadow preparation to avoid a stale optical
profile after a world/view change. Runtime visual acceptance remains pending.

Release verification of the incoming-extinction change: fixed Burst seed 590085
at 1.03 s selected the bolid with 39 complete casters; at 4 s the source returned
to the Moon. Capture target/bolid-incoming-extinction.png shows the compact
head/trail and illuminated terrain. Residual facet-shaped shadow artifacts
remain visible; this does not establish final shadow or atmospheric accuracy.
Client/server release build, 23 shader variants, architecture and file-length
checks passed. No automated tests were run.

Point-shadow receiver displacement now follows the geometric normal oriented
toward the source, bounded by the existing half-texel/minimum and maximum
world-space bias. Both UV and depth are projected from that displaced receiver;
the perspective plane gradient is evaluated there too. Moving only along the
light ray had left facet-shaped artifacts on front-facing lowland terrain.
A release Burst replay at 52 N, 20 E (seed 590085, age 0.99 s, confirmed
shadow_source=bolid) removed that pattern in material and visibility views:
target/bolid-normal-offset-active.png and bolid-normal-offset-visibility.png.
The earlier normal-offset-ground capture used lunar shadows and is not evidence
for this fix. Contact-shadow separation and steep-terrain acceptance remain open.
The diagnostic shadow view now marks back-facing point-light facets red.
All 23 WGSL variants, release compilation and the 500-line check passed.

A second release replay at Bergen (60.392989 N, 5.324150 E), Burst age
1.15 s with source=bolid and complete coverage, still shows patterns on steep
terrain. Red diagnostic facets confirm some genuine back-facing geometry,
while front-facing facets retain residual striping. Captures:
target/bolid-normal-offset-bergen.png and
target/bolid-normal-offset-bergen-visibility.png. The lowland improvement is
therefore retained without claiming general shadow acceptance.

Cloud bolid lighting now uses a normalized Henyey-Greenstein lobe (g=0.65)
blended toward isotropic scattering as the existing optical depth increases.
A face-orientation entry-depth approximation attenuates the incoming beam to
the boundary, allowing thin cloud undersides to transmit a flash without a
constant night fill. Both normal cloud composition and the foreground bolid
mask call the same radiance function. The 0.85 albedo and boundary approximation
are visual model parameters, not a measured cloud microphysics fit; there is
no volume ray march or cloud-cast terrain shadow. Angular convention follows
incoming photon travel (-direction to emitter) against direction to viewer.
Reference: https://www.pbr-book.org/4ed/Volume_Scattering/Phase_Functions
WGSL validation and file-length checks passed; visual calibration is pending.

The cloud-scattering release ran at Bergen with 493 cloud clusters and weighted
OIT. Fixed Burst captures at age 1.1 s and 4 s show the cloud flash and its
extinction (target/bolid-cloud-phase-flash.png, bolid-cloud-phase-after.png).
A second path at yaw 133/pitch 20, FOV 30 crosses a foreground cloud at ages
0.7 and 0.9 s (bolid-cloud-phase-head-crossing.png, bolid-cloud-phase-crossing.png).
These demonstrate runtime composition, not quantitative opacity or full-overcast
acceptance. A nearby bright flash also saturates much of the terrain under
fixed night exposure; exposure calibration remains to be reviewed. Release,
architecture, all 23 shader variants and file-length checks passed.

Flash exposure audit: a nearby Burst at Bergen produced incoming luminance
0.000281 with base exposure 1.25 and night gain 32768 (exposed incident 11.51).
A logarithmic output-shoulder experiment did not restore terrain readability
and was discarded; the original shared tone mapper remains. The new pure
bolid_light::exposure_scale meters received flash luminance instead. Exposed
incident <=1 leaves exposure unchanged; a continuous-derivative shoulder
reduces scene exposure for stronger flashes, asymptotically bounding the
reference illumination near 2.78. Source energy, inverse-square attenuation,
shadow selection and display-calibrated celestial emission are unchanged.
HDR and fallback apply the same factor to their respective exposure paths.
This is analytic per-frame metering, not temporal eye adaptation.
Release replay at age 1.35 s with unmetered exposed incident 10.13 retained
cream-colored terrain variation instead of clipped white (captures:
target/bolid-flash-metering-sky.png and bolid-flash-metering-ground.png).
An albedo diagnostic also showed that this terrain material itself has little
variation; absence of strong texture is not solely highlight clipping.
Fallback/daylight visual acceptance of the metering is still pending.

New daughter fragments and grain packets retain their exact release position
in the cached CPU history. Their first visible wake starts at that point when
the 65 ms lookback predates their serial, instead of collapsing to zero length.
Reused slots never connect to the old occupant. Birth positions receive the
same physical-to-world scale as emitter positions. GPU/wire layouts and body
counts are unchanged; the history adds one Vec3 per emitter (240 bytes per
snapshot, bounded by 1501 snapshots per event).

Release Cascade seed 327825 at age 0.8 s produced a newly born emitter-1
wake of 213.68 world metres while the primary wake was 277.81 m, confirming
that the shorter birth-to-current segment is uploaded. At 1.5 s eight luminous
solid emitters retained individual positions and wakes of roughly 224..246 m.
Their projected separation is only a few pixels at FOV 30; the bright heads
still merge around the peak, so broad visual fragmentation acceptance is open.
Captures: target/bolid-cascade-birth-wakes.png and bolid-cascade-late-fragments.png.
Core/fragment shader paths now skip atmosphere transmission and distance
accumulation when the spatial spot evaluation returned zero. This avoids
expensive transmission calls across the large event bounding sphere.

Bolid core and luminous line filtering now derive their angular pixel footprint
from screen-space ray derivatives before divergent event branches. This follows
camera FOV as well as the reduced bolid render-target resolution. Unresolved
wakes and train segments dilute peak radiance by physical/filtered width; cores
retain the squared ratio appropriate to a two-dimensional spot. Fixed angular
line-width floors no longer add light to distant or narrow trails. All 23 WGSL
variants, client/server cargo check, architecture and file-length checks passed.
Visual acceptance of the new filtering is pending.

Release replay verified filtered Cascade seed 327825 at age 1.0 s with FOV
30 and 70, and at age 1.3 s with FOV 30. The long thin train survives both
views, and the head extinguishes independently from the residual train.
Captures: target/bolid-filtered-cascade-peak.png,
bolid-filtered-cascade-fov70.png and bolid-filtered-cascade-late.png.
This is not a matched old/new trajectory benchmark. The peak fragment
cluster still merges, so individual secondary wakes remain an open item.

Fragment interpolation now switches reused slots at the recorded birth time
inside a 40 ms history interval. Before release the previous occupant advances
with its stored velocity; afterwards the daughter moves from its release point
toward the next snapshot. Solid fragments retain inherited birth emission;
grain packets start with zero emission. The interpolated emitter powers are
normalized to the cached total light curve, keeping visible emission and the
point source synchronized without counting inherited parent light twice.
History storage adds one f32 per emitter; wire/GPU contracts remain unchanged.

Release replay confirmed the first Cascade solid daughter absent at age
0.499 s (9 emitters), present at its 0.500 s release (10 emitters, zero wake),
and carrying a 28.275-world-metre wake at 0.505 s. It no longer waits for the
0.520 s snapshot. Runtime incoming luminance at 0.505 s was 0.000023784.
Capture: target/bolid-birth-aware-cascade.png. Release compilation, formatting,
architecture and file-length checks passed; no automated tests were run.

The foreground-cloud transmission attachment now uses R16Float instead of
Rgba8Unorm. Multiplicative UNORM blending can round back to the same nonzero
value on successive faces, leaking a bright bolid through accumulated optical
depth. The floating-point channel keeps relative precision at low transmission,
matching the existing cloud OIT revealage format. The bolid-layer allocation
is now 26 bytes/pixel (previously 28), still capped at 64 MiB. Both texture and
mask pipeline formats changed together; composition remains multiplicative.

R16Float pipeline creation and rendering were verified in the release client
with 493 clouds / 39250 faces and weighted OIT. Burst seed 590085 at age
0.45 s overlaps the foreground cloud at yaw 133, pitch 20, FOV 30 (capture
target/bolid-float-transmission-cloud-crossing.png). This verifies the live
composition path, not a measured opaque-cloud extinction threshold. Release,
client/server check, architecture and file-length checks passed.

Point-source shadow projection now uses a receiver-centered coordinate frame.
The CPU builds the perspective matrix around zero; caster and receiver shaders
subtract receiver_center_relative_radius.xyz before multiplying it. Directional
Sun/Moon projection is unchanged. This reduces cancellation when the scene
origin is distant from the receiving region without increasing shadow bias or
changing light direction, cone coverage, caster bounds or uniform layout.
All 23 WGSL variants, client/server check, architecture and file-size checks
passed. This numerical change alone does not establish steep-terrain acceptance.

Release Bergen replay at age 1.05 s confirmed source=bolid, complete coverage
and 25 casters after recentering (target/bolid-shadow-centered-projection.png).
The shadow debug image still contains shallow facet-shaped visibility bands;
recentering is not a demonstrated fix for that remaining artifact.

A temporary one-tap point-shadow experiment at Bergen reproduced hard black
triangular patches with source=bolid and 25 complete casters (capture
target/bolid-shadow-single-tap-diagnostic.png). Thus removing PCF does not
remove the underlying depth-comparison artifact. The experiment was discarded
and the continuous 3x3 filter restored. Surface and caster preparation both
consume the same exposed geometry and precise origin-relative vertex positions;
no separate lower-detail caster mesh was found in this path.

A second discarded diagnostic stored perspective point-shadow depth as linear
light-space distance via frag_depth and converted the receiver-plane PCF taps
to the same space. The release run still showed facet-shaped bands with
source=bolid and 25 complete casters (target/bolid-shadow-linear-depth-diagnostic.png).
No visual benefit justified the fragment-stage cost, so the original depth-only
pipeline and receiver comparison were restored byte-for-byte. This rules out
that depth-encoding replacement as a sufficient fix; receiver/caster agreement
at individual samples still needs direct inspection.

A temporary center-tap diagnostic displayed (receiver plane depth minus stored
depth) multiplied by the light depth range, after the existing receiver bias.
Bergen source=bolid with 25 complete casters showed mostly nonpositive values
(blue), but triangular patches in the 0.1..1 world-metre bin (magenta), plus
0.01..0.1 patches (yellow). Capture: target/bolid-shadow-depth-error-bins.png.
This is an approximate light-axis depth residual, not a measured surface-normal
displacement. It points beyond simple depth-storage rounding; projected surface
slope and the actual sampled caster need investigation. Diagnostic shaders were
restored; no false-color mode remains in the production source.

A Chiang 2019 local shadow-terminator factor was evaluated on bolid terrain
reflection (reference https://blog.yiningkarlli.com/2020/02/shadow-terminator-in-takua.html).
The release Bergen view at age 1.2 s, source=bolid and complete 25-caster
coverage made coarse facet boundaries more conspicuous rather than improving
the image (target/bolid-terminator-correction-rejected.png). The factor was
removed. It must not be mistaken for a retained fix for the depth residual.

Fresh bolid wakes now retain 250 ms of individual emitter history rather than
65 ms. Up to 70% of fresh emission is handed from the centroid train to those
wakes, blending back between 180 and 250 ms. Only historical serials represented
by uploaded wake endpoints are subtracted; retired/reused slots stay in the
common train. Wake emission interpolates past/current power and cools/expands,
so a recently extinguished head can leave a briefly visible wake. The split
is an approximate presentation energy allocation, not additional source power.
The point-light curve and wire/GPU sizes are unchanged. optics.w now packs
grain identity in the integer part and wake duration in the fractional part;
tails.w carries historical endpoint power. Bounds include the longer tails.

Release Cascade seed 327825 now shows separated narrow wake tips at age
1.12 s, after the heads have dimmed, with uploaded wake lengths 1132..1205
world metres (target/bolid-separated-wakes-late.png). At 1.4 s the older
common train remains without luminous heads (bolid-separated-wakes-after.png).
Peak heads can still merge within their physical angular separation. Static
checks, 23 WGSL variants and release compilation passed. These frozen-age
views establish visible branches, not a full temporal/performance acceptance.

Daylight train contrast now suppresses old cooling gas as well as chemical
afterglow, blending over deposited ages 0.35..2 s. Fresh flight and the separate
daylight dust-scattering path are unchanged. At Bergen local noon (Sun +10.26
deg), fixed-age 8 s paired seeds 93/279 showed no visible glowing gas line
without dust and a faint gray dust ribbon with dust (target/bolid-day-gas-after-contrast.png
and bolid-day-dust-after-contrast.png). At twilight Sun -4.66 deg, the residual
emissive train remains visible but dust readability is not established. Night
paired dust-on/off captures differ by at most 3/255 RGB levels in the central
sky crop; this is not an exact pixel identity claim. HDR/fallback sky brightness
parity remains open. Release, all 23 WGSL variants and static checks passed.

Common-train segments now use the finite Gaussian-line integral rather than
adding full circular endpoint caps. The odd erf approximation shares endpoint
weight between adjacent segments, removing regularly spaced artificial bright
beads while preserving deposited-power variations. Filtering and early angular
rejection remain bounded; no extra trail nodes or GPU fields were added.
Release at fixed age 8 s showed a smooth continuous seed-93 train and two
separated luminous regions for Intermittent seed 1114326, whose diagnostics
reported two emission episodes (target/bolid-train-seamless-smooth.png and
bolid-train-seamless-intermittent.png). Release compilation, 23 WGSL variants,
architecture and file-length checks passed. No automated tests were run.

Fallback bolid emission now uses the shared eight-sample finite RGB clear-air
transmission between camera and emitter. It previously used grey analytic sky
transmission with an unscaled world-distance/8000 factor. The new path applies
the same optical scale and Rayleigh/Mie/ozone coefficients as point lighting,
including elevated/orbital receivers without assuming an infinite air column.
HDR retains aerial-LUT transmission (including regional fog); fallback does
not acquire a regional fog volume or extra textures. Angular rejection still
precedes transmission evaluation. This fixes an extinction-model mismatch,
not the remaining sky exposure and cloud-compositing parity differences.
Release fallback check confirmed hdr=false and physical=false at Bergen,
Cascade seed 327825/event 1 frozen at 0.8 s, FOV 30. The core and thin warm
train render without GPU validation errors (target/bolid-fallback-finite-air.png).
Release build, 23 WGSL variants, architecture and file-length checks passed.
This is a ground-view smoke check, not orbital or HDR/fallback visual acceptance.
No automated tests were run.

Finite bolid light transmission now clips its integration interval to the
atmospheric-top sphere before distributing the eight cubic quadrature samples.
Long orbital vacuum legs previously consumed part of that fixed sample budget,
which could miss a narrow atmospheric crossing. The CPU source-selection path
and WGSL receiver/emitter path use the same clipped interval; a missed shell
has zero optical depth. This changes neither inverse-square power nor surface
cosines and adds no samples, buffers or protocol fields.
Fallback partial-cloud replay at Bergen yaw 55/pitch 46, Burst event 5 age
1.4 s crossed a visible cloud slab (target/bolid-fallback-partial-cloud-age1_4.png).
It does not prove full-overcast occlusion; that acceptance remains open.
Release smoke check after fly on / set_altitude 500: fallback Cascade
seed 327825/event 1 at 0.8 s, yaw 90/pitch -55, FOV 45 remains visible above
the lit terrain (target/bolid-fallback-orbit-clipped-air.png). Status reported
mid-flight world distance 50651.9 m and source altitude 7886.2 world m; the
astronomy observer height is capped at 80000 physical m and is not camera
altitude evidence. Release compilation, 23 WGSL variants, architecture and
file-length checks passed. This does not establish quantitative limb-path
accuracy, complete cloud occlusion or HDR/fallback parity. No tests were run.

Bolid emission and premultiplied dust scattering now occupy separate RGBA16F
textures. Emission stays linear in both render paths until foreground cloud
transmission and dust extinction are applied. The analytic fallback then tone
maps that transmitted emission in its resolve entry point; HDR leaves it linear
for the existing final scene output. Previously fallback tone-mapped emission
before cloud extinction, producing a different attenuation response. Dust and
foreground cloud color retain the existing scene/display-space calibration.
The fallback main scene is still composed in display space; this does not claim
full HDR equivalence of background-plus-emission or cloud scattering.
Layer storage is now 34 bytes/pixel, still capped at 64 MiB by resolution scaling.
BYTES_PER_PIXEL is shared by allocation sizing and telemetry (which previously
still reported 28 despite the R16F transmission change). A separate dust pass
always clears its target; its draw is skipped when there is no dust.
Release runtime checks created both HDR and fallback resolve pipelines without
validation errors. Night Burst/cloud overlap captured in both modes; fallback
noon seed 279 at age 8 shows a faint dust ribbon, and switching to seed 93
removes it (no stale dust target). Captures: bolid-late-tonemap-fallback-cloud,
bolid-split-layer-fallback-dust, bolid-split-layer-fallback-no-dust and
bolid-split-layer-hdr-cloud under target as PNGs. Paths differ across launches,
so these are runtime checks rather than a quantitative HDR comparison.
Release build, 23 WGSL modules (both composite entries), architecture and file
length checks passed. Full-overcast and shadow acceptance remain open. No tests run.

Controlled shadow-bias experiment used the exact path in
target/bolid-shadow-repro.txt and the same prepared weight 0.68391114, source
position and received luminance. Replacing the capped 0.1 world-m normal
offset with one receiver texel (0.2525 m) weakened but did not remove the
triangular bands; target/bolid-one-texel-offset-rejected.png. The experiment
was rejected because it increases contact-separation risk without resolving
the artifact. shadow_sampling.wgsl was restored byte-for-byte from the
pre-experiment copy. No increased production bias is retained. This supports
further investigation of grazing receiver-plane filtering at adjacent facets,
not a claim that the numerical/geometric cause is proven.
In a fixed 1700x750 shadow-debug crop, baseline RGB range was 218..232;
one-texel bias reduced it to 225..232 (mean absolute difference 1.50/255).
The remaining spatial pattern was visible, so this is partial contrast
reduction rather than a shadow fix. Image statistics exclude HUD/title areas.

A second controlled experiment interpolated terrain position relative to the
shadow receiver center directly from the vertex stage (one additional vec3),
then used that value in point-shadow projection. The fixed path, source power,
weight 0.68391114 and terrain view matched the baseline. Bands remained; crop
RGB range stayed 218..232 and mean absolute RGB difference was only
0.000332/255. Capture: target/bolid-local-interpolation-rejected.png. This
weakens scene-origin interpolation precision as the primary cause. Both
terrain.wgsl and shadow_sampling.wgsl were restored from exact pre-experiment
copies; no extra interpolator or alternate sampling API remains. Compilation
and 23 WGSL validation modules passed for the experiment; no tests were run.

## Ambient cosmetic meteors (CPU contract - ADR 0013, E047-01)

`docs/adr/0013-ambient-meteors.md` freezes the E047 design; E047-01 lands this
crate's CPU-only contract in `meteors`. `meteors::model` holds
`MeteorSource { Cosmetic, Tracked }`, `MeteorSample`, `MeteorTrailSample` and
`TransientLightSample` with hard finite/non-negative/bounds validation.
`meteors::coords` is the single physical-metre ↔ game ↔ render-origin adapter:
`PlanetSurfaceScale` mirrors the observer camera mapping (Earth relief and
uniform procedural scales, manifest `height_scale_permille`, no hard-coded
radius), and `render_relative_m` subtracts the floating origin in `f64` before
any `f32` cast. `meteors::frame` owns the bounded, non-allocating
`MeteorFrame` with `MeteorFrameContext` and deterministic priority pushes
(importance-at-observer with id tie-breakers, sorted cores/trails/lights).
Frame caps: at most 16 bright cores, 4 transient lights and 128 trail points
(8 per core), with `RENDER_REL_M_MAX` bounding origin-relative magnitudes;
`MAX_METEOR_HISTORY_SAMPLES` and `MAX_DUST_SEGMENTS` reserve the lifecycle and
dust envelopes. GPU positions stay origin-relative f32; lifetimes run on the
client monotonic clock, and world UTC only gates exposure conditions.

The first producer is the client-local manual runtime (E047-02,
`apps/client/src/renderer/meteors`): manual presets seed deterministic events
that rebuild one bounded `MeteorFrame` per frame on the client monotonic clock;
`Tracked` remains an input marker with no server producer, protocol message,
save field or world entity.
`RenderFrame::meteors` carries the frame on every prepared render frame with
explicit disabled states for interior/unavailable environments and an
explicitly empty ready state otherwise; the scene snapshot exposes it and
`GpuResources::meteors_status` records the per-frame status surfaced as
`meteors_state`/`meteors_cores`/`meteors_contract` diagnostics without GPU
work while all counts stay zero. The observer astronomy reference now maps
eye height through `PlanetSurfaceScale`, keeping one physical-metre
convention for Earth-relief and procedural planets, and the identical
resolved scale threads into `EnvironmentFrame::meteor_surface_scale` for the
meteor runtime. The existing
`BolidEvent`/`bolid_dust`/point-light stack is untouched. Later passes slot
into the `frame_encoder::encode_frame` order described in the ADR, and
`TransientLightUniform` (E047-09) extends the shared frame bind group. The
frame implementation got its own `meteors/frame.rs` so every hand-maintained
file stays within the 500-line policy.
