# A2 — E004-15 time, wire, session and persistence audit

Date: 2026-10-30. Base: `93b1264502e33f01a9769901fbbc72697bf904e0`.
Commit: `c3a8fab98e9f18973cc11ffd09382c0e4ceb330f`.
Branch/worktree: `work/e004-15-a2-clock`, `D:/Git/minimium-e004-a2`.
State after commit: clean. Static review complete for A2 scope; no unresolved static defect found.
Runtime: **RUNTIME NOT RUN**. No game, server, loadtest or test binary was executed.

## Changes

| File | Change |
| --- | --- |
| `crates/protocol/src/tests.rs` | Rename existing `environment` fixture helper and both calls to `environment_fixture`; remove local-binding shadowing compilation failure without adding a test/scenario. |
| `apps/client/src/session/receive.rs` | Transition commit retains the TCP send buffer/offset and pending Ping state. |
| `apps/client/src/session/environment.rs` | Keep the in-flight clock probe across a same-session world transfer. |
| `apps/client/src/session/clock.rs` | Remove the now-unused transfer-specific probe cancellation method. |
| `apps/server/src/runtime/admission.rs` | Portal and teleport inputs observe the existing pending-input budget, including reserved slots for client departure. |

### Defects fixed

1. **A2-01 — truncated TCP frame at world commit.** Before the fix, `TransitionCommitted` discarded all pending client bytes, including the remainder of an already partially written frame. Subsequent bytes could then be consumed as the missing payload, corrupting framing and disconnecting or misinterpreting probes/commands. The transport queue now drains normally. `ClientSession::queue` wraps gameplay inputs in the old `WorldContext`, and the server rejects that old instance/revision/membership epoch after commit. Global Ping and TimeProbe remain valid because the clock/nonce is shared across the transfer. Relevant sources: `session.rs:245`, `session/receive.rs:119` and `:372`, `session/environment.rs:41`, `server/runtime/admission.rs:112`, `server/world_registry.rs:223` and `:456`. Future regressions: V03/V06/V07/V19, including a partial frame plus probe/Ping spanning commit.
2. **A2-02 — missing portal admission bound.** The contextual UsePortal branch pushed into `pending_world_inputs` without checking the 4096-entry budget. MapTeleport checked the count but omitted departure reservations. Both now use the same `pending_world_inputs.len() + clients.len() < MAX_PENDING_WORLD_INPUTS` invariant as ordinary gameplay admission, leaving room for the mandatory Leave events. Overflow increments rejected-input metrics and does not mutate game state. Sources: `server/main.rs:34`, `runtime/admission.rs:20`, `:46`, `runtime/network.rs:96`. Future regressions: V03/V07/V19 (flood at a tick boundary, simultaneous disconnects).
3. **A2-03 — protocol fixture compilation.** A local `environment` binding shadowed the helper used again in the same function. Renamed helper. Existing test source compiles; runtime assertions remain NOT RUN.

## Source audit

### Authoritative clock and units

- `crates/core/src/time/types.rs`: POSIX UTC `i64` milliseconds in `[946684800000, 4133980800000)`, monotonic `u64` milliseconds, `u32` ppm with 1x = 1,000,000 and max 1,440,000,000. ClockId only 1; revisions are nonzero and checked. ClockSample and SavedClockState reject RealTime rates other than 1x.
- `crates/core/src/time/evaluation.rs:25`: clock evaluation uses the immutable anchor and checked `i128` subtraction/multiplication/addition, not accumulated ticks. `set_time_scale` evaluates the previous date first, so a rate change does not jump UTC. `set_world_time` preserves a TimeScale rate or uses 1x when leaving RealTime. `set_clock_mode` uses supplied wall UTC only when entering RealTime. Revision increment is checked before returning a replacement anchor.
- `crates/platform/src/commands/time.rs:31` and `:100`: RFC3339 UTC accepts Z/+00:00 and 0–3 fractional digits, rejects leap seconds/out-of-range dates; ppm parser accepts exact decimal values with at most six fractional digits and checked integer arithmetic, without float rounding.
- `apps/server/src/environment/clock.rs:37`: fresh random 16-byte session nonce, `Instant` epoch, bounded UTC startup and restore. `:88`, `:285`: range exhaustion pauses at the last valid date and increments revision; `:102` checks wall-clock difference against 250 ms. `runtime/operations.rs:4` samples UTC every five real seconds even without players, and uses independent `Instant` autosave deadlines.
- `runtime/tick.rs:8` and `:196`: skipped simulation ticks are accounted for without adding clock deltas; physics remains `TICK.as_secs_f32()`. Snapshot time at `:248` remains process uptime. Fire/chat cooldowns in admission use `Instant`; session Welcome/heartbeat timers use `Instant`. Environment scale never enters these paths.

