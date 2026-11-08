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

The current wire contract is protocol v18; save files use format v10 and
generated worlds use generator revision 5. These are separate version domains.
Older wire, save and generated-world revisions are rejected without fallback or
migration. Create a fresh world in a separate data directory when replacing old
saved data. Terrain and material ownership is split across assets, asset-io,
environment, procgen, renderer and client-runtime; see
[terrain implementation and pending acceptance](terrain-materials.md) for the
current approximation and the visual work that remains unverified.

The client's optional `desktop` feature enables GPU, window, clipboard and
astronomy adapters. A `--no-default-features` client defaults to the terminal,
skips material cooking and consumes only CPU camera/quality/debug contracts from
the renderer with its `gpu` feature disabled. This provides a terminal Cargo
graph without wgpu/winit in addition to the existing `--console` desktop mode.

`architecture.toml` is the machine-readable declaration of current Cargo package responsibilities and selected forbidden dependencies. `scripts/check_architecture.py` rejects duplicate, missing and stale package declarations, then checks resolved all-feature normal/build edges including target-specific and activated optional dependencies; dev edges are reported separately. Role rules prohibit sample-game dependencies in domains/runtimes, GPU dependencies in the server and window/transport/storage/platform dependencies in the renderer. A source scan derives each package root from Cargo metadata and scans Rust sources in every declared domain/runtime plus the server application for filesystem/process/network/GPU/FFI effects that Cargo edges cannot express. This regex guardrail is not a Rust parser or proof: macro-generated code, dynamically constructed names and indirect wrappers may escape it, while matching comments or strings can require review. `scripts/check_static_refactor.py` checks Rust/WGSL layouts and symbol owners; Cargo dependency rules live only in the architecture checker.

Current packages and responsibilities:

| Package | Current responsibility |
| --- | --- |
| `minimium-core` | Foundational identifiers, player profiles, authenticated Principal values and server-granted permission bits. |
| `minimium-world` | World definitions, topology, addressing, geometry, maps, entities and neutral component data. |
| `minimium-simulation` | Mutable world state, movement, collision queries, combat, snapshots and fixed-step results. |
| `minimium-environment` | World clock and environment values, renderer-independent sky/climate/solar-system contracts, and deterministic weather model. |
| `minimium-procgen` | Seeded revisioned world generation, typed errors, sector geometry and terrain sampling through explicit prepared geography inputs. |
| `minimium-sectors` | Bounded ticket, readiness and cache lifecycle. |
| `minimium-assets` | Map, OBJ, sky catalog, material manifest and world-definition parsing from supplied data. |
| `minimium-asset-io` | Bounded authored asset/geography access, material HTTP adapters, and leased content cache. |
| `minimium-platform` | Application-resolved process paths, durable file operations, identity, locks and bounded local administration transport with versioned structured error decoding. |
| `minimium-storage` | Exact version-10 save format, entity/component validation, snapshot budget, player store and bounded persistence worker. |
| `minimium-transport` | Bounded socket byte flushing without protocol or game policy. |
| `minimium-commands` | Generic command registry; domains own command values and validation, while hosts own execution. |
| `minimium-protocol` | Versioned message models, codecs and validation. |
| `minimium-diagnostics` | Bounded domain events, stable codes and the nonblocking diagnostic sink contract. |
| `minimium-gameplay` | Neutral module, command, system and component registration plus bounded world effects. |
| `minimium-localization` | Typed UI messages, English/Polish Fluent catalogs, fallback and formatting. |
| `minimium-astronomy` | Client astronomy calculations and FFI adapter consuming environment/world contracts. |
| `minimium-telemetry` | Per-process run directories, bounded asynchronous diagnostics backend, metrics, process sampling and optional allocation instrumentation. |
| `minimium-server` | Server application, process paths/log lifetime, explicit development identity provider and sample composition. |
| `minimium-server-runtime` | Authoritative server lifecycle/admission, principal permissions, tick and coordination of transport, persistence and gameplay contracts. |
| `minimium-client-runtime` | UI/GPU-independent session, resolver contract, connection/child lifecycle, prediction, reconciliation, replication and clock sync. |
| `minimium-renderer` | CPU frame and geometry publication contracts, camera/visibility/LOD, GPU resources and WGSL pass encoders; client owns preparation workers and frame scheduling. |
| `minimium-client` | Desktop/text application state, shared world resolver/settings/transitions, input, localization and renderer integration. |
| `minimiumctl` | Administrative CLI. |
| `minimium-loadtest` | Headless scenario client. |
| `minimium-game-sample` | Demo maps/presets, sample world composition, portal component/system and combat defaults; consumed by applications. |

