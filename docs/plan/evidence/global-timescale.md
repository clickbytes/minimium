# Global timescale implementation and verification

Implemented on 2026-11-19 in the working tree of `work/tool-viewmodels` at `4ef8abc78906a9a34f4700cfab51807cba8eb901`. The inventory includes files shared with the pre-existing prediction and viewmodel work.

## Owners and contracts

| Owner | Contract |
| --- | --- |
| Server runtime and environment | The server owns the last completed 50 ms step and its UTC. Host time drives admission, administration, deadlines and the bounded demand accumulator. A required hydrology result or durable transaction holds the step. Pause acknowledges a committed boundary and clears outstanding input. |
| Storage and process server | Save v12 records the clock and journal boundary alongside world data. Existing v11 data requires a separate directory. Startup checks for incompatible formats before normal startup writes. Restore requires a snapshot and journals from a matching boundary. If a crash leaves newer journal records, restore the matching backup set. |
| Protocol and clients | Protocol v22 distributes the committed clock and input acknowledgements. The client interpolates confirmed boundaries and uses real deadlines to refresh held controls. The load client and example clients use the same protocol. |
| Desktop, renderer and environment weather model | World effects, audio and sky use the presentation time. Cloud geometry, cover and fog select data valid at frame UTC. Source occlusion and transparency ordering use that frame's cloud selection. Observer darkness reports bind location and date to a validity interval. |
| World, localization and command surfaces | Pause has a typed rejection, translated feedback and operator diagnostics. Camera, UI and service deadlines use host time. |

## Static checks

Commands ran from the repository through `rtk` in the current working tree unless the scope says otherwise.

| Command | Result | Evidence |
| --- | --- | --- |
| `cargo fmt --all -- --check` | PASS (0) | Rust formatting |
| `cargo check --workspace --locked` | PASS (0) | Workspace |
| `cargo check -p minimium-client --no-default-features --locked` | PASS (0) | Headless client |
| `cargo check -p minimium-client --features desktop-audio --locked` | PASS (0) | Desktop audio backend |
| `cargo check --workspace --examples --locked` | PASS (0) | Example targets compiled with warnings in shared example support |
| `cargo check --workspace --tests --locked` | PASS (0) | Compile-only check of existing test targets. Fixed the crate name used by shared client fixtures. |
| `python scripts/check_architecture.py` | PASS (0) | Declared package roles and dependency boundaries |
| `python scripts/check_feature_contracts.py` | PASS (0) | Consumer feature combinations |
| `python scripts/check_file_lengths.py` | PASS (0) | 841 source files within 500 physical lines |
| `git diff --check` | PASS (0) | Tracked diff whitespace |
| `python scripts/check_static_refactor.py` | FAIL (1) in place | All 115 duplicate declarations point into ignored checkouts under `.local/worktrees`. |
| `python scripts/check_static_refactor.py` | PASS (0) on current sources | The unchanged checker ran in a temporary tree containing the tracked and unignored local Rust and WGSL sources with their repository paths. |

## Runtime acceptance

Game, tests, benchmarks, GPU measurements and listening checks are **NOT RUN** under the project's final-verification policy. The approved scenarios cover two-client movement, water and impact parity at 1x and 0.1x, pause during active effects and I/O, sustained acceleration under load, weather at matching UTC, dawn and teleport invalidation, and restart and failure recovery. Achievable acceleration, frame continuity and audible phase continuity remain unmeasured.

## Cloud flicker correction

The desktop log from the reported session identifies Vulkan on an RTX 3070 Ti with weighted blended OIT. Inspection found that a worker mesh and its local fallback could share a GPU upload revision. That allowed the face buffer from one representation to accompany the record indices and draw count of the other. Individually staggered birth and morph boundaries could also invalidate an entire publication and temporarily remove neighbouring regions.

The environment owner groups shape samples on a six-second UTC grid and includes the clouds whose lives intersect the population window. Age still controls birth and disappearance. The desktop prepares the successor before the active interval ends, gives fallback geometry a distinct revision, and retains surviving regional IDs and valid per-cloud meshes if a worker is late. Both transparency paths consume the selected geometry. Global surface history yields to queued cloud work between atlas rows.

| Owner | Files changed for the correction |
| --- | --- |
| Environment | `crates/environment/src/weather/mod.rs`, `crates/environment/src/weather/population.rs`, `crates/environment/README.md` |
| Desktop weather | `apps/client/src/renderer/clouds/mod.rs`, `apps/client/src/renderer/clouds/runtime.rs`, `apps/client/src/renderer/clouds/selection.rs`, `apps/client/src/renderer/clouds/status.rs`, `apps/client/src/renderer/clouds/worker.rs`, `apps/client/README.md` |
| Renderer | `crates/renderer/src/surface_atlas.rs`, `crates/renderer/README.md` |
| Documentation | `docs/weather.md`, `docs/plan/evidence/global-timescale.md` |