### Complete producer/consumer path

- `server/runtime/environment.rs:13`: EnvironmentClock is sampled at an explicit process-monotonic instant; the instance supplies its profile/ClockId, and the embedded asset identity supplies model/catalogue metadata. `admission.rs:212` sends a valid sample/descriptor with Welcome and marks its current context; `environment.rs:41` only syncs that context every five seconds or after a revision change.
- `protocol/environment_codec.rs:10`: 54-byte ClockSample + 49-byte descriptor, exactly 103 bytes; IDs/mode/rate/policy and matching clock IDs are checked. `framing.rs:18` rejects a version other than 12 once the 4-byte header is available. `reading.rs:25` bounds every field; `reader.finish()` rejects extra payload. `encoding.rs:299` rolls back the output vector on failed encoding.
- `protocol/limits.rs`: common payload limit 4096 B and full frame 4100 B. Server preallocation maximum is the 1124-byte full Snapshot; new environment frames (139 B including header) and Welcome/transition suffixes remain below it. Queue post-encode checks enforce the 64 KiB cap even if an encoded message exceeds the preallocation estimate.
- `client/session/receive.rs:204`: validation precedes publishing Welcome/environment changes. `session/environment.rs:108` rejects changed nonce/clock ID, regressing revision or monotonic time, and same-revision changes to mode/rate/policy or decreasing date. Descriptor validation checks profile against topology, current model/catalogue identity and embedded SHA before entering a world, including Interior.
- `session/clock.rs:187`: NTP-style `t1..t4` uses checked `i128`, exact echoed t1/sequence, 0..=2000 ms corrected RTT and current nonce. `clock/estimate.rs:53` projects UTC from the server sample plus local monotonic offset; no network wall-clock dependency. `clock/smoothing.rs:19` confines gradual same-revision corrections to presentation, while a new revision takes effect immediately.
- `session/clock.rs:262` creates one EnvironmentFrameInput (raw UTC/render UTC/context/revision/rate/quality). `renderer/environment/clock.rs:8` publishes it only for the installed world's instance/content revision. The terminal uses the same ClientSession (`terminal_client.rs:139`); loadtest setup and worker use the shared v12 decoder and safely consume EnvironmentSync/TimeProbeReply (`client_setup.rs:224`, `worker.rs:367`). The load generator does not itself implement astronomical rendering or precision clock probes; V03/V20 still require real runs.

### Lifecycle

- Prepare: `server/runtime/inputs.rs:144` obtains a fresh clock sample for the target offer; `client/session/environment.rs:25` stores its descriptor as pending while refreshing the shared clock. Source profile/context stay active until commit.
- Ready: `session.rs:197` only queues Ready for the pending transition; `server/world_registry.rs:162` validates source membership, transition ID and target context. Commit at `:223` requires both client ready and sector ready, updates membership once, then `runtime/tick.rs:149` captures a new sample for TransitionCommitted (not the old prepare sample).
- Commit: `session/receive.rs:334` requires the pending transition/context and validated manifest; `session/environment.rs:41` requires the pending descriptor. The profile/context and shared clock sample switch together. A2-01 preserves transport, Ping and probe state over this operation.
- Cancel: server `world_registry.rs:190` only cancels a matching pending transfer; the client removes only that pending descriptor. The common clock continues in the source world and Interior.
- Stale sync: `session/environment.rs:66` ignores non-current context before changing state. Stale commit with no matching pending tuple is ignored. Current-session nonce mismatch is a connection error, never a fallback to local time.
- Teleport: `runtime/inputs.rs:198` changes position/sector loading within the same context; it does not set clock/config/seed. Renderer MapTeleportResult updates the observer position and origin. V07 must check the resulting cache and visual scene with A4.
- Reconnect/close: `session.rs:220` clears clock sample, descriptor, pending environment, buffers and estimator; a new session has a new local epoch. The Welcome nonce establishes the new server session.
- Demo: `environment/demo.rs:25` is an explicit local UTC anchor plus monotonic elapsed; renderer clears it before a network attempt. A network failure cannot silently reuse the machine wall clock.

