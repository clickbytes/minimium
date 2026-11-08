> Status 2026-11-07: E046-01–17 have source-review reports and closed GitHub issues. E046-17 final formatting, static checks, builds and WGSL validation passed. E046-18 reports the final evidence and separate runtime backlog. Testów i scenariuszy uruchomieniowych nie wykonywano. Earlier Complete labels below are historical.

# Architecture execution

This document tracks the requested engine-foundations plan. Steps are closed only after their implementation and consumer review are complete.

The current source evidence for every step 01–45, with producer and consumer,
is in [E046 step evidence](architecture-issues/E046-step-evidence.md). The table
separates source review from E046-17 compilation and later runtime acceptance.
The final E046-17 command results and E046-18 runtime backlog are in
[E046 final verification](architecture-issues/E046-final-verification.md).

## Initial state

- Worktree started on `codex/epic-008-config-cli-console` at `f08e8a9` with a large set of tracked and untracked changes. They were preserved and reviewed in place; no reset, commit, or push was made.
- The complete entry `git status --porcelain=v1 -uall` is preserved in `docs/architecture-issues/E046-initial-git-status.txt` (captured before that snapshot file was created).
- `concat.txt` was untracked at start and remains untouched.
- `codegraph status` reported that this repository is not initialized; no `.codegraph/` index was present and no index was created.
- Current package ownership is recorded in `architecture.toml`; current architecture guidance is in `docs/architecture.md`.

## Step status

| Step | Status | Change / verification state |
|---|---|---|
| 01 | Source reviewed | Starting branch/HEAD, broad dirty tree, untouched `concat.txt`, and absent CodeGraph index recorded above. Code changed: documentation only. Compilation: not run. Runtime behavior: unverified. |
| 02 | Source reviewed | Property reads dispatch by category. Time uses only the active session's environment frame and returns unavailable data when absent; movement reads use `ConnectionController`'s server policy; presentation reads use `ClientOptions` and explicitly report no renderer. Evidence and property map below. Code changed: yes. Compilation: not run. Runtime behavior: unverified. |
| 03 | Source reviewed | Receiver gradient guards degenerate normal/light axes and returns at alignment <=0.005 before division by `nz`; the correction fades smoothly through 0.05. Slope normalization is guarded. Center-texel comparison, 3x3 PCF, and existing bias remain. Numerical source review only; WGSL validation and visual behavior are unverified. |

### E046-01 source evidence

`TextProperties::execute` in `apps/client/src/terminal_client/properties.rs` separates runtime-owned reads from presentation reads. The complete catalog is in `apps/client/src/client_commands/properties/catalog.rs`:

| Category | Keys | Read path |
|---|---|---|
| Time | `world.time.mode`, `world.time.scale`, `world.time.offline_policy`, `world.time.utc`, `world.time.revision` | `ConnectionController::session()` → `environment_frame()` → `format_time_value`; absent session/time data is rendered as unavailable. |
| Movement | `player.movement.fly`, `player.movement.speed`, `player.movement.accel` | `ConnectionController`'s current `flight_enabled` and `MovementPolicy`; output includes server-policy revision. |
| Application preferences | `ui.locale`, `client.input.bindings` | `TextProperties::execute` reads/writes `ClientOptions` and `SettingsStore`; `set` and `reset` update these values in the text frontend. `session_settings` supplies save overrides. Neither key is marked renderer-unavailable. |
| Presentation | `client.camera.fov`, `client.input.mouse_sensitivity`, `client.graphics.sky_quality`, `client.sky.names`, `client.weather.cloud_detail`, `client.debug.atmosphere`, `client.debug.terrain`, `client.debug.clouds`, `client.sky.demo` | `TextProperties::get` → `current_value` reads the terminal's options; `TextProperties::new` marks effective state unavailable because the text frontend has no renderer. |

The shader review in `crates/renderer/src/shaders/shadow_sampling.wgsl` found these divisions/normalizations and guards:

| Expression | Guard or denominator bound |
|---|---|
| Normalize receiver normal and projected light axis | Return if either squared length is `<= 1e-20`; use `inverseSqrt` only after this check. |
| `dot(normal,z) / dot(z,z)` (`nz`) | `dot(z,z)` is bounded below by `1e-20`; alignment `<= 0.005` returns first, and `abs(nz) <= 1e-20` is checked before dividing the gradient by it. |
| Gradient components over `dot(u,u)` and `dot(v,v)` | Each denominator is bounded below by `1e-20`. |
| Normalize normal and light direction for slope | Squared lengths are checked via the `select` condition and denominators are bounded below by `1e-20`. |
| `clip.xyz / clip.w` | Earlier return when `clip.w <= 0.000001`. |
| `bias_m / depth_range_m` | `depth_range_m` is bounded below by `0.000001`. |
| Texel-center divisions by texture dimensions | `crates/renderer/src/shadow_target.rs::create_target` clamps resolution to at least `1` (including the disabled target), so both texture dimensions are nonzero. |
| `pcf_sum / sample_count` | `sample_count` is bounded below by `1.0`. |

The 3x3 loops retain nine candidate PCF taps, with depth compared at each sample texel's center. The world-space bias inputs and shadow pass/resolution are unchanged in this shader diff. This establishes source-level safeguards only; no claim is made about visible flicker or shadow quality. Compilation and WGSL validation remain deferred to E046-17.

`crates/renderer/src/shader_source.rs` includes this shared sampling function for world, terrain, and ocean pipelines; the corresponding `world.wgsl`, `terrain.wgsl`, and `ocean.wgsl` call sites pass their receiver normal and selected light direction into `shadow_visibility`.

## E046-02 source evidence

Current review addendum: `apps/server/src/main.rs::run` now captures `working_dir`
once and passes it to both initial `AppPaths::resolve_from` calls and
`resolve_server_paths` via `AppPaths::resolve_selected_from`. The startup
configuration-failure status uses the same base directory. This removes a
second `current_dir()` read between diagnostics setup and final path selection.
`LocalServer::shutdown` now returns its underlying `io::Result` directly;
`ConnectionController::finish` and its replacement cleanup worker no longer
convert the error to a new text-only `io::Error`.
During E046-03 review, the generated-map resolver exposed a remaining typed
error gap. `games/sample::materialize_manifest_map` now returns
`GenerationError` instead of flattening procgen errors to `String`.
`SampleWorldContentResolver::resolve_sample` maps it with
`SessionFailure::caused_by(error.code(), ..., error)`,
retaining both the stable generation code and the original source. E046-02 was
reopened for this owner-contract correction before world-contract work resumed.
The sample paired-world producers also return `GenerationError` through the
server builder and both client startup paths. Each consumer logs the stable
code, and the UI formats the error at the presentation boundary.
The E046-02 source review and both follow-up error corrections are recorded in
the issue's GitHub evidence. Compilation and runtime remain deferred.

## E046-03 current source review

`client-runtime::session::receive::decode::dispatch::manifest_matches_content`
now calls `validate_world_manifest` before accepting a Welcome or transfer.
Generated content requires exact revision 5 and the topology digest; Authored
content requires the `AUTH` digest of the offered fingerprint. The two former
inline OR checks in Welcome and initial installation use the same helper.
`WorldRegistry::try_from_world_definitions` and `register` validate composed
manifests before publication; authored definitions also match their digest to
the materialized map fingerprint.
`SampleWorldContentResolver` rechecks the expected fingerprint and returns the
request ticket with the verified map/geography; session installation checks
ticket, manifest, fingerprint and Earth package digest. Both desktop and terminal
install this resolver. This is source review only; compilation/runtime are pending.

## E046-04 current source review

`ClientSectorCache::ensure_scene` now rolls back all newly pinned sector tickets
if admitting a replacement interest fails partway through the key list.
`ContentCache::remove_abandoned_staging`, while holding the global quota lock,
now removes only installer-shaped digest or receipt staging names with numeric
run/task identifiers; unrelated `.part` files remain untouched. Server sector
generation remains in the 1–16 worker pool with 64 outstanding jobs and at most
8 completed installations per tick. The client materialization worker has a
32-job bound; scene and collision workers publish only matching revisions.
These are source-level conclusions; no runtime failure scenario was executed.