The world, simulation, command and environment packages own their public domain contracts. `minimium-gameplay` defines static extension contracts; the sample game implements them without reaching server infrastructure. `minimium-renderer` owns camera-frame preparation, visibility/LOD, GPU resources and scene/shadow/cloud/atmosphere/UI/output encoders, while the client application owns settings, world/transfer lifecycle, input, preparation workers and frame scheduling. `ClientView` is adapted to CPU-only `RenderFrame` values. Server authority and client prediction share the simulation integrator. `minimium-assets` parses supplied data; `minimium-asset-io` owns bounded file/HTTP/cache access. `minimium-storage` owns v10 snapshots and its persistence worker; `minimium-transport` owns bounded socket byte progress. `minimium-diagnostics` is the domain contract and `minimium-telemetry` owns run directories and the file sink.

## Repository package layout

~~~text
apps/client/          desktop and terminal client application
apps/server/          headless server application
apps/ctl/             local administration CLI
apps/loadtest/        headless network scenario client
crates/               current workspace engine, adapter, runtime and presentation packages
games/sample/         demo content and sample module using public engine APIs
tools/geography/      offline GEBCO package preparation tools
scripts/              source boundary, file length and WGSL validation tools
docs/                 architecture, protocol, storage and implementation notes
~~~

`architecture.toml` lists every current Cargo package and its role. It is the source for enforced dependency rules; no placeholder crates are planned here.

## Enforced dependency boundaries

The current Cargo graph and source boundary checks live in `architecture.toml`
and `scripts/check_architecture.py`. The server/runtime cannot reach GPU/window
packages; the renderer cannot reach window, transport, storage, platform or
runtime packages; domains and runtimes cannot depend on sample content;
simulation and generation remain free of process, filesystem, network and GPU
effects. The checker also scans Rust sources for explicit effects that package
edges cannot express.

## Server tick and ownership

The server uses a fixed simulation step. Network and admin input enter bounded queues and are applied at named tick boundaries. The simulation owns mutable world state. Each entity belongs to one zone. Parallel jobs read a stable phase snapshot and return results for a controlled merge. Network and console threads never mutate world state directly. Persistence work leaves the time-critical tick path.

### Modular engine capabilities

`minimium-gameplay` provides static startup registration through `EngineModule` and `ModuleRegistry`. A module declares an ID, version and dependencies, then registers typed commands, systems and component descriptors. This Rust API does not load dynamic libraries or define a stable plugin ABI. The server registers its built-in world module and the application-provided `SampleGameplayModule`. Systems run at named stages with a current tick, read-only world views and bounded effects; authoritative mutation remains on the server lane. Stable dependency ordering rejects missing, duplicate or cyclic registration before runtime startup.

`minimium-world::entity` exposes world-scoped `EntityStore` and exact component schemas. Spawn, upsert, removal and despawn append revisioned changes to a bounded journal. `minimium-storage` stores current entities/components and validates them against schemas registered before load. Protocol v18 sends full or delta entity images per client; visibility policy filters components and the client stages/validates a sync before atomically publishing its replica and acknowledging. `content_initialized` is persisted per world so New-only Startup effects are not replayed after restore. `minimium-environment::universe` owns the bounded solar-system definition graph with fixed or simplified circular ephemerides, not an N-body solver.

`ServerRuntime` owns `GameState`, `ConnectionState`, `OperationsState`, and `Scheduler`. Each iteration polls persistence completions and autosave, samples memory, polls admin/terminal, then accepts, receives, dispatches, handles EOF/timeouts, sends and captures disconnects. A due tick executes commands, applies inputs, expires stale movement and simulates; a due snapshot follows, then the scheduler sleeps. A stop command still completes the current tick and snapshot check. Admin or accept errors enter the same controlled shutdown path, which reports and attempts the final save while the endpoint guard and data lock remain held. Runtime does not claim a tested failure recovery guarantee.

Admission checks connection phase, current world context, bounded queue capacity and the authenticated `Principal` permission before an action reaches the authoritative lane. `Gameplay` is required for ordinary mutating inputs; movement-policy and developer actions require their dedicated server-granted permission. The owner lane applies queued inputs and scheduled movement, resolves sequence acknowledgements only after an input can no longer execute, then advances simulation and emits snapshots. Leave is ordered after prior accepted inputs and its pending profile blocks reconnect until applied.

