# ADR 0012: Renderer presentation boundary

Status: accepted for the current extraction.

## Context

The desktop renderer combines camera-frame preparation, visibility/LOD, GPU resources, drawing, client session state and window integration. These responsibilities have different owners and dependencies.

## Decision

Create `minimium-renderer` for presentation contracts and algorithms. It owns `RenderViewFrame` and CPU-only `RenderFrame`, camera matrix validation, shared camera/environment uniform layouts and buffers/bind group, planet-sector visibility/LOD, visual lighting profiles and calculations, WGSL sources/composition, main scene and shadow-depth pass encoding, cloud and atmosphere pass encoding, final UI/output pass encoding, celestial halo/lunar pipelines, surface-frame acquisition/submission operations, and extracted GPU resource owners including the overview globe. It consumes world manifests, prepared geography, generated geometry and public astronomy/environment observations. The client remains the composition adapter and owns frame scheduling, surface configuration, visibility and caster decisions, some remaining GPU buffers and window events.

Renderer code does not own client sessions, prediction, server authority, storage, transport or window lifecycle. The runtime publishes GPU-independent `ClientView`; the application maps its world identity, player position, health and sector readiness into `RenderFrame::client` alongside camera/environment/UI data. Frame scheduling remains in the app; renderer wraps surface acquisition and presentation. Scene, shadow, cloud, atmosphere and final presentation passes live in the renderer crate.

## Consequences

The camera, visibility, visual lighting, shader-source contracts and scene encoders can be used independently of the client shell; the crate has no window dependency. The client still owns frame scheduling, surface configuration, selected WebGPU resources and scene/caster decisions. The separation preserves current matrix checks, shader uniform layout, sector caps, LOD thresholds and geography inputs.
