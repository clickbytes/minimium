# minimium-client-runtime

E051 distinguishes a session sample and a submitted intent from a server commit. [Public operation contract](../../docs/console-api/contract.md)

A host can correlate a local command with the session identity already used by content jobs and the current active world context.

One local admin worker carries a checked request ID and nonce through the owner reply into `ClientStepOutput.admin_result`. A disconnect cancels an unsent request. Once sent, the worker returns the verified result or an unknown outcome. [Wire contract](../../docs/console-api/admin-v1.md)

## CPU world and collision readiness

`world_readiness::WorldReadiness` owns the client sector manager, immutable
source/overlay inputs, pinned bootstrap/active/pending interests and the existing
bounded materialization/collision workers. `ensure_collision` takes a typed owner,
manifest, world position, exact map and `TerrainInputs`; it needs no renderer,
FOV, viewport, render origin or GPU. Pending work returns `Ok(None)`.
`ensure_collision_for_step` requests the union of the unchanged neighborhoods at
the current and speculative positions. A narrower request reuses a still-valid
superset interest, so retrying the current position does not cancel target work.
Long-lived hosts call `retain_interests` to release obsolete memberships/transfers.

Only this owner constructs `CollisionSnapshot`. Its private identity binds the
world/membership or pending transfer, manifest, map fingerprint, material/relief
and geography source, independent terrain revisions, collision revision and
installed sector coverage. `matches`, `matches_terrain` and `covers_position`
validate consumers. Coverage certifies the existing fixed-step neighborhood,
not an arbitrary capsule or unbounded sweep; hosts still validate their water
coverage and speculative destination before committing movement.

The existing limits remain 512 cached/queued sectors, 32 materialization slots
and up to 32 installations per poll, plus three scene collision interests. The
public owner also bounds retained world sources to 512 and active interests to
three. Release cancels ticket publication and aggregation; physical generation
already running may finish, but its stale ticket cannot install. Promotion accepts
only the matching pending membership or matching bootstrap world; it cannot
relabel another world's collision. Collision revisions use checked allocation.
`shutdown_until` uses an absolute physical resource deadline, not logical time.

The presentation adapter consumes the snapshot's same immutable sector payloads.
Presentation readiness is a separate host report bound to this CPU identity.
Desktop retains its visible-terrain gate; a CPU host can prepare collision and
complete transfer readiness without rendering. `collision_ready` is a real public
CPU provider and prediction consumer, compiled without running it.
`ClientPrediction::skip_unready_step` consumes a due input interval without
integrating unproved geometry. The default host uses it to keep neutral movement
at the existing fixed-step cadence while waiting for CPU/presentation readiness;
it does not create an input sequence, mutate the character or accumulate debt.

## Purpose / Non-goals

Owns the nonblocking protocol session, connection controller, connector, local-server child guard, fixed-step local movement prediction, authoritative snapshot reconciliation, remote-player interpolation, world transition handshake, synchronized world-clock estimate and client-runtime diagnostic events. It receives a fixed-domain logger through its application-facing controller/configuration APIs; the telemetry file sink stays outside the runtime. It owns no window, GPU, console rendering, localized environment status formatting or process event loop.

`ClientSession::tool_action_pending` exposes whether the current context has an
unconfirmed tool action. Developer overlays combine it with confirmed inventory
and the same simulation collision query used by the server; they cannot approve edits.

## Public CPU application core

`ClientApplicationCore` is the single GPU-independent owner for active session,
world identity/catalog, prediction, reconciliation, typed actions and transfer
state. Its constructor accepts prepared maps/manifests, logical policy, an
explicit tool profile, optional already-prepared bootstrap terrain, the shared
`ConnectionController`, an optional already-prepared AwaitingWelcome session and
host `ClientTime`. It starts no workers or transports; session adoption remains
inside the controller and does not create a second session owner.

Hosts advance the core through `step`; raw server messages are applied only
there. Prediction accepts explicit horizontal/vertical input suppression and
separate CPU-readiness and optional presentation gates. A CPU host can omit
prediction and presentation; the desktop supplies UI policy and its additional
presentation gate. `view()` returns a read-only `ClientView`, while commands use
typed actions and host readiness uses validated time-bearing reports.
Authoritative altitude, health, teleport and snapshot results are applied by the
core during session dispatch.

Bootstrap terrain is host-prepared and bound to the exact current instance,
manifest, map fingerprint and overlay world. `None` remains valid for cold Earth
content. Disconnect clears the prior source; the host can reprovision terrain
for the current offline world after the core releases the session. The
`cpu_client` example composes bounded local transport/content ports, dispatches
Welcome, initial readiness sync and Snapshot through `step`, and reads the
immutable view. It is compiled only; examples and tests are not executed here.

