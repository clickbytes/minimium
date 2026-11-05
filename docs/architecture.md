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

## Current ownership and target boundaries

`architecture.toml` is the machine-readable declaration of current Cargo package responsibilities and selected forbidden dependencies. The check in `scripts/check_architecture.py` reads this file and evaluates the full resolved Cargo production graph. Planned boundaries below are architectural destinations, not claims that those packages exist today.

Current packages and responsibilities:

| Package | Current responsibility |
| --- | --- |
| `minimium-core` | Foundational identity and identifier primitives. |
| `minimium-world` | World definitions, topology, addressing, geometry, maps, entities and neutral component data. |
| `minimium-simulation` | Mutable world state, movement, collision queries, combat, snapshots and fixed-step results. |
| `minimium-environment` | World clock and environment values, renderer-independent sky and solar-system contracts, and deterministic weather model. |
| `minimium-procgen` | Seeded world generation, sector geometry and terrain sampling through explicit prepared geography inputs. |
| `minimium-sectors` | Bounded ticket, readiness and cache lifecycle. |
| `minimium-assets` | Map, OBJ, sky catalog and world-definition parsing from supplied data. |
| `minimium-asset-io` | Bounded authored map, mesh and world-definition file access adapter. |
| `minimium-platform` | Durable file operations, identity, locks and local administration transport. |
| `minimium-storage` | Save snapshot formats, validation, player store and bounded persistence worker. |
| `minimium-transport` | Bounded socket byte flushing without protocol or game policy. |
| `minimium-commands` | Generic command registry; domains own command values and validation, while hosts own execution. |
| `minimium-protocol` | Versioned message models, codecs and validation. |
| `minimium-astronomy` | Client astronomy calculations and FFI adapter consuming environment/world contracts. |
| `minimium-telemetry` | Metrics and process/allocation instrumentation. |
| `minimium-server` | Server process entry point, allocator selection and argument forwarding. |
| `minimium-server-runtime` | Server runtime, transport coordination, persistence lifecycle and authoritative simulation composition. |
| `minimium-client-runtime` | UI-independent session, connection controller, connector, local-server guard, fixed-step movement prediction, snapshot reconciliation/interpolation, transition validation and environment synchronization state. |
| `minimium-renderer` | Presentation frame contract including CPU-only `RenderFrame`, camera matrix preparation, render/UI vertex layouts/overlay pipelines/geometry conversion, surface-frame acquire/present operations, ordered shadow/scene/atmosphere/cloud/UI/output pass encoding, globe overview texture/pipeline/LOD mesh, overview and world geometry GPU owners, UI/shadow GPU owners, planet visibility/LOD, visual quality and lighting, WGSL source composition, shared GPU frame bindings, stars, Milky Way and lunar pipelines; frame scheduling, visibility/caster selection and remaining GPU resources stay in the client. |
| `minimium-client` | Client application shell, renderer integration, console and command presentation. |
| `minimiumctl` | Administrative CLI. |
| `minimium-loadtest` | Headless scenario client. |
| `minimium-game-sample` | Demo map, stable seed, planet presets, sample planet/dungeon composition, demo celestial catalogue and sample combat balance; depends on reusable procgen, world, simulation, environment and platform APIs. |

The world, simulation, command and environment packages own public contracts and implementations. `minimium-renderer` owns camera-frame preparation, planet visibility/LOD, visual lighting and WGSL composition, and encodes the main scene, shadow depth, cloud, atmosphere, UI and output passes. It also owns surface image acquisition and queue submission/presentation operations; the client schedules frames, selects visible sectors/casters and composes renderer resource owners at the application boundary. `ClientView` is adapted into the CPU-only `RenderFrame` used for that frame. Server authority and client prediction share the `minimium-simulation` movement integrator. The deterministic weather model lives in environment and receives terrain and lake sampling through a host adapter. Dungeon layout contracts live in world; the asset parser validates structure and the procedural host validates the generator digest before materializing it. `minimium-assets` consumes data and a host-provided mesh reader; `minimium-asset-io` owns bounded map/mesh/world-definition reads and prepared GEBCO packages. Persistence snapshots and the bounded save worker live in `minimium-storage`; bounded socket writes live in `minimium-transport`. `minimium-server-runtime` coordinates server authority, while `minimium-client-runtime` owns session state, connection lifecycle, local server guards, fixed-step prediction, snapshot reconciliation/interpolation and clock synchronization.

