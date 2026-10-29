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

## Target future layout

~~~text
apps/client/       client executable and platform shell
apps/server/       headless server executable and runtime shell
apps/ctl/          admin CLI
apps/loadtest/     simulated network clients and scenarios
apps/tools/        asset cooking and map validation
crates/core/       IDs, time, config, math primitives and errors
crates/procgen/    pure, seeded, versioned world generation
crates/sectors/    bounded sector tickets, readiness and cache lifecycle
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

## Target future dependency rules

1. Server depends on world simulation, protocol, transport, storage, commands and telemetry. It has no renderer, audio, or window dependency.
2. Client depends on protocol, transport, assets, renderer, commands and telemetry.
3. Simulation owns authoritative rules and does not call network, storage, GPU, or UI APIs. It emits explicit results and events.
4. Protocol is independent of transport and defines versions, message limits and validation.
5. Commands defines typed inputs, scope, permissions and results. Terminal, minimiumctl and client console are adapters.
6. Game-specific rules use a public engine API and data definitions; engine crates do not depend on the sample game.
7. Procgen depends only on core domain types. It uses addressed integer randomness and does not perform I/O, networking, parallel dispatch, or GPU work.

## Server tick and ownership

The server uses a fixed simulation step. Network and admin input enter bounded queues and are applied at named tick boundaries. The simulation owns mutable world state. Each entity belongs to one zone. Parallel jobs read a stable phase snapshot and return results for a controlled merge. Network and console threads never mutate world state directly. Persistence work leaves the time-critical tick path.

`ServerRuntime` owns `GameState`, `ConnectionState`, `OperationsState`, and `Scheduler`. Each iteration polls persistence completions and autosave, samples memory, polls admin/terminal, then accepts, receives, dispatches, handles EOF/timeouts, sends and captures disconnects. A due tick executes commands, applies inputs, expires stale movement and simulates; a due snapshot follows, then the scheduler sleeps. A stop command still completes the current tick and snapshot check. Admin or accept errors enter the same controlled shutdown path, which reports and attempts the final save while the endpoint guard and data lock remain held. Runtime does not claim a tested failure recovery guarantee.

Admission checks session identity, queue reservation and cooldowns without mutating the world. The single ordered `WorldInput` queue is applied on the authoritative lane; each resulting shot or chat event is delivered before the next input. Leave remains queued after prior accepted inputs, and its pending profile blocks reconnect until application. Output lookup checks that the indexed connection still has the intended player identity. Movement does not allocate an event.

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

The implemented Cargo graph includes `minimium-procgen` and `minimium-sectors`. Core has no dependencies; procgen and sectors depend only on core; assets depends on core and procgen to validate authored dungeon blueprints; platform and protocol depend on core; telemetry is independent. Server and client consume world and sector contracts. Storage remains a server module because its persistence worker and lifecycle are specific to the server. Run `python scripts/check_architecture.py` for selected Cargo graph restrictions and `python scripts/check_file_lengths.py` for source length. The former follows workspace dependencies from Cargo metadata, but does not inspect transitive dependencies of external crates or prove the absence of I/O.

The workspace contains `minimium-core`, `minimium-procgen`, `minimium-sectors`, `minimium-assets`, `minimium-platform`, `minimium-protocol`, `minimium-telemetry`, `minimium-server`, `minimium-client`, `minimiumctl`, and `minimium-loadtest`. Platform owns bounded persistent-file reads, atomic writes/replacement, profile files, server locks and endpoint/admin helpers; server storage owns snapshot parsing and validation. Assets parses maps, OBJ and versioned manual dungeon definitions. The desktop client uses wgpu/winit and can launch a local server or connect remotely. Press F1 for its command console; `--console` selects the text client. Client and server accept `--map PATH` or `--world-definition PATH`; the local client passes the same path to its child server. The default client/server generate matching demo worlds. The server's `--game-port 0` asks the OS for an ephemeral port.