## Public API

`ClientSession::new` and `with_presentation` take a boxed
`minimium_transport::client::ClientTransportPort` and a `ContentJobsSession`; they configure no socket and
return typed queue failure if the initial Presentation/Authenticate admission
fails. `ConnectionController` wraps the connector's connected stream in the
single `TcpClientTransport` adapter for both desktop and terminal. Logical
authentication, context/replica/transfer checks and permission-dependent actions
remain in the session. Constructors also require a host-supplied `ClientTime`;
they never sample a clock themselves.

`ContentJobsPort` accepts typed world and material requests, returns an accepted
`ContentJobHandle` or Busy/rejection, and supports targeted completion polling,
cancellation and deadline-bounded host shutdown. Acceptance does not mean that
content is ready. The default application adapter has one world slot and one
material slot, including unread completions and cancelled work that has not yet
exited. Alternate implementations must declare and enforce finite capacities.
An accepted handle contains its ticket and a fresh atomic cancellation capability;
recording that intent is nonblocking even when adapter cleanup is contended.
Requested cancellation is distinct from confirmed work termination.

The host installs one service with `ConnectionController::set_content_jobs`
before the first connection attempt. `ContentJobsSession` binds it to a nonzero
session identity consumed by one session. IDs must never be reused during that
service's lifetime. The default controller uses checked monotonic attempt IDs.
The session owns no worker handle, cache path or cache budget and never joins
workers. The `injected_content_jobs` example demonstrates a real session request,
separate host preparation and ticketed completion using public prepared data.

`PreparedWorldContent` has private fields and prepares a base-only water catalog
from the exact resolved source before a host returns completion. Session-side
ticket/manifest/fingerprint and active/pending validation still gates publication.
Material completions require the offered manifest hash and requested variant.

The adapter keeps bounded wire bytes, codecs, partial I/O and typed peek/consume.
Session dispatch can retain a deferred world message and inspect the next typed
message without consuming unrelated input, preserving cancellation behavior.
The session tracks Ping's stable frame token; its timeout/RTT starts at the
host sample supplied to the flush that actually completes the frame. Time-probe t1 keeps its existing
enqueue-time semantics. `SessionIoFailure` retains a cloneable concrete I/O
cause, its kind and stable diagnostic code on `DisconnectReason::Io`.
`close` discards the transport and pending output immediately, as before.
The compile-only `injected_transport` example supplies a bounded bidirectional
local port to a real session; no network, GPU or application execution is needed.

Resolved planetary content includes a successfully sampled water catalog before
CPU readiness can be reported. Construction or sampling failure becomes a typed
`SessionFailure` retaining `GenerationError` as its source; no pending content
bundle is published from that failed resolver result. Nonplanet content has no
planetary catalog. Transfer ticket/context validation still rejects stale results.

Presentation clients negotiate `ContentOffer` before `Welcome` under protocol v22.
Content preparation has its own session-owned 300-second deadline. A bounded host worker fetches the
required package over a separate HTTP endpoint at the game peer's IP and caches
verified hash blobs. The host takes the complete package, installs GPU resources,
then calls `finish_content`. A zero material hash selects the offline base set;
required custom sets never silently fall back. Failures disconnect with a retry
message; `connect` retries and reuses verified blobs. The offered climate scenario
is immutable for the session. Text clients declare no presentation and skip assets.

`ClientSession` is created with a host-supplied bounded ClientTransportPort and identity. `ConnectionController` coordinates session, remote/local connection attempts and process cleanup; `Connector` exposes typed attempt events. The host content adapter supplies a `WorldContentResolver` which receives a session/context/transition/request ticket, manifest, expected map fingerprint and cancellation token. It returns `ResolvedWorldContent::prepare(...)`, which validates a private `PreparedTerrainSource` bound to that ticket's world instance, manifest and exact map fingerprint, including the resolved material/relief policies and matching Earth geography. Generated planets require the relief binding; missing or mismatched identities reject the result before publication. Resolution runs on one host-owned worker; `poll` publishes the complete result only after ticket, context, manifest, fingerprint and source binding checks. `ResolvedWorldContent` exposes read-only getters and the prepared source; callers cannot replace its manifest, map, geography or profiles after preparation.

