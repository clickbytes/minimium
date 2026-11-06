# minimium-server-runtime

## Purpose / Non-goals

Owns the authoritative server lifecycle, world registry, tick, networking coordination, admission and persistence composition. It is headless and has no GPU or window role.

## Public API

`run(args, composition)` parses server process arguments, initializes modules and startup state, then runs the authoritative runtime. The application supplies a world-composer callback and combat values through `composition`; the runtime does not choose sample content.

## Dependencies

Coordinates world, simulation, environment, protocol, transport, storage, platform, telemetry, assets and procgen. `composition.rs` defines the neutral world/portal and combat data accepted from the host application. Sample-game code is not a production dependency. Astronomy and GPU crates are forbidden.

## Data flow

The process validates startup inputs and reads world.seed before invoking the application world composer. The application owns saved planet and snapshot seed discovery and requests randomness only when no seed is available. The runtime installs the returned worlds, migrates legacy snapshots, loads save state and then applies bounded network inputs on the owner lane. Workers publish typed results back to the runtime; immutable save snapshots cross into storage.

## Ownership and execution

`ServerRuntime` owns mutable authoritative state on one lane. Worker threads do not mutate active worlds. Runtime shutdown joins workers and persists final state.

`runtime/operations.rs` keeps the loop order explicit and delegates autosave scheduling/capture to `poll_autosave`. `lifecycle.rs` coordinates admin drain, client socket closure, final snapshot capture, persistence shutdown, endpoint cleanup and shutdown status in that order. Storage owns snapshot serialization and worker status; the runtime supplies the live world data and final snapshot.

`WorldRegistry` is the stable runtime façade for world instances. Its `membership.rs` module owns membership and epoch changes, `transfers.rs` owns staged and committed transfers, and `accessors.rs` owns read-only queries and save snapshots. Constructors and entity, sector, environment, and diagnostic operations stay in their dedicated modules.

`sector_streaming.rs` coordinates the per-tick pipeline. `sector_streaming/requests.rs` collects read-only player and transfer requests; `tickets.rs` owns retain/pin/release reconciliation; `install.rs` performs bounded synchronous materialization; `readiness.rs` evaluates collision-ready players and transfers; and `collision.rs` rebuilds changed instances from installed payloads.

## Invariants and errors

Preserve current protocol/save compatibility, tick cadence, movement behavior, admission limits and shutdown ordering. Worker queue, protocol, storage and startup errors remain typed at their boundaries.

## Commands

Startup accepts `--material-package PATH`, `--material-http-port PORT` (0 selects
an available port), and `--climate-scenario PATH`. Both material variants are
validated before admission. The HTTP endpoint serves only manifest-listed files;
it owns one bounded I/O worker and joins it at shutdown. Open the selected HTTP
port alongside the game port for remote clients. Climate initializes and persists
in `world.climate.ron` under the world data lock, outside snapshots; subsequent
starts restore it and reject a replacement initialization argument. The required
material set is session-wide and retained through existing world prepare/ready.
The corresponding properties are `world.climate.initial_scenario` (creation-only)
and `world.climate.relief_influence` (restart-required); climate is not mutable
through a live admin command.
`--environment-config PATH` maps to `world.time.initial_config`, a creation-only
property that initializes a new world's clock; saved snapshots remain authoritative.

Server-owned handlers compose shared command metadata. Built-ins reserve names before module registration, so module aliases cannot shadow host commands. Recognized built-ins with invalid arguments remain parse errors and are never reinterpreted as module commands. Game rules stay with their owning domain.

## Change guide

Change authoritative coordination and process lifecycle here. Reusable movement, collision, environment and storage logic belongs in their respective crates.

## Verification

Run `rtk cargo check -p minimium-server-runtime -p minimium-server` and architecture/file-length checks. Runtime acceptance is deferred.


Protocol v16 admits context-bound `SetAltitude` requests through the bounded
world-input queue. The authoritative handler requires fly mode, a planet, no
pending transfer, finite 0..10000 km and valid destination coordinates. It samples
local collision terrain, applies Earth relief scaling, preserves angular location
and movement settings, resets velocity and replies with the precise destination.
The existing player-store/snapshot path records the relocation; saves are unchanged.

Protocol v17 carries the immutable v5 peak-relief definition and independent
ClimateScenario.relief_influence. Startup --climate-relief-influence accepts a
finite 0..1, applies it to the loaded climate scenario, and atomically persists
the changed scenario under the data lock. Other scenario values are preserved.
It is valid for existing worlds without rebuilding terrain. Omission restores
the persisted setting; legacy climate files default to 0.

Malformed explicit climate-relief numbers report the option name and accepted
range. Omitted options are handled by bootstrap; the server never substitutes a
default for an explicitly supplied invalid number.

Server startup configuration resolves defaults, schema v1 RON, mapped
`MINIMIUM_SERVER_*` environment variables, and CLI values in that order. Repeated
`--set KEY=VALUE` values share typed validation with legacy startup flags. Offline
`config validate` and `config show` run before runtime startup; JSON show output
is versioned, bounded to 64 KiB, and includes resolved values and source labels.
These commands do not lock storage, create worlds, write files, or bind listeners.

`server.autosave_interval` is the first live server property. Its change runs on
the owner lane, updates the autosave deadline, and leaves a save already in
progress untouched. Clock properties use the existing `TimeCommand` and
`EnvironmentClock` validation; world seed and generation identity reads come
from the active manifest.