## Target future layout

~~~text
apps/client/       client executable and platform shell
apps/server/       headless server executable
apps/ctl/          admin CLI
apps/loadtest/     simulated network clients and scenarios
tools/geography/  offline GEBCO package preparation tools
crates/core/       shared IDs and small platform-neutral primitives
crates/procgen/    pure, seeded, versioned world generation
crates/sectors/    bounded sector tickets, readiness and cache lifecycle
crates/commands/   generic command registry and argument metadata
crates/protocol/   versioned messages and validation limits
crates/transport/  network transport adapters
crates/world/      manifests, topology, addressing, static geometry and entity contracts
crates/simulation/ fixed-step movement, collision, combat, AI and events
crates/storage/    snapshots and durable state operations
crates/assets/     map, model, catalogue and world-definition formats
crates/asset-io/   opened packages, geography and runtime asset access
crates/server-runtime/ authoritative server runtime
crates/client-runtime/ session and clock synchronization runtime
crates/renderer/   camera frames, visibility/LOD, visual lighting, WGSL sources and GPU contracts
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
7. Procgen depends on world contracts and explicitly supplied asset data. It uses addressed integer randomness and does not perform I/O, networking, parallel dispatch, or GPU work.

## Server tick and ownership

The server uses a fixed simulation step. Network and admin input enter bounded queues and are applied at named tick boundaries. The simulation owns mutable world state. Each entity belongs to one zone. Parallel jobs read a stable phase snapshot and return results for a controlled merge. Network and console threads never mutate world state directly. Persistence work leaves the time-critical tick path.

### Modular engine capabilities

`minimium-server-runtime::modules` provides explicit startup registration through `EngineModule` and `ModuleRegistry`. A module declares an ID, version and dependencies, then registers typed commands, systems and component descriptors. This is a static Rust API: it does not load dynamic libraries, define a stable plugin ABI, or add network message kinds. The server currently registers its built-in `world` module. Its `world list`, `world inspect` and `entity list` commands are server-admin commands; its startup system records registry diagnostics. Systems run at named stages, while authoritative world mutation remains on the server lane.

`minimium-world::entity` exposes `EntityStore`; component entities are scoped to a `WorldInstanceId`, and the opt-in `MEC1` envelope stores entity ID, component type, schema and payload inside existing opaque world-object data. Legacy objects remain owned by their serializers. Replication policy is explicit and defaults to `ServerOnly`; the server currently offers bounded, read-only changed-component staging and metrics, but does not transmit these components. `minimium-environment::universe` owns the bounded solar-system definition graph with fixed or simplified circular ephemerides, not an N-body solver.

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

The implemented Cargo graph includes `minimium-renderer`, `minimium-server-runtime`, `minimium-client-runtime`, `minimium-world`, `minimium-simulation`, `minimium-environment`, `minimium-commands`, `minimium-procgen`, `minimium-sectors`, `minimium-assets`, `minimium-asset-io`, `minimium-storage`, `minimium-transport`, the client-only astronomy backend and `minimium-game-sample`. Core has no dependencies; world depends on core; simulation depends on core/world; environment depends on core/world and the generic command registry; commands has no project dependencies; procgen and sectors consume core/world contracts; assets parses supplied data and is independent of procgen and filesystem adapters; asset-io owns bounded reads for authored assets; procgen remains independent of assets and sample content. The sample-game package composes the demo scenario using procgen and world APIs. Platform and protocol consume public contracts; telemetry is independent. Server and client compose these domains and sample content. Storage owns snapshots and the persistence worker, while transport currently supplies bounded socket write progress and server retains connection policy. Run `rtk python scripts/check_architecture.py` for Cargo graph restrictions and `rtk python scripts/check_file_lengths.py` for source length. The architecture checker follows the full resolved Cargo graph with all features and targets, including external transitive normal/build edges and excluding dev edges. It prints the dependency path for each violation. Core, protocol, server and ctl cannot reach astronomy, wgpu, wgpu-core, wgpu-hal or winit. This graph check does not prove the absence of I/O.