`ClientSession::terrain_inputs()` returns `Option<TerrainInputs>` only when active content matches the current context. `terrain_inputs_for_context(context)` selects the exact active or pending bundle for that context. Each `TerrainInputs` requires a prepared source and a separate `TerrainOverlaySnapshot`; no default or geography-only input can enter terrain workers. Active edits, remnants and volumes are included only when their owner state is ready; pending target preparation starts with overlays bound to the target instance and does not borrow source-world overlays. `SectorContentVersion` keeps the existing independent monotone revisions. `prepared_geography()` exposes only the active world's immutable geography; `pending_geography(context, transition)` is an explicit destination-preparation read.

The session keeps one `ActiveWorldContent` and at most one context-bound
`PendingWorldContent`. A target's preparation never changes active geography,
material profile, relief policy or water. A matching server commit swaps the
complete pending bundle into active state; confirmed cancellation, failure or
disconnect drops pending data. A late worker result cannot publish after its
ticket is superseded. While resolution runs, the decoder retains only the newest
transition request. The 300-second deadline covers active and waiting work;
GEBCO and optional WorldCover hashes are checked in 8 MiB chunks.

`SessionFailure` retains its typed source and can carry the `ContentResolutionTicket` that failed, including resolver, water-catalog, worker, timeout and install failures. Material failures similarly retain MaterialContentTicket; retry formatting preserves their typed cause. Ticket metadata survives clearing the worker state and does not replace the stable failure code or cause.

`ClientPrediction` accumulates elapsed time, reconciles authoritative local snapshots and returns one shared-integrator step with rollback state; the host supplies collision snapshots. `SnapshotReconciler` applies bounded remote-player snapshots and interpolates positions into a reusable host buffer. `ClientView` is an immutable, GPU-independent snapshot of active world, local character/position, health, remote positions and sector readiness for the presentation adapter. The host calls `poll` and consumes typed `SessionEvent` values; `queue` returns typed backpressure and state errors.

## Dependencies

Consumes core identity, environment clock values, protocol, world contracts, procgen interfaces and asset adapters. Sample scenario resolution is supplied by the application and is not a production runtime dependency. It does not depend on the client application, server runtime, renderer, astronomy FFI or windowing.

## Data flow

Welcome and transfer messages use one manifest check before requesting content:
generated worlds require exact revision 6 and the topology digest, while
authored worlds require the `AUTH` digest of the offered map fingerprint.

The connector performs DNS, socket and local process startup away from the UI lane. The host polls `ConnectionController`; the session consumes typed messages, asks the injected content service for exact world content and emits typed events and synchronization estimates to presentation adapters. Runtime owns the transfer ticket, duplicate-prepare identity, cancellation race and `TransitionReady` queue retry. The terminal reports readiness after resolved CPU content is available; desktop also reports it after its target scene has been published. `queue_world_input` binds the current context and assigns movement sequence/tick values from server snapshots, committing counters only after the frame enters the bounded send buffer. The host supplies each fixed-step collision snapshot to `ClientPrediction`, which shares the server's simulation integrator and records each transmitted predicted step with its policy and collision revisions. `SnapshotReconciler` owns authoritative time ordering and remote-player interpolation; the host turns output positions into presentation geometry. Prepared geography remains immutable and is held inside the ticket-bound source shared by `Arc` with generation and presentation workers.

## Ownership and execution

The caller owns and polls the controller on its event-loop lane. A session owns logical protocol state and its injected message port; minimium-transport owns the default TCP adapter, framing and bounded wire buffers; the controller owns the connector and optional local-server guard. The runtime owns local prediction and snapshot/interpolation state, while the host owns collision snapshots and rendered geometry. Prepared geography is immutable and shared by `Arc` with generation and presentation workers.

`ConnectionController::start_remote` and `start_local` reject an existing session,
attempt or pending cleanup before calling the connector or changing owned state.
`StartError::Busy` carries an optional attempt ID: session/cleanup rejection has
no synthetic ID, while a live connector worker retains its actual ID. Cancelled
workers remain busy until drained; after session removal and cleanup completion,
the same controller can start again without losing its local-server guard.

`TransferTicket` binds a transition ID to its target context. Hosts report
readiness through `ConnectionController`; the runtime queues one
`TransitionReady` after CPU content resolution and host readiness both succeed,
retrying if the bounded send buffer is full. A local cancel suppresses readiness
that has not yet been queued. If readiness was queued first, the pending bundle
remains available for an authoritative commit or server cancellation.

`ConnectionController::finish_until` accepts the process-owned absolute work
deadline; desktop and terminal use the same 30-second endpoint cleanup and
diagnostics deadline, with the last two seconds reserved for the log writer.
Forced owned-child cleanup uses a bounded post-kill poll instead of an
unbounded `Child::wait`.
It closes the session, requests content cancellation and asks the host service
to drain only until that same deadline. Pending means work still owns resources;
it does not claim interrupted I/O. Explicit later calls may retry that drain.
Drop does not add another content wait after an explicit shutdown attempt.