| Requirement | Owner and evidence |
|---|---|
| Resolve paths once and pass absolute paths to consumers | Client `ClientOptions::parse` captures the working directory and supported environment values; `ClientOptions::paths` and `options/paths.rs` use `AppPaths::resolve_from` once for the option/configuration set. Desktop `renderer/window.rs` and terminal startup pass resolved data, log, cache, and content paths to runtime APIs. The server entrypoint captures its supported environment and resolves `data_dir`, `log_dir`, and optional content paths in `resolve_server_paths` before `run_with_config`. `ctl` and loadtest resolve selected paths in their application entrypoints. Relative/default log paths use `./logs` against the captured process working directory. `AppPaths::resolve` is called only at application entrypoints; runtime path consumers receive `PathBuf`s. |
| No runtime configuration reads from process environment | `crates/server-runtime/src/config/parse/values.rs::environment_values` now consumes the application-supplied map. `RuntimeServerConfig::parse_with_environment` receives the captured values; the app also passes `MINIMIUM_ADMIN_TOKEN` through the config value without logging it. Server startup no longer calls `env::var`. Scoped source search found only `current_exe()` in client runtime (for locating the sibling executable), process `current_dir()` in `AppPaths`, and test-only `temp_dir()` calls in the platform/server runtime crates. |
| Bound diagnostic fields and encoded JSON | `DiagnosticEvent::field` in `crates/diagnostics/src/lib.rs` caps raw fields at 8 KiB and 32 fields before copying. `RunWriter::write_record` in `crates/telemetry/src/logging/writer.rs` computes JSON-escaped fragment length before formatting; it leaves space for the closing object and `truncated` flag, skips a field that would exceed the 8 KiB record cap, and marks the record truncated. |
| Allocate unique UTC run IDs and preserve parent correlation | `RunDirectory::create` samples UTC once, stores `start_day`, and uses atomic `create_dir` collision handling for the daily sequence. The active lock and marker are created before cleanup; child server launch carries the parent's `run_id`, and the server supplies that value as `parent_run_id` to its diagnostics runtime. Each JSONL record reuses the run's fixed identifier even after midnight. |
| Keep both queues nonblocking and report losses | `QueueSink::emit` uses `try_send` into separate 4096-event normal and 128-event warning/error channels. Full/disconnected queues increment separate atomics. `drain_events` emits one bounded loss summary through the writer when counts advance, without feeding the summary back into the sink; shutdown emits the final outstanding summary. |
| Bound rotation and retention | `RunWriter::write_domain_line` rotates a domain file after 16 MiB; `rotate` retains two prior segments. `cleanup_completed_runs` scans only date/run directories with a valid numeric name and marker under the configured root, acquires each run's active-file lock, skips locked runs, removes expired completed runs after seven days, and evicts oldest completed runs when total size exceeds 2 GiB. Paths are derived from entries under the log root, not from event or command fields. |
| Connect diagnostic domains at the correct boundary | App entrypoints create fixed loggers for platform, server runtime, storage, transport, environment, protocol, world, sectors, simulation, procgen, asset I/O, sample game, client runtime, and renderer where those operations exist. `minimium-assets` is a pure parsing contract with no process or logging effects; parser failures leave through the asset-I/O adapter and are logged at the application/adapter boundary. Credentials and full chat payloads are not fields in authentication/chat diagnostic events; CLI replies remain stdout output. |
| Match startup status to the launched child | Client runtime generates an attempt ID, sends it with child arguments, and now accepts only a matching `Ready` record together with the expected endpoint token, live admin endpoint, and process state. Server app atomically writes `Starting`, then `Ready` after runtime startup or `Failed` with a stable code on startup failure. Missing status remains a bounded-wait condition; malformed status and non-`NotFound` endpoint/status I/O errors retain their cause. Bounded child stderr is used only if the child fails before publishing a status. |
| Preserve typed failures to the owner or final UI boundary | Asset errors retain `AssetIoErrorKind`, code, and source through `load_map`/`load_world_definition` to both client frontends and diagnostics. Generation errors retain their typed code through manifest/materialization callers. Storage load/write/worker operations preserve `StorageError` and its I/O source to server lifecycle and application results. Client controller/session failures preserve `SessionFailure`/`ControllerError` sources through the host mapping. Server startup/runtime failures preserve `ServerProcessError` stage and source; command/admin paths carry `CommandErrorCode` in replies and diagnostics. The diagnostics writer now keeps its first `io::Error` in the worker slot and returns that error from `shutdown_until`/`take_worker_error` instead of flattening it to text. |

Representative error path map (`code → variant → origin → consumer`):

| Code | Variant | Origin | Consumer |
|---|---|---|---|
| `asset_io.not_found`, `asset_io.invalid_format`, `asset_io.limit_exceeded` | `AssetIoErrorKind` | Bounded file open/read and `minimium-assets` parser adapters | Desktop/terminal world setup emits the stable code and maps the typed error to a localized UI failure; source remains on the error. |
| `procgen.invalid_manifest`, `procgen.unsupported_revision`, `procgen.materialization_failed` | `GenerationErrorKind` | Manifest validation and world materialization | Client world resolver/server composition retain the code; application logs the failure at the procgen boundary before presenting it. |
| `storage.invalid_format`, `storage.unsupported_version`, `storage.shutdown_timeout` | `StorageErrorKind` | Snapshot load/decode, format version check, persistence worker | Server startup/lifecycle receives `StorageError`; it remains an application error and persistence diagnostics include the bounded cause. |
| `session.world_content_timeout` | `SessionFailure` | `ClientSession::poll_world_content` deadline in `session/receive.rs` | `ClientSession` returns the failure to both client frontends; desktop and terminal log its stable code and display a UI message while preserving the source chain when present. |
| `client_runtime.time_request_failed` | `ControllerError` | Admin time command worker in `connection_controller.rs` | `ConnectionController::poll_time_scale_result` returns the typed result; desktop and terminal map it at the UI boundary and emit the stable code through client diagnostics. |
| `server.configuration_failed`, `server.process_failed` | `ServerProcessErrorKind` | Server config and runtime execution | `apps/server` logs stage/code, writes a matching startup failure code when an attempt was published, and returns the source-bearing process error. |
| `CommandErrorCode::*` stable `as_str()` value | `CommandError` | CLI/property/admin command parsing or validation | Shared command output, `CommandReply`, structured admin error and diagnostics carry the code to the final client/server boundary. |

E046-02 changes include application-supplied server environment parsing, captured admin-token handoff, checked startup-status/endpoint reads, preflight of escaped JSON size, and source-preserving telemetry writer errors. Source review is complete for the listed contracts. Compilation, tests, runtime behavior, cross-process retention, and log-pressure behavior remain unverified and are not claimed here.

## E046-04 worker and content lifecycles

| Work | Owner and capacity | Success / stale result | Cancellation / failure / recovery | Shutdown |
|---|---|---|---|---|
| Sector ticket lifecycle | `SectorManager<T>` is single-owner; configured cache and pending queue capacities are checked before creating a ticket. Ticket serial binds world/sector/revisions/task. | `begin_next` changes only a current `Queued` entry to `Generating`; `complete` accepts only its current ticket and marks `DataReady`; owner installs collision data to reach `Installed`. | `fail` makes a current generation retryable; `cancel` releases queued/active entry capacity; replacement changes ticket serial so old results return `Stale`. Callers fail tickets for missing maps, unavailable workers, queue pressure, spawn/worker failure. | Manager has no threads; runtime owner cancels interests and worker pools stop under the shared lifecycle deadline in E046-10. |
| Client sector collision materialization | `CollisionWorker` has one worker and a 32-entry bound across active, pending and completed work. All topology materialization is submitted asynchronously; UI lane only polls and installs results. | Worker returns `(SectorTicket, Arc<SectorGeometry>)`; owner checks the ticket before installing. | Missing map/manifest, queue full or stopped worker resolves the current ticket as failed. Worker exit reports active/pending/completed tickets; stale tickets cannot publish. | Drop requests stop and joins; bounded server/client process shutdown policy is audited in E046-10. |
| Client collision index | `SceneCollisionWorker` owns one active build and coalesces pending/completed work by world instance; each snapshot has an immutable collision revision. | Owner installs only if the current interest still carries that revision; readiness reports that same revision with geometry. | Newer pending revision replaces older queued work; completed older results are discarded. Worker stop is surfaced to the owner. | Stop clears pending jobs; lifecycle deadline/reporting is completed in E046-10. |
| Client terrain presentation | `SectorWorker` has one active build, one latest pending request, bounded renderer geometry/prepared caches, and checks full world/manifest/view/origin request identity while building and publishing. | Only a full-key matching result is returned; prepared scenes from another view are not reused. | Latest pending request replaces older pending work; superseded requests cancel at renderer preparation checkpoints. Thread spawn/stop errors reach the caller. | Worker drop signals stop and joins; overall deadline remains E046-10. |
| Client world/material content | `ClientSession` keeps one world resolver worker and one deferred latest world message; preparation deadline is 300 s. Material package preparation is one worker per session. | Ticket, manifest, map fingerprint, and Earth digest are checked before world content is installed; material package reaches `awaiting_gpu` only after verified cache install. | Cancel token is checked between 8 MiB geography hash chunks and during HTTP transfer; stale world results are discarded, disconnect cancels work, and errors/timeouts disconnect instead of leaving a pending state. | Session cancellation requests stop; worker joins only after completion during polling. Cross-process/material HTTP stop behavior is separately bounded and included below. |
| Server sector generation/collision | Worker pool uses 1–16 workers, default `min(max(parallelism-1,1),4)`. Sector queue/outstanding work is capped at 64; owner installs at most 8 results/tick. Collision index work coalesces by world/revision. | Immutable map/manifest/geography/ticket jobs return typed generation results; install rechecks current ticket/pin and collision revision before publication. | Missing world, queue full, spawn failure, worker exit and generation error fail tickets; there is no owner-lane generation fallback. Old tickets/revisions are discarded. | Pool clears queued work and returns unfinished worker names against the shared shutdown deadline in E046-10. |
| Material cache | `ContentCache` reserves quota under a cross-process `.quota.lock`; default cap is 8 GiB. Downloaded blob and receipt staging names carry run/task identifiers. | Verify hash/size before atomic blob publish; publish receipt only after package validation; per-digest leases protect active data. | Reservation guard releases on return/error; abandoned staging cleanup happens only while holding quota lock. Eviction acquires per-digest lease lock and deletes cache-owned digest/receipt files only. | No owned worker; operation is bounded by caller HTTP/deadline and file/package limits. |
| Material HTTP | `ContentServer`: 4 workers, queue 32, header <=8 KiB / 5 s, idle 15 s, transfer 300 s; exact file allowlist from validated manifest. | Accept lane uses nonblocking enqueue; worker opens only an allowlisted published path, checks declared file size, then streams it. | Full queue returns 503; unknown path 404; malformed/oversized/slow header returns bounded error; timeout or I/O failure closes transfer and emits diagnostics. | Stop closes admission and joins listener/workers until caller deadline; unfinished worker names are returned. Overall server stop budget is E046-10. |

Source review only; no runtime failures, cache pressure, network behavior or worker shutdown scenario was executed.

## E046-05 message admission and mutation map

The server pipeline is `decode v18 → identity/content negotiation → world-control routing → current-context routing → ordinary admitted actions`. Active messages also pass the global `Gameplay` Principal check. A denied privileged action emits `server.permission_denied` with an action code and a monotone process-local connection ID; the ID does not depend on the mutable client-vector index.

