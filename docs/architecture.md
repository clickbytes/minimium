# Minimium Architecture

## Product definition

Minimium is a Rust engine for networked first-person games in full 3D, with an MMO target. Client and server are separate processes, including during local play. The server owns authoritative simulation and persistence. Art direction may use 2.5D presentation without limiting the world model.

## Runtime processes

- Client: input, renderer, audio, HUD, console, prediction and interpolation.
- Server: headless simulation, game networking, persistence, metrics, terminal console and local admin endpoint.
- minimiumctl: invokes typed admin commands over loopback IPC.
- Load test: headless clients and repeatable load scenarios.
- Tools: offline asset cooking and map validation, then an editor.

Local play launches the server as a child process and connects through the normal game protocol. The admin endpoint is separate, binds to loopback, and uses an OS-generated secret stored in an endpoint file. `minimium-server run --headless` disables stdin commands for a service manager; `minimiumctl` still connects to the same command registry. The server stays in the foreground so the service manager can own its lifetime.

## Workspace layout

~~~text
apps/client/       client executable and platform shell
apps/server/       headless server executable and runtime shell
apps/ctl/          admin CLI
apps/loadtest/     simulated network clients and scenarios
apps/tools/        asset cooking and map validation
crates/core/       IDs, time, config, math primitives and errors
crates/commands/   typed command schemas, parsing and permissions
crates/protocol/   versioned messages and validation limits
crates/transport/  network transport adapters
crates/world/      3D world data, spatial cells and zone ownership
crates/simulation/ fixed-step movement, collision, combat, AI and events
crates/storage/    snapshots and durable state operations
crates/assets/     manifests, cooked packages and runtime loading
crates/renderer/   visibility, GPU scene and presentation
crates/telemetry/  logs, counters, traces and profiling
games/sample/      sample game and map using the public engine API
docs/              vision, architecture, performance, protocol, commands, ADRs
tests/integration/ process-level client/server/admin scenarios
~~~

This is the target ownership map, not a reason to create empty crates. Add a crate when it contains real code and enforces a useful dependency boundary. Keep the binaries thin.

## Dependency rules

1. Server depends on world simulation, protocol, transport, storage, commands and telemetry. It has no renderer, audio, or window dependency.
2. Client depends on protocol, transport, assets, renderer, commands and telemetry.
3. Simulation owns authoritative rules and does not call network, storage, GPU, or UI APIs. It emits explicit results and events.
4. Protocol is independent of transport and defines versions, message limits and validation.
5. Commands defines typed inputs, scope, permissions and results. Terminal, minimiumctl and client console are adapters.
6. Game-specific rules use a public engine API and data definitions; engine crates do not depend on the sample game.

## Server tick and ownership

The server uses a fixed simulation step. Network and admin input enter bounded queues and are applied at named tick boundaries. The simulation owns mutable world state. Each entity belongs to one zone. Parallel jobs read a stable phase snapshot and return results for a controlled merge. Network and console threads never mutate world state directly. Persistence work leaves the time-critical tick path.

Planned phases: drain and validate input; movement; collision; interactions and combat; AI and world events; per-client interest sets; state/event encoding; timing and queue metrics. Each phase has its own duration counter.

## Thread budget

The server accepts --threads 1, --threads 2, or --threads auto. The main loop is lane one. One lane executes independent work inline with nonblocking I/O; additional lanes are persistent workers for measurable independent tasks such as per-client visibility and snapshot preparation. Work queues are bounded and have explicit merge points. No busy-waiting. Report both configured lanes and actual process threads because dependencies may create helper threads.

The same scenario must reach the same authoritative outcome under every lane count. Add parallel work only after measuring its benefit and synchronization cost.

## World, simulation and replication

World data represents full 3D static geometry, dynamic entities, collision shapes and render data separately. Spatial cells support nearby queries, streaming and interest management. First, one server process hosts one zone; later, zone processes can be coordinated.

Replication separates frequent replaceable state from reliable events. Each client receives only relevant objects. The local player uses prediction and server reconciliation; remote objects are interpolated.

## Console and operations

Foreground server input and minimiumctl use the same server command registry. Detached operation has no stdin dependency. The local admin endpoint is not part of the public game protocol. Remote administration needs authentication and roles.

Initial server commands: help, status, players list, metrics show, world save, stop. Initial client commands: help, connect, disconnect, fps show, net stats, quit.

## Current implementation

The current workspace contains `minimium-core`, `minimium-protocol`, `minimium-server`, `minimium-client`, `minimiumctl`, and `minimium-loadtest`. The desktop client uses wgpu/winit to render the parsed first-person 3D map with WASD and mouse look. It can launch a distinct local server process or connect to a supplied address. Press F1 for the in-window command console; `--console` remains a separate text client prototype. Both client and server accept `--map PATH`; local client-launched servers receive the same path.

The server is headless, caps connections at 512, tracks players in 32-unit spatial cells, and prepares snapshots at 10 Hz. Snapshot jobs share immutable world indexes through reference-counted maps while the worker pool reads them. Each client receives up to 16 nearby player records within a 72-unit radius; the bounded list uses inline storage and encodes directly into the connection's reusable output buffer. Static solid geometry is indexed into spatial cells; a reusable query buffer limits each character step to nearby colliders, with oversized boxes kept in a fallback list. At 20 Hz, the server integrates normalized horizontal input, gravity, jump velocity and axis-aligned collision; small obstacles allow a 0.55-unit step up. The client predicts with the same shared controller and collision index, then reconciles position, velocity and grounded state from snapshots. The default map is a bundled versioned text asset; external map files are bounded and validated before the server binds its ports. Box records provide rendered geometry and optional solid colliders. Version 2 maps also render visual-only triangles; mesh collision, streaming and authored map tools remain future work.

`--threads 1|2|auto` selects execution lanes. One lane executes snapshot preparation inline; additional persistent workers handle batches of clients. `metrics show` reports tick and phase timing percentiles, missed ticks and snapshots, traffic, rejected inputs, dropped snapshots, and queue high-water marks. The server polls connected clients every 8 ms and backs off to a 50 ms poll while idle. `world save` updates records for active players, clones 256 shard handles and queues that immutable view through a bounded channel to a dedicated storage worker; record traversal, sorting, serialization and disk writes happen outside the simulation loop. Copy-on-write updates only the affected shard while a save is running. `status` reports pending saves and the last result. Controlled shutdown saves the final snapshot, writes a shutdown result, and waits for the storage worker; `minimiumctl stop` waits for that result and reports a final save error. Player records use fixed-size inline names to avoid per-player name allocations. The server loads saved records at startup but only adds a character to the live world after the matching client profile reconnects, so offline players do not appear as ghosts. The client profile ID is a durable local identifier, not account authentication; public-game authentication and encryption remain future work.

Game traffic uses protocol v4 over prototype TCP. A client identifies its local profile before receiving Welcome. The codec is independent of the transport, but authenticated accounts, encryption, congestion handling, and a production transport adapter remain future work. The 512 connection limit and load generator are scaffolding; no result yet demonstrates the Pentium 500 MHz / 500-player goal. Mesh assets, asset cooking, map authoring, durable inventory and world state, production security, audio, and multi-zone hosting remain future work.