## Invariants and errors

`ClientTime::new(Instant)` wraps a monotonic sample supplied by the host. Every
logical constructor, poll, queue/action, readiness, cancellation and shutdown
entry accepts that sample explicitly. Equal samples are valid. Pure
`validate_time` checks reject backward samples and unrepresentable deadlines or
relative milliseconds with `ClientTimeError::{Backward, Overflow}` before
consuming completions, writing transport bytes or changing lifecycle, counters,
queues and cooldown state. The session also validates its injected transport
before admitting its initial handshake. Hosts composing multiple mutable owners
must preflight all of them before any mutation, then pass the same sample into
the operation. Invalid samples leave those logical owners unchanged.

| Clock sites | Domain and preserved policy |
| --- | --- |
| Session state/Welcome, heartbeat/Ping, world/material preparation, transfer retry | Supplied `ClientTime`; existing 5s Welcome, 10s heartbeat cadence, 20s heartbeat timeout, 300s preparation remain unchanged. Ping starts only when its tracked frame is completely written; tool cooldowns advance on the game timeline. |
| Session clock epoch/reset, probe enqueue/receive, stale age and estimates | The session converts the supplied sample to checked relative milliseconds. Read-only clock getters report the last admitted sample. |
| Controller/connector starts, polls, cancellation and retry | Supplied sample is validated before lifecycle effects. Empty connector polls compare the logical attempt deadline against this sample. |
| Connector worker DNS/socket/process startup and completion | Physical `Instant` budgets bound actual OS work. An already available result completed within that resource budget still wins a late logical poll, preserving prior behavior. Worker availability remains an external resource fact. |
| Snapshot receipt/interpolation | Committed simulation milliseconds from the current presentation frame. |
| Prediction and input tick IDs | Fixed steps follow presented simulation time. Intent changes and held refreshes use the network clock. |
| Bootstrap, worker/child drain and controller cleanup | Physical OS/resource deadlines, including the existing 28s default controller cleanup budget. Drop uses the last admitted session sample for logical teardown and reads physical time only to bound resource drain. |
| Environment/astronomy | Authoritative UTC/world clock samples; never substituted for lifecycle time. |

The session interpolates between distinct committed boundaries using their server observation interval. It holds the last available endpoint when samples stop and snaps to the acknowledged pause tick. The same segment supplies game duration, calendar UTC and playback speed. RTT samples retain their monotonic probe clock.

The compile-only `host_clock` example drives the real injected session/transport
and content adapters with controlled equal, advancing and backward samples and
handles checked-add overflow through the public API. Desktop and terminal hosts
sample `Instant::now()` at each logical entry; they own sampling cadence.

Preserve bounded read/write work, frame limits, heartbeat behavior, world fingerprint checks and transition ordering. Queue, protocol, timeout, map, package, content-preparation and controller failures use typed errors. Session and controller failures have stable codes, and adapter causes remain available through `Error::source` until presentation formats them.

## Commands

The client application owns command parsing and dispatch. Runtime requests for
time scale and time-of-day changes use the authenticated endpoint of the owned
local server. `request_set_time` maps the requested minute and observer longitude
to UTC before dispatch; server clock validation remains authoritative.

## Change guide

Change connection/session lifecycle and clock synchronization behavior here. Keep presentation and user interaction in the application host.

Movement snapshots replace predicted physics with the authoritative state and carry the server's active movement intent for replay's initial held input. The client discards only the server-resolved sequence prefix. Replay uses each later sample's target tick, captured movement policy and collision revision. The server ack is bounded below the earliest still-executable sequence, including samples waiting in either server queue; rejection, busy, replacement, application and lateness resolve through the same owner. Revision mismatch, stale samples or history overflow clears replay history, held input, fixed-step time and presentation offset. History is capped at 128 samples. World installation and accepted teleports reset pending input history and the fixed-step accumulator.

The camera offset decays with elapsed monotonic time. Each accepted snapshot
adds the difference between the pre- and post-replay positions to the remaining
offset, using millimetre positions where available. The host receives the offset
after all session events for that step, alongside the corresponding player state.

## Verification

Run `rtk cargo check -p minimium-client-runtime -p minimium-client` and architecture/file-length checks. Runtime acceptance is deferred.