| Client message / operation | Admission state and context | Permission | First state change / rejection boundary |
|---|---|---|---|
| `Presentation` | `AwaitPresentation`; exactly once | None; handshake capability only | Stores the presentation choice and advances to `AwaitAuthentication`; duplicate/order mismatch is disconnected. |
| `Authenticate` | `AwaitAuthentication`; one attempt per connection | Provider validates bounded provider/credential inputs; Principal not yet available | `begin` starts one async attempt; no profile is claimed. A 10-second attempt deadline cancels/rejects it. |
| `ContentReady` | `PreparingContent`; matching material hash and pending Principal required | Principal already returned by provider | `activate_profile` claims the profile through `PlayerStore`; active-profile duplication, capacity and pending departure reject before world join. |
| `WorldInput::SetName` | Active; `runtime.route_message` requires current `WorldContext` | `Gameplay` | Bounded `queue::try_queue(Rename)`; simulation applies the name later. |
| `WorldInput::Motion` | Active; current context, nonzero sequence, target tick in `(current, current+8]` | `Gameplay`; nonzero vertical intent additionally requires `DeveloperFly` | `admit_motion` records valid queued sample/sequence. An unauthorized vertical axis is zeroed before queueing horizontal intent. |
| `WorldInput::Fire` | Active; current context; finite near-unit vector; cooldown | `Gameplay` | `queue::try_queue(Fire)`; cooldown advances only when accepted. |
| `WorldInput::Chat` | Active; current context; bounded printable text; per-player/global cooldown | `Gameplay` | `queue::try_queue(Chat)`; cooldown timestamps advance only when accepted. Full payload is not logged. |
| `WorldInput::InteractEntity` | Active; matching world context | `Gameplay` | Queues the typed interaction; registered game system checks entity/component type and interaction range before effects. |
| `WorldInput::SetAltitude` / `MapTeleport` | Active; matching context; finite bounded altitude or finite unit direction | `DeveloperTeleport` plus active `Gameplay` | Queues request; owner mutation rechecks planet/fly/context/range constraints, resets active movement and persists player location. |
| `WorldInput::SetMovementPolicy` | Active; matching context; finite speed/accel in `0..=10000`; policy revision must increment | `DeveloperMovementPolicy` plus `Gameplay` | Validated server-owned policy is replaced, then returned to that client; wire has no client-supplied revision or flight bit. |
| `WorldInput::SetFlightEnabled` | Active; matching context; boolean | `DeveloperFly` plus `Gameplay` | Server-owned `can_fly` is changed with a checked revision and returned to the client. |
| `TransitionReady` / `CancelTransition` | Active; pending transition ID and target/source context must match | `Gameplay` | Registry accepts readiness or cancels the matching pending offer; stale IDs/context are rejected without changing membership. |
| `ReplicationAck` | Active; context, sync ID, target revision and completed sync must match | `Gameplay` | A positive ack advances that connection's replicated revision; a negative ack clears its replication baseline. |
| `TimeProbe`, `Ping`, `Quit` | Active; time probes are rate-limited | `Gameplay` | Clock-probe state/Pong/close response only; no gameplay world mutation. |
| Time/config, save, stop admin commands | Separate loopback admin session | Authenticated admin token; game Principal is not used for admin transport | Only authenticated requests reach command queue. Save/stop retain their command-specific lifecycle behavior. |

Bypass rejection points: client-authored permission bits are absent from wire models; server uses only the provider-issued `Principal`. Stale contexts are consumed by `runtime.route_message` or the specialized transfer/world-control context check. `MovementIntent` has only bounded direction, jump and vertical intent; movement speed, acceleration and `can_fly` come from server `MovementPolicy`. Client/server decoders reject protocol versions other than 18 and payloads over 4096 bytes; movement codecs reject NaN/Inf and axes outside `[-1,1]`, while fire/teleport require a near-unit vector. Altitude and policy handlers repeat their semantic bounds before mutation.

