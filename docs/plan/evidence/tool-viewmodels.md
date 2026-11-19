# Held tools: digging motion and refined models

Source base: `eb1aa15c27d0a81f7096e553101e43323464ffda`.
Branch: `work/tool-viewmodels`; changes are in the local working tree.

## Delivered scope

1. A single model-independent 480 ms digging pose: wind-up (0–18%), strike
   (18–40%), short recoil (40–48%), recovery (48–100%). Smooth interpolation
   samples elapsed monotonic presentation time, independent of redraw frequency.
2. Initial-click and held-repeat ToolUse outcomes start the same motion after
   local queue acceptance. The server still decides whether digging succeeds.
   Suppression/cooldown does not enqueue delayed animation. No gameplay rules,
   protocol, inventory slots, tool identity, save or generator changes.
3. The desktop host retains one start time and a world-context/model scope.
   Focus/cursor loss, console/map mode, stowing/model changes, world-context
   changes and disconnect cancel playback. A completed cycle returns to rest.
4. The shovel has a tapered wooden shaft, D grip, steel socket, stepping
   shoulders and a closed cupped blade with back and bevelled edges.
5. The pickaxe has an octagonal shaft, dark grip, steel eye and two curved
   forged arms ending in solid tapered points. Face shading uses linear RGB.
6. One 80-byte pose/projection uniform animates the existing static vertex
   buffers. Aspect-correct perspective and the existing depth attachment provide
   self-occlusion after the final world-depth consumer; UI/output follow as before.
7. Source review and static compilation cover both frontends. Visual acceptance
   is explicitly separate from these results.

## Owners and resource bounds

- Sample owns tool geometry and model ordering. By mesh construction, shovel
  uses 432 vertices and pickaxe 384; both are below the existing 512-per-model
  cap. The catalog still contains the same two kinds in the same order.
- Renderer owns neutral pose evaluation, projection, uniform upload and drawing.
  One fixed 80-byte uniform is added; no per-frame mesh rebuild, extra render
  pass, depth texture, worker or animation backlog is introduced.
- Desktop host owns playback lifetime and input/outcome adaptation.
- Existing server action validation, reach/material policy, cooldown and
  terrain/equipment commit paths are preserved.

Flow: click/held input → existing UseEquipped executor → queued ToolUse →
desktop scoped playback → HeldToolPose → GPU uniform → existing viewmodel pass.

## Changed files

- `architecture.toml`
- `apps/client/README.md`
- `apps/client/src/renderer.rs`
- `apps/client/src/renderer/camera.rs`
- `apps/client/src/renderer/excavation.rs`
- `apps/client/src/renderer/gpu.rs`
- `apps/client/src/renderer/held_tool.rs` (new)
- `apps/client/src/renderer/init.rs`
- `apps/client/src/renderer/render.rs`
- `apps/client/src/renderer/session.rs`
- `apps/client/src/renderer/window.rs`
- `apps/client/src/renderer/window_events.rs`
- `crates/renderer/README.md`
- `crates/renderer/src/frame_encoder.rs`
- `crates/renderer/src/held_tool.rs`
- `crates/renderer/src/held_tool_gpu.rs`
- `crates/renderer/src/shaders/held_tool.wgsl`
- `games/sample/README.md`
- `games/sample/src/tool_visuals.rs`
- `games/sample/src/tool_visuals/mesh.rs` (new)
- `docs/excavation.md`
- `docs/plan/evidence/tool-viewmodels.md` (this report)

## Static verification

Commands ran in `D:/Git/minimium` on the base plus this local change unless noted.

| Command | Result |
| --- | --- |
| `rtk proxy cargo check -p minimium-client --locked` | PASS / 0 after correcting wgpu 30 optional depth-state fields; first iteration FAIL / 1 on `bool` versus `Option<bool>` and comparison type |
| `rtk proxy cargo check --workspace --locked` | PASS / 0 |
| `rtk proxy cargo check -p minimium-client --no-default-features --locked` | PASS / 0 |
| `rtk proxy cargo fmt --all -- --check` | PASS / 0 |
| `rtk proxy git diff --check` | PASS / 0 |
| `rtk proxy python scripts/check_architecture.py` | PASS / 0; 11 feature graphs |
| `rtk proxy python scripts/check_feature_contracts.py` | PASS / 0; all 22 compile invocations, including desktop, CPU renderer and process/embedded hosts; existing shared-example dead-code diagnostics remain |
| `rtk proxy cargo run --manifest-path scripts/wgsl_validator/Cargo.toml --locked` | PASS / 0; static Naga validation of 25 shader variants, including held_tool; no GPU adapter/game execution |
| `rtk proxy python scripts/check_file_lengths.py` | PASS / 0; isolated source overlay, 821 source files, max 500 lines |
| `rtk proxy python scripts/check_static_refactor.py` | PASS / 0; isolated source overlay |

The isolated verification tree is
`D:/Git/minimium/.local/worktrees/tool-viewmodels-check`: base checkout overlaid
with the exact changed files and new source modules, SHA-256 compared to the
working source. It avoids scanning archived nested worktrees as duplicate code.
No checker rules were weakened. Shader validation is a static tool, not a test
suite or an application/visual session.

## Deferred visual acceptance

NOT RUN: test suites, game/runtime, GPU/visual sessions and benchmarks under
the repository final-testing gate. Compilation does not establish visual quality.
Review both tools at rest/wind-up/strike/recovery, single click and held use,
server rejection/cooldown, rapid equip/stow, focus loss, console/map, transfer
and disconnect. Check depth overlap and clipping at wide/narrow window aspects,
different redraw rates, and HDR/SDR. No runtime/visual PASS is claimed.
