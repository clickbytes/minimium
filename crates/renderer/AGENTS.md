# Renderer instructions

This file supplements root/crates `AGENTS.md` for `crates/renderer/`. Read `crates/renderer/README.md`, `architecture.toml`, ADR 0012 (presentation boundary) and relevant sky/weather/meteors ADRs when editing those systems.

## Presentation boundary

- The renderer consumes immutable, context-bound prepared world, camera and environment data. It owns CPU presentation algorithms, shader sources, GPU resources, upload and frame encoding; never server authority, session handshakes, input mappings, save state or gameplay cooldowns.
- The application chooses visible content, schedules frames, polls sessions and owns its declared remaining GPU resources. Preserve existing separation while extraction continues; do not claim that client integration is already fully independent of renderer orchestration.
- CPU-only frame/quality/debug contracts must remain usable by the terminal client without GPU features. Do not accidentally require `wgpu` or `winit` for `--no-default-features`.

## Frame and data coherence

- Track `WorldContext`, generation, geometry publication stamp, sector revisions, active/candidate resource sets and floating-origin epoch. Do not publish half-uploaded terrain, ocean or shadow content.
- Preserve consistent world-space and camera-relative coordinates: domain geography and UTC/optics are prepared before conversion to render-origin-relative GPU coordinates. Use explicit physical/game scale and correct radial normals.
- Render scene, shadow depth, atmosphere, cloud, celestial/meteor lighting, UI and final output through the **existing** pass graph. Inspect its real order and depth targets before inserting a pass; never create a competing renderer for a single effect.
- Preserve one linear HDR exposure/composition policy. Distinguish scene radiance from display-space sprites; avoid repeated tone mapping, global flashes or sampling a resource being written in the same pass.
- Sky, climate, weather, planetary terrain and real-time astronomical inputs belong to domain or adapter owners. Render shaders approximate their **presentation**, not a second authoritative solver.

## GPU and shader ownership

- For every buffer, texture, bind group, pipeline, cache and upload: identify the owner, creation/destruction, capacity, version key, invalidation, asynchronous completion and resize/reconnect behavior.
- Bound CPU/GPU work. Late workers may install data only for the still-active matching context/revision. Use staged publication, resource budgets and documented fallback for unsupported GPUs.
- Keep WGSL files and reusable encode pipeline in the renderer package. When Rust/WGSL uniform layouts change, update both definitions and validate with the existing static checker/WGSL validation if available.
- Cosmetic ambient meteors remain local client presentation, not network entities. Tracked impacts and server bolids have separate authoritative producers but may share neutral GPU presentation and transient-light inputs.

## Review and acceptance

1. Identify prepared input → conversion → GPU owner → render pass → output, including cloud/shadow occlusion, HDR and version/fallback semantics.
2. Inspect producer and consumer contracts in renderer + application; do not conceal cross-package state coupling behind a second `impl Renderer` file.
3. Report actual Cargo/feature and shader-layout checks with exit codes; separately list required visual comparisons, artifacts and frame-time acceptance as **NOT RUN** until the user authorizes final testing.
4. Update renderer README and associated ADR only for actual changes. Do not change GPU behavior in a documentation-only governance task.