The runtime orders each due tick as command application, admitted input application, scheduled movement, simulation, world effects/transfers, interest/readiness updates, replication and metrics. Work that needs materialization or collision indexing is ticketed to bounded workers and published only after the current world/revision check.

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

The following describes source-level ownership and contracts. Compilation is deferred to E046-17; runtime behavior and performance are not established here.

The workspace contains the packages listed in `architecture.toml`, including the diagnostics, gameplay and localization contracts. Core owns identity and principal permissions; world owns manifests and component/entity data; simulation owns shared movement/collision rules; gameplay owns registration/effect interfaces; storage owns v10 persistence; protocol owns v18 wire types; procgen accepts only generated revision 5; asset-io and platform own external I/O adapters. Runtime/app packages compose these boundaries. The architecture checker traverses all-feature production normal/build edges, including target-specific dependencies, reports dev edges separately, and source-scans declared domain/runtime roots for explicit effects. This is a guardrail, not proof that indirect effects are absent. The static scripts are scheduled for E046-17 and have not yet been run.

The workspace contains `minimium-core`, `minimium-renderer`, `minimium-server-runtime`, `minimium-client-runtime`, `minimium-world`, `minimium-simulation`, `minimium-environment`, `minimium-commands`, `minimium-astronomy`, `minimium-procgen`, `minimium-sectors`, `minimium-assets`, `minimium-asset-io`, `minimium-platform`, `minimium-protocol`, `minimium-storage`, `minimium-transport`, `minimium-telemetry`, `minimium-server`, `minimium-client`, `minimium-game-sample`, `minimiumctl`, and `minimium-loadtest`. Platform owns bounded persistent-file reads, atomic writes/replacement, profile files, server locks and endpoint/admin helpers; storage owns snapshot parsing and validation. Assets parses maps, OBJ and versioned manual dungeon definitions; asset-io opens bounded map, mesh and world-definition files. The desktop client uses wgpu/winit and can launch a local server or connect remotely. Press F1 for its command console; `--console` selects the text client. Client and server accept `--map PATH` or `--world-definition PATH`; the local client passes the same path to its child server. The default client/server generate matching demo worlds. The server's `--game-port 0` asks the OS for an ephemeral port.

`minimium-environment::time` owns validated UTC, scale, clock anchor/sample and saved-clock types, plus pure clock evaluation, mutation and restore operations. The environment crate root owns profile and versioned astronomy/catalogue identifiers. `minimium-environment::sky` owns observer and sky-model contracts, `universe` owns the solar-system catalogue, and `weather` owns deterministic weather/cloud models behind a `TerrainSampler` host port. These contracts use platform-neutral values; the server supplies UTC and monotonic samples, storage v10 owns serialization, protocol v18 owns wire encoding, and the client owns astronomy and rendering. Clock state is independent of `WorldManifest`, generator inputs and geometry fingerprints.

The server runtime adapts one `EnvironmentClock` to the same `server_started` monotonic epoch used by uptime snapshots. It creates a separate OS-random 16-byte session nonce, checks UTC every five seconds in RealTime mode, and reanchors on corrections of at least 250 ms. Time mutations share the authenticated local admin/terminal command path and never scale physics, cooldowns or network deadlines. The bounded `--environment-config` initializes a new world only. Save v10 persists clock state in the same atomic player/world snapshot; protocol v18 carries samples and environment descriptors.

Connection establishment and an established session have separate owners. `Connector` runs DNS resolution, sequential address attempts, and local server bootstrap on one worker thread. Its timeout is a common result deadline; cancellation or timeout discards a late result, but cannot interrupt a blocking resolver or startup call, so the connector remains busy until its worker exits. Once a socket is returned, `ClientSession` owns a nonblocking TCP stream, bounded read/write buffers and the per-session clock estimator, but no thread. The desktop redraw loop and text-client loop poll it and handle typed events. The estimator uses one local monotonic epoch, a bounded offset filter and session-local nonce; it publishes the active date/descriptor/context as one immutable `EnvironmentFrameInput`. Portal commits switch descriptors without resetting the shared clock, while reconnect discards every estimator state. The text client prints concise quality/date/rate/revision status on changes. Explicit `--demo` mode has a separate monotonic RealTime adapter and is not a network fallback.