The workspace contains `minimium-core`, `minimium-renderer`, `minimium-server-runtime`, `minimium-client-runtime`, `minimium-world`, `minimium-simulation`, `minimium-environment`, `minimium-commands`, `minimium-astronomy`, `minimium-procgen`, `minimium-sectors`, `minimium-assets`, `minimium-asset-io`, `minimium-platform`, `minimium-protocol`, `minimium-storage`, `minimium-transport`, `minimium-telemetry`, `minimium-server`, `minimium-client`, `minimium-game-sample`, `minimiumctl`, and `minimium-loadtest`. Platform owns bounded persistent-file reads, atomic writes/replacement, profile files, server locks and endpoint/admin helpers; storage owns snapshot parsing and validation. Assets parses maps, OBJ and versioned manual dungeon definitions; asset-io opens bounded map, mesh and world-definition files. The desktop client uses wgpu/winit and can launch a local server or connect remotely. Press F1 for its command console; `--console` selects the text client. Client and server accept `--map PATH` or `--world-definition PATH`; the local client passes the same path to its child server. The default client/server generate matching demo worlds. The server's `--game-port 0` asks the OS for an ephemeral port.

`minimium-environment::time` owns validated UTC, scale, clock anchor/sample and saved-clock types, plus pure clock evaluation, mutation and restore operations. The environment crate root owns profile and versioned astronomy/catalogue identifiers. `minimium-environment::sky` owns observer and sky-model contracts, `universe` owns the solar-system catalogue, and `weather` owns deterministic weather/cloud models behind a `TerrainSampler` host port. These contracts use platform-neutral values; the server supplies UTC and monotonic samples, persistence owns serialization, protocol owns wire encoding, and the client owns astronomy and rendering. Clock state is independent of `WorldManifest`, generator inputs and geometry fingerprints.

The server runtime adapts one `EnvironmentClock` to the same `server_started` monotonic epoch used by uptime snapshots. It creates a separate OS-random 16-byte session nonce, checks UTC every five seconds in RealTime mode, and reanchors on corrections of at least 250 ms. Time mutations share the authenticated admin/terminal command path and never scale physics, cooldowns or network deadlines. The bounded `--environment-config` initializes a new world only. Save v9 persists clock state in the same atomic player/sector snapshot; protocol v13 carries samples and environment descriptors.

Connection establishment and an established session have separate owners. `Connector` runs DNS resolution, sequential address attempts, and local server bootstrap on one worker thread. Its timeout is a common result deadline; cancellation or timeout discards a late result, but cannot interrupt a blocking resolver or startup call, so the connector remains busy until its worker exits. Once a socket is returned, `ClientSession` owns a nonblocking TCP stream, bounded read/write buffers and the per-session clock estimator, but no thread. The desktop redraw loop and text-client loop poll it and handle typed events. The estimator uses one local monotonic epoch, a bounded offset filter and session-local nonce; it publishes the active date/descriptor/context as one immutable `EnvironmentFrameInput`. Portal commits switch descriptors without resetting the shared clock, while reconnect discards every estimator state. The text client prints concise quality/date/rate/revision status on changes. Explicit `--demo` mode has a separate monotonic RealTime adapter and is not a network fallback.

The desktop and text front ends share the startup `client_commands` registry (`command_specs`, `help_lines`) for syntax, help, aliases and completion. Interface, connection, gameplay and environment modules register their commands once. It currently accepts `help`, `host local [--threads 1|2|auto]`, `connect HOST:PORT`, `disconnect`, `fps show`, `net stats`, `time status`, `sky info`, `sky quality low|standard`, `ping`, `move X Z`, `jump`, `portal`, `map_telepo`, `fire`, `say TEXT` (alias `chat`), `name NAME`, `state`, `clear`, and `quit` (alias `exit`). Front ends handle UI or process actions; commands that send game traffic queue typed protocol messages on the active session. Server commands are parsed by `ServerCommand::parse` and share terminal/admin dispatch for `help`, `status`, `players list`, `metrics show`, `world save`, `stop`, and the parameterized `time status|mode|set|scale|offline` family. Client time diagnostics are read-only; clock mutation stays in the administrative channel.