Desktop/terminal share `ClientSession::queue_world_input`, which binds current context and movement sequence/tick. Loadtest uses the same v18 encoders and authenticates through the configured provider. No adversarial packet or runtime scenario was executed.
| 04 | Complete | `platform::AppPaths` resolves selected paths against one captured working directory. Client resolves profile/data/cache/config/log and local-server asset paths before runtime; server application captures supported settings and the admin token, resolves data/log/content paths before `run_with_config`, and passes only resolved paths to runtime. `ctl` and loadtest resolve paths at entrypoints. Default logs remain `./logs`. Code changed and source reviewed; compilation not run. Runtime behavior: unverified. |
| 05 | Complete | `minimium-diagnostics` has a closed domain registry, validated stable event codes, fixed-domain loggers, raw event bounds of 8 KiB/32 fields, and a nonblocking sink contract. `minimium-telemetry` now preflights fully JSON-escaped field sizes before allocating record fragments and keeps serialized records within 8 KiB including the `truncated` marker. Server emits `server.starting` through the contract. Code changed and source reviewed; compilation not run. Runtime behavior: unverified. |
| 06 | Complete | UTC run folders use one stored start day, atomic daily sequence allocation, active locks and parent correlation. Separate 4096/128 queues, loss summaries, 7-day/2-GiB completed-run cleanup, and 16-MiB segments with two retained copies are source-reviewed. Writer `io::Error` and its kind/source now reach the owner. Code changed and source reviewed; compilation not run. Runtime behavior: unverified. |
| 07 | Complete | Per-attempt atomic startup status (`starting`, `ready`, `failed`) carries attempt/run IDs. Client accepts only a matching `Ready` status with the expected endpoint and live child. The server writes `Failed` for config errors when the attempt and data path are available; bounded stderr remains the fallback before status publication. Server/client entrypoints create loggers for composed operations. `minimium-assets` stays a pure parser; errors are logged at the asset-I/O boundary. Diagnostic field review found no credentials or full chat payloads; CLI replies remain stdout. Code changed and source reviewed; compilation not run. Runtime behavior: unverified. |
| 08 | Complete | Added `AssetIoError` / `AssetIoErrorKind` for authored maps/definitions, GEBCO and WorldCover opening, material reads/packages/installation and HTTP fetching, with stable categories/codes for missing files, invalid format, unsupported versions, digest mismatch, limits, cancellation, timeout, protocol and I/O. Package parser and mapping I/O preserve `Error::source`; bounded-file overflow now has a distinct limit category. Earth and material worker operations keep the typed adapter error through their operation boundary. Added `GenerationError` categories/codes for invalid manifests, unsupported revisions, invalid blueprints and materialization failures; manifest validation and current map materialization use typed errors. Added `SessionFailure` and `ControllerError` with stable codes and preserved source chains; typed controller and content failures are mapped to presentation only at the host. Connector startup failures now expose stable codes and preserve thread-spawn causes. `ConnectionController` cleanup and time requests no longer flatten I/O errors to strings. Server command/property parsers now return `CommandError` directly with stable categories, removing prose-based category inference. `CommandReply` carries execution error codes into console output, admin error envelopes and diagnostics. Added `StorageError` categories/codes for snapshot format/version, limits, busy queues and stopped workers; load, capture, write and worker operations preserve the source I/O error and persistence logs include bounded causes. Server environment-clock operations return `EnvironmentClockError` with stable categories and preserved `TimeError`, clock-system and I/O causes; clock observation and time-probe failures emit coded diagnostics. Added `ServerProcessError` categories for configuration, startup, runtime setup, startup status and runtime execution, preserving each source; the application logs its stage and code. Startup status reads now ignore only `NotFound` while propagating malformed data and other I/O errors; geography sampling's optional paths retain their explicit fallback semantics. Code changed and source reviewed; compilation not run. Runtime behavior: unverified. |
| 09 | Complete | `validate_world_manifest` accepts generated content only at revision 5 with an exact topology digest, and enforces planet settings only on planet topology. Authored manifests use a distinct fingerprint digest; server composition verifies that digest against the materialized map. Sample producers attach planet configuration only to planets. Removed legacy planet-map materialization/export, automatic sample-world injection and server `migration_worlds` composition. The producer retains its existing dungeon seed derivation to preserve world geometry, but clients no longer reconstruct it from IDs or reverse an XOR. Snapshot format readers are separately owned by storage and removed in step 25. Code changed and source reviewed; compilation not run. Runtime behavior: unverified. |
| 10 | Complete | Added `WorldContentResolver` and ticketed session result contracts; Welcome and transfers validate the returned manifest, fingerprint and Earth geography. Desktop and terminal install the same application resolver. Its generated-map path dispatches from manifest topology and regenerates from the recorded `GenerationSpec` seed, without special instance IDs; authored maps must be locally known with a matching manifest and fingerprint, otherwise resolution fails. Removed the duplicate renderer generator loader, while client-runtime production dependencies remain independent of `game-sample`. The session retains active/pending resolved content and clears it at lifecycle boundaries. Code changed and source reviewed; compilation not run. Runtime behavior: unverified. |
| 11 | Complete | SectorManager tickets bind world/sector, generation/content revisions and a unique task serial; lifecycle exposes complete/fail/cancel with checked transitions. Client and server fail tickets when source data or workers are unavailable. Client collision materialization now uses the bounded worker for every topology; missing source, queue pressure and stopped workers resolve affected tickets. Interest replacement cancels old requests; stale serials cannot install. Updated lifecycle report and sectors README. Code changed and source reviewed; compilation not run. Runtime behavior: unverified. |
| 12 | Complete | World-content resolution runs on one worker after message type/state/context/manifest checks. The decoder retains one deferred world message and the latest transition request replaces it; cancellation prevents stale results from installing, while only one worker is active. Results are validated against unique session/request/world/content tickets, manifest, fingerprint and Earth digest before publication. Polling remains bounded and does not perform map generation, file hashing, mmap or materialization; it applies the existing 300-second preparation deadline. Disconnect/cancel releases pending work, and GEBCO plus optional WorldCover hashing checks cancellation every 8 MiB. `git diff --check` passed; compilation not run. Runtime behavior: unverified. |
| 13 | Complete | Added a fixed server sector-generation pool, default worker-count formula and `--sector-workers`/`server.sector_workers` override (1..16), including offline config resolution. A bounded shared queue retains at most 64 queued/running/completed jobs; tick owner installs up to eight completed results and rechecks ticket and interest pins before publication. Workers receive immutable map/manifest/geography/ticket inputs and return `Result<Arc<SectorGeometry>, GenerationError>`. Worker failures, exits, missing worlds and exhausted worker pools resolve tickets and emit diagnostics. Removed synchronous owner-lane materialization. Existing API callsites were adjusted mechanically. `git diff --check` passed; compilation not run. Runtime behavior: unverified. Worker shutdown deadline remains step 33. |
| 14 | Complete | Server and client build collision indexes from immutable sector snapshots on dedicated workers. Per-world revisions coalesce pending work; stale results are ignored; server movement/transfer and client scene readiness wait for the matching revision. Unchanged sector sets do not trigger a rebuild. Client terrain presentation now also gates cancellation/publication on the full world/manifest/view/origin key. Code changed and source reviewed; compilation not run. Runtime collision behavior is unverified. |
| 15 | Complete | Added `ContentCache` with configurable byte capacity (8 GiB default), global cross-process quota reservation, per-digest advisory update locks, shared active package leases, digest verification, atomic receipts, and bounded eviction of only cache-owned assets. Staging names include run/task identifiers; stale `.part` files are collected only while holding the cache reservation lock. The client passes `--cache-limit-bytes` to runtime downloads. `git diff --check` passed; compilation not run. Multi-process and cache-pressure behavior remains unverified. |
| 16 | Complete | Climate loading and persistence now live in `environment::ClimateConfiguration`; runtime content composition only owns the optional HTTP adapter. Added `asset_io::ContentServer` with four fixed workers, a 32-connection queue, 8 KiB header cap, five-second header deadline, 15-second idle timeout, 300-second transfer deadline, allowlisted published files, response/transfer diagnostics, and bounded shutdown reporting. `git diff --check` passed; compilation not run. Network and shutdown behavior remains unverified. |
| 17 | Complete | Added `Principal`, distinct `PrincipalId`/`PlayerProfileId`, and server-granted permission bits in core. `ServerComposition` now requires a nonblocking `AuthenticationProvider` port with begin/poll/cancel, and admission has explicit presentation, authentication, content-preparation, and active phases with a 10-second authentication deadline and one attempt per connection. Replaced wire `Identify` with bounded provider-id/credential authentication; the client host supplies credentials. The sample server explicitly installs an insecure developer provider that treats the current profile id as a claim, and profiles are claimed only after a Principal is returned. Existing active-profile exclusion remains. Code changed: yes. `git diff --check` passed; compilation not run. Authentication behavior remains unverified. |
| 18 | Complete | Added `server.dev-mode` plus `--dev` parsing for the server and an explicit client `--dev` option for owned local servers; remote connect options reject local developer settings. The sample provider grants only `Gameplay` normally and adds developer permissions only when `--dev` is resolved for that server. Admission checks Principal permissions before privileged mutations and logs denial action plus a monotone connection ID, not the shifting client-vector index. Time/save/stop remain on the token-authenticated admin channel. Code changed and source reviewed; compilation not run. Runtime behavior: unverified. |
| 19 | Complete | Sole wire version is 18; v17 and uncontextualized/duplicate movement forms are rejected/removed. Every gameplay action uses `WorldInput` with explicit `WorldContext`; the host convenience API binds context and assigns movement sequence/tick. Desktop, terminal, server and loadtest producers/consumers use this contract. The decoder enforces the 4096-byte frame cap. Source review found no old action variants or v17 compatibility branch; compilation not run. Runtime behavior: unverified. |
| 20 | Complete | `MovementIntent` contains direction, jump and vertical intent only; `MovementPolicy` owns server-side flight, speed, acceleration and revision. Protocol validates finiteness and component ranges; authoritative input composition uses the current per-player policy. Developer policy changes require permission, valid limits and checked revision increments, then return policy to the requesting client. Default speed/acceleration remain 5 m/s and 30 m/s². Code changed and source reviewed; compilation not run. Runtime movement/prediction behavior is unverified. |
| 21 | Complete (source review) | `ServerRuntime::resolve_movement_input` advances the acknowledged prefix only before the smallest accepted sequence that may still execute. Admission rejection, Busy, same-tick replacement, scheduled application and lateness use the same resolver; client queue counters advance only after the frame enters the send buffer. Source-level traces are summarized in E046-06; no runtime test was run. |
| 22 | Complete | Prediction retains at most 128 actually queued movement samples with target ticks, captured movement policies and exact active collision revisions. Snapshots set authoritative physics, discard resolved samples, and replay remaining ticks with the shared integrator; held direction continues between samples while jump impulses are single-tick. Policy/collision mismatch, stale history or overflow clears replay state. Position correction is a decaying presentation-only camera offset. World installation and accepted altitude/map teleports clear history and the fixed-step accumulator. `git diff --check` reports an existing trailing blank line in `crates/sectors/src/tests.rs`; compilation and runtime verification not run. |
| 23 | Complete | Split world entity contracts into identifiers, registry, store and change-log modules. The store enforces exact registered schemas, 2048-byte components, 16 components per entity and 16,384 entities; spawn/upsert/component removal/despawn produce monotone change records. Removed the old sector-object-to-component wrapper, origin map and dead loaded-state rehydrate fields. World state remains separate from sector blobs. The single runtime batch path stages world stores, change journals and snapshot reservations together. Source review and `git diff --check` only; compilation and runtime behavior remain unverified. |
| 24 | Complete (source review) | Snapshot cap is 128 MiB. Counting and final streaming use the same encoder. `SnapshotBudget` caps itself at 128 MiB; each profile reserves its maximum legal encoded row and the profile sub-budget is 64 MiB. Sector/entity/component records use actual encoder counts. Component raw payloads have a shared 4 MiB aggregate reservation, per-item max 2048 bytes, and release with component/entity removal. Metadata reserve is derived from at most 16,384 worlds × 1024 counted bytes/world plus 64 KiB for primary header/clock; writer checks those same encoded bounds. Batch mutations stage candidate stores and budget, reserve/release in effect order, then publish together; a rejected batch leaves live data and budget unchanged. |
| 25 | Complete (source review) | Sole save version is 10; versions 2–9 and other versions fail in header parsing before startup creates a persistence worker or writes a replacement snapshot. Writer streams through a 128 MiB limit into atomic replacement. Records serialize in stable world/entity/component/player order. Load stages all records, rejects duplicate IDs/records, invalid refs, counter underflow, component counts/size and total payload above 4 MiB, and reserves exact encoded entity/component sizes. Module schemas are passed into `PlayerStore::load` and checked by the parser before it returns `LoadedState`. |
| 26 | Complete (source review) | `run_with_config` installs modules and builds one `ComponentRegistry` before startup loads the snapshot; runtime restores all entities (including empty ones), next-ID counters and `content_initialized` markers into staged stores before publishing. New initial entity templates are emitted by the per-world Startup system through the shared budgeted batch; the marker commits only after successful initialization, so saving, loading, then despawning a portal preserves the true marker and does not reseed it. Autosave and shutdown both call `WorldRegistry::snapshot_for_save`; capture sorts and copies the current full entity set rather than merging stale loaded entities. Each immutable `SaveSnapshot` has a monotone process-local revision; the worker rejects stale revisions, serializes queued autosave before final shutdown save, and waits for final completion. Source review only; E046-09/10 will add later wire/shutdown integration evidence. Compilation and runtime behavior remain unverified. |
| 27 | Complete | Moved descriptors and transactional registration into `minimium-gameplay`; the server uses one startup registry for commands, systems and schema restoration, and the hidden command-module `OnceLock` is gone. Exposed `GameplaySystemContext` with the owner-lane simulation tick as the runtime-neutral callback contract and kept the real `SampleGameplayModule` consumer in `games/sample`. The application explicitly installs it. Code changed: yes. Source review and `git diff --check` only. Compilation: deferred to step 46. Runtime behavior: unverified. |
| 28 | Complete (source review) | System stages receive the scheduler's current tick, including Environment and PostSimulation. Startup is invoked once per world instance and carries its ID plus New/Restored state; the persisted initialization marker is published after all that world's startup effects commit. `ordered_systems` uses stable IDs for topological ties, rejects duplicate declarations/missing targets and returns `ModuleError::DependencyCycle` for cycles. Any rejected buffer discards the whole stage; entity effects use staged stores/journals/budget, and transfer requests are preflighted for source context, target placement, duplicate players and ID capacity before those entity effects commit. Compilation and runtime behavior remain unverified. |
| 29 | Complete (source review) | Sample Startup reads prepared initial entity templates and emits them through the public `WorldEffect` API only for New worlds. The schema registration exposes the portal position decoder for replication; PostSimulation selects the requested replica entity, checks component type/schema and three-metre distance, then emits `RequestTransfer`. The runtime preflights context/placement and retains the existing readiness/commit handshake. Desktop and terminal `portal` actions select an entity from the replica and send `InteractEntity`; the old `UsePortal` command variant and no-target wire path are absent. Code changed: yes. Compilation and runtime behavior remain unverified. |
| 30 | Complete (source review) | Protocol v18 begin/record/complete/ack messages carry context, nonzero sync ID and base/target revisions. Revision zero is valid for restored full state before any live mutation; record revision must equal the staged target on the client. `Reader::bounded_bytes` checks the 2048-byte component limit before copying; all frames remain under 4096 payload bytes. `ClientKind` contains only client-origin kinds, so server-only sync types are rejected as unknown. `InteractEntity` carries a nonzero entity ID. Source review only; compilation and runtime behavior remain unverified. |
| 31 | Complete (source review) | `EntityStore` retains a 4 MiB bounded change journal and exposes its oldest retained revision; each client keeps an independent acknowledged revision and visible component map. Full images clone the current state at one target revision, so concurrent mutations remain in the journal for the next delta. Spatial components are included only when their decoded position maps to an active ticket; Global components are explicitly included; ServerOnly components are excluded. The server sorts by nearest visible spatial distance then ID, caps at 256 entities and reports queued/filtered totals in metrics. Empty persistent entities have no replicated component view; when an entity loses its final visible component, the client receives Despawn. Each sync has one bounded pending operation queue, expires after 30 seconds, and restarts as full after timeout/journal overrun. Exact encoded frame bytes, including begin/record/end overhead, consume the 64 KiB per-client tick budget; one max server frame remains reserved for control. |
| 32 | Complete (source review) | `ReplicatedEntities::apply` stages full or cloned delta state, checks entity/component/raw-payload limits and exact registered schemas, and publishes only after matching completion. The client queues ack after publication; malformed late records or mismatched end disconnect and clear staging without exposing partial state. Full revision zero is supported. Disconnect and transfer clear active and staged replicas. Source review only; compilation and runtime behavior remain unverified. |
| 33 | Complete (source review) | Ctrl-C/termination, admin `server stop`, and the local owner converge on `ShutdownRequest`; admin/game admission closes before the owner lane freezes. All server workers, persistence, endpoint/status, and process diagnostics share one lifecycle absolute deadline (30s total; diagnostics receives its final 2s). Final-save errors reach shutdown status and the executable's nonzero result; snapshot replacement is atomic, and timeout is not described as cancelling OS I/O. `Attached` clients cannot stop a server; renderer worker joins are bounded and Drop only signals/detaches. Code changed: yes. Source review and `git diff --check` passed. Compilation: deferred to step 46. Runtime/signal behavior: unverified. |
| 34 | Complete | Added the `minimium-localization` package, Fluent English/Polish catalogs with plural rules and fallback, typed message values, stable-key diagnostics, and `ui.locale` config parsing/persistence with default `en` and accepted values `en`/`pl`. Added the live `ui.locale` property to terminal and desktop clients; set/reset affects subsequent UI formatting without reconnecting and `config save` captures an override. Both frontends can format the same typed message through the shared localizer. Code changed: yes. Compilation is deferred to step 46. Runtime language switching has not been exercised. |
| 35 | Complete | Added typed Fluent entries for all current `ClientText` variants and routed terminal session events, connection states, network summaries, desktop window title/errors, Earth overview/transfer/loading states, material preparation, environment status, console startup/help headers, property help/list descriptions, command help and autocomplete guidance through the selected locale. Help requests preserve typed targets until frontend formatting. Desktop console retains typed ClientText/UiMessage values and reformats them when locale changes. `ClientText` has no English `Display` fallback now. Localized command rejection, config-save results, map/transfer/time notices, common error results, and typed notices for flight and portal feedback. Technical time/config/environment failure details go to diagnostics. Removed the English status/title formatters from client-runtime; the application owns those localized presentations. UI text history retains typed messages where reformatting is needed. Code changed: yes. `git diff --check` reviewed; compilation is deferred to step 46. Runtime language switching has not been exercised. |
| 36 | Complete | Replaced the pre-generated ASCII/Polish glyph sheet with a bounded runtime atlas rasterized from bundled Roboto Mono via `fontdue`. Cache keys include font hash, raster size and character; glyph placement, advance and baseline use font metrics. Added source/license provenance and removed the old atlas binary/generator. Console editing, selection and mouse positioning use grapheme boundaries through `unicode-segmentation`; caret, hit targets and selection use the measured monospaced advance. IME preedit is shown and only committed text enters the editor. UI layout converts physical window dimensions and pointer positions through the live DPI factor. RTL/CJK remain outside scope. Code changed: yes. `git diff --check` reviewed; compilation and GPU presentation remain deferred to step 46. |
| 37 | Complete | Added a platform-neutral `InputMapper` and persisted `client.input.bindings` setting with defaults matching current controls, explicit global/gameplay/console/map/modal contexts, and duplicate-control validation per context. Desktop keyboard and mouse events route through actions, preserving text/IME handling as a separate path; focus loss clears held actions and pending jump. Bindings are parsed for both desktop and terminal configuration and applied when the desktop renderer starts. Code changed: yes. `git diff --check` reviewed. Compilation is deferred to step 46; rebinding and focus behavior remain runtime-unverified. |
| 38 | Complete | `SettingsStore` now holds typed locale/binding requests and per-setting requested/effective/source/reason records. Desktop `get/set/reset`, terminal locale/binding operations, `config show --sources` and `config save` read or update the same record contract. Reset restores the resolved startup record; save persists requested values, and graphics fallback updates effective/reason without replacing the request. Config preview reports effective as unavailable until a consumer applies a value. Mutual-exclusion changes between debug views update both requested values. Code changed: yes. Source review and scoped `git diff --check` only; compilation and runtime behavior remain deferred to steps 46 and 47. |
| 39 | Complete | Split `ClientApplication` state into settings, transitions, game and UI modules. Logical `ClientGameState`/`GameplayState` now live in the application; renderer-only remote geometry, hit feedback and shot cooldown live in `GameplayPresentation`. The application facade owns connect/start/disconnect, catalog install, world activation, transfer phase changes and overview entry/exit without GPU handles. Transition tickets validate source context for cancel and require matching ready target context for commit; failed preparation releases its ticket. Terminal-only property storage uses the same `SettingsStore` without constructing the renderer. Desktop and terminal frontends keep the shared command/runtime contracts. Source review and scoped `git diff --check` only; compilation and runtime behavior remain deferred to step 46 and runtime verification. |
| 40 | Complete | Added CPU-only `PreparedShadowFrame` with context, world/generation/content identity, selected caster keys, uniform, invalidation reason and immutable map-caster vertices. `shadow_prepare::prepare_shadow_frame` consumes environment values and a read-only sector-bounds/residency snapshot; map selection and `local_map_geometry`/`sector_geometry_relative` preparation live in `ShadowMapPreparationCache`, outside `GpuResources`. `RenderSceneSnapshot` passes that one immutable result. GPU render validates identity, returns Ready/Pending/Failed for map publication, records GPU cache metadata only after a complete bounded upload, then writes the matching uniform and encodes only when publication is ready. World release owns world-geometry plus dependent shadow-map buffer release; origin/settings changes invalidate publication. `shadow_prepare_cpu` now measures CPU selection and geometry preparation. `RenderFrame` remains CPU values without GPU handles and is gated behind the renderer GPU feature. Source review and scoped `git diff --check` only; build and visual/runtime behavior remain deferred/unverified. |
| 41 | Complete (source review) | `GeometryPublicationStamp` binds instance, generation/content revision, map fingerprint, monotonic geometry revision and origin generation. `WorldGeometryGpu` exposes one active publication only after all required keys have resident buffers at the current origin generation; prepared uploads stage under a generation and atomically replace the active catalog once complete. Receiver hints and caster selection use the same cloned publication. Rebase advances origin/publication generations together and clears publication on failure; pending work from an earlier origin is discarded. Shadow-map CPU keys/results and GPU upload carry the stamp, and frame encoding requires it to match the receiver publication; remote casters are complete only for the same stamp. Diagnostics expose scene revision/origin generation and distinguish stale context, no source, budget and incomplete/upload failure. Source traces: LOD B uploads retain A until complete, then replace/publish B; rebase drops pending old-origin work and clears remote/map readiness until rebuilt; world transfer publishes the new instance and old-instance completion cannot pass the stamp check. Source review and `git diff --check` only; compilation/runtime/visual behavior remain deferred. |
| 42 | Complete (source review) | Audited replaced E046-01–14 paths and consumers. Removed `TerrainView::legacy` after switching client initialization to `TerrainView::new`; removed unused `render_patches*` wrappers after routing the worker through `render_patches_for_view`. Removed the duplicate asset-io diagnostics dependency and broad dead-code suppressions; retained only narrowly explained configuration/re-export allows. Renderer README now names the surviving terrain API. Legacy format descriptions remain only where they document accepted/rejected persisted or admin formats. Source review only. |
| 43 | Complete (source review) | `architecture.toml` declares every current workspace package, including diagnostics, gameplay and localization. The checker detects duplicate, absent and unknown declarations; traverses all-feature normal/build production edges with target labels, reports dev edges separately, and applies transitive sample/server-GPU/renderer-boundary restrictions. Source effects are scanned from Cargo manifest roots for declared engine-domain/runtime roles and server application. Regex limitations are documented in `docs/architecture.md`; checker was not run. |
| 44 | Complete | Static refactor checker no longer requires old exact exports or `run(args)`, and retains Rust/WGSL layout and owner-path invariants. File-length policy includes untracked sources and WGSL and no longer excludes a plain `runtime` directory. WGSL validator has the standalone shadow shader variant. E046-17 ran all three checkers successfully on the final tree. |
| 45 | Complete (source review) | Reconciled current package roles, protocols and data flows in architecture/protocol/storage and package READMEs. The evidence matrix and remaining limits are recorded in E046-16. Compilation and runtime remain separate. |
| 46 | Complete | E046-17: lockfile, formatting, three static scripts, workspace all-target/all-feature build, terminal no-default-features build and all 22 WGSL variants passed. Compiler warnings remain; runtime behavior was not exercised. |
| 47 | Complete | E046-18: final matrix maps steps 01–47 to owners/evidence, records new contracts/removed paths and E046-17 results, and leaves runtime scenarios as a separate backlog. |
## E046-05 source review addendum (steps 17–20)

