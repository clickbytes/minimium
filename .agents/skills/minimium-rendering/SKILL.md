---
name: minimium-rendering
description: Implement or review Minimium Rust/wgpu and WGSL rendering changes: frame composition, atmosphere, clouds, shadows, HDR, geometry upload, sky and GPU resources. Use for presentation features, not for server gameplay or protocol-only work.
---

# Renderer and atmosphere workflow

Read `crates/renderer/AGENTS.md`, the root instructions, renderer README, relevant client app README and domain contracts for sky/environment/terrain. Preserve ADR 0012 (renderer separation) and ADR 0013 (ambient meteors).

## When to use

- Editing Rust/wgpu render resource owners, WGSL shaders, frame graph, atmosphere LUT, cloud geometry/lighting, HDR exposure, shadows, terrain/ocean visibility or celestial effects.
- Adding a client-local cosmetic visual effect to an existing presentation path.
- Debugging floating-origin, stale GPU upload, visual context mismatch, depth/occlusion or shader uniform layouts.

## When NOT to use

- Server-authoritative combat, inventory, movement or world save changes.
- Pure session/protocol/CLI changes without presentation impact.
- An architectural ownership review without implementation (use `minimium-boundary-review`).

## Workflow

1. Capture current HEAD, hardware/runtime assumptions (if known) and the actual render graph. Find `frame_encoder`, `scene_encoder`, `gpu_frame`, existing GPU holders and shader source; **do not assume a pass order from an old issue**.
2. Define owner and input: immutable `RenderFrame`/`RenderViewFrame`, context-bound `EnvironmentFrame`, prepared terrain/water and observer snapshots. Domain weather, astronomy, gravity and terrain mutations remain outside renderer/shaders.
3. Express geometry in explicit units. Perform f64 physical/game-world conversions at the adapter boundary; use f32 render-origin-relative positions on GPU. Track `WorldContext`, source/geometry revisions and origin shift for every cached upload.
4. Specify full path for a visual feature: producer → frame snapshot → bounded CPU preparation → staging/upload → shader binds → main/shadow/cloud/atmosphere passes → linear HDR composition and final encode. Identify where depth, horizon, clouds and opaque terrain may occlude it.
5. Keep one coherent lighting and exposure model: no double pre-exposure or tone mapping, no sampling and writing the same target simultaneously, no unexplained full-screen bloom for local meteors. Check Sun/Moon direction and current dominant shadow-source policy.
6. For each texture, buffer, uniform, pipeline and worker: document owner, maximum size/work count, lifetime, source revision, cancellation/stale result checks, resize/reconnect behavior and capability fallback. Stage complete patch/caster sets before publishing.
7. Keep reusable shader sources and GPU owners inside `minimium-renderer`, application/window scheduling in `apps/client`, and CPU-only contracts accessible with GPU feature disabled. Reuse current ambient meteor, transient lighting and bolid composition rather than duplicating passes.
8. Update both Rust and WGSL when layouts change. Use available Cargo fmt/check, `scripts/check_static_refactor.py`, any verified WGSL validator and package/feature checks; record real exit codes. This is not evidence of correctness on a GPU.
9. Define a **separate** human visual acceptance plan: surface and globe, daylight/twilight/night, full/crescent Moon, clouds, teleport, origin shift, low-spec fallback, artifact/seam/flicker and performance. Do not execute it before explicit final-test authorization.

## Example

**Input:** `Add localized cloud glow around an existing bright bolid without a second sky renderer.`

**Expected plan (illustrative, not an implemented feature):** inspect the shared transient-light frame producer, feed bounded light records into the current cloud/terrain HDR composition, use the same context and rendering origin, preserve occlusion and exposure, and define no-light fallback. Produce a source→pass ownership diagram, uniform changes, budget, cache invalidation and visual review matrix.

## Completion and evidence

Provide changed paths and current source graph, domain/presentation ownership, resource budgets, layout/feature results and real static check statuses. Explicitly mark visual/GPU/benchmark and unit/integration/E2E as deferred under root AGENTS. Never infer visual quality or FPS from successful compilation.
