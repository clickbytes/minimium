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
- `planet_visibility::render_patches` returns visible terrain patches and LOD levels for a manifest, camera, collision cache and optional prepared geography.
- `profiles::LightingProfile`, `lighting` and `moon_lighting` provide visual lighting values and calculations. `shader_source` composes WGSL fragments; `shaders::SHADOW_WGSL` exposes the standalone shadow pass.

## Dependencies

The default `gpu` feature gates rendering modules and GPU/astronomy dependencies.
With default features disabled, camera metadata and quality/debug selections are
CPU-only contracts usable by the terminal command frontend.

Uses `glam` for camera and geometry calculations; `bytemuck` and `half` for GPU layouts and optical LUT values; `wgpu` for rendering resources; `pollster` for bounded GPU error-scope resolution; `minimium-assets` for the embedded star catalogue; `minimium-procgen` and `minimium-world` for manifests, geography and generated patches; and public astronomy/environment observations for visual lighting. It has no window, client-runtime, session, protocol, storage or transport dependency.

## Data flow

The client runtime publishes an immutable, GPU-independent `ClientView`; the app adapts its world, position, health and readiness values into `RenderFrame::client` alongside camera, environment and console data. The app prepares scene visibility flags and selects live world/shadow buffers; renderer encoders consume those values and handles to encode shadow, scene, cloud, atmosphere, UI and output passes. The client schedules frames and handles window events; renderer `surface_frame` wraps image acquisition and submission/presentation.

## Ownership and execution

Frames and visibility results are value data owned by the caller. Quality profiles are immutable values selected by the application. Geometry conversion and visibility selection are synchronous deterministic transformations without I/O. GPU resources remain split across the crate and client: renderer owns extracted pipelines, pass encoders, surface-frame operations, overview buffers, world geometry, shadow, UI, cloud and atmosphere resources, while the application owns the surface handle, remaining buffers and update/scheduling state. The client retains the world-clock star epoch and supplies immutable instances; render encoders consume explicit references and flags.

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
