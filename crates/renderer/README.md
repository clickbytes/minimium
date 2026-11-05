# minimium-renderer

## Purpose / Non-goals

Owns presentation contracts, reusable rendering primitives and pass encoding: camera/frame values, geometry conversion, visibility and LOD selection, visual lighting, quality profiles, WGSL sources, GPU resources and pipelines. The client still owns window lifecycle, frame scheduling, scene visibility decisions, shadow caster selection and some GPU resources. This crate does not own sessions, simulation state, or a window.

## Public API

- `RenderViewInput` and `RenderViewFrame` carry camera matrices, viewport and world-space presentation metadata. `RenderViewFrame::from_camera` rejects non-finite values and clamps empty viewports to one pixel.
- `RenderFrame` carries the prepared view, camera/environment uniforms, adapted `ClientFrameState`, halo count and UI vertices for one frame. It owns CPU values only; `upload` copies uniforms and UI vertices into caller-owned buffers; the client returns the reusable allocation to its console after submission.
- `quality::{SkyQuality, SkyQualityProfile, SkyQualityState}` defines renderer quality choices and shadow parameters.
- `geometry::Vertex`, its layout, and `sector_geometry`, `sector_geometry_relative` and `cube` convert world geometry to render data.
- `globe_detail::{Region, DetailGpu, patch_vertices}` defines globe detail regions, their GPU resource bundle, and patch mesh generation. `GlobeGpu` owns the overview texture, bind group, pipeline and LOD mesh; the client supplies geography pixels and shared layouts. Detail texture sampling and upload scheduling remain in the client.
- `scene_encoder::{MainScenePass, EnvironmentDrawFlags, encode_main_scene_pass}` encodes the scene pass from renderer GPU handles and app-prepared visibility decisions. `frame_encoder::encode_frame` orders shadow, scene, atmosphere, cloud, UI and output passes; `encode_final_passes` is also available independently.
- `surface_frame::{acquire, SurfaceAcquire, SurfaceFrame}` wraps surface image acquisition, recoverable suboptimal configuration and queue-submit/present. The app still decides when to schedule a frame and handles window events.
- `CloudGpu::encode`, `AtmosphereGpu::encode` and `OverviewGpuBuffers` own cloud/atmosphere pass encoding and overview vertex-buffer lifecycle. `WorldGeometryGpu`, `ShadowGpu`, `GlobeGpu`, `DynamicVertexBuffer`, `StaticVertexBuffer` and `UiGpuResources` own extracted GPU resource groups and upload behavior.
- `shadow_uniform::prepare`, `ShadowSettings`, `shadow_target`, `shadow_geometry` and `ShadowGpu` prepare shadow data and own shared shadow resources.
- `AtmosphereUniform`, `AtmosphereGpu`, `atmosphere::{Optics, EARTH}`, `atmosphere_table` and `atmosphere_pipeline` define clear-air optics and its LUT/GPU resources.
- `EnvironmentPipelines`, `gpu_frame`, `gpu_bindings`, `CelestialHaloGpu`, `MoonGpu`, `MoonSphereGpu`, `MilkyWayGpu` and `StarGpu` provide environment presentation resources and pipelines.
- `OutputEncoder` owns the linear scene target and final sRGB encode pipeline. Its dimensions are plain pixels, with no window dependency.
- `planet_visibility::render_patches` returns visible terrain patches and LOD levels for a manifest, camera, collision cache and optional prepared geography.
- `profiles::LightingProfile`, `lighting` and `moon_lighting` provide visual lighting values and calculations. `shader_source` composes WGSL fragments; `shaders::SHADOW_WGSL` exposes the standalone shadow pass.

## Dependencies

Uses `glam` for camera and geometry calculations; `bytemuck` and `half` for GPU layouts and optical LUT values; `wgpu` for rendering resources; `pollster` for bounded GPU error-scope resolution; `minimium-assets` for the embedded star catalogue; `minimium-procgen` and `minimium-world` for manifests, geography and generated patches; and public astronomy/environment observations for visual lighting. It has no window, client-runtime, session, protocol, storage or transport dependency.

## Data flow

The client runtime publishes an immutable, GPU-independent `ClientView`; the app adapts its world, position, health and readiness values into `RenderFrame::client` alongside camera, environment and console data. The app prepares scene visibility flags and selects live world/shadow buffers; renderer encoders consume those values and handles to encode shadow, scene, cloud, atmosphere, UI and output passes. The client schedules frames and handles window events; renderer `surface_frame` wraps image acquisition and submission/presentation.

## Ownership and execution

Frames and visibility results are value data owned by the caller. Quality profiles are immutable values selected by the application. Geometry conversion and visibility selection are synchronous deterministic transformations without I/O. GPU resources remain split across the crate and client: renderer owns extracted pipelines, pass encoders, surface-frame operations, globe and overview buffers, world geometry, shadow, UI, cloud and atmosphere resources, while the application owns the surface handle, remaining buffers and update/scheduling state. The client retains the world-clock star epoch and supplies immutable instances; render encoders consume explicit references and flags.

## Invariants and errors

Invalid matrices, origins, eyes, or negative/non-finite planet radii return `None`. Zero viewport dimensions normalize to one. Coarse planet traversal caps visited sectors at 512; dense planet subdivision retains its existing horizon and subdivision bounds.

## Commands

This crate exports no commands.

## Change guide

Change frame metadata, camera preparation, geometry conversion, quality profiles, visible patch selection, visual lighting, atmosphere/star preparation, extracted GPU resource owners, pass encoding, surface-frame operations and WGSL composition here. Window startup/scheduling, visibility/caster selection and remaining client-coupled GPU resources stay in the application while extraction continues.

## Verification

Run `rtk cargo check -p minimium-renderer -p minimium-client` and `rtk python scripts/check_architecture.py`. Runtime visual acceptance is deferred.