All client frames pass `decode_client`, `dispatch_message` and the close-phase
guard. Active gameplay frames also pass the Principal permission check before
a world mutation. A network
poll can decode multiple frames; the guard now drops frames after `Quit` moves
the connection to `Draining`, while departure cleanup still owns its identity.

| Wire input | Handler and first mutation | Admission, context, permission |
| --- | --- | --- |
| `Authenticate` | `identity::begin_authentication`: provider `begin`; `activate_profile`: `PlayerStore::claim`, then queued Join | AwaitAuthentication once; provider returns Principal; profile claim excludes an active ID |
| `Presentation`, `ContentReady` | `set_presentation`; `activate_profile` | AwaitPresentation and PreparingContent respectively; hash and pending Principal required |
| `WorldInput::SetName` | `movement::admit_name`: queued Rename | Active, current `WorldContext`, Gameplay; name validated by protocol decoder |
| `WorldInput::Motion` | `movement::admit_motion`: queued Move, sequence bookkeeping | Active, current context, Gameplay; finite bounded axes/vertical in codec; vertical cleared without DeveloperFly; server policy applied at tick |
| `WorldInput::Fire` | `combat::admit_fire`: queued Fire and cooldown | Active, current context, Gameplay; unit direction and cooldown |
| `WorldInput::Chat` | `chat::admit_chat`: queued Chat and cooldown | Active, current context, Gameplay; bounded text and cooldown |
| `WorldInput::InteractEntity` | `world_control::route_message`: queued InteractEntity | Active, current context, Gameplay; nonzero entity ID; game module checks type/range |
| `WorldInput::MapTeleport`, `SetAltitude` | `world_control::route_message`: queued privileged input | Active, current context, Gameplay and DeveloperTeleport; unit vector or finite bounded altitude |
| `WorldInput::SetMovementPolicy` | `world_control::route_message`: changes per-player policy, queues revised policy reply | Active, current context, Gameplay and DeveloperMovementPolicy; valid finite policy and checked revision |
| `WorldInput::SetFlightEnabled` | `world_control::route_message`: changes per-player policy, queues revised policy reply | Active, current context, Gameplay and DeveloperFly; checked revision |
| `TransitionReady`, `CancelTransition` | `world_control::route_message`: marks ready or cancels pending transfer | Active and Gameplay; transition ID and expected target context checked by WorldRegistry |
| `ReplicationAck` | `technical::handle_message`: commits or resets per-client replica cursor | Active and Gameplay; pending sync ID, context, revision and completed end must match |
| `TimeProbe`, `Ping`, `Quit` | technical handler: probe limiter/reply, pong, close phase | Active and Gameplay; no gameplay world mutation |

