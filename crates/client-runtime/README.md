# minimium-client-runtime

## Purpose / Non-goals

Owns the nonblocking protocol session, connection controller, connector, local-server child guard, fixed-step local movement prediction, authoritative snapshot reconciliation, remote-player interpolation, world transition handshake, synchronized world-clock estimate and client-runtime diagnostic events. It receives a fixed-domain logger through its application-facing controller/configuration APIs; the telemetry file sink stays outside the runtime. It owns no window, GPU, console rendering, localized environment status formatting or process event loop.

## Public API

Presentation clients negotiate `ContentOffer` before `Welcome` under protocol v19.
Content preparation has its own 300-second deadline. A bounded worker fetches the
required package over a separate HTTP endpoint at the game peer's IP and caches
verified hash blobs. The host takes the complete package, installs GPU resources,
then calls `finish_content`. A zero material hash selects the offline base set;
required custom sets never silently fall back. Failures disconnect with a retry
message; `connect` retries and reuses verified blobs. The offered climate scenario
is immutable for the session. Text clients declare no presentation and skip assets.

`ClientSession` is created with a connected stream and identity. `ConnectionController` coordinates session, remote/local connection attempts and process cleanup; `Connector` exposes typed attempt events. The application supplies a `WorldContentResolver` which receives a session/world/request ticket, manifest, expected map fingerprint and cancellation token, then returns a verified manifest, map and optional prepared geography. Resolution runs on one worker; `poll` only observes completion and publishes it after ticket, manifest, fingerprint and Earth package checks. While it runs, the decoder retains one newest transition request and replaces it when a newer request arrives. The 300-second preparation deadline covers active and waiting work. Disconnect or transition cancellation cancels outstanding preparation. GEBCO and optional WorldCover hashes are checked in 8 MiB chunks. `ClientPrediction` accumulates elapsed time, reconciles authoritative local snapshots and returns one shared-integrator step with rollback state; the host supplies collision snapshots. `SnapshotReconciler` applies bounded remote-player snapshots and interpolates positions into a reusable host buffer. `ClientView` is an immutable, GPU-independent snapshot of active world, local character/position, health, remote positions and sector readiness for the presentation adapter. The host calls `poll` and consumes typed `SessionEvent` values; `queue` returns typed backpressure and state errors. `prepared_geography()` exposes the immutable geography resolved for the active Earth manifest.

## Dependencies

Consumes core identity, environment clock values, protocol, world contracts, procgen interfaces and asset adapters. Sample scenario resolution is supplied by the application and is not a production runtime dependency. It does not depend on the client application, server runtime, renderer, astronomy FFI or windowing.

## Data flow

Welcome and transfer messages use one manifest check before requesting content:
generated worlds require exact revision 5 and the topology digest, while
authored worlds require the `AUTH` digest of the offered map fingerprint.

The connector performs DNS, socket and local process startup away from the UI lane. The host polls `ConnectionController`; the session decodes bounded frames, asks the application resolver for exact world content and emits typed events and synchronization estimates to presentation adapters. `queue_world_input` binds the current context and assigns movement sequence/tick values from server snapshots, committing counters only after the frame enters the bounded send buffer. The host supplies each fixed-step collision snapshot to `ClientPrediction`, which shares the server's simulation integrator and records each transmitted predicted step with its policy and collision revisions. `SnapshotReconciler` owns authoritative time ordering and remote-player interpolation; the host turns output positions into presentation geometry. Prepared geography is immutable and shared by `Arc` with generation and presentation workers.

## Ownership and execution

The caller owns and polls the controller on its event-loop lane. A session owns its socket and bounded wire buffers; the controller owns the connector and optional local-server guard. The runtime owns local prediction and snapshot/interpolation state, while the host owns collision snapshots and rendered geometry. Prepared geography is immutable and shared by `Arc` with generation and presentation workers.

`ConnectionController::finish_until` accepts the process-owned absolute work
deadline; desktop and terminal use the same 30-second endpoint cleanup and
diagnostics deadline, with the last two seconds reserved for the log writer.
Forced owned-child cleanup uses a bounded post-kill poll instead of an
unbounded `Child::wait`.

## Invariants and errors

Preserve bounded read/write work, frame limits, heartbeat behavior, world fingerprint checks and transition ordering. Queue, protocol, timeout, map, package, content-preparation and controller failures use typed errors. Session and controller failures have stable codes, and adapter causes remain available through `Error::source` until presentation formats them.

## Commands

The client application owns command parsing and dispatch. Runtime requests for
time scale and time-of-day changes use the authenticated endpoint of the owned
local server. `request_set_time` maps the requested minute and observer longitude
to UTC before dispatch; server clock validation remains authoritative.

## Change guide

Change connection/session lifecycle and clock synchronization behavior here. Keep presentation and user interaction in the application host.

Movement snapshots replace predicted physics with the authoritative state and carry the server's active movement intent for replay's initial held input. The client discards only the server-resolved sequence prefix. Replay uses each later sample's target tick, captured movement policy and collision revision. The server ack is bounded below the earliest still-executable sequence, including samples waiting in either server queue; rejection, busy, replacement, application and lateness resolve through the same owner. Revision mismatch, stale samples or history overflow clears replay history, held input, fixed-step time and presentation offset. History is capped at 128 samples. Position correction is a decaying presentation offset; it does not alter simulation state. World installation and accepted teleports reset pending input history and the fixed-step accumulator.

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

Material downloads use the host-provided cache directory and configured
`--cache-limit-bytes` budget (8 GiB by default). Package leases remain active
through presentation handoff so cache eviction cannot remove in-use content.
The host also supplies bounded provider credentials; this runtime has no
account store or hard-coded authentication provider.
# Local Earth startup

Local server startup allows up to 30 seconds for a cold Earth package, inside
a 35-second local connection attempt. Remote connection attempts retain their
10-second deadline. Startup status and the attempt ID still determine whether
the client connects or reports failure.

The session accepts Bolid only for its active world context and discards it during transfer; server monotonic time is estimated from the existing probe clock.
