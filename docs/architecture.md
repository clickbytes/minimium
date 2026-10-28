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

The current server accepts `--threads 1..64|auto`; the client local-server option is narrower and accepts `1|2|auto`. The main loop is lane one. One lane executes independent work inline with nonblocking I/O; additional lanes are persistent workers for measurable independent tasks such as per-client visibility and snapshot preparation. Work queues are bounded and have explicit merge points. No busy-waiting. Report both configured lanes and actual process threads because dependencies may create helper threads.

The same scenario must reach the same authoritative outcome under every lane count. Add parallel work only after measuring its benefit and synchronization cost.

## World, simulation and replication

World data represents full 3D static geometry, dynamic entities, collision shapes and render data separately. Spatial cells support nearby queries, streaming and interest management. First, one server process hosts one zone; later, zone processes can be coordinated.

Replication separates frequent replaceable state from reliable events. Each client receives only relevant objects. The local player uses prediction and server reconciliation; remote objects are interpolated.

## Console and operations

Foreground server input and minimiumctl use the same server command registry. Detached operation has no stdin dependency. The local admin endpoint is not part of the public game protocol. Remote administration needs authentication and roles.

Initial server commands: help, status, players list, metrics show, world save, stop. Initial client commands: help, connect, disconnect, fps show, net stats, quit.

## Current implementation

The following describes implemented paths, not executed gameplay or performance verification. Tests and measurements remain a separate final phase.

The current workspace contains the packages `minimium-core`, `minimium-platform`, `minimium-protocol`, `minimium-telemetry`, `minimium-server`, `minimium-client`, `minimiumctl`, and `minimium-loadtest`; the broader crate map above is a target ownership map, not the current repository layout. Platform owns profile file creation, atomic file replacement, server locks, endpoint and admin client, and the shared server command schema. Core still contains map/OBJ file loading and an executor tied to telemetry; their removal is scheduled separately. The desktop client uses wgpu/winit to render the parsed first-person 3D map with WASD and mouse look. Its renderer is split across Rust modules for window lifecycle, GPU setup, drawing, geometry, camera/input, command handling, and session events. `renderer/world.wgsl` supplies the world pipeline and `renderer/ui.wgsl` supplies the console/HUD pipeline; both are embedded at compile time. It can launch a distinct local server process or connect to a supplied address. Press F1 for the in-window command console; `--console` remains a separate text client prototype. Both client and server accept `--map PATH`; local client-launched servers receive the same path. The server supports a configured `--game-port`; zero asks the OS for an ephemeral port.

Connection establishment and an established session have separate owners. `Connector` runs DNS resolution, sequential address attempts, and local server bootstrap on one worker thread. Its timeout is a common result deadline; cancellation or timeout discards a late result, but cannot interrupt a blocking resolver or startup call, so the connector remains busy until its worker exits. Once a socket is returned, `ClientSession` owns a nonblocking TCP stream and bounded read/write buffers but no thread. The desktop redraw loop and text-client loop poll it, handle typed events, and retain their own lifecycle state. The session sends Identify first, accepts ordinary client messages only after Welcome and map-fingerprint validation, and maintains heartbeat timers and RTT state.

The desktop and text front ends share `client_commands::COMMAND_CATALOG` for command syntax, help, aliases, and completion. It currently accepts `help`, `host local [--threads 1|2|auto]`, `connect HOST:PORT`, `disconnect`, `fps show`, `net stats`, `ping`, `move X Z`, `jump`, `fire`, `say TEXT` (alias `chat`), `name NAME`, `state`, `clear`, and `quit` (alias `exit`). Front ends handle UI or process actions; commands that send game traffic queue typed protocol messages on the active session. Server commands are parsed by `ServerCommand::parse` and have the same six entries in terminal and admin dispatch: `help`, `status`, `players list`, `metrics show`, `world save`, and `stop`.

The server is headless, caps active players at a configurable maximum (512 by default) and pre-authentication handshakes at 64, tracks players in 32-unit spatial cells, and prepares snapshots at 10 Hz. `--max-players` sets the active cap from 1 to 512 and sizes per-player tables accordingly. Total sockets are bounded by that cap plus the 64 pending handshakes. The server accepts at most 64 new sockets per poll and requires each new connection to identify within five seconds. Snapshot jobs share immutable world indexes through reference-counted maps while the worker pool reads them. Each client receives the 16 nearest player records within a 72-unit radius; a bounded stack buffer selects them without per-snapshot allocation. The initial message is a full snapshot; later messages contain only changed player records and explicit removals. Per-client replication baselines advance only after a snapshot is accepted into that connection's bounded output queue. Connected interactive clients send a Ping every 10 seconds and the server closes clients that send no valid game message for 30 seconds; the desktop client also drops a connection if its Pong is not received within 20 seconds. Players have 100 health; a rate-limited 60-unit hitscan weapon applies damage against the nearest player unobstructed by indexed solid boxes or triangles, and lethal hits respawn the target at the map spawn. Static solid geometry is indexed into spatial cells; reusable movement and ray queries limit work to nearby collision candidates, with oversized geometry kept in a fallback list. At 20 Hz, the server integrates normalized horizontal input, gravity, jump velocity, axis-aligned box collision and triangle collision; walkable triangles support floors, ceilings and shallow ramps, while steeper triangles block the player hitbox. The client predicts with the same shared controller and collision index, reconciles its own position from snapshots, and renders remote players through a 100 ms interpolation buffer driven by server time. Remote-player geometry is uploaded only when its rendered positions change. The default map is a bundled versioned text asset; external map files are bounded and validated before the server binds its ports. Box and triangle records provide rendered geometry and optional solid colliders. Streaming and authored map tools remain future work.