The same client runtime `Session::queue_world_input` wraps desktop and terminal
commands with the current context. Loadtest constructs contextual frames in
`client_setup` and `worker`. A custom packet still meets server-side checks:
protocol framing rejects versions other than 18 and payloads above 4096 bytes;
the decoder rejects NaN/Inf, invalid axes and unit vectors, and the server
rejects stale context. `PermissionSet` comes from the server provider. Its
ordinary mode grants Gameplay only; `--dev` is forwarded from the client local
launcher through resolved server config to the server application provider,
which grants the three developer bits. Remote clients cannot grant them.
Developer profile credentials are explicitly an insecure local declaration.
The admin endpoint separately checks its token before accepting clock or stop
commands.

Compilation, formatting and static checks are deferred to E046-17. Runtime
behavior and tests were not executed.

## E046-08 source review addendum (steps 27–29)

| Stage | Tick/world/lifecycle | Order and commit |
| --- | --- | --- |
| Startup | `runtime.rs::run_system_stage` supplies current scheduler tick, one world instance and New/Restored from its persisted marker | `ServerSchedule::run_stage` runs once per instance; bounded batch and transfer/entity preflight precede commit; initialization marker is set after all startup commits succeed. |
| FixedUpdate | `runtime/tick.rs` increments tick, applies inputs, passes the same tick with no startup world | Validated system order; batch commits before shared simulation step. |
| Environment | Same tick, no startup lifecycle | Runs after simulation; bounded batch validation/commit. |
| PostSimulation | Same tick, no startup lifecycle; receives gameplay events | Runs after Environment; sample portal emits public RequestTransfer, then runtime preflights context, target and placement before entity commit and transfer preparation. |

`ModuleRegistry::install` stages module commands, systems and components before
publication. It rejects duplicate IDs/names, missing module dependencies and
invalid descriptors. `ordered_systems` creates before/after edges, rejects
missing targets, duplicate edges and cycles, and uses `(Stage, system ID)` as
the deterministic ready-set order. The host builds a fixed `ServerSchedule`
from this order at startup. `WorldEffectBuffer::push` enforces 256 operations
and 1 MiB component payload before appending; a rejected effect rejects the
whole stage. `commit_system_batch` validates all requested transfers, then
`apply_entity_effect_batch` atomically publishes staged stores, budget and
journals. The sample module only imports neutral gameplay/world contracts.
Its Startup handler spawns portal components for New worlds, and its
PostSimulation handler checks the selected entity's schema, payload and 3 m
range before emitting `RequestTransfer`. Desktop and terminal choose an entity
from their replica and send context-bound `InteractEntity`; runtime owns
readiness and placement. There is no `WorldDefinition.portal` or `UsePortal`
path.

Compilation, formatting and static checks are deferred to E046-17. Runtime
behavior and tests were not executed.

## E046-06 source review addendum (steps 21–22)

`resolve_movement_input` removes one sequence and publishes at most the number
before the lowest still pending sequence. The pending set begins at admission,
so it covers both `pending_world_inputs` and the target-tick map. It is bounded
by the 4096-input admission cap; no sequence-range-sized allocation is made.

| Trace | Before → after on server | Snapshot ack; client discard |
| --- | --- | --- |
| 10 accepted, 11 rejected (future tick) | pending `{10}`; `last=11`; reject 11 resolves only through 9; later execute 10 empties set | ack 9 preserves 10, later ack 11 removes both |
| 10 accepted, 11 Busy | pending `{10}`; failed `try_queue` advances last to 11 and calls resolver | ack 9 until 10 resolves; Busy alone removes nothing; later ack 11 discards |
| 10/11 same target tick | both pending through admission; `apply_input` replaces 10 and resolves it, then scheduled 11 executes | intermediate ack 10 cannot discard 11; final ack 11 |
| late target tick | admission rejects a tick already passed, or scheduler resolves a queued sample whose target passed | ack advances only after earlier pending input resolves; late sample is never applied |
| no new packet | server `active_inputs` is retained until 250 ms idle neutralization; snapshots include `self_movement` | replay starts from snapshot `self_movement`, not zero, through unsent ticks |
| transfer | commit removes old queued/scheduled sequences, resets server seq and active intent, changes world context | client drops old-context snapshots, resets prediction and restarts seq/tick from committed server tick |
| successful map teleport | scheduled samples resolve and later Move entries in the same input batch are discarded and resolved | client resets prediction, time and presentation offset on result |

Desktop `renderer/camera.rs` queues and records every fixed-step predicted
sample after a successful `Session::queue_world_input`, using the returned exact
sequence and target tick. The session starts targets two ticks ahead; admission
limits to eight ticks ahead. Loadtest constructs its own contextual motion;
terminal does not run local prediction. `ClientPrediction` stores up to 128
entries `(seq, tick, intent, policy, collision revision)`. Reconcile installs
the authoritative position, velocity, grounded flag and active intent, discards
only the resolved prefix, and replays each intervening 50 ms tick with the
shared integrator. Policy or collision revision mismatch and history overflow
reset history/time; world install, transfer and teleport reset prediction.
Visual smoothing changes only the presentation offset.

Compilation, formatting and static checks are deferred to E046-17. Runtime
behavior and tests were not executed.

## E046-07 source review addendum (steps 23–26)

`WorldRegistry::apply_entity_effect_batch` is the live persistent mutation
entry point. Spawn, component upsert, component removal and despawn operate on
cloned stores and a cloned `SnapshotBudget`; only after the whole effect batch
validates does the registry publish the stores and budget. The cloned stores
carry their bounded change journals, so a rejected batch publishes neither a
world mutation nor replication records. Runtime `commit_system_batch` also
preflights transfers before this commit. All production callers use this path;
`EntityStore::restore` and `restore_component` belong to staged load only.

| Transition | Encoded reservation and durable path |
| --- | --- |
| Spawn | Global 16384 count checked across stores; `entity_record_bytes` reserves the entity even with zero components; each validated component reserves its encoded record and raw payload. |
| Upsert | Exact registered schema, 2048-byte blob and 16-per-entity cap checked by `EntityStore::insert`; encoded record and raw payload replace their keyed reservations in staged budget. |
| Remove | Store records a tombstone; staged component record and payload reservation are released at commit. |
| Despawn | Store records despawn; staged entity and all component reservations are released at commit. |
| Autosave | `runtime/operations::poll_autosave` captures `WorldRegistry::save_input`, then `PlayerStore::save_snapshot` assigns a revision and immutable shards. |
| Final save | `lifecycle::capture_final_snapshot` uses the same `save_input` and `save_snapshot`; `PersistenceWorker::shutdown_until` queues a higher final revision after earlier autosave and waits for its result. |
| Load | `startup::initialize` passes the already registered component schemas to `PlayerStore::load`; complete staged parsing validates world references, duplicate IDs/components, exact schema, global count/payload cap, counters and initialization markers before runtime publication. |

`save_input` iterates entity IDs separately from components, preserving an empty
entity. It also captures per-world next ID and `content_initialized`. The v10
writer sorts records, counts the exact encoder output, then streams to a
size-limited temporary file before atomic replacement. Profile reservation
measures the maximum legal encoded profile; fixed world metadata reserve is
checked against actual encoders. Snapshot budget is 128 MiB, raw component
payloads 4 MiB globally. Versions 2–9 are rejected during load before the
persistence worker is spawned. Sample portal Startup emits a spawn only for
`WorldLifecycle::New`; after save/load the restored marker is true. Despawn
then save/load leaves no portal while retaining the marker and monotonically
restored entity counter.

E046-04 follow-up found an owner-lane interaction scan: each portal event had
called `components(world)` and copied/searched every world component. The
neutral context now exposes `component(world, entity, kind)` backed by
`EntityStore::get`; the sample portal uses one indexed lookup per event. The
existing bounded 4096-event queue therefore cannot multiply a full world scan
per interaction.

Compilation, formatting and static checks are deferred to E046-17. Runtime
behavior and tests were not executed.