The server is headless, caps players at 512 by default and pending handshakes at 64, tracks players in 32-unit spatial cells and prepares snapshots at 10 Hz. Protocol v13 snapshots and gameplay inputs carry world context; per-instance simulations route events and discard stale membership epochs. The demo's planet uses radial up and tangent movement, while dungeon movement uses fixed Y-up. Server and client share bounded ticketed sector caches and topology-specific neighborhood policies; installed slices provide movement collision and active renderer buffers. Server movement pauses until its full neighborhood is ready, while client prediction waits for its local collision neighborhood. A full immutable map index remains on the server for hitscan blockers; source maps stay in client CPU memory for deterministic slicing. Client/server transfers use prepare/ready/commit and acknowledge only after target data, collision and GPU buffers are ready, then switch scenes without rebuilding the window or TCP session. A new data directory samples and persists one random procedural seed; external map and authored world-definition files are validated before binding.

`--threads 1..64|auto` selects execution lanes. One lane executes work inline; with at least 32 active players, additional workers simulate independent movement batches against shared immutable geometry. `world save`, autosave and shutdown capture player, sector and clock state on the owner thread, then pass one immutable snapshot to the persistence worker. Save v9 records every loaded world's manifest and map fingerprint, player locations and return points, revisioned logical object-state sets per sector, and a single `CLOCK` record. v2/v3 data is associated only with a loaded map matching its legacy fingerprint; v2–v7 snapshots retain their world-pinning rules and initialize a RealTime clock at current UTC. The demo currently has no interaction that changes sector objects; mesh geometry edits are not implemented. The client profile ID is a durable local identifier, not account authentication; public-game authentication and encryption remain future work.

Game traffic uses protocol v13 over prototype TCP. A client identifies its local profile before receiving Welcome. Snapshot messages include server monotonic time, world context and bounded player deltas; Fire requests use server-side spatial hitscan and cooldown validation. Portal transfer carries transition IDs and target manifests. Bounded UTF-8 chat uses capped output queues. Authenticated accounts, encryption, congestion handling and a production transport remain future work. The 512 connection limit and load generator are scaffolding; no result yet demonstrates the Pentium 500 MHz / 500-player goal. Textures, sector geometry edits, production security, audio and multi-zone hosting remain future work.

## Current module contracts

| Boundary | Input and output | Ownership / integration |
| --- | --- | --- |
| assets::parse_map / load_map | Bounded text or file to validated map, or an error | Both client and server use the same canonical fingerprint; maps and OBJ source are the current data extension surface. |
| World::step_players_with | Player input map, fixed delta and dispatch function to collision query counters and updated world | Main lane owns mutations; jobs read immutable collision geometry and return results merged in input order. |
| World::network_snapshot / visit_nearby | Immutable snapshot and recipient to bounded nearest player records | No socket/GPU dependency; server builds full/delta output only for recipients. |
| protocol codecs | Typed messages to byte buffers; byte slice to complete decoded frame + consumed length, incomplete, or error | v13 framing, world context and transition messages are independent of TCP. Transport adapters own partial buffers. |
| ServerCommand::parse / server commands::execute | Text to typed base/time commands; context to success/text/stop reply | Terminal and authenticated IPC enqueue; one dispatcher executes up to 8 commands before simulation. |
| admin_client::request / stop_and_wait | Captured endpoint + typed-command text to payload/error; data directory + endpoint to final stop result | 2s connect, 3s response, 64 KiB reply cap; stop waits up to 120s; no game-protocol authority. |
| Connector / ClientSession | Connection request to worker result; connected socket to bounded nonblocking protocol events | Connector worker does resolver/connect/bootstrap work; host loop polls session and owns UI events. |
| minimium-commands::CommandRegistry | Typed specs to longest-name/alias parsing and usage metadata | Client command groups and server admin syntax share the parser; each host owns execution. |
| renderer WGSL | Shared `frame.wgsl` plus world/globe fragments, a linear-output encode shader when needed, and separate `ui.wgsl` | Small compile-time sources are composed once per pipeline; no runtime shader loader. |
| Executor::map | Owned items + immutable operation to ordered results | Inline one lane, bounded worker queue for other lanes; phase context inherited by jobs. |
| PlayerStore / PersistenceWorker | Validated player records to immutable snapshot view and asynchronous result | Storage owns serialization/disk I/O; final shutdown joins storage before releasing the data lock. |
| telemetry | Durations and process samples to fixed histograms/optional fields | Independent of core/world/protocol/UI. Allocation wrapper is optional, thread-local by phase. |

