# minimium-client-runtime

## Purpose / Non-goals

Owns the nonblocking protocol session, connection controller, connector, local-server child guard, fixed-step local movement prediction, authoritative snapshot reconciliation, remote-player interpolation, world transition handshake and synchronized world-clock estimate. It owns no window, GPU, console rendering or process event loop.

## Public API

`ClientSession` is created with a connected stream, identity and expected map fingerprint. `ConnectionController` coordinates session, remote/local connection attempts and process cleanup; `Connector` exposes typed attempt events. `ClientPrediction` accumulates elapsed time, reconciles authoritative local snapshots and returns one shared-integrator step with rollback state; the host supplies collision and geography snapshots. `SnapshotReconciler` applies bounded remote-player snapshots and interpolates positions into a reusable host buffer. `ClientView` is an immutable, GPU-independent snapshot of active world, local character/position, health, remote positions and sector readiness for the presentation adapter. The host calls `poll` and consumes typed `SessionEvent` values; `queue` returns typed backpressure and state errors. `prepared_geography()` exposes the immutable geography prepared for the active Earth manifest.

## Dependencies

Consumes core identity, environment clock values, protocol, world contracts, procgen interfaces, assets and the sample scenario for deterministic manifest validation. It does not depend on the client application, server runtime, renderer, astronomy FFI or windowing.

## Data flow

The connector performs DNS, socket and local process startup away from the UI lane. The host polls `ConnectionController`; it validates frames and environment transitions, then emits typed session events and synchronization estimates to presentation adapters. The host supplies each fixed-step collision snapshot to `ClientPrediction`, which shares the server's simulation integrator. `SnapshotReconciler` owns authoritative time ordering and remote-player interpolation; the host turns output positions into presentation geometry.

## Ownership and execution

The caller owns and polls the controller on its event-loop lane. A session owns its socket and bounded wire buffers; the controller owns the connector and optional local-server guard. The runtime owns local prediction and snapshot/interpolation state, while the host owns collision snapshots and rendered geometry. Prepared geography is immutable and shared by `Arc` with generation and presentation workers.

## Invariants and errors

Preserve bounded read/write work, frame limits, heartbeat behavior, world fingerprint checks and transition ordering. Queue, protocol, timeout, map and package failures use typed errors.

## Commands

No user commands. The client application owns command parsing and dispatch.

## Change guide

Change connection/session lifecycle and clock synchronization behavior here. Keep presentation and user interaction in the application host.

## Verification

Run `rtk cargo check -p minimium-client-runtime -p minimium-client` and architecture/file-length checks. Runtime acceptance is deferred.