**PASS (0)** for `cargo build --release -p minimium-client --locked`, `cargo check --workspace --locked`, `cargo check -p minimium-client --no-default-features --locked`, `cargo fmt --all -- --check`, `python scripts/check_architecture.py`, `python scripts/check_feature_contracts.py`, `python scripts/check_file_lengths.py` and `git diff --check`. The in-place refactor check again reported 115 findings in ignored worktrees (exit 1). The same checker passed against the isolated current Rust and WGSL sources (exit 0).

**NOT RUN** for unit, integration, end-to-end, GPU and benchmark sessions. Visual acceptance should observe a stationary cloud field across successive morphology windows, repeat while moving across region boundaries and changing timescale, and inspect both OIT and sorted alpha during delayed builds.

## Changed files

### Documentation and workspace

- `README.md`
- `architecture.toml`
- `docs/adr/0006-world-time-and-sky.md`
- `docs/adr/0013-ambient-meteors.md`
- `docs/adr/0015-host-driven-authoritative-runtime.md`
- `docs/adr/0016-committed-simulation-time.md`
- `docs/ambient-meteors.md`
- `docs/architecture.md`
- `docs/commands.md`
- `docs/excavation.md`
- `docs/plan/evidence/global-timescale.md`
- `docs/protocol.md`
- `docs/weather.md`

### apps/client

- `apps/client/README.md`
- `apps/client/src/application/desktop_ops.rs`
- `apps/client/src/audio/backend.rs`
- `apps/client/src/audio/meteors.rs`
- `apps/client/src/audio/mod.rs`
- `apps/client/src/client_commands/environment.rs`
- `apps/client/src/client_commands/presentation.rs`
- `apps/client/src/environment/demo.rs`
- `apps/client/src/messages.rs`
- `apps/client/src/renderer.rs`
- `apps/client/src/renderer/bolids.rs`
- `apps/client/src/renderer/camera.rs`
- `apps/client/src/renderer/clouds/mod.rs`
- `apps/client/src/renderer/clouds/runtime.rs`
- `apps/client/src/renderer/clouds/selection.rs`
- `apps/client/src/renderer/clouds/status.rs`
- `apps/client/src/renderer/clouds/worker.rs`
- `apps/client/src/renderer/dev_input.rs`
- `apps/client/src/renderer/dev_input/bolid_path.rs`
- `apps/client/src/renderer/environment/clock.rs`
- `apps/client/src/renderer/environment/prepared.rs`
- `apps/client/src/renderer/environment/runtime.rs`
- `apps/client/src/renderer/excavation_debug.rs`
- `apps/client/src/renderer/held_tool.rs`
- `apps/client/src/renderer/impacts.rs`
- `apps/client/src/renderer/meteors/README.md`
- `apps/client/src/renderer/meteors/mod.rs`
- `apps/client/src/renderer/meteors/runtime.rs`
- `apps/client/src/renderer/meteors/settings.rs`
- `apps/client/src/renderer/meteors/sounds.rs`
- `apps/client/src/renderer/render.rs`
- `apps/client/src/renderer/session/message_handling.rs`
- `apps/client/src/renderer/window_events.rs`

### apps/loadtest

- `apps/loadtest/src/client_setup.rs`
- `apps/loadtest/src/worker.rs`

### apps/server

- `apps/server/src/main.rs`

### crates/client-runtime

- `crates/client-runtime/README.md`
- `crates/client-runtime/examples/cpu_client.rs`
- `crates/client-runtime/examples/injected_content_jobs.rs`
- `crates/client-runtime/src/application/actions.rs`
- `crates/client-runtime/src/application/game.rs`
- `crates/client-runtime/src/application/messages.rs`
- `crates/client-runtime/src/application/mod.rs`
- `crates/client-runtime/src/application/ops.rs`
- `crates/client-runtime/src/application/readiness.rs`
- `crates/client-runtime/src/application/step.rs`
- `crates/client-runtime/src/environment.rs`
- `crates/client-runtime/src/lib.rs`
- `crates/client-runtime/src/prediction.rs`
- `crates/client-runtime/src/session.rs`
- `crates/client-runtime/src/session/clock.rs`
- `crates/client-runtime/src/session/clock/estimate.rs` (removed)
- `crates/client-runtime/src/session/clock/smoothing.rs` (removed)
- `crates/client-runtime/src/session/environment.rs`
- `crates/client-runtime/src/session/excavation.rs`
- `crates/client-runtime/src/session/receive/decode.rs`
- `crates/client-runtime/src/session/receive/decode/dispatch.rs`
- `crates/client-runtime/src/session/receive/decode/dispatch/transitions.rs`
- `crates/client-runtime/src/session/tests.rs`
- `crates/client-runtime/src/snapshots.rs`

### crates/environment