`--threads 1..64|auto` selects execution lanes. One lane executes work inline; with at least 32 active players, additional persistent workers simulate independent player movement batches against a shared immutable collision index. Position changes merge back in stable batch order, preserving the single-lane cell-index update order. Snapshot preparation also uses the worker pool. `metrics show` reports tick and phase timing percentiles, collision-query cells and candidates, fallback scans, missed ticks and snapshots, traffic, rejected inputs, dropped snapshots, queue high-water marks, sampled current and peak process memory, and RSS per active player. The server polls connected clients every 8 ms and backs off to a 50 ms poll while idle. Game reads are capped at 1 KiB and 32 decoded messages per client per poll. Game writes are capped at 16 KiB per client and 256 KiB total per poll; the starting client index advances by one each poll to rotate which client gets first access to the shared budget. `world save` updates records for active players, including name, position, and health, clones 256 shard handles and queues that immutable view through a bounded channel to a dedicated storage worker; record traversal, sorting, serialization and disk writes happen outside the simulation loop. Autosaves use the same worker every 300 seconds by default and can be configured with `--autosave-seconds` (zero disables periodic saves). Copy-on-write updates only the affected shard while a save is running. `status` reports pending saves and the last result. Controlled shutdown saves the final snapshot, writes a shutdown result, and waits for the storage worker; `minimiumctl stop` waits for that result and reports a final save error. Player records use fixed-size inline names to avoid per-player name allocations. The server loads saved records at startup but only adds a character to the live world after the matching client profile reconnects, so offline players do not appear as ghosts. The client profile ID is a durable local identifier, not account authentication; public-game authentication and encryption remain future work.

Game traffic uses protocol v8 over prototype TCP. A client identifies its local profile before receiving Welcome. Snapshot messages include server monotonic time and bounded player deltas; Fire requests use server-side spatial hitscan and cooldown validation. Bounded UTF-8 chat is broadcast through each client's existing capped output queue and rate-limited per sender. The desktop HUD shows health and briefly marks confirmed hits using the existing UI atlas and vertex buffer. The codec is independent of the transport, but authenticated accounts, encryption, congestion handling, and a production transport adapter remain future work. The 512 connection limit and load generator are scaffolding; no result yet demonstrates the Pentium 500 MHz / 500-player goal. Static Wavefront OBJ geometry can be loaded by map files and is expanded into triangles at startup. Textures, cooked/batched runtime assets, map authoring, durable inventory and world state, production security, audio, and multi-zone hosting remain future work.

## Current module contracts

| Boundary | Input and output | Ownership / integration |
| --- | --- | --- |
| WorldMap::parse / load_file | Bounded text or file to validated map, or an error | Both client and server use the same canonical fingerprint; maps and OBJ source are the current data extension surface. |
| World::step_players_parallel | Player input map, fixed delta, Executor to collision query counters and updated world | Main lane owns mutations; jobs read immutable collision geometry and return results merged in input order. |
| World::network_snapshot / visit_nearby | Immutable snapshot and recipient to bounded nearest player records | No socket/GPU dependency; server builds full/delta output only for recipients. |
| protocol codecs | Typed messages to byte buffers; byte slice to complete decoded frame + consumed length, incomplete, or error | v8 framing/limits are independent of TCP. Transport adapters own partial buffers. |
| ServerCommand::parse / server commands::execute | Text to six typed commands; context to success/text/stop reply | Terminal and authenticated IPC enqueue; one dispatcher executes up to 8 commands before simulation. |
| admin_client::request / stop_and_wait | Captured endpoint + typed-command text to payload/error; data directory + endpoint to final stop result | 2s connect, 3s response, 64 KiB reply cap; stop waits up to 120s; no game-protocol authority. |
| Connector / ClientSession | Connection request to worker result; connected socket to bounded nonblocking protocol events | Connector worker does resolver/connect/bootstrap work; host loop polls session and owns UI events. |
| client_commands::COMMAND_CATALOG | Canonical command specifications to parser, help, aliases, and completion | Shared by desktop and text client front ends. |
| renderer WGSL | Embedded `world.wgsl` and `ui.wgsl` source to world and UI pipelines | Compile-time assets included by GPU setup; no runtime shader loader. |
| Executor::map | Owned items + immutable operation to ordered results | Inline one lane, bounded worker queue for other lanes; phase context inherited by jobs. |
| PlayerStore / PersistenceWorker | Validated player records to immutable snapshot view and asynchronous result | Storage owns serialization/disk I/O; final shutdown joins storage before releasing the data lock. |
| telemetry | Durations and process samples to fixed histograms/optional fields | Independent of core/world/protocol/UI. Allocation wrapper is optional, thread-local by phase. |

ServerDataLock is acquired before loading player data or changing endpoint/shutdown files. EndpointGuard removes only the current instance endpoint. LocalServer distinguishes an attached server from an owned child; only the latter is stopped/reaped. The child receives a private per-start nonce through MINIMIUM_ADMIN_TOKEN and publishes it as the existing admin token, proving ownership without adding a game message. server.shutdown keeps its legacy body; the private server.shutdown.owner sidecar binds the result to that instance. Neither token is logged or included in load reports. Client options separate profile identity from local server storage; `ClientSession` becomes Connected only after Welcome and map-fingerprint validation. The profile ID remains a public reconnect identifier, not authenticated account identity.

The current extension API is Rust source plus map data, not a dynamic plugin ABI. Add maps using the documented format and keep matching copies on clients/server. New simulation rules belong behind core world APIs, never in the renderer or transport. New protocol messages require a separately planned version/compatibility decision; this stage changes neither protocol v8 nor save v3. The sample combat/controller rules are still implemented in core; extracting a general game-rule plugin layer is future work.