### Boundedness

- `session/clock.rs:38`: 1-second bootstrap until four accepted replies, then 5 seconds; one in-flight probe, 2-second timeout, at most eight retained offsets with 60-second age, 15-second stale and 60-second unavailable state. Poll gaps over 10 seconds clear estimator state and restart bootstrap without catch-up loops.
- `server/connection.rs:132`: two probes per sliding real second per identified connection; rejection replies throttled to once a second in admission. Output is capped at 64 KiB/256 frames. Client buffers are 64 KiB, reading at most 16 KiB per poll. Server reads 1024 B/32 messages per client per poll. A2-02 closes the portal exception to the 4096 pending-world-input budget.
- `storage/persistence.rs:42`: request channel capacity one; queue_world_save refuses another pending save. Completion channel is technically unbounded but production admissions allow only one pending save, so completions cannot accumulate without another owner's poll/admission. Shutdown is a separately awaited request.

### Save v8 and authority

- Manual save captures through `server/commands.rs:95`; autosave through `runtime/operations.rs:37`; shutdown through `lifecycle.rs:54`. All capture `SavedClockState` on the state-owning thread before enqueueing. `player_store.rs:275` copies immutable clock/manifests/sectors and Arc player shards; later updates use Arc::make_mut. The worker writes this captured snapshot without reading a clock.
- `storage/format.rs:45`: v2–v8 remain accepted; v1 remains rejected. V2 players get full health, v3 preserves health, v4+ locations/manifests, v5+ world manifests, v6+ sectors, v7 existing planet manifests. V8 additionally requires exactly one valid CLOCK after WORLD and before SECTOR/players. A malformed/duplicate/missing/out-of-order record fails before returning loaded state; the loader never overwrites the source snapshot.
- `startup.rs:60`: environment config is for a new data directory only. Existing v2–v7 snapshots therefore receive default RealTime/1x/revision 1/current UTC. The historical demo geometry migration path is preserved separately with its backup and explicitly initializes a clock; E004 does not change that historical migration's geometry contract.
- `core/time/evaluation.rs:162`: v8 RealTime uses current UTC; TimeScale/Pause uses saved date; Advance adds nonnegative wall elapsed times rate. Backward wall time is reported and gives zero advance. Overflow/range errors stop startup without replacing the snapshot.
- `storage/format.rs:334`: the 16 MiB serialized limit is checked before replacement. `platform/files.rs:29`: temporary write + sync + atomic rename/MoveFileExW, and directory sync on Unix; errors clean only the temporary file. No deletion of the previous valid snapshot before replace.
- Date/rate/clock revision are only clock state; seed, map fingerprint, generator digest and content revision remain separate immutable inputs. The audit found no time-setting path into generation state.
- Time mutations are exposed only by `platform/commands/time.rs` through the token-authenticated loopback admin endpoint (`server/admin.rs:152`) and local server console. Client command registry exposes only `time status` (`client_commands/environment.rs:17`); no game-protocol time mutation exists. Parsing finishes before execution at tick boundary.
- Local server ownership: `client/bootstrap.rs:46` is a no-op for Attached and performs shutdown only for Owned. `connection_controller.rs:202` preserves/reaps owned children; no external process is selected by process name.

## Exact static checks performed

Windows host `x86_64-pc-windows-msvc`; `rustc 1.99.0 (b940084d7 2026-09-28)`, LLVM 23.1.1.
All Cargo checks used isolated `CARGO_TARGET_DIR=D:/Git/minimium-e004-a2/target/a2`.
Checks were executed on the exact source tree later committed as `c3a8fab98e9f18973cc11ffd09382c0e4ceb330f`; no subsequent source changes.