## E046-09 source review addendum (steps 30–32)

`replicate_entities` plans a client-specific immutable pending sync with its
context, ID, base/target revision, visible component set and queued operations.
It does not mutate an already pending image when the world changes. A client
ack matching context/ID/target revision and a sent Complete advances only that
client's revision; another client's ack has no effect. The 4 MiB per-store
journal retains mutations until eviction; a client below its floor receives a
new full image. Interest uses active sector tickets, explicit Global/spatial
visibility, deterministic distance/ID ordering and a 256-entity cap with
metrics. Empty persistent entities have no replica; losing the final visible
component produces Despawn. Encoded begin/record/end frames consume the
64 KiB per-client tick budget including headers, with one maximum frame kept
available for control messages. A pending image older than 30 s is discarded
and restarted as full, not acknowledged.

| Trace | Server pending/acked; client staging/active; ack |
| --- | --- |
| Two clients with different ack | Each `Client` holds its own `entity_sync`, revision and visible set. Ack A advances only A; B plans from its own revision or full if journal floor passed. |
| Mutation during full | Pending operations and target revision stay frozen. Client stages those bytes; after Complete and ack, next server pass plans a delta from target revision (or full on journal eviction). |
| Interest change | Leaving entity produces Despawn, entering entity produces Spawn plus its visible components; server updates the acked visibility set only on matching ack. |
| Invalid last record | Protocol bounds and replica schema/reference/size checks fail before Complete; active replica is unchanged, staging is cleared by disconnect, no ack. |
| Lost Complete | Client keeps bounded staging and sends no ack; server discards pending after 30 s and sends a fresh full image. |
| Transfer | Commit clears server pending and acked entity state; client clears staging/active on context change, old-context messages are skipped. Next sync is full in new world. |

The transfer reset in `runtime/tick.rs::send_transition_committed` closes a
source-level gap: an already acknowledged old-world replica had no pending
sync to trigger the context-mismatch branch in `replicate_entities`, so it
could otherwise start a delta from the old world's revision. Client
`apply_replication_message` builds full or delta in `ReplicaAssembly`, checks
the entire sequence and limits (entity, component and 4 MiB payload), publishes
only on matching Complete, then queues ack. Protocol v18 bounds each frame to
4096 payload bytes and each component to 2048 bytes before allocation.

Integration with E046-07/08: entity effect commit publishes the journal with
the persistent store and snapshot budget; portal components enter the visible
replica and clients send `InteractEntity` only for a received entity ID.

Compilation, formatting and static checks are deferred to E046-17. Runtime
behavior and tests were not executed.

## E046-10 source review addendum (step 33)

| Worker/owner | Handle and stop | Bounded result |
| --- | --- | --- |
| Server sector pool | `SectorGenerationPool::workers`, shared stop/condvar and cleared queues | `shutdown_until(deadline)` joins finished handles and returns unfinished IDs. |
| Server collision index | `CollisionIndexWorker::thread`, stop state | Same absolute deadline, ID in unfinished list. |
| Server simulation executor | `Executor::workers`, sender dropped | Same deadline; unfinished and panic IDs returned. |
| Material HTTP | `ContentServer` accept handle and four worker handles, atomic stop | At most two seconds within common work deadline; unfinished names returned. Drop requests stop without a second wait. |
| Persistence | `PersistenceWorker::thread`, bounded Save/Shutdown channel | Final revision queued after autosave, result received until common deadline; timeout/error returned, worker join bounded. |
| Telemetry | `DiagnosticsRuntime::worker`, sink closed | Host flushes until same absolute log deadline; timeout detaches and returns error. |
| Client content preparation | `ContentPreparation::worker`, cancel token | Cancellation releases session-owned result; no unconditional join on Drop. |
| Client connector and owned child | `Connector` handle, abandoned token; `OwnedServer` child | `finish_on_exit_until` and `finish_until` use process work deadline; owned child receives authenticated admin stop, then bounded kill/reap fallback. Attached server is no-op. |
| Desktop terrain, collision and cloud | sector, collision, scene collision and cloud worker handles | `shutdown_until(work_deadline)` collects unfinished names; Drop requests stop. |

`ctrlc` callback only calls the shared `ShutdownRequest::request`; admin Stop
does the same in `runtime/tick.rs`; an owned client sends admin Stop, while
Attached never does. `lifecycle::shutdown` starts one 30-second absolute
deadline, reserves its final two seconds for logs, stops admin admission,
rejects pending admin commands, closes client sockets, stops worker intake,
captures the final immutable v10 snapshot, queues its higher revision after
any autosave, waits for result, then removes the owned endpoint and writes
shutdown status. The host uses the same log deadline. Final capture checks
the work deadline before and after its synchronous copy. Error or timeout of
capture, save, worker or endpoint cleanup produces an error for `apps/server`
and a nonzero process exit. Temporary-file atomic replacement preserves the
previous complete snapshot if the final write fails before replacement.
The timeout is a report of unfinished OS I/O, not cancellation of it.
Desktop and terminal now pass one 30-second absolute deadline through client
cleanup and log flush rather than stacking independent 28+2+2-second waits.

Compilation, formatting and static checks are deferred to E046-17. Runtime
behavior and signal scenarios were not executed.

## E046-11 source review addendum (steps 34–37)

| Message group | Typed producer and formatting | Frontend consumer |
| --- | --- | --- |
| Connection, authentication, timeout and failure | `ClientText::localized` maps each variant to `UiMessage` with detail/reason arguments; `Localizer::format` uses Fluent PL, then EN, then stable key with diagnostics. | Desktop `ClientStatus::format` and retained `Console::push_client_text`; terminal `terminal_client` connection event printing. |
| Gameplay, transfer, chat and snapshot | `messages::ClientText`, `UiMessage::{Chat, PlayerState, TerminalSnapshot, WorldTransferPrepared, WorldTransferCommitted, WorldTransferCancelled}` keep IDs, names and data as arguments. | Desktop session message handlers retain typed lines; terminal `terminal_client/events.rs` formats at print time. |
| Commands, properties, notices and environment | `UiMessage::{CommandHelp, PropertyDescription, Notice, EnvironmentStatus, EnvironmentTitle}` and typed `UiNotice`/`UiStatus`; command and JSON tokens remain data. | Desktop console/status; terminal command and property action dispatch. |
| Preserved history on locale switch | `ConsoleLine.localized` retains `UiMessage` or `ClientText`; `Console::relocalize` reformats all retained lines from the new locale. | `renderer/settings.rs` calls relocalize on set and reset. Already printed terminal stdout is immutable. |

| Input or edit operation | Context and owner | Consumer/evidence |
| --- | --- | --- |
| Physical key/mouse | `InputMapper::down` resolves global or active context, rejects gameplay actions outside Gameplay and map toggles under Console/Modal; `window_events::input_context` chooses Console, Map or Gameplay. | `window_events` dispatches fullscreen, console, map, cancel, jump, interact and fire; `camera::movement` reads held movement only while gameplay owns input. |
| Focus/context change | `window.rs` clears held on focus loss; `window_events` and `overview::toggle_overview` clear held on context switch. | Prevents a held gameplay control from surviving into or out of console/map. |
| Text and IME | `window.rs` routes `Ime::Preedit` to separate editor state and `Ime::Commit` to insertion; keyboard Enter checks `is_composing`. | Preedit never executes a command; `console_events` receives non-composition keyboard text. |
| Left/right/word/delete/select/mouse hit | `Editor` uses `UnicodeSegmentation` for navigation, deletion and boundary normalization; `input_draw::hit` maps a grapheme column to its byte boundary. | Console input editor; insertion respects whole-grapheme capacity and cursor is advanced to a valid boundary after text changes. |
| Preferences | `config.rs` validates `ui.locale` and `client.input.bindings`; `SettingsStore` holds requested/effective/source and session overrides. | Desktop startup creates `InputMapper`; live set/reset rebuild it and relocalize; terminal shares config and property validation, while its line input does not use the desktop physical-key mapper. |

The repository contains `apps/client/assets/fonts/RobotoMono.ttf` and the adjacent SIL OFL 1.1 license. `glyph_atlas::build` rasterizes the actual bundled font at fixed 20 logical pixels, uses font advance/bearing/ascent, checks every glyph against its 32-pixel cell and holds atlas memory below 1 MiB; `text::append_text` positions combining marks against the previous base. `Console::set_dpi_scale` records a bounded scale, both drawing and hit testing convert physical coordinates to logical coordinates, and the GPU transform applies the scale to framebuffer pixels. The atlas is deliberately fixed-size across DPI changes; `font_hash`, size and character identify its bounded glyph map. The old `console-atlas.bin` has no consumer and is removed from the worktree.

Changed code: Fluent format errors now try the next catalog; editor insertion preserves whole graphemes and valid cursor boundaries; active controls are cleared on context changes and gameplay actions are suppressed outside Gameplay. Source review above is the evidence for remaining existing behavior. Compilation/formatting/static checks remain deferred to E046-17; runtime, visual appearance and tests remain unverified.

## E046-12 source review addendum (steps 38–39)

| Setting path | Requested/source | Effective/reason | Consumer |
| --- | --- | --- | --- |
| Startup/config show | `config::resolve_preferences` validates all resolved values, then `SettingsStore::new` captures source from `ConfigResolution`; `config show --sources` uses the same record with unavailable effective/reason before a frontend applies it. | Preview is explicitly unavailable; no renderer outcome is invented. | Both desktop and terminal use the shared config resolver. |
| Live desktop get/set/reset | `renderer/settings::apply_property` takes parsed `ClientPropertyValue`, writes session request or resets to `SettingsStore.startup`. | Consumers update effective; sky quality reports actual GPU tier and fallback reason. `property_line` reads requested/effective/source/reason from the store; runtime movement/time are separate live policies. | Renderer applies presentation effects; locale re-localizes history and bindings rebuild `InputMapper`. |
| Terminal get/set/reset | `TextProperties` owns a `SettingsStore`; locale and bindings change both options and record; renderer-only preferences report effective unavailable and reason. | `TextProperties::get` reads record values. | Terminal command executor and shared config save use session requested values. |
| Save | `SettingsStore::session_values` selects only `PropertySource::Session` requested values. | Device fallback is never serialized as requested. | Desktop and terminal config save actions. |

