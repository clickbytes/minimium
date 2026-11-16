# E050-10 / #533: explicit host logical time

Base: `172a1b5d6c74cc27c342e3b9655468bff95156f1` (merged PR #555).
Branch/worktree: `work/e050-10-logical-clock`, `.local/worktrees/e050-10`.
Predecessor #532 was verified CLOSED with its merged main tree and successful
CI run `43916575464` before implementation. This document records source review
and static compilation, not runtime acceptance.

## Ownership and complete migration

Before, session construction/polling, connection attempts, probe epochs,
snapshot receipt and action cooldowns sampled `Instant` internally or received
an inconsistently reused host cadence instant. Transport sampled another instant
when a Ping frame completed. Logical policy already belonged to the session,
controller and application; its time input was implicit.

After, the host samples `Instant::now()` through `apps/client/src/host_time.rs`
and supplies `ClientTime` at construction and every logical operation. The
wrapper does not sample the OS, own an epoch singleton or advance itself.

```text
desktop/terminal host sample -> ClientApplication pure multi-owner preflight
  -> action executor / controller -> connector / ClientSession
  -> supplied sample for lifecycle, queues, cooldowns and content deadlines
  -> neutral Instant passed to injected transport flush
  -> supplied sample returned only at actual tracked full-frame completion
```

| Owner / caller | Before -> after |
| --- | --- |
| `ClientTime`, `ClientTimeError` | New explicit host value and typed Backward/Overflow errors, checked addition and duration conversion; no new crate or virtual-clock framework. |
| `ClientSession` constructors | Hidden epoch/state time -> required supplied sample; validates itself, relative clock conversion and injected transport before Presentation/Authenticate queue admission. |
| Session poll/queue/tool/world input/close | Hidden lifecycle samples -> explicit time argument and pure preflight before completions, dispatch, transport reads/writes, counters or cooldown effects. Internal dispatch uses the already admitted sample. |
| Session content/transfer APIs | Readiness, installation acknowledgment and cancellation use explicit samples; tickets, result validation and active/pending ownership remain unchanged. |
| `session/clock.rs` and `clock/time_input.rs` | Host epoch/current relative milliseconds are explicit, reset uses supplied time; cached getters report the last admitted sample. Existing estimator and UTC calculations stay here. |
| `ConnectionController` | Explicit construction and time on start/poll/admin/queue/cancel/take-session/shutdown. Pure validation includes connector and any current session before controller mutation. |
| `Connector` | Logical deadline and operation watermark use supplied time. A valid start sample is recorded even when Busy rejects admission, without changing attempt ID, worker or active attempt. |
| `ClientTransportPort` / `TcpClientTransport` | Hidden flush clock -> neutral standard-library `Instant` argument and pure guard. Runtime types do not enter transport dependencies. Byte/token completion algorithm remains unchanged. |
| `SnapshotReconciler` | Hidden receipt instant -> explicit receipt/interpolation sample, checked host elapsed conversion and guard before changing tracks or output buffer. |
| `ClientApplication` and action executor | Explicit constructor/step/action time, including actions between steps. Preflight includes app, executor, controller/nested owners and reconciler before prediction or queue mutation. |
| Desktop callers | Camera step, renderer actions, developer input, content and sector readiness pass fresh samples per logical entry. Render cadence and presentation measurements remain separate. |
| Terminal callers | Construction, commands, polling, readiness, disconnect and shutdown use the same explicit contract as desktop. |
| External consumers | Both earlier injected examples, local transport adapter and existing session fixture migrate in this change; `host_clock` compiles the public controlled-sample path. No test cases were added. |
| Ownership documents | Runtime, transport and application READMEs plus `architecture.toml` describe actual clock ownership and resource exceptions. |

## Clock-site classification

This map covers the logical runtime and app clock sites reviewed before editing
and the retained reads reviewed afterwards. Integer counter `checked_add` calls,
byte lengths and simulation tick IDs are not host-clock arithmetic.

| Sites | Clock domain / decision |
| --- | --- |
| `session.rs`, `session/receive.rs`, `receive/decode.rs` | Explicit `ClientTime`: creation/state-since, Welcome timeout, heartbeat cadence/timeout, incoming sample decode time and accepted session state. |
| `session/transport.rs`, transport `client.rs` / `client/tcp.rs` | Explicit sample for Ping actual-send completion and RTT; flush guard precedes I/O/accounting. Enqueue remains distinct from completed write. |
| `session/clock.rs`, `clock/time_input.rs`, `clock/estimate.rs`, environment synchronization | Explicit monotonic epoch/probe enqueue/receive/poll-gap/staleness; checked host relative milliseconds. Estimated UTC/world time remains authoritative sample plus the existing estimator, never `SystemTime` lifecycle time. |
| `session/content.rs`, `world_jobs.rs`, `receive.rs` | Explicit logical 300-second material/world preparation deadlines, including waiting/deferred work and existing GPU acknowledgment window. Host worker HTTP budgets are a separate resource clock. |
| `session/excavation.rs`, `replication.rs`, dispatch paths | Explicit operation time for tool use and queue/counter commit; world context, readiness and authoritative result checks are unchanged. |
| `connection_controller.rs`, `connection_controller/time.rs`, `connector.rs` | Explicit logical start/poll/retry/cancel/admin/session lifecycle watermark. Empty receiver polls compare supplied time with logical attempt deadline. |
| `connector.rs`, `connector/worker.rs` | Retained physical OS connect/DNS budgets, `completed_at` resource fact and worker-drain waits. A ready result completed within its physical budget wins a late logical poll, as before. Availability is still externally scheduled. |
| `bootstrap.rs`, `bootstrap/process.rs`, `bootstrap/launch_config.rs` | Retained physical process/config-preview/startup deadlines and socket wait budgets. These bound actual OS work rather than session policy. |
| `connection_controller/shutdown.rs`, connector cleanup | Explicit public logical preflight plus physical absolute drain deadline; 28-second default controller cleanup. Drop closes resources using stored admitted session state and physical bounded drain. |
| `snapshots.rs` | Explicit local receipt/interpolation time. Server timestamp monotonic ordering, `saturating_add` and interpolation-delay clamps remain model arithmetic rather than masking backward host time. |
| App `application.rs`, `application/time.rs`, `application/step.rs`, actions and messages | Explicit logical app/action sample, prediction elapsed input and remote snapshot receipt. Pure composed validation precedes any participating owner's effects. |
| App desktop/terminal entry points and `host_time.rs` | OS reads belong to the host adapter; fresh sample for each logical operation, including an action before the next step. |
| `environment/demo.rs`, consumed only by renderer environment clock | Deliberate offline-demo UTC producer: one `SystemTime::now()` anchor plus independent `Instant::now()`/`elapsed()` progression. No network fallback, session timeout, lifecycle gate or input cooldown consumes this clock. Its algorithm is unchanged. |
| App `content_jobs.rs`, content workers and resource adapters | Retained physical worker shutdown and HTTP/package/cache operation budgets (including existing 300-second HTTP and 30-second socket budgets); not logical preparation authority. |
| Renderer collision/sector/cloud workers | Retained physical worker timing, polling/drain deadlines and preparation measurements. CPU completion does not authorize session/world changes. |
| Renderer init/window/camera/render/environment runtime, `stats.rs` | Retained window redraw scheduling, cosmetic epoch, FPS/frame/GPU/CPU profiling and visual readiness feedback. They do not supply lifecycle/cooldown time. |
| Renderer impacts/bolids/meteors, developer visual input, session visual message handling | Retained visual expiry, animation, local meteor seed/spawn and hit-feedback clocks. Server monotonic estimates still provide network event age where previously used. |
| Console editor/input drawing/state and terminal status | Retained cursor blink, chat visibility and five-second status presentation cadence. |
| App configuration filenames and diagnostic/resource uniqueness | Retained `SystemTime` uniqueness values, independent of logical policy. |
| `session/tests.rs` | Existing unexecuted fixture supplies a host sample explicitly. No test suite was added or run. |
| Fixed simulation and authoritative environment | Physics uses fixed simulation steps; input tick/sequence counters use server ticks. UTC/world clock and `time_scale` remain separate from host monotonic time. |

## Validation and source cases

`ClientTime::validate_after` checks ordering and representability of the longest
300-second logical horizon. Session clock preflight also checks conversion to
relative `u64` milliseconds and its horizon. Transport preflight checks its own
last accepted flush sample before the session can enqueue its initial handshake.
Errors retain typed causes through QueueError, controller errors and transport
errors. No error path silently clamps a backward host sample to zero.

Preflight is pure. Each composed app entry validates all participating owners
before admitting its sample; subsequent calls use that same sample. A valid
sample can advance an operation watermark even when domain admission returns
Busy or another semantic rejection. Time rejection itself changes no logical
watermark, queues, counters, lifecycle, completion consumption or transport I/O.

| Case | Source-review evidence / preserved behavior |
| --- | --- |
| Equal sample | Ordering is nondecreasing. Repeated polls/actions are accepted with zero elapsed; existing timeout comparisons still use their prior boundaries. |
| Backward sample | Public owner guards return `ClientTimeError::Backward` before effects. Composed app preflight prevents an externally advanced nested session from causing partial app/action mutation. |
| Overflow | Checked Instant deadline addition and checked elapsed-millisecond conversion return `Overflow` before queue/lifecycle changes. `host_clock` explicitly handles checked-add overflow. |
| Busy then earlier poll | Connector records the valid Busy-call sample after all logical time/deadline checks; a later sample earlier than that call is rejected. Attempt counters and worker ownership do not change on Busy. |
| Action between ticks | Host takes a fresh sample per logical operation; later step does not reuse an older redraw/cadence sample. Weapon/tool gates still commit only after queued output. |
| Large dt / long gap | Lifecycle timeout decisions use supplied elapsed time. Existing >10-second probe gap reset and estimator staleness remain; prediction input keeps its previous fixed-step elapsed cap/catchup behavior. No unbounded catchup was introduced. |
| Reconnect | Controller ordering survives session replacement; each newly constructed session and clock receives the admitted current sample. Existing attempt IDs, content session IDs and cancellation exclusion remain. |
| Paused or scaled UTC | `time_scale` and authoritative UTC samples never drive heartbeats, preparation deadlines, retries or cooldowns; those use independent supplied monotonic time. |
| Late poll with ready connector result | Completion timestamp is an actual OS resource fact. Ready on-budget result wins before empty-poll logical timeout evaluation, preserving existing ordering. |
| Partial writes / Ping queueing | Ping starts only after the stable tracked frame crosses its completed-byte boundary, using that flush's supplied sample. Queue acceptance does not start RTT. |
| Invalid time at constructor | Session validates the injected port before Presentation/Authenticate enqueue; failure drops owned resources without admitting handshake frames. |
| Snapshot near server timestamp limit | Checked host elapsed conversion remains; server timestamp saturation remains unchanged and does not reject a valid host sample. |

`host_clock` uses the real public ClientSession with the stage-08 bounded local
transport and stage-09 local content adapter. It supplies equal/advancing samples,
handles a backward error, compares transport accounting before/after rejection
and handles overflow. It was compiled only. It is not a fake session or a
replacement content/transport implementation framework.

## Preserved intervals and compatibility

Welcome 5s; heartbeat cadence 10s and timeout 20s; Ping timeout starts on actual
flush completion. Clock probes retain 1s bootstrap / 5s stable cadence, 2s probe
timeout, 2s maximum RTT, eight offset samples, >10s gap reset, 15s stale and 60s
unavailable thresholds. Tool cooldown remains 500ms, weapon fallback 300ms.
World/material logical preparation remains 300s; remote connect 10s and local
connect 35s with the 30s cold Earth allowance remain. App shutdown retains 30s
with two seconds reserved for logging; controller default resource cleanup 28s.

Protocol 21, save 11, generator 6, assets, frame bytes, transport queue capacities,
partial-write/read fairness, content tickets/cancellation, transfer readiness,
world publication, physics, estimator and interpolation algorithms are unchanged.
Public Rust call signatures intentionally require explicit samples; all current
production callers, earlier external examples and existing fixture are migrated.
No extra dependency, generic scheduler or virtual-clock framework was introduced.

## Verification

All PASS entries below were actually observed with exit 0 in this worktree.
Implementation and coordinator independently reviewed the diff. The coordinator
repeated the full matrix after the final Connector Busy watermark fix;
the confirmed post-fix checks are identified below.

| Exact command | Status / snapshot |
| --- | --- |
| `rtk proxy cargo check --workspace --locked` | PASS, implementation before and coordinator again after final Busy fix. |
| `rtk proxy cargo check -p minimium-client-runtime --locked --examples` | PASS before and after Busy fix, implementation and coordinator. Shared helper warnings: host_clock 7, injected_transport 4, injected_content_jobs 1. |
| `rtk proxy cargo check -p minimium-client-runtime --locked --example host_clock` | PASS, coordinator explicitly checked again after Busy fix; compiled only. |
| `rtk proxy cargo check -p minimium-client --locked` | PASS, app helper before and coordinator again after Busy fix. |
| `rtk proxy cargo check -p minimium-client --no-default-features --locked` | PASS before and after Busy fix, implementation/helper/coordinator. |
| `rtk proxy cargo fmt --all -- --check` | PASS before and after Busy fix. |
| `rtk proxy python scripts/check_architecture.py` | PASS before and after Busy fix; renderer 37, terminal 73, embedded server 49 production-package graphs. |
| `rtk proxy python scripts/check_feature_contracts.py` | PASS, coordinator before and again after Busy fix; three consumers compiled, none executed. |
| `rtk proxy python scripts/check_file_lengths.py` | PASS before and after Busy fix; 796 source files, maximum 500 physical lines. |
| `rtk proxy python scripts/check_static_refactor.py` | PASS before and after Busy fix; selected structural checks only. |
| `rtk proxy git -c safe.directory=D:/Git/minimium/.local/worktrees/e050-10 diff --check` | PASS before and after Busy fix; line-ending conversion notices only. |
| Tests, executable examples, applications, network/game/GPU runs and benchmarks | NOT RUN; deferred final testing policy. |

Shared example helper dead-code warnings were not hidden with broad allows.
Earlier intermediate compile/privacy/file-length failures were corrected before
the final PASS snapshots. No required static failure remains. Static checks do
not prove live DNS scheduling, partial socket writes, reconnect races, cancellation
latency, visual output or shutdown timing. These remain deferred runtime/visual
acceptance. This change itself does not claim real-time determinism of workers.

## Exact changed files

The list includes new files and this evidence document.

```text
apps/client/README.md
apps/client/src/application.rs
apps/client/src/application/actions.rs
apps/client/src/application/desktop_ops.rs
apps/client/src/application/desktop_view.rs
apps/client/src/application/game.rs
apps/client/src/application/messages.rs
apps/client/src/application/step.rs
apps/client/src/application/time.rs
apps/client/src/bootstrap.rs
apps/client/src/client_commands.rs
apps/client/src/client_commands/movement.rs
apps/client/src/host_time.rs
apps/client/src/main.rs
apps/client/src/messages.rs
apps/client/src/renderer/bolids.rs
apps/client/src/renderer/camera.rs
apps/client/src/renderer/controller.rs
apps/client/src/renderer/controller/action.rs
apps/client/src/renderer/dev_input.rs
apps/client/src/renderer/excavation.rs
apps/client/src/renderer/init.rs
apps/client/src/renderer/materials.rs
apps/client/src/renderer/overview.rs
apps/client/src/renderer/session.rs
apps/client/src/renderer/session/remote_players.rs
apps/client/src/renderer/session/sector_readiness.rs
apps/client/src/renderer/window_events.rs
apps/client/src/terminal_client.rs
apps/client/src/terminal_client/actions.rs
architecture.toml
crates/client-runtime/README.md
crates/client-runtime/examples/host_clock.rs
crates/client-runtime/examples/injected_content_jobs.rs
crates/client-runtime/examples/injected_transport.rs
crates/client-runtime/examples/support/local_transport.rs
crates/client-runtime/src/client_time.rs
crates/client-runtime/src/connection_controller.rs
crates/client-runtime/src/connection_controller/shutdown.rs
crates/client-runtime/src/connection_controller/time.rs
crates/client-runtime/src/connector.rs
crates/client-runtime/src/controller_error.rs
crates/client-runtime/src/lib.rs
crates/client-runtime/src/session.rs
crates/client-runtime/src/session/clock.rs
crates/client-runtime/src/session/clock/time_input.rs
crates/client-runtime/src/session/content.rs
crates/client-runtime/src/session/content_access.rs
crates/client-runtime/src/session/excavation.rs
crates/client-runtime/src/session/operation_error.rs
crates/client-runtime/src/session/receive.rs
crates/client-runtime/src/session/receive/decode.rs
crates/client-runtime/src/session/receive/decode/dispatch.rs
crates/client-runtime/src/session/receive/decode/dispatch/excavation.rs
crates/client-runtime/src/session/replication.rs
crates/client-runtime/src/session/tests.rs
crates/client-runtime/src/session/transfer.rs
crates/client-runtime/src/session/transport.rs
crates/client-runtime/src/session/world_jobs.rs
crates/client-runtime/src/snapshots.rs
crates/transport/README.md
crates/transport/src/client.rs
crates/transport/src/client/tcp.rs
docs/plan/evidence/epic-050/10-logical-clock.md
```