| Command | Result |
| --- | --- |
| `rtk cargo fmt --all` | Exit 0; only intended files changed. |
| `rtk cargo fmt --all -- --check` | Exit 0. |
| `rtk cargo check -p minimium-client -p minimium-server -p minimium-protocol --locked` | Exit 0, 132 crates, 23.26 s; no emitted warnings. |
| `rtk cargo check -p minimium-protocol --tests --locked` | Exit 0, one crate, 0.23 s. Compilation of existing test source only; no tests executed. |
| `rtk cargo clippy -p minimium-client -p minimium-server -p minimium-protocol --locked -- -D warnings` | Exit 0; No issues found. |
| `rtk python scripts/check_file_lengths.py` | Exit 0; 248 source files, maximum 500 physical lines. |
| `rtk git -c safe.directory=D:/Git/minimium-e004-a2 diff --check` | Exit 0. |
| `rtk powershell -NoProfile -Command '<process-scoped safe.directory>; & C:/Users/ppotepa/.codex/scripts/repo-check.ps1'` | Exit 0, identity/HTTPS auth PASS as clickbytes, executed immediately before commit. |

Git's worktree ownership check needed a process-scoped `safe.directory` override; no global Git config, identity or credentials were changed.
No test suite/scenario was added. No runtime result is inferred from compilation.
Whole-workspace release/allocation-metrics build and architecture graph checks are A1's combined-head gate.

## E004-16 handoff

V01–V10 and backend V19 remain NOT RUN; server V20 remains NOT MEASURED.
Use controlled clock values rather than changing the host clock; per-run data dirs/ports/process handles.
Add explicit regression cases for A2-01 (TCP partial write at commit, Ping/probe preserved, old context rejected) and A2-02 (portal/teleport flood plus departures stays within the pending-input budget).
Other required scenarios remain those in the user's V01–V10/V19/V20 plan, including v2–v7 fixtures, restart nonce, timing asymmetry, real loopback processes and owned/attached server lifetime.
No outstanding A2 implementation blocker remains after this commit. Final runtime/visual correctness is intentionally unverified.

## Cross-review follow-up

Reviewed baseline `93b1264..190fa3e` read-only in the integration worktree. The full Cargo package-ID graph, transitive normal/build filters (including external packages), four explicit protected roots, path diagnostics and `--all-features` satisfy the architecture boundary requirement. The path-scoped LF attributes preserve pinned/generated bytes without changing the declared source pins. FFI locking remains private to the client astronomy crate; no new server/admin capability is exposed.

The prepared GPU batch is a single fixed `[StarInstanceGpu; 500]`, uploaded after successful surface acquisition and before draw. A skipped surface leaves the uploaded stamp unchanged and drops the prepared batch, requesting preparation again next frame. Draw checks the same frame stamp. Uniform upload and caster preparation timers have separate CPU scopes; no GPU timing claim is made. No blocker found in that data path by A2 source review.

A4 found retained target resources after cancellation: the protocol cancel context is the source/current context, while the renderer must release the stored pending target. A4 owns retaining the target after Ready and releasing it on accepted cancel/disconnect.

**A2-04 — stale cancellation crossed the session boundary.** `ClientSession` previously left its state unchanged for mismatched TransitionCancelled but still published that message to the renderer. Fix commit `90fe22dee1b140fd26cb92a61f38e1d525ba1ee3` (only `apps/client/src/session/receive.rs`) ignores unmatched pending transition IDs or non-current source WorldContexts before event publication. Accepted cancel clears pending/ready/environment and forwards the source-context event; A4 can safely use its retained target. Formatting, file-length and diff checks exit 0; repo-check identity/auth PASS immediately before commit. No build or test executed during cross-review. Combined-head build remains A1's gate.

`docs/plan/epic-004-final-verification.md` covers the required A2 V01–V10/V19/V20 matrix and explicit authorization gate. Add regression detail for A2-04/A4: cancel before and after Ready, stale ID/context, and disconnect/reconnect while a target is prepared (V06/V07/V18/V19).

One documentation correction was reported to A1: startup prose in architecture must place clock restore after Startup's persistence worker/bind/endpoint setup, but before the runtime accepts game clients. Source ordering is `startup.rs::initialize` then `runtime.rs::new` then `run`.

Final A2 cross-review acceptance: **`8a0a33d703ae356fe3774df7cf5461e52d296975`**. The session now filters stale ID/source-context cancels; the renderer retains target context after Ready, releases the stored non-active target on accepted cancellation, and cleans pending targets on disconnect/new session. Commit installation clears the completed target entry. The final verification document explicitly includes these regressions and skipped-star-upload stamps; startup prose is corrected. No remaining A2 blocker found. This acceptance is source review, not runtime evidence; no builds/tests were run during the final review.
