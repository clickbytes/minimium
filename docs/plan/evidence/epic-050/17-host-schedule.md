# E050-17 — host supplied schedule evidence

## Repository and scope

- Repository/worktree: `D:/Git/minimium/.local/worktrees/e050-17`
- File paths listed below are repository-relative to that exact canonical worktree path.
- Branch: `work/e050-17-host-schedule`
- Base and current HEAD: `e161333c564888eb109daf02db70da6bf972b210`
- Issue: #540, after #539 CLOSED and merged at the exact base tree.
- Owner: `minimium-gameplay` validates neutral graph structure; `minimium-server-runtime` owns the concrete production operation IDs, graph composition and execution dispatch; `minimium-game-sample` consumes the neutral anchors.
- Worktree started clean. At implementation handoff the task owner had performed no commit, push, PR, issue edit or test-suite operation; integration belongs to the coordinator.

## Files and caller migration

- `crates/gameplay/src/schedule.rs`: replaces the closed built-in operation enum and graph with `ScheduleDefinition`, neutral stage anchors, typed `ScheduleError`, and immutable `SchedulePlan` steps.
- `crates/gameplay/src/lib.rs`: exports the neutral plan API and reserves stage anchor IDs during system registration.
- `crates/gameplay/examples/host_schedule.rs`: external public-API consumer constructs a second small plan, registers a neutral system, propagates the typed validation error and reads the returned plan; it contains no executor loop.
- `crates/gameplay/README.md` and `architecture.toml`: describe neutral graph ownership, startup separation and the actual host-sized bootstrap bounds.
- `crates/server-runtime/src/runtime/schedule/host_plan.rs`: owns `BuiltinOperation`, the complete default production graph and exhaustive operation-ID mapping.
- `crates/server-runtime/src/runtime/schedule.rs`, `schedule/diagnostics.rs`, and `schedule/system_context.rs`: keep the runtime schedule owner, bounded diagnostics and system-context adapter under the 500-line file limit.
- `crates/server-runtime/src/commands.rs`: passes the supplied definition to the validator and builds the runtime schedule from its result.
- `crates/server-runtime/src/hosting.rs` and `runtime.rs`: `RuntimeModules::new` validates once and retains the resulting `ServerSchedule`; runtime construction moves that same schedule to the tick owner.
- `crates/server-runtime/src/runtime/tick.rs`: dispatches the validated `Operation` / `Stage` stream.
- `crates/server-runtime/README.md`: records production dispatch and shutdown behavior.
- `games/sample/src/portal.rs` and `games/sample/README.md`: remove the `BuiltinOperation` import and use neutral anchor constants. Their values preserve the old dependency IDs.
- `docs/plan/evidence/epic-050/17-host-schedule.md`: this implementation and static verification record.

Canonical changed file paths (repository-relative):

- `architecture.toml`
- `crates/gameplay/README.md`
- `crates/gameplay/src/lib.rs`
- `crates/gameplay/src/schedule.rs`
- `crates/gameplay/examples/host_schedule.rs`
- `crates/server-runtime/README.md`
- `crates/server-runtime/src/commands.rs`
- `crates/server-runtime/src/hosting.rs`
- `crates/server-runtime/src/runtime.rs`
- `crates/server-runtime/src/runtime/schedule.rs`
- `crates/server-runtime/src/runtime/schedule/diagnostics.rs`
- `crates/server-runtime/src/runtime/schedule/host_plan.rs`
- `crates/server-runtime/src/runtime/schedule/system_context.rs`
- `crates/server-runtime/src/runtime/tick.rs`
- `games/sample/README.md`
- `games/sample/src/portal.rs`
- `docs/plan/evidence/epic-050/17-host-schedule.md`

All production `BuiltinOperation` references are now inside `minimium-server-runtime`. The prior sample-only concrete enum dependency was replaced with `SIMULATION_SCHEDULE_ANCHOR` and `POST_SIMULATION_SCHEDULE_ANCHOR`, whose values remain `engine simulation` and `engine post simulation effects`. Other current system descriptors use empty dependency lists. The real construction chain is `RuntimeModules::new` → `commands::build_schedule` → `ModuleRegistry::ordered_schedule` → `ServerSchedule::from_plan`; `ServerRuntime::new` moves the retained result. The only tick consumer is the validated stream in `runtime/tick.rs`.

## Tick trace: before and after

Before this change, `minimium-gameplay::BuiltinOperation::ALL` hardcoded the eleven-node skeleton and its phase numbers. `ModuleRegistry::ordered_schedule()` inserted module systems around that closed list. `ServerSchedule` copied a built-in enum vector, and `runtime/tick.rs` iterated that vector and matched each enum value. FixedUpdate, Environment and PostSimulation systems ran at their respective effects enum cases.