The desktop and text front ends share the startup `client_commands` registry (`command_specs`, `help_lines`) for syntax, help, aliases and completion. Interface, connection, gameplay and environment modules register their commands once. It currently accepts `help`, `host local [--threads 1|2|auto]`, `connect HOST:PORT`, `disconnect`, `fps show`, `net stats`, `time status`, `sky info`, `sky quality low|standard`, `ping`, `move X Z`, `jump`, `portal`, `map_telepo`, `fire`, `say TEXT` (alias `chat`), `name NAME`, `state`, `clear`, and `quit` (alias `exit`). Front ends handle UI or process actions; commands that send game traffic queue typed protocol messages on the active session. Server commands are parsed by `ServerCommand::parse` and share terminal/admin dispatch for `help`, `status`, `players list`, `metrics show`, `world save`, `stop`, and the parameterized `time status|mode|set|scale|offline` family. Client time diagnostics are read-only; clock mutation stays in the administrative channel.

The server is headless, caps players at 512 by default and pending handshakes at 64, tracks players in spatial cells and prepares snapshots at 10 Hz. Protocol v18 snapshots and gameplay inputs carry world context; per-instance simulations route events and discard stale membership epochs. The demo's planet uses radial up and tangent movement, while dungeon movement uses fixed Y-up. Server and client share bounded ticketed sector caches and topology-specific neighborhood policies; server movement and client prediction wait for matching collision readiness. Client/server transfers use prepare/ready/commit and switch context only after target validation and readiness. A new data directory samples and persists a procedural seed; external map and authored world-definition files are validated before use.

`--threads 1..64|auto` selects execution lanes. One lane executes work inline; additional workers perform bounded independent tasks against immutable snapshots. `world save`, autosave and shutdown capture player, sector, clock and current entity/component state on the owner thread, then pass one immutable snapshot to the persistence worker. Save v10 accepts only its exact header and validates records against the registered component schemas. It counts and streams the same encoding through a size-limited writer before atomic replacement. Versions 1–9 and unknown versions are rejected; no automatic migration is supported. Use a separate data directory for a new-format world. The client profile ID is a durable local identifier, not account authentication.

Game traffic uses protocol v18 over TCP. A bounded provider credential is mapped by the configured authentication provider to a server-side `Principal`; the example `dev.profile` provider is an insecure development identity claim, not an account system. Snapshot messages include the server tick, world context, safe resolved input prefix and bounded player deltas. Fire requests use server-side spatial hitscan and cooldown validation. Entity/component replication uses per-client acknowledged revisions. Portal interaction carries an entity ID; transfer carries transition IDs and target manifests. Bounded UTF-8 chat uses capped output queues. Public accounts and TLS are outside the current implementation. No performance result is claimed.

## Current module contracts

| Boundary | Input and output | Ownership / integration |
| --- | --- | --- |
| assets::parse_map / load_map | Bounded text or file to validated map, or an error | Both client and server use the same canonical fingerprint; maps and OBJ source are the current data extension surface. |
| World::step_players_with | Player input map, fixed delta and dispatch function to collision query counters and updated world | Main lane owns mutations; jobs read immutable collision geometry and return results merged in input order. |
| World::network_snapshot / visit_nearby | Immutable snapshot and recipient to bounded nearest player records | No socket/GPU dependency; server builds full/delta output only for recipients. |
| protocol codecs | Typed messages to byte buffers; byte slice to complete decoded frame + consumed length, incomplete, or error | v18 framing, world context, input sequencing and entity sync are independent of TCP. Transport adapters own partial buffers. |
| ServerCommand::parse / server commands::execute | Text to typed base/time commands; context to success/text/stop reply | Terminal and authenticated IPC enqueue; one dispatcher executes up to 8 commands before simulation. |
| admin_client::request / stop_and_wait | Captured endpoint + typed-command text to payload/error; data directory + endpoint to final stop result | 2s connect, 3s response, 64 KiB reply cap; stop waits up to 120s; no game-protocol authority. |
| Connector / ClientSession | Connection request to worker result; connected socket to bounded nonblocking protocol events | Connector worker does resolver/connect/bootstrap work; host loop polls session and owns UI events. |
| minimium-commands::CommandRegistry | Typed specs to longest-name/alias parsing and usage metadata | Client command groups and server admin syntax share the parser; each host owns execution. |
| renderer WGSL | Renderer-owned frame, world, terrain, ocean, shadow, atmosphere, cloud, UI and output shader sources | Compile-time shader composition; no runtime shader loader. |
| Executor::map | Owned items + immutable operation to ordered results | Inline one lane, bounded worker queue for other lanes; phase context inherited by jobs. |
| PlayerStore / PersistenceWorker | Validated player records to immutable snapshot view and asynchronous result | Storage owns serialization/disk I/O; final shutdown joins storage before releasing the data lock. |
| diagnostics / telemetry | Bounded domain events to process-scoped JSONL run files; metrics and process samples to bounded stores | Domains use `minimium-diagnostics`; application owns sink/run lifetime and telemetry owns files, rotation and retention. |