| Transition | Owner and accepted event | Effect/consumer |
| --- | --- | --- |
| Connect/disconnect | `ClientApplication::start_remote/start_local/disconnect_session` call the GPU-free `ConnectionController` and clear session context. | Desktop session controller handles presentation cleanup; terminal calls the same runtime controller without GPU. |
| World prepare → ready | `WorldTransitionController::register` rejects duplicate ticket; `PendingSectorTransition::advance(Ready)` accepts only Preparing. | Renderer session readiness prepares CPU/GPU resources and sends ready once. |
| Ready → commit | `can_commit/commit` compare ticket and target context and require ReadySent. | Renderer installs verified world content, then application changes current instance/session context; stale commit fails. |
| Prepare/ready → cancel/fail | `cancel` checks source context; `fail` checks ticket; each removes terminal phase. | Renderer releases prepared target on cancel and fails connection on invalid content. |
| Overview | `ClientApplication` privately owns `MapOverviewModel`; open is `Some(instance)`, close is `None`, zoom is finite/bounded, drag has begin/end methods. | Renderer has immutable overview view and invokes application lifecycle methods; GPU handles stay in renderer. |

`ClientGameState` lives in `application/game.rs`, and its `view` builds the GPU-independent `ClientView`; `RenderFrame` carries CPU presentation input to renderer upload. `renderer/client_game.rs` is removed, and no application file imports `GpuResources`, `wgpu` or renderer client-game code. The new `ClientApplication::new` constructor keeps transition/overview fields private while initializing the desktop host. Compilation and static checks remain deferred to E046-17; runtime and tests were not executed.

## E046-13 source review addendum (step 40)

| Operation | Owner and input | Publication or release |
| --- | --- | --- |
| Selection, bounds, source and uniform | CPU `shadow_prepare::prepare_shadow_frame` receives `ShadowPreparationInput` with environment, `GeometryPublicationStamp`, versions, resident sector keys/bounds, world map, render origin and settings. | Returns immutable `PreparedShadowFrame` with context, stamp, keys, uniform, prepared map geometry and invalid reason; invalid input returns a reason and clears valid state. |
| Local map caster conversion | CPU `ShadowMapPreparationCache::prepare` calls `shadow_geometry::local_map_geometry` and `geometry::sector_geometry_relative`, quantizes center, checks 2 MiB byte limit and caches an `Arc<[Vertex]>` keyed by publication/world/origin. | Produces `PreparedMapCasters`; cache metadata updates only after complete CPU preparation. |
| Sector and remote caster upload | GPU `world_geometry_gpu::prepared_upload` stages sector buffers until complete publication; `GpuResources::update_remote_shadow_vertices` uploads remote caster batch and records matching stamp only on success. | Incomplete or failed upload leaves shadow publication disabled. |
| Map caster upload and uniform | GPU `GpuResources::upload_prepared_shadow_map` checks publication stamp/version/2 MiB and returns Ready, StaleContext or Failed; `render` maps absent preparation to Pending. | Cache key updates only after successful bounded upload; `render` writes uniform and enables shadow pass only on Ready with matching context, stamp and instance. |
| Encode | GPU `GpuResources::render` consumes one immutable `RenderSceneSnapshot` carrying environment, `RenderFrame`, publication and shadow result. | Shadow pass uses owned buffers only when `shadow_publish_ready`; no session, storage, process config or `ConnectionController` enters this method. |
| World release | `Renderer::release_world_sectors` is the only caller-facing release path; it clears `ShadowMapPreparationCache` and invokes GPU release. | GPU release drops world geometry and matching map caster cache, resets readiness; callers are world install, sector readiness and transfer cleanup. |

`shadow_prepare_cpu` is measured around `Renderer::prepare_shadow_frame` in `renderer/camera.rs`: it includes reading immutable GPU publication metadata and CPU selection/geometry, but excludes upload and encoding. `shadow_gpu.rs` has no retained EnvironmentFrame/EnvironmentFrameStatus/Zeroable imports. `RenderFrame` in `crates/renderer/src/render_frame.rs` contains CPU values and `ClientView`, with no `wgpu` handle; terminal-only build does not construct desktop GPU resources. No new code change was required for this already connected boundary; the ownership README above records its release and timing scope. Compilation, static checks, runtime and visual appearance remain deferred.

## E046-14 source review addendum (step 41)

`GeometryPublicationStamp` contains instance, generation, content revision,
world fingerprint, monotonic geometry revision and origin generation. A
`GeometryPublication` holds the exact active `(SectorKey, LOD)` set. The scene
encoder reads this publication; `PreparedShadowFrame` and
`ShadowMapCacheKey` carry the same stamp; remote casters hold the same stamp.
`GpuResources::render` compares shadow context/instance/stamp with the scene
snapshot before bounded map upload and checks the stamp again at shadow pass
encode. Only Ready writes the uniform and enables sampling. Failure and
Pending leave `shadow_publish_ready=false` with a distinct diagnostic reason.

| Manual transition | Active publication and invalidation | Earliest permitted shadow |
| --- | --- | --- |
| LOD A→B over several uploads | `cache_prepared` stages B within 8-patch/4 MiB frame budget. Existing A publication and buffers remain intact while B is incomplete; `ready` now requires both matching source and origin generation for active reuse. On complete B, pending buffers replace the whole selection, `publish_scene` increments revision, and old shadow/map cache stamps fail equality. | After B publication, remote casters and CPU map casters are prepared/uploaded for B's new stamp. A can remain shaded only while the complete A scene is still the active publication. |
| Rebase during pending upload | `set_render_origin` calls `WorldGeometryGpu::rebase` for active and pending buffers, increments origin generation/revision, clears remote casters and map cache. On rebase failure it clears world geometry. | After new-generation publication plus matching remote/map caster uploads; stale old-origin results fail `cache_sector_scene`/`activate_sector_scene`. |
| Transfer | `Renderer::release_world_sectors` invalidates the old world's publication and dependent CPU/GPU map cache. `install_world` can publish only the target world with verified fingerprint and complete keys. | After target publication and matching context, remote stamp and map upload; old-world stamps fail equality. |
| Late sector result | `cache_sector_scene` rejects origin or generation mismatch before staging; `cache_prepared` drops pending entries whose source/origin differs; `publish_scene` requires every selected key resident, current origin and not pending. | Only a newly complete publication; an old result never makes its stale stamp active. |

`push_shadow_fields` now reports active scene revision and origin generation
from the current publication, plus separate expected shadow revision/origin
for diagnosis. It also reports light source, texel size, caster count and
`shadow_incomplete_reason` (NoSource, BudgetExceeded, StaleContext,
IncompleteCasters, upload_pending or upload_failed). The fix to
`prepared_upload::cache_prepared` prevents source identity alone from making
an old-origin active buffer count as ready. Resolution, map/pass count and bias
were unchanged. Source and diff reviewed; compilation, runtime and visual
flicker/appearance remain unverified until their designated phases.

## E046-15 source review addendum (steps 42–44)

| Rule | Scope | Detected violation | Limit |
| --- | --- | --- | --- |
| Exactly one workspace declaration | `check_architecture.py` reads `architecture.toml` package list before mapping and compares both directions with Cargo metadata workspace members. | Duplicate, missing or unknown declaration produces a named error before package lookup. | Cargo metadata must resolve under the final lockfile. |
| Production dependency boundaries | Cargo metadata `--all-features` resolve graph: normal/build, optional activated, target-specific; dev graph counted separately. | Transitive forbidden path from every engine-domain/runtime to sample; domain→application/game, server→GPU, renderer→transport/storage/window and per-package forbidden edges. | Metadata is a resolved all-feature graph; it does not prove behavior of code behind inactive external cfg. |
| Source effects | Every declared domain/runtime package root from manifest paths, plus server application, recursive `.rs` source scan. | Explicit std fs/process/net/io, GPU/window imports, forbidden adapter imports, FFI/link attributes and sample references. | Regex catches text/import syntax, not semantic aliases imported through another module, macro expansion or runtime effects; it may flag comments/strings. |
| Rust/WGSL layout and ownership | `check_static_refactor.py` compares HaloSource, EnvironmentUniform, CameraUniform, AtmosphereUniform, CloudUniform and CloudRecord fields; scans Rust definitions, WGSL placement and `RenderFrame`. | Layout mismatch, duplicate/missing owner, GPU handle in CPU frame, old client-game/earth path, old terrain entry point, old halo body special case. | Text parser is intentionally narrower than rustc/naga; no dependency checks are duplicated here. |
| Physical size | `check_file_lengths.py` walks disk source suffixes including `.rs` and `.wgsl`, tracked and untracked, and checks scoped exception registry. | >500 physical lines, stale/duplicate/malformed exception. | Generated-name/build/vendor/data artifacts excluded as documented in `docs/plan/file-length-policy.md`. |
| Shader variants | `wgsl_validator` composes current shader_source variants plus standalone shadow/rebase and checks Naga parse/validate and selected binding/layout contracts. | Invalid WGSL or selected resource contract. | Does not create adapter or assess appearance/performance. |

Source search found no production caller of the removed `renderer/client_game.rs`,
`procgen/earth.rs`, `generate_console_atlas.py` or binary console atlas; all four
paths are absent. Names describing explicit rejection of old wire/save formats
remain documentation and error semantics, not compatibility decoders. Removed
the unused `TransferOffer` re-export, the broad `allow(unused_imports)` and
`allow(dead_code)` in server registry, and the broad feature-only dead-code
allow on client messages. The WGSL validator no longer carries an unreachable
`globe` branch; its current atmosphere variants share the actual binding rule.
No tests or static scripts were run; their execution is reserved for E046-17.
