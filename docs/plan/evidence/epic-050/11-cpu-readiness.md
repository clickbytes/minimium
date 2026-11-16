# E050-11 / #534: CPU world and collision readiness

Base/main before this change: `4d3c7d5662105e2829bd05b71c88241c0062423a`.
Branch/worktree: `work/e050-11-cpu-readiness`, `.local/worktrees/e050-11`.
Predecessor #533 was read CLOSED after PR #556 merged and main tree read-back;
CI `43924048539` was SUCCESS before this task began.

## Owner and complete caller migration

Before, application prediction requested `ActiveSceneReady` from a bridge taking
mutable Renderer, FOV and viewport. `ClientSectorCache::ensure_scene` coordinated
sector materialization, collision aggregation, presentation preparation and GPU
activation before returning a report containing a loosely supplied collision index.
Terminal transfer readiness checked resolved content without preparing collision.

After, `minimium-client-runtime::world_readiness::WorldReadiness` owns the actual
SectorManager, pinned interests, immutable world sources/overlays and the existing
two bounded CPU workers. It consumes the prepared source delivered through the
stage-09 content contract. That contract still owns world/material jobs; the
existing sector workers move with their CPU owner rather than extending it into
a generic executor or duplicating generation.

```text
host content completion -> session ticket/source validation -> TerrainInputs
  -> WorldReadiness.ensure_collision(owner, manifest, position, map, inputs)
  -> installed sector tickets -> shared immutable CollisionSnapshot
       -> fixed-step prediction + destination/water validation
       -> desktop derived preparation -> GPU staging/activation -> separate report
       -> terminal CPU readiness -> matching transfer host-ready operation
```

The snapshot constructor and mutable fields are private. Owner, world context or
pending transfer, manifest, map fingerprint, source material/relief/geography,
independent terrain revisions, collision revision and sector payloads stay bound.
`matches`, `matches_terrain` and `covers_position` are read-only admission checks.
The renderer consumes these same Arc sector payloads, not a second terrain solver.
For a speculative step, `ensure_collision_for_step` requests both endpoint
neighborhoods without changing their generator or precision. Valid superset
interests are reused when retrying narrower current-position requests; target work
therefore keeps its tickets. Terminal retains only its current active/bootstrap
and pending owners, releasing old memberships across transfers and reconnect.

| Path | CPU owner and consumer |
| --- | --- |
| Bootstrap | Runtime provider prepares the existing authored/procedural source; absent Earth package remains unavailable. Desktop stages derived geometry separately. |
| Active world | Provider interest uses the exact active membership and current source/overlay revisions. Snapshot installation rechecks current app/session identity. |
| Prediction candidate | Callback returns a CPU snapshot. Current-position CPU readiness precedes integration; candidate coverage, water capsule and session impact/excavation gates precede commit. Missing candidate readiness rolls back speculative character state. |
| Desktop presentation | Host explicitly applies the visible-terrain policy after CPU preparation; flight may bypass that additional gate, never missing CPU coverage. |
| Pending transfer | Separate Pending(transition, context) interest and target source; CPU and presentation reports bind the same destination identity/version. Target data is not installed as active before commit. |
| Terminal | One CPU provider prepares active and pending destination content; host-ready requires its matching snapshot without a GPU or viewport. |
| Commit/cancel/reconnect | Matching promotion moves pending ownership; cancel/release retires tickets and aggregation. Late results cannot install over replacement interests. |
| Public compile consumer | `collision_ready` constructs a real authored prepared source, drives the asynchronous provider, checks identity/coverage, then consumes the snapshot in existing ClientPrediction. It retries destination coverage with restored speculative state/time. |

## Bounds, failure and compatibility

Preserved sector cache/queue capacity: 512; materialization has 32 total pending,
active and completed slots; up to 32 dispatches per poll; collision aggregation
has three owner slots. Public provider admission bounds interests to three and
retained world sources to 512. Cache exhaustion and stopped/spawn-failed workers
return typed errors. Invalid positions/source bindings reject before requesting
work, including unrepresentable dungeon neighbor addresses and a zero planetary
direction. Collision revision allocation is checked instead of wrapping.

Cancellation retires publication authority; already-running pure generation can
finish physically. Results whose tickets are no longer Generating are ignored
before inspecting their success/failure. Aggregation results must match owner and
collision revision. Promotion cannot relabel cross-world geometry: matching
Pending -> Active membership or compatible Bootstrap -> Active is required.
Physical drain uses the existing absolute resource deadline and may report an
unfinished worker; it does not advance the stage-10 logical client clock.

The sector neighborhood, materialization algorithm, full-map/static geometry
handling, collision leaf construction, simulation integrator and generator remain
the existing implementations. Coverage certifies the existing fixed-step sector
neighborhood, not arbitrary capsule extents or unbounded sweeps. Water readiness
remains a separate session/capsule condition. Presentation view, origin and LOD
inputs never enter the CPU provider.

Protocol 21, save 11, generator 6, map/content fingerprints and asset encodings
remain unchanged. No new crate, solver, application-core extraction or execution
framework is introduced. The added dependency is client-runtime -> sectors.
The existing readiness bug requiring geography for every planetary world is
corrected at this owner: only Earth mode 1 requires its prepared package;
procedural planets still require their validated source and water catalog.
Current-position readiness precedes integration. `skip_unready_step` consumes
only due fixed intervals without physics, so neutral-input queueing keeps its
existing cadence/sequence allocation instead of running once per host callback.

## Negative source-review cases

