# minimium-client-runtime

## Purpose / Non-goals

Owns the nonblocking protocol session, connection controller, connector, local-server child guard, fixed-step local movement prediction, authoritative snapshot reconciliation, remote-player interpolation, world transition handshake and synchronized world-clock estimate. It owns no window, GPU, console rendering or process event loop.

## Public API

Protocol v15 presentation clients negotiate `ContentOffer` before `Welcome`.
Content preparation has its own 300-second deadline. A bounded worker fetches the
required package over a separate HTTP endpoint at the game peer's IP and caches
verified hash blobs. The host takes the complete package, installs GPU resources,
then calls `finish_content`. A zero material hash selects the offline base set;
required custom sets never silently fall back. Failures disconnect with a retry
message; `connect` retries and reuses verified blobs. The offered climate scenario
is immutable for the session. Text clients declare no presentation and skip assets.

`ClientSession` is created with a connected stream, identity and expected map fingerprint. `ConnectionController` coordinates session, remote/local connection attempts and process cleanup; `Connector` exposes typed attempt events. `ClientPrediction` accumulates elapsed time, reconciles authoritative local snapshots and returns one shared-integrator step with rollback state; the host supplies collision and geography snapshots. `SnapshotReconciler` applies bounded remote-player snapshots and interpolates positions into a reusable host buffer. `ClientView` is an immutable, GPU-independent snapshot of active world, local character/position, health, remote positions and sector readiness for the presentation adapter. The host calls `poll` and consumes typed `SessionEvent` values; `queue` returns typed backpressure and state errors. `prepared_geography()` exposes the immutable geography prepared for the active Earth manifest.

## Dependencies

Consumes core identity, environment clock values, protocol, world contracts, procgen interfaces, assets and the sample scenario for deterministic manifest validation. It does not depend on the client application, server runtime, renderer, astronomy FFI or windowing.

## Data flow

The connector performs DNS, socket and local process startup away from the UI lane. The host polls `ConnectionController`; the session decodes bounded frames, dispatches connection and transition messages, and emits typed events and synchronization estimates to presentation adapters. Client message contextualization is isolated at the queue boundary. The host supplies each fixed-step collision snapshot to `ClientPrediction`, which shares the server's simulation integrator. `SnapshotReconciler` owns authoritative time ordering and remote-player interpolation; the host turns output positions into presentation geometry.

## Ownership and execution

The caller owns and polls the controller on its event-loop lane. A session owns its socket and bounded wire buffers; the controller owns the connector and optional local-server guard. The runtime owns local prediction and snapshot/interpolation state, while the host owns collision snapshots and rendered geometry. Prepared geography is immutable and shared by `Arc` with generation and presentation workers.

## Invariants and errors

Preserve bounded read/write work, frame limits, heartbeat behavior, world fingerprint checks and transition ordering. Queue, protocol, timeout, map and package failures use typed errors.

## Commands

The client application owns command parsing and dispatch. Runtime requests for
time scale and time-of-day changes use the authenticated endpoint of the owned
local server. `request_set_time` maps the requested minute and observer longitude
to UTC before dispatch; server clock validation remains authoritative.

## Change guide

Change connection/session lifecycle and clock synchronization behavior here. Keep presentation and user interaction in the application host.

## Verification

Run `rtk cargo check -p minimium-client-runtime -p minimium-client` and architecture/file-length checks. Runtime acceptance is deferred.


Protocol v16 adds contextual `SetAltitude` wrapping and `AltitudeResult`.
The application owns parsing and presentation; the server computes and validates
the destination. No GPU dependency is introduced into the runtime.

Protocol v17 carries peak geometry settings and climate-relief influence. Local
server launch settings are supplied through `LocalServerLaunchConfig`; this
runtime does not parse client environment variables or shell command strings.
The launcher obtains a bounded, versioned offline JSON resolution from the
sibling server, validates local bind/headless constraints, and passes paths and
settings as argv values. Explicit non-default startup settings refuse attachment
to an existing server; `--server-data-dir` remains the discovery target selector.
No graphics dependency or automatic server shutdown is added.

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