ServerDataLock is acquired before loading player data or changing endpoint/shutdown files. EndpointGuard removes only the current instance endpoint. LocalServer distinguishes an attached server from an owned child; only the latter is stopped/reaped. The child receives a private per-start nonce through MINIMIUM_ADMIN_TOKEN and publishes it as the existing admin token, proving ownership without adding a game message. server.shutdown keeps its legacy body; the private server.shutdown.owner sidecar binds the result to that instance. Neither token is logged or included in load reports. Client options separate profile identity from local server storage; `ClientSession` becomes Connected only after Welcome and map-fingerprint validation. The profile ID remains a public reconnect identifier, not authenticated account identity.

Both interactive frontends use `ConnectionController`, which owns the connector, session and local-server guard. DNS/connect/bootstrap work runs in the connector worker; a cancelled or late local result returns to that worker for disposal. A replaced owned guard is cleaned up by a tracked worker, and a new attempt waits for that cleanup. Session polling remains on the frontend loop. Explicit remote connections never create a local guard; an attached guard does not stop a server.

The client application composes the window, GPU resources, renderer-local presentation state, input/UI and settings with the `minimium-client-runtime` connection controller. GPU construction and drawing are split across renderer-owned encoders/resources and app-owned surface/state; movement prediction, snapshot reconciliation and remote-player interpolation are implemented in client-runtime and adapted to renderer-local geometry. After session polling and portal installation, the renderer publishes one `EnvironmentFrameInput` clock/context snapshot, then builds one `EnvironmentFrame` from that snapshot, the physical player observer, the active manifest and the renderer camera. The frame contains local and geocentric Sun/Moon data, horizon/view matrices and the fixed 224-byte camera plus 352-byte environment GPU uniforms; the separate shadow uniform is 128 bytes. The extra environment vector carries monotonic display time for independent star scintillation and does not change world time. The 1000-star batch is shared by surface and globe views; the globe and Moon cover background stars. In globe view the Moon is an analytic world-space sphere whose geocentric orbital position and radius use the same Earth-to-game scale; Sun illumination is evaluated from its surface normal, with real depth against the globe. A separate bounded halo pass adds warm solar glow and phase/orientation-dependent lunar glow. World and globe pipelines share group 0; the globe keeps its texture at group 1, and UI keeps its own layout. sRGB surfaces render directly; other surfaces use a linear `Rgba8Unorm` target followed by one encode pass, keeping UI blending linear. Invalid clock/context/observer input writes an explicit invalid uniform so the previous world's sky data cannot persist. The window title shows the clock quality/date indicator. The explicit demo adapter is selected only by `--demo`. Remote geometry and console buffers are reused; rendered behavior remains subject to the deferred visual scenarios.

Startup acquires the data lock, loads the map and player snapshot with the optional saved clock, starts the persistence worker, binds listeners and publishes the endpoint. ServerRuntime::new then restores the clock before entering the loop that accepts clients; a restore failure does not admit players. Shutdown drains admin replies, closes clients, captures a final immutable player/sector/clock snapshot, joins the worker, removes the owned endpoint and writes the owner-bound result before releasing the lock. A runtime error remains primary if final save or cleanup also fails; secondary errors are reported. This describes ordering, not a verified crash recovery guarantee.

The current extension API is Rust source, map data and validated world definitions, not a dynamic plugin ABI. Keep matching map/world-definition files on clients and server. New simulation rules belong in `minimium-simulation` behind `minimium-world` contracts, never in the renderer or transport. New protocol messages require a separately planned version decision; gameplay remains protocol v13 while world snapshots are save v9. Sample combat mechanics live in simulation; game-specific defaults and scenario setup belong to `minimium-game-sample`. Historical E002/E003 test records cover procedural generation, sector lifecycle, movement, isolated instances, transfer, save migration and profile restoration. A historical wire-level v9 demonstration covered planet → dungeon → planet and restart reconnect. These are not E004 acceptance evidence: protocol v13, save v9 and rendered time/sky scenarios remain RUNTIME NOT RUN after the user-directed closeout without E004-16 verification.