ServerDataLock is acquired before loading player data or changing endpoint/shutdown files. EndpointGuard removes only the current instance endpoint. LocalServer distinguishes an attached server from an owned child; only the latter is stopped/reaped. The child receives a private per-start nonce through MINIMIUM_ADMIN_TOKEN and publishes it as the existing admin token, proving ownership without adding a game message. server.shutdown keeps its legacy body; the private server.shutdown.owner sidecar binds the result to that instance. Neither token is logged or included in load reports. Client options separate profile identity from local server storage; `ClientSession` becomes Connected only after Welcome and map-fingerprint validation. The profile ID remains a public reconnect identifier, not authenticated account identity.

Both interactive frontends use `ConnectionController`, which owns the connector, session and local-server guard. DNS/connect/bootstrap work runs in the connector worker; a cancelled or late local result returns to that worker for disposal. A replaced owned guard is cleaned up by a tracked worker, and a new attempt waits for that cleanup. Session polling remains on the frontend loop. Explicit remote connections never create a local guard; an attached guard does not stop a server.

The client application owns `ClientApplication`, typed settings, connection and world-transition lifecycle, localized UI models, input mapping, world content catalog and frame scheduling. `minimium-client-runtime` owns session/prediction/reconciliation and publishes GPU-independent views. The application adapts those views into CPU-only `RenderFrame` values; the renderer owns GPU resources and ordered pass encoding, while the client owns surface/window state and bounded preparation workers. Shadow CPU selection is prepared outside `GpuResources`; a frame encodes the shadow pass only when map and receiver geometry share a complete `GeometryPublicationStamp`. The application owns the locale and formats typed messages through `minimium-localization`; desktop and terminal share settings, content resolution and runtime contracts. Rendered behavior remains subject to deferred visual review.

Startup acquires the data lock, loads the map and player snapshot with the optional saved clock, starts the persistence worker, binds listeners and publishes the endpoint. ServerRuntime::new then restores the clock before entering the loop that accepts clients; a restore failure does not admit players. Shutdown drains admin replies, closes clients, captures a final immutable player/sector/clock snapshot, joins the worker, removes the owned endpoint and writes the owner-bound result before releasing the lock. A runtime error remains primary if final save or cleanup also fails; secondary errors are reported. This describes ordering, not a verified crash recovery guarantee.

The extension API is Rust source through `minimium-gameplay`, map data and validated world definitions; it is not a dynamic plugin ABI. Keep generated manifests and verified authored content available to both clients and server. New protocol messages require a separately planned version decision. The active contracts are wire v18, save format v10 and generated revision 5; each rejects unsupported revisions without migration or fallback. Sample systems/components live in `minimium-game-sample`; engine domains do not depend on that package. The example `dev.profile` provider is a local development claim, not public account authentication. TLS is not implemented. Source review, compilation and runtime acceptance are tracked separately in `docs/architecture-execution.md`.

Atmosphere presentation now has one camera-relative renderer path from the ground
to orbit. Renderer owns bounded optical/sky/aerial LUTs, regional fog presentation,
RGB scene composition and optional asynchronous GPU timing. Common frame bindings
include a separate 256-byte atmosphere view/optics uniform; the 224-byte camera
uniform remains unchanged. The client prepares a render-only astronomical
reference at the camera's latitude/longitude with bounded reference altitude,
while optical rendering uses the uncapped camera. This supersedes the earlier
player-only render-observer description above. Environment exposes physical
source elevation through its terrain port, and client workers publish
sea-level-relative weather/fog grids. No new server, protocol or save data is
introduced by this atmosphere integration. Runtime visual acceptance is pending.