Connection establishment and an established session have separate owners. `Connector` runs DNS resolution, sequential address attempts, and local server bootstrap on one worker thread. Its timeout is a common result deadline; cancellation or timeout discards a late result, but cannot interrupt a blocking resolver or startup call, so the connector remains busy until its worker exits. Once a socket is returned, `ClientSession` owns a nonblocking TCP stream and bounded read/write buffers but no thread. The desktop redraw loop and text-client loop poll it, handle typed events, and retain their own lifecycle state. The session sends Identify first, validates Welcome map fingerprint and generator digest, then wraps gameplay inputs in the active v9 world context.

The desktop and text front ends share `client_commands::COMMAND_CATALOG` for command syntax, help, aliases, and completion. It currently accepts `help`, `host local [--threads 1|2|auto]`, `connect HOST:PORT`, `disconnect`, `fps show`, `net stats`, `ping`, `move X Z`, `jump`, `fire`, `say TEXT` (alias `chat`), `name NAME`, `state`, `clear`, and `quit` (alias `exit`). Front ends handle UI or process actions; commands that send game traffic queue typed protocol messages on the active session. Server commands are parsed by `ServerCommand::parse` and have the same six entries in terminal and admin dispatch: `help`, `status`, `players list`, `metrics show`, `world save`, and `stop`.

The server is headless, caps players at 512 by default and pending handshakes at 64, tracks players in 32-unit spatial cells and prepares snapshots at 10 Hz. Protocol v9 snapshots and gameplay inputs carry world context; per-instance simulations route events and discard stale membership epochs. The demo's planet uses radial up and tangent movement, while dungeon movement uses fixed Y-up. Server and client share bounded ticketed sector caches and topology-specific neighborhood policies; installed slices provide movement collision and active renderer buffers. Server movement pauses until its full neighborhood is ready, while client prediction waits for its local collision neighborhood. A full immutable map index remains on the server for hitscan blockers; source maps stay in client CPU memory for deterministic slicing. Client/server transfers use prepare/ready/commit and acknowledge only after target data, collision and GPU buffers are ready, then switch scenes without rebuilding the window or TCP session. The default world is generated from fixed versioned seeds; external map and authored world-definition files are validated before binding.

`--threads 1..64|auto` selects execution lanes. One lane executes work inline; with at least 32 active players, additional workers simulate independent movement batches against shared immutable geometry. `world save`, autosave and shutdown queue player snapshots through the persistence worker. Save v6 records every loaded world's manifest and map fingerprint, player locations and return points, and revisioned logical object-state sets per sector. v2/v3 data is associated only with a loaded map matching its legacy fingerprint; v4/v5 snapshots remain readable under their world-pinning rules. The demo currently has no interaction that changes sector objects; mesh geometry edits are not implemented. The client profile ID is a durable local identifier, not account authentication; public-game authentication and encryption remain future work.

Game traffic uses protocol v9 over prototype TCP. A client identifies its local profile before receiving Welcome. Snapshot messages include server monotonic time, world context and bounded player deltas; Fire requests use server-side spatial hitscan and cooldown validation. Portal transfer carries transition IDs and target manifests. Bounded UTF-8 chat uses capped output queues. Authenticated accounts, encryption, congestion handling and a production transport remain future work. The 512 connection limit and load generator are scaffolding; no result yet demonstrates the Pentium 500 MHz / 500-player goal. Textures, sector geometry edits, production security, audio and multi-zone hosting remain future work.

## Current module contracts