After this change, the runtime host supplies the same eleven ordered nodes. The three stage nodes are neutral publication barriers carrying the legacy IDs. The generic validator places each stage's systems in its barrier phase, forces the complete system batch before that barrier, and returns immutable steps. The runtime maps every returned operation ID to a concrete operation before services start; unknown IDs fail with `ScheduleError::UnknownOperation`. Tick execution walks only the retained step stream.

| Order | Existing owner call / stage | New validated node | Publication and shutdown behavior |
| --- | --- | --- | --- |
| 1 | `BuiltinOperation::Commands` → `apply_commands` | `Operation("engine commands")` | Command outcomes remain per-command. |
| 2 | `BuiltinOperation::Bolids` → `update_bolids` | `Operation("engine bolids")` | Same owner call. |
| 3 | `BuiltinOperation::Impacts` → `update_impacts` | `Operation("engine impacts")` | Same owner call and owner policy. |
| 4 | `BuiltinOperation::Excavation` → `update_excavation` | `Operation("engine excavation")` | Same owner call and coordinator. |
| 5 | `BuiltinOperation::ShutdownGate` | `Operation("engine shutdown gate")` | Gate remains after excavation and before inputs. Stop returns before later stages, tick completion metrics and deadline advancement. |
| 6 | `BuiltinOperation::WorldInputs` → `apply_world_inputs` | `Operation("engine world inputs")` | Same input owner call. |
| 7 | `FixedEffects` → all `Stage::FixedUpdate` systems, then `run_system_stage` commit | `StageBarrier(FixedUpdate)`, ID `engine fixed effects` | One ordered batch and the existing publication/receipt point. |
| 8 | `BuiltinOperation::Simulation` → `simulate_tick` | `Operation("engine simulation")` | Simulation transaction remains indivisible. |
| 9 | `EnvironmentEffects` → all `Stage::Environment` systems and commit | `StageBarrier(Environment)`, ID `engine environment effects` | One ordered batch and the existing publication/receipt point. |
| 10 | `PostSimulationEffects` → all `Stage::PostSimulation` systems and commit | `StageBarrier(PostSimulation)`, ID `engine post simulation effects` | One ordered batch and the existing publication/receipt point. |
| 11 | `BuiltinOperation::ClearEvents` | `Operation("engine clear events")` | Same event cleanup call. |

`simulation_tick` still increments before the stream. A command-triggered stop still reaches bolids, impacts and excavation before the shutdown gate returns; it still skips world inputs and stage batches and does not record completed-tick metrics or advance the tick deadline. Replication remains outside this schedule.

Startup is deliberately outside the recurring stream. Validation injects a separate required `schedule startup` anchor; a `StageBarrier(Startup)` among recurring nodes returns `StartupInTick`. The runtime continues to run Startup once per prepared world after restoration, passing `New` or `Restored`, committing each world batch before content initialization.

## Validation and invariants

- The validator has no operation enum, operation names, operation count or mechanic-specific match. Host operation and dependency IDs are supplied in `ScheduleDefinition`.
- IDs are checked for invalid syntax, duplicate graph nodes, host/system collisions and reserved stage anchor collisions. Every dependency target must resolve. Repeated targets or a duplicated declared edge return `DuplicateDependency`.
- Each required barrier must exist exactly once. Startup is always required and separate. Recurring barriers must retain the neutral phase-role order FixedUpdate → Environment → PostSimulation; a host may omit unused recurring stage roles. Reversed barrier order or a backward dependency returns `InvalidPhaseOrder`.
- Cycles return `DependencyCycle`. Invalid definitions never return a partial plan. A system declared before its own publication anchor remains valid; a system declared after its own barrier is rejected. Systems of one stage share the barrier's phase and are ordered by `(phase, system ID)`. Operations occupy distinct host-declared phase positions, so none can interleave in that batch.
- The plan is created during trusted host bootstrap, not from network data. There is no new fixed registration cap: checked arithmetic covers the virtual startup node, host nodes and installed systems. With `V` total graph vertices, each descriptor's unique targets bound declared edges to `V²`; temporary graph storage is `O(V + E)`, target resolution is `O(E * V)`, and the returned immutable plan retains `O(V)` steps/systems after edge storage is dropped. It is not rebuilt per tick.
- Stable effect-producer IDs, system order within a stage, batch commit points and receipt delivery remain unchanged from #528. Component owner-only authorization from #539 is untouched.

Review cases and source-level outcomes (not executed tests):