Protocol v19 carries peak geometry settings, climate-relief influence and context-bound gameplay actions. Local
server launch settings are supplied through `LocalServerLaunchConfig`; this
runtime does not parse client environment variables or shell command strings.
The launcher obtains a bounded, versioned offline JSON resolution from the
sibling server, validates local bind/headless constraints, and passes paths and
settings as argv values. Explicit non-default startup settings refuse attachment
to an existing server; `--server-data-dir` remains the discovery target selector.
No graphics dependency is added. `LocalServer::Attached` makes explicit shutdown a no-op; `LocalServer::Owned` alone sends the authenticated admin stop request, waits for the matching shutdown status and reaps its child. Dropping an owned guard attempts graceful shutdown and then bounded process cleanup; it never sends stop to a server it did not start.
The local-server shutdown API returns `io::Result` directly, so controller cleanup
workers retain the OS error kind and source instead of flattening the cause to text.

Protocol v19 entity synchronization is applied inside `ClientSession` to a
bounded replica store. Full snapshots are staged and published only on a
matching completion record; deltas are applied to a staged copy of the active
revision. Hosts register the component schemas supplied by their game modules.
Unknown component types and schemas terminate the session with a typed
replication failure. The session sends an acknowledgement only after the new
replica state has been committed. `replicated_entities()` exposes a read-only
view scoped to the current world context; it contains no renderer or GPU state.
Staging and active replicas share the engine limits of 16,384 entities, 16
components per entity and 4 MiB of aggregate component payload. An initial full
sync may legitimately commit revision zero before any live mutation.

Local bootstrap continuously drains child stderr on one reader thread while
forwarding it to the terminal. It retains at most 8 KiB and includes the final
server error (up to 2048 characters) in the connection failure shown by the host.
The reader is installed after the process guard, so capture failures still reap
only the owned child. Failed-start diagnostics wait for EOF only after child exit.
This changes error reporting, not world loading or saved data.

The application captures the mapped `MINIMIUM_LOCAL_*` aliases and supported
`MINIMIUM_SERVER_*` values once during option parsing. Empty aliases mean
omitted, explicit zero remains a climate override, and preview and child launch
receive the same captured environment. The runtime clears inherited setting
variables before applying that snapshot so a later environment read cannot
change precedence between validation and launch.
The local server configuration preview is authoritative for `world.seed` and
`world.planet_config`; explicit values are accepted because the same captured
inputs are passed to the owned child. Client map and authored definition paths
are still checked against that preview, and attaching to an existing server
rejects unapplied explicit settings.

The application content adapter owns the cache directory and configured
`--cache-limit-bytes` budget (8 GiB by default). Package leases remain active
through presentation handoff so cache eviction cannot remove in-use content.
The host also supplies bounded provider credentials; this runtime has no
account store or hard-coded authentication provider.
# Local Earth startup

Local server startup allows up to 30 seconds for a cold Earth package, inside
a 35-second local connection attempt. Remote connection attempts retain their
10-second deadline. Startup status and the attempt ID still determine whether
the client connects or reports failure.

The session accepts Bolid only for its active world context and discards it during transfer; event age uses the presentation sample and authoritative simulation start time.

`ConnectionController::request_random_bolid(count, now)` validates 1..64 and forwards
the bounded count through the existing owned-server admin command channel. It
does not send N separate requests or change the game protocol.

## Impact replica

Protocol 20 adds a context-bound `ImpactReplica` and initial-image readiness.
Out-of-world/old-membership messages are ignored; regressing or conflicting states
are rejected. Source-world updates remain valid during a pending transfer so a
cancellation cannot lose a durable edit. Disconnect clears the replica. Once
ready, `terrain_inputs` exposes the resolved content-bound material profile and
separate immutable crater and remnant snapshots;
`prepared_geography` remains the active world's unmodified geographic dataset.
Destination preparation receives its geography through the matching pending
context and transfer ticket, without exposing source-world crater or remnant
state to destination collision and terrain workers.
The replica owns no GPU, simulation authority or disk writes.

Resolved world content also carries the canonical terrain material profile
bound to its exact manifest and map fingerprint. Resolver results with mismatched
bindings are rejected before entering active or pending session state, and
`terrain_inputs_for_context` forwards the same profile to terrain consumers.
Generated planet content additionally carries a resolved relief-sampling policy
bound to those same identities. `ResolvedWorldContent::matches` rejects absent or
mismatched generated-planet policy before it can become active or pending;
`terrain_inputs_for_context` forwards the policy to terrain, collision and
render workers. This runtime binding adds no protocol, save or manifest field.

EnvironmentFrameInput exposes the committed session sample for clock operations and labels a local preview LocalDemo. ConnectionController::request_clock sends a correlated typed mutation only to the owned administrative endpoint of a connected session. A solar request captures the player from Welcome and the active WorldContext. The server resolves the position when the request executes.