| Boundary | Input and output | Ownership / integration |
| --- | --- | --- |
| assets::parse_map / load_map | Bounded text or file to validated map, or an error | Both client and server use the same canonical fingerprint; maps and OBJ source are the current data extension surface. |
| World::step_players_with | Player input map, fixed delta and dispatch function to collision query counters and updated world | Main lane owns mutations; jobs read immutable collision geometry and return results merged in input order. |
| World::network_snapshot / visit_nearby | Immutable snapshot and recipient to bounded nearest player records | No socket/GPU dependency; server builds full/delta output only for recipients. |
| protocol codecs | Typed messages to byte buffers; byte slice to complete decoded frame + consumed length, incomplete, or error | v9 framing, world context and transition messages are independent of TCP. Transport adapters own partial buffers. |
| ServerCommand::parse / server commands::execute | Text to six typed commands; context to success/text/stop reply | Terminal and authenticated IPC enqueue; one dispatcher executes up to 8 commands before simulation. |
| admin_client::request / stop_and_wait | Captured endpoint + typed-command text to payload/error; data directory + endpoint to final stop result | 2s connect, 3s response, 64 KiB reply cap; stop waits up to 120s; no game-protocol authority. |
| Connector / ClientSession | Connection request to worker result; connected socket to bounded nonblocking protocol events | Connector worker does resolver/connect/bootstrap work; host loop polls session and owns UI events. |
| client_commands::COMMAND_CATALOG | Canonical command specifications to parser, help, aliases, and completion | Shared by desktop and text client front ends. |
| renderer WGSL | Embedded `world.wgsl` and `ui.wgsl` source to world and UI pipelines | Compile-time assets included by GPU setup; no runtime shader loader. |
| Executor::map | Owned items + immutable operation to ordered results | Inline one lane, bounded worker queue for other lanes; phase context inherited by jobs. |
| PlayerStore / PersistenceWorker | Validated player records to immutable snapshot view and asynchronous result | Storage owns serialization/disk I/O; final shutdown joins storage before releasing the data lock. |
| telemetry | Durations and process samples to fixed histograms/optional fields | Independent of core/world/protocol/UI. Allocation wrapper is optional, thread-local by phase. |

ServerDataLock is acquired before loading player data or changing endpoint/shutdown files. EndpointGuard removes only the current instance endpoint. LocalServer distinguishes an attached server from an owned child; only the latter is stopped/reaped. The child receives a private per-start nonce through MINIMIUM_ADMIN_TOKEN and publishes it as the existing admin token, proving ownership without adding a game message. server.shutdown keeps its legacy body; the private server.shutdown.owner sidecar binds the result to that instance. Neither token is logged or included in load reports. Client options separate profile identity from local server storage; `ClientSession` becomes Connected only after Welcome and map-fingerprint validation. The profile ID remains a public reconnect identifier, not authenticated account identity.

Both interactive frontends use `ConnectionController`, which owns the connector, session and local-server guard. DNS/connect/bootstrap work runs in the connector worker; a cancelled or late local result returns to that worker for disposal. A replaced owned guard is cleaned up by a tracked worker, and a new attempt waits for that cleanup. Session polling remains on the frontend loop. Explicit remote connections never create a local guard; an attached guard does not stop a server.

The desktop renderer composes `GpuResources`, `GameplayView`, `InputState`, `UiState`, `FrameMetrics`, client settings and the connection controller. GPU construction, resize and drawing operate on GPU resources; input direction and gameplay prediction/snapshot interpolation operate on their own state. The renderer coordinates these owners and forwards typed session events. Remote geometry and console buffers are reused; rendering and interaction behavior still require the deferred visual and gameplay scenarios.

Startup loads the map, acquires the data lock, reads the player snapshot, starts the persistence worker, binds listeners and publishes the endpoint. Shutdown drains admin replies, closes clients, captures a final immutable snapshot, joins the worker, removes the owned endpoint and writes the owner-bound result before releasing the lock. A runtime error remains primary if final save or cleanup also fails; secondary errors are reported. This describes ordering, not a verified crash recovery guarantee.

The current extension API is Rust source, map data and validated world definitions, not a dynamic plugin ABI. Keep matching map/world-definition files on clients and server. New simulation rules belong behind core world APIs, never in the renderer or transport. New protocol messages require a separately planned version decision; the world-model migration uses protocol v9 and save v6. The sample combat/controller rules are still implemented in core; extracting a general game-rule plugin layer is future work. Workspace tests cover procedural generation, sector lifecycle, movement, isolated instances, transfer, save migration and profile restoration. A full rendered-client transfer and restart/reconnect demonstration still need manual review.