| Case | Required outcome / inspected guard |
| --- | --- |
| Stale context or reconnect result | Snapshot owner/context and current app/session identity must match; retired tickets/aggregation cannot publish. |
| New terrain/source while upload is pending | Provider invalidates affected tickets; snapshot compares source and revisions. Presentation key includes map fingerprint and terrain revision, so an old prepared scene cannot certify new CPU data. |
| CPU ready / GPU not ready | CPU snapshot remains independently usable. Default desktop waits for its additional presentation gate; terminal can proceed with CPU policy. |
| Missing water coverage | Candidate capsule/session water condition remains required independently of terrain collision. |
| Candidate outside coverage | Speculative position is restored and coverage is requested again; no unproved collision snapshot becomes current. |
| Target cancelled after preparation | Pending interest and GPU stage are released; ticket/transition/current-context checks reject late reports. |
| Origin shift / resize | Only derived presentation is restaged. CPU source, physical coordinates and collision identity remain unchanged. |
| Same manifest with changed map | Source fingerprint invalidates CPU tickets and presentation cache scope; GPU activation also checks the snapshot's fingerprint. |
| Failed cancelled job | Stale ticket is ignored before an old worker error can fail newer content. |
| Promotion of unrelated owner | Controlled promotion rejects cross-instance or unrelated membership changes without moving interests. |

## Static verification

Working directory for every command: `D:/Git/minimium/.local/worktrees/e050-11`.
PASS means the command was observed with exit 0; no application/example was run.

| Exact command | Status / evidence |
| --- | --- |
| `rtk proxy cargo check -p minimium-client-runtime --locked --example collision_ready` | PASS, real public consumer. |
| `rtk proxy cargo check -p minimium-client-runtime --locked --examples` | PASS; existing shared helper dead-code warnings remain visible (host_clock 7, injected_transport 4, injected_content_jobs 1). |
| `rtk proxy cargo check -p minimium-client-runtime -p minimium-procgen -p minimium-simulation --locked` | PASS. |
| `rtk proxy cargo check -p minimium-renderer --locked --no-default-features --example cpu_frame` | PASS. |
| `rtk proxy cargo tree -p minimium-client-runtime --locked --edges normal,build,dev --target all --prefix none` | PASS; 118 output lines inspected, no wgpu/winit/renderer/client package. Existing sample dev dependency remains CPU-only. |
| `rtk proxy python scripts/check_architecture.py` | PASS; production graphs renderer 37, terminal 73, embedded server 49. |
| `rtk proxy python scripts/check_static_refactor.py` | PASS; selected structural checks only. |
| `rtk proxy cargo check --workspace --locked` | PASS, final source snapshot. |
| `rtk proxy cargo check -p minimium-client --locked` | PASS, final desktop source snapshot, no production warning. |
| `rtk proxy cargo check -p minimium-client --no-default-features --locked` | PASS, final CPU/terminal source snapshot. |
| `rtk proxy cargo fmt --all -- --check` | PASS after removing collateral formatting edits outside scope. |
| `rtk proxy python scripts/check_file_lengths.py` | PASS, 804 source files, maximum 500 physical lines. |
| `rtk proxy python scripts/check_feature_contracts.py` | PASS, three declared graphs and actual renderer CPU, terminal and embedded-host compile consumers; none executed. |
| `rtk proxy git -c safe.directory=D:/Git/minimium/.local/worktrees/e050-11 diff --check` | PASS; repository line-ending notices only. |
| Unit/integration/E2E, executable examples, game/network/GPU and benchmarks | NOT RUN; explicitly deferred final testing. |

Intermediate app compile, format and length failures exposed unfinished consumers
and were corrected before this final PASS snapshot. The independent source review
also found duplicate elapsed accumulation during preflight integration; the final
code has exactly one accumulation before readiness, preserving fixed-step cadence.
The public example was corrected to restore its due interval and request both
endpoint neighborhoods when retrying a speculative candidate. Coordinator and
independent reviewer inspected the final CPU, presentation, transfer and terminal
paths and found no remaining confirmed blocker. These are source/static findings,
not runtime evidence.

Static review/compilation cannot establish live collision correctness, visual
continuity, water-boundary behavior, cancellation latency or performance. Those
remain runtime/visual acceptance risks for the final verification phase.

## Exact changed files

The list includes new files, deletions and this evidence document.

```text
Cargo.lock
apps/client/README.md
apps/client/src/application.rs
apps/client/src/application/desktop_view.rs
apps/client/src/application/messages.rs
apps/client/src/application/step.rs
apps/client/src/application/step/readiness.rs
apps/client/src/application/world_readiness.rs
apps/client/src/renderer.rs
apps/client/src/renderer/camera.rs
apps/client/src/renderer/collision_worker.rs
apps/client/src/renderer/gpu_scene.rs
apps/client/src/renderer/init.rs
apps/client/src/renderer/sector_scene.rs
apps/client/src/renderer/sector_scene/collision.rs
apps/client/src/renderer/sector_scene/interests.rs
apps/client/src/renderer/sector_worker.rs
apps/client/src/renderer/sector_worker/prepared.rs
apps/client/src/renderer/session.rs
apps/client/src/renderer/session/sector_readiness.rs
apps/client/src/renderer/session/world_install.rs
apps/client/src/terminal_client.rs
apps/client/src/world_content.rs
architecture.toml
crates/client-runtime/Cargo.toml
crates/client-runtime/README.md
crates/client-runtime/examples/collision_ready.rs
crates/client-runtime/src/lib.rs
crates/client-runtime/src/prediction.rs
crates/client-runtime/src/world_readiness.rs
crates/client-runtime/src/world_readiness/collision.rs
crates/client-runtime/src/world_readiness/error.rs
crates/client-runtime/src/world_readiness/interests.rs
crates/client-runtime/src/world_readiness/snapshot.rs
crates/client-runtime/src/world_readiness/workers.rs
crates/renderer/README.md
docs/plan/evidence/epic-050/11-cpu-readiness.md
```