| Case | Source-level result |
| --- | --- |
| Same-phase system dependency cycle | `ScheduleError::DependencyCycle` |
| Dependency directed backward across phases | `ScheduleError::InvalidPhaseOrder` |
| Unknown node/system target | `ScheduleError::UnknownDependency` |
| Unknown runtime operation ID | `ScheduleError::UnknownOperation` before runtime service startup |
| Duplicate host node ID | `DuplicateNodeId` at graph validation |
| System ID collides with an operation ID | `DuplicateNodeId`; a system ID colliding with a reserved stage anchor returns `ReservedAnchorCollision` |
| Repeated canonical stage barrier node | `DuplicateNodeId` because its reserved ID is repeated |
| Repeated target or repeated declared edge | `DuplicateDependency` |
| Operation uses a reserved stage anchor ID | `ReservedAnchorCollision` |
| Wrong stage anchor ID or invalid ID form | `InvalidNodeId` |
| Backward dependency or reversed recurring barrier roles | `InvalidPhaseOrder` |
| Required stage barrier omitted | `MissingStageBarrier` |
| Repeated stage in required-stage list | `DuplicateStageBarrier` |
| Startup stage barrier placed in recurring nodes | `StartupInTick` |
| Owner operation inserted within a system batch | Not representable: each stage's systems are forced before its unique barrier node, and host nodes have distinct phases. |
| Stop requested while processing commands | Bolids, impacts and excavation remain before the gate; inputs and later stages remain after it. |

## Static checks

Working directory for every command: `D:/Git/minimium/.local/worktrees/e050-17`; base/current HEAD: `e161333c564888eb109daf02db70da6bf972b210`.

| Command | Status | Exit | Evidence |
| --- | --- | ---: | --- |
| `rtk git -c safe.directory=D:/Git/minimium/.local/worktrees/e050-17 -C D:/Git/minimium/.local/worktrees/e050-17 diff --check` | PASS | 0 | No whitespace diagnostics. |
| `rtk proxy cargo fmt --all -- --check` | PASS | 0 | Root workspace formatting check. |
| `rtk cargo check -p minimium-gameplay --locked --example host_schedule` | PASS | 0 | Public external-plan consumer compiles. |
| `rtk cargo check -p minimium-gameplay -p minimium-game-sample -p minimium-server-runtime -p minimium-server --locked` | PASS | 0 | Gameplay, sample, runtime and process packages compile. |
| `rtk cargo check -p minimium-gameplay -p minimium-server-runtime --examples --locked` | PASS | 0 | Compiles current gameplay examples (including prior 01–06 consumers) and runtime examples (including 16 owned-components consumer). |
| `rtk cargo check -p minimium-server-runtime --locked --example embedded_host` | PASS | 0 | Embedded host consumer compiles. |
| `rtk proxy cargo check --workspace --locked` | PASS | 0 | Final post-guard root check: 1.89 s. |
| `rtk proxy python scripts/check_feature_contracts.py` | PASS | 0 | Root check: four declared graphs and four compile consumers; consumers were compiled, not executed. |
| `rtk proxy python scripts/check_architecture.py` | PASS | 0 | Root check: architecture and declared feature graphs validated. |
| `rtk proxy python scripts/check_file_lengths.py` | PASS | 0 | Root check: 819 files, maximum 500 physical lines. |
| `rtk proxy python scripts/check_static_refactor.py` | PASS | 0 | Root check passed. |
| Unit/integration/E2E tests, examples execution, runtime/game, GPU and benchmarks | NOT RUN | — | Deferred by the project policy; the public example was compiled only. |

Changed hand-maintained Rust file counts checked during review: `gameplay/src/schedule.rs` 376; `server-runtime/src/runtime/schedule.rs` 432; `server-runtime/src/runtime/schedule/host_plan.rs` 137; `server-runtime/src/runtime/schedule/system_context.rs` 82; `server-runtime/src/runtime/schedule/diagnostics.rs` 10; `server-runtime/src/runtime/tick.rs` 478; `gameplay/examples/host_schedule.rs` 72. No test suite or example execution was run.

## Final source and consumer review

Coordinator and independent review: PASS on the complete diff, including the final Startup error-priority guard. The static trace retains all eleven legacy IDs, the same shutdown position, per-world Startup, stage batch boundaries and receipt timing. Reversed recurring stages are rejected, and the neutral validator contains no concrete mechanic enum, names or count. Runtime retains and executes the exact mapped validation result. The owner authorization and persistence contracts from prior stages are unchanged.

`rtk proxy cargo tree -p minimium-gameplay --edges normal,build,dev --prefix none --format '{p}'` ran in this worktree with exit 0. The 16-package graph contains no server/client runtime, storage, transport, platform, telemetry, sample, renderer, wgpu or winit package. The real external `host_schedule` target compiles against public gameplay API and does not execute systems. No tests or test targets, examples/apps/services, runtime/GPU or benchmarks were executed. Implementation/merge SHA, CI and main read-back are recorded in issue #540 during coordinator integration.