Terrain presentation preparation now runs on client-owned latest-request workers,
independently for active and prepared target worlds. Renderer owns bounded mesh
selection/cache, pure CPU vertex preparation, frame-wide upload budgets and GPU
rebasing. Sector collision generation/lifecycle remains with the existing
procgen/sectors/application path. Protocol v18 carries context-bound altitude
request; server runtime composes authoritative collision sampling and relocation.
The headless client runtime and protocol retain no GPU dependencies.

The client also schedules planet collision materialization on a bounded worker.
The immutable map/manifest/geography cross the worker boundary; ticket installation,
pinning, readiness and CollisionIndex publication remain on the client owner lane.

## Shared planet presentation update (2026-11-06)

This supersedes the earlier separate globe ownership description. Both planet
cameras use the terrain worker/quadtree/material pipeline. Presentation interest
carries camera projection scale independently of collision interest. Procgen
owns static landforms/coverage, environment owns shared site climate/history,
renderer owns edge conformance/LOD/atomic GPU publication, and the client owns
worker/camera lifetimes. Sample content owns the surface recipe.
No new crate, protocol version, save change or generator revision is introduced.

The baked globe palette/height texture and detail sphere have been removed.
Global/local surface weather uses the same UTC/scenario model; fog retains its
own local presentation grid. See [terrain presentation](terrain-materials.md)
for contracts, diagnostics, approximation limits and deferred acceptance.

Earth water presentation (2026-11-06): procgen exposes separate signed ground
and canonical sea-level samples while authoritative sampling stays unchanged.
Renderer owns a sea-level sphere tessellated with the ground quadtree, paired
edge conformance, submerged-ground rejection, ocean vertex/shader resources and
atomic publication/rebasing with land. The client worker supplies both meshes.
Binary shoreline clipping and material water blending no longer define the coast.
This introduces no filesystem, transport or GPU effects into domain sampling.

Peak relief and climate policy (2026-11-06): world owns the pure bounded curve;
sample content owns the preset adapter; procgen applies it identically to
authoritative and presented Earth height. Generator v5 opts in, preserving v3/v4.
Storage v10 gates four extra manifest fields on generator revision 5.
Protocol v18 carries curve settings, climate influence and context-bound gameplay actions to all consumers.
Environment owns the separately persisted climate-relief influence, with source
height defaulting to Earth. Renderer carries both elevations (112-byte vertices)
and uses the same influence for temperature/snow correction. Fog/cloud clearance
continues to follow displayed geometry. Server startup owns validated, atomic
updates to the independent world.climate.ron configuration.

Local child-process stderr collection belongs to client-runtime bootstrap. A
bounded tail is forwarded with connection errors; the application only renders
the supplied message. Startup diagnostics do not introduce a storage/GPU coupling.

Optional process-environment values are normalized by client-runtime bootstrap
before attachment decisions or child argument construction. The Earth launcher
removes omitted entries explicitly; server-runtime owns numeric CLI validation.

Renderer owns guarded overview frustum selection and cached source-height coast
classification on the procgen-owned render block lattice. The application supplies
viewport/camera values and runs existing preparation workers. Shared ground/ocean
geometry, collision generation, saves and shader ownership remain unchanged.


Camera-dependent shadows (2026-11-06): renderer owns pure ShadowRegion fitting,
world-anchored f64 light-space projection, bounded off-screen caster LOD and
pre-exposed HDR surface/atmosphere presentation. The client owns live buffer
selection, source selection, map/player readiness and padded authored-map caching.
Both planetary cameras use one dominant Sun/Moon depth pass at fixed resolution.
Night view exposure is derived from the rendered footprint and preserves lunar
irradiance; it does not modify astronomy, climate, world time or snapshots.
Existing celestial sprites remain display-calibrated. No new package, protocol
revision, persistent world field or gameplay effect is introduced.

## Ambient cosmetic meteors baseline (2026-11-08)

[ADR 0013](adr/0013-ambient-meteors.md) freezes the E047 contract before any
implementation: standard meteors are client-local cosmetic presentation effects
with no server tick, world entity, replication, collision, save, protocol message
or generator change. `minimium-renderer` owns the CPU-only `MeteorFrame`/
transient-light contract, the f64-physical-metre-to-f32-render-origin adapter and
the shared GPU/bind-group extension; `minimium-client` owns the local lifecycle,
spawner and developer commands. Lifetimes run on the client monotonic clock;
synchronized world UTC only gates exposure conditions. The existing protocol-v19
`BolidEvent` demo path and the `sky demo`/`sky quality` semantics are unchanged.
No meteor code exists at this baseline.