- `crates/environment/README.md`
- `crates/environment/src/time/boundary.rs`
- `crates/environment/src/time/evaluation.rs`
- `crates/environment/src/time/mod.rs`
- `crates/environment/src/time/types.rs`
- `crates/environment/src/weather/mod.rs`
- `crates/environment/src/weather/population.rs`

### crates/localization

- `crates/localization/locales/en.ftl`
- `crates/localization/locales/pl.ftl`
- `crates/localization/src/lib.rs`
- `crates/localization/src/messages.rs`
- `crates/localization/src/tools.rs`

### crates/protocol

- `crates/protocol/README.md`
- `crates/protocol/src/decoding/client.rs`
- `crates/protocol/src/decoding/excavation.rs`
- `crates/protocol/src/decoding/server.rs`
- `crates/protocol/src/encoding/client.rs`
- `crates/protocol/src/encoding/excavation.rs`
- `crates/protocol/src/encoding/server.rs`
- `crates/protocol/src/environment_codec.rs`
- `crates/protocol/src/error_code.rs`
- `crates/protocol/src/lib.rs`
- `crates/protocol/src/limits.rs`
- `crates/protocol/src/models.rs`
- `crates/protocol/src/tests.rs`

### crates/renderer

- `crates/renderer/README.md`
- `crates/renderer/src/meteors/model.rs`
- `crates/renderer/src/surface_cover.rs`
- `crates/renderer/src/transient_cloud_cache.rs`

### crates/server-runtime

- `crates/server-runtime/README.md`
- `crates/server-runtime/src/command_output.rs`
- `crates/server-runtime/src/composition.rs`
- `crates/server-runtime/src/connection.rs`
- `crates/server-runtime/src/environment/clock.rs`
- `crates/server-runtime/src/lifecycle.rs`
- `crates/server-runtime/src/replication.rs`
- `crates/server-runtime/src/runtime.rs`
- `crates/server-runtime/src/runtime/action_state.rs`
- `crates/server-runtime/src/runtime/admission/actions/chat.rs`
- `crates/server-runtime/src/runtime/admission/actions/combat.rs`
- `crates/server-runtime/src/runtime/admission/actions/movement.rs`
- `crates/server-runtime/src/runtime/admission/dispatch.rs`
- `crates/server-runtime/src/runtime/admission/identity.rs`
- `crates/server-runtime/src/runtime/admission/technical.rs`
- `crates/server-runtime/src/runtime/admission/world_control.rs`
- `crates/server-runtime/src/runtime/bolid.rs`
- `crates/server-runtime/src/runtime/bolid/shower.rs`
- `crates/server-runtime/src/runtime/clock.rs`
- `crates/server-runtime/src/runtime/environment.rs`
- `crates/server-runtime/src/runtime/excavation.rs`
- `crates/server-runtime/src/runtime/excavation/admission.rs`
- `crates/server-runtime/src/runtime/excavation/coordinator.rs`
- `crates/server-runtime/src/runtime/excavation/coordinator/advance.rs`
- `crates/server-runtime/src/runtime/excavation/hydrology.rs`
- `crates/server-runtime/src/runtime/excavation/impacts.rs`
- `crates/server-runtime/src/runtime/excavation/ports.rs`
- `crates/server-runtime/src/runtime/excavation/publication.rs`
- `crates/server-runtime/src/runtime/hosted.rs`
- `crates/server-runtime/src/runtime/impacts.rs`
- `crates/server-runtime/src/runtime/impacts/checkpoints.rs`
- `crates/server-runtime/src/runtime/impacts/replication.rs`
- `crates/server-runtime/src/runtime/impacts/spawn.rs`
- `crates/server-runtime/src/runtime/inputs.rs`
- `crates/server-runtime/src/runtime/loop.rs`
- `crates/server-runtime/src/runtime/network.rs`
- `crates/server-runtime/src/runtime/operations.rs`
- `crates/server-runtime/src/runtime/schedule/host_plan.rs`
- `crates/server-runtime/src/runtime/service_inputs.rs`
- `crates/server-runtime/src/runtime/tick.rs`
- `crates/server-runtime/src/runtime/tick/command_execution.rs`
- `crates/server-runtime/src/runtime/tick/dispatch_commands.rs`
- `crates/server-runtime/src/runtime/tick/properties.rs`
- `crates/server-runtime/src/server_command.rs`

### crates/storage

- `crates/storage/README.md`
- `crates/storage/src/backend.rs`
- `crates/storage/src/excavation.rs`
- `crates/storage/src/format.rs`
- `crates/storage/src/format/clock.rs`
- `crates/storage/src/format/load.rs`
- `crates/storage/src/format/write.rs`
- `crates/storage/src/impact_journal.rs`
- `crates/storage/src/player_store.rs`
- `crates/storage/src/player_store/snapshot.rs`

### crates/world

- `crates/world/src/items.rs`
